//! Durable deposit outbox inspection and at-least-once webhook delivery.

use kaspa_frontier_engine::{
    ClaimedLedgerEvent, DeliveryFailureOutcome, DepositLedger, LedgerEvent,
};
use reqwest::{Client, Url};
use serde::Serialize;
use std::env;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const DEFAULT_DATABASE: &str = ".local/tn10-wrpc-live.sqlite";
const DEFAULT_LIMIT: usize = 100;
const LEASE_SECONDS: u64 = 60;
const DEFAULT_MAX_ATTEMPTS: u32 = 5;
const DEFAULT_RETRY_BASE_SECONDS: u64 = 5;
const DEFAULT_RETRY_MAX_SECONDS: u64 = 300;

enum Command {
    List {
        dead_only: bool,
    },
    Ack {
        id: i64,
    },
    Requeue {
        id: i64,
    },
    Deliver {
        endpoint: Url,
        limit: usize,
        max_attempts: u32,
        retry_base_seconds: u64,
        retry_max_seconds: u64,
    },
}

struct Options {
    database: PathBuf,
    command: Command,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DeliveryBody<'a> {
    schema_version: u8,
    event: &'a LedgerEvent,
}

#[derive(Debug, Default, PartialEq, Eq)]
struct DeliveryReport {
    delivered: usize,
    retry_scheduled: usize,
    dead_lettered: usize,
}

#[derive(Debug, Clone, Copy)]
struct DeliveryPolicy {
    max_attempts: u32,
    retry_base_seconds: u64,
    retry_max_seconds: u64,
}

fn usage() -> &'static str {
    "usage: tn10-outbox <list|dead|ack ID|requeue ID|deliver URL> [--database PATH] [--limit N] [--max-attempts N] [--retry-base-seconds N] [--retry-max-seconds N]"
}

fn parse_args(arguments: impl IntoIterator<Item = String>) -> Result<Options, String> {
    let mut args = arguments.into_iter();
    let action = args.next().ok_or_else(|| usage().to_string())?;
    let mut command = match action.as_str() {
        "list" => Command::List { dead_only: false },
        "dead" => Command::List { dead_only: true },
        "ack" => {
            let id = args
                .next()
                .ok_or("ack requires an event ID")?
                .parse::<i64>()
                .map_err(|_| "event ID must be an integer")?;
            if id <= 0 {
                return Err("event ID must be positive".into());
            }
            Command::Ack { id }
        }
        "requeue" => {
            let id = args
                .next()
                .ok_or("requeue requires an event ID")?
                .parse::<i64>()
                .map_err(|_| "event ID must be an integer")?;
            if id <= 0 {
                return Err("event ID must be positive".into());
            }
            Command::Requeue { id }
        }
        "deliver" => {
            let raw = args.next().ok_or("deliver requires a webhook URL")?;
            Command::Deliver {
                endpoint: safe_endpoint(&raw)?,
                limit: DEFAULT_LIMIT,
                max_attempts: DEFAULT_MAX_ATTEMPTS,
                retry_base_seconds: DEFAULT_RETRY_BASE_SECONDS,
                retry_max_seconds: DEFAULT_RETRY_MAX_SECONDS,
            }
        }
        _ => return Err(usage().into()),
    };
    let mut database = PathBuf::from(DEFAULT_DATABASE);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--database" => database = PathBuf::from(args.next().ok_or("--database needs a path")?),
            "--limit" => {
                let limit = args
                    .next()
                    .ok_or("--limit needs a value")?
                    .parse::<usize>()
                    .map_err(|_| "--limit must be an integer")?;
                if !(1..=10_000).contains(&limit) {
                    return Err("--limit must be 1-10000".into());
                }
                let Command::Deliver {
                    limit: command_limit,
                    ..
                } = &mut command
                else {
                    return Err("--limit is valid only with deliver".into());
                };
                *command_limit = limit;
            }
            "--max-attempts" => {
                let value = args
                    .next()
                    .ok_or("--max-attempts needs a value")?
                    .parse::<u32>()
                    .map_err(|_| "--max-attempts must be an integer")?;
                if !(1..=100).contains(&value) {
                    return Err("--max-attempts must be 1-100".into());
                }
                let Command::Deliver { max_attempts, .. } = &mut command else {
                    return Err("--max-attempts is valid only with deliver".into());
                };
                *max_attempts = value;
            }
            "--retry-base-seconds" => {
                let value = parse_retry_seconds(
                    args.next().ok_or("--retry-base-seconds needs a value")?,
                    "--retry-base-seconds",
                )?;
                let Command::Deliver {
                    retry_base_seconds, ..
                } = &mut command
                else {
                    return Err("--retry-base-seconds is valid only with deliver".into());
                };
                *retry_base_seconds = value;
            }
            "--retry-max-seconds" => {
                let value = parse_retry_seconds(
                    args.next().ok_or("--retry-max-seconds needs a value")?,
                    "--retry-max-seconds",
                )?;
                let Command::Deliver {
                    retry_max_seconds, ..
                } = &mut command
                else {
                    return Err("--retry-max-seconds is valid only with deliver".into());
                };
                *retry_max_seconds = value;
            }
            _ => return Err(format!("unknown argument {argument}")),
        }
    }
    if let Command::Deliver {
        retry_base_seconds,
        retry_max_seconds,
        ..
    } = &command
    {
        if retry_base_seconds > retry_max_seconds {
            return Err("--retry-base-seconds cannot exceed --retry-max-seconds".into());
        }
    }
    Ok(Options { database, command })
}

fn parse_retry_seconds(raw: String, flag: &str) -> Result<u64, String> {
    let value = raw
        .parse::<u64>()
        .map_err(|_| format!("{flag} must be an integer"))?;
    if !(1..=86_400).contains(&value) {
        return Err(format!("{flag} must be 1-86400"));
    }
    Ok(value)
}

fn safe_endpoint(raw: &str) -> Result<Url, String> {
    let endpoint = Url::parse(raw).map_err(|error| format!("invalid webhook URL: {error}"))?;
    if !endpoint.username().is_empty() || endpoint.password().is_some() {
        return Err("webhook URL must not contain credentials".into());
    }
    if endpoint.fragment().is_some() {
        return Err("webhook URL must not contain a fragment".into());
    }
    match endpoint.scheme() {
        "https" => {}
        "http" => {
            let loopback = matches!(endpoint.host_str(), Some("127.0.0.1" | "::1" | "localhost"));
            if !loopback {
                return Err("cleartext webhook delivery is restricted to loopback".into());
            }
        }
        _ => return Err("webhook URL must use HTTPS or loopback HTTP".into()),
    }
    Ok(endpoint)
}

fn now_epoch_seconds() -> Result<u64, Box<dyn std::error::Error>> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())
}

fn retry_delay_seconds(attempts: u32, base_seconds: u64, max_seconds: u64) -> u64 {
    let shift = attempts.saturating_sub(1).min(63);
    base_seconds
        .checked_mul(1u64 << shift)
        .unwrap_or(u64::MAX)
        .min(max_seconds)
}

fn record_delivery_failure(
    ledger: &mut DepositLedger,
    claim: &ClaimedLedgerEvent,
    owner: &str,
    error: &str,
    now: u64,
    policy: DeliveryPolicy,
) -> Result<DeliveryFailureOutcome, Box<dyn std::error::Error>> {
    let stored_error = if error.len() <= 1_024 {
        error
    } else {
        "webhook delivery failed; details exceeded storage limit"
    };
    let delay = retry_delay_seconds(
        claim.attempts,
        policy.retry_base_seconds,
        policy.retry_max_seconds,
    );
    let next_attempt_at = now.checked_add(delay).ok_or("outbox retry time overflow")?;
    Ok(ledger.fail_claim(
        claim.event.id,
        owner,
        stored_error,
        now,
        next_attempt_at,
        policy.max_attempts,
    )?)
}

async fn deliver(
    ledger: &mut DepositLedger,
    endpoint: &Url,
    limit: usize,
    policy: DeliveryPolicy,
) -> Result<DeliveryReport, Box<dyn std::error::Error>> {
    let client = Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(15))
        .user_agent("kaspa-frontier-engine/tn10-outbox")
        .build()?;
    let owner = format!(
        "tn10-outbox-{}-{}",
        std::process::id(),
        now_epoch_seconds()?
    );
    let mut report = DeliveryReport::default();
    let mut processed = 0usize;
    while processed < limit {
        let now = now_epoch_seconds()?;
        let Some(claim) = ledger.claim_next_delivery(&owner, now, LEASE_SECONDS)? else {
            break;
        };
        processed += 1;
        let response = client
            .post(endpoint.clone())
            .header("Idempotency-Key", &claim.event.event_key)
            .json(&DeliveryBody {
                schema_version: 1,
                event: &claim.event,
            })
            .send()
            .await;
        match response {
            Ok(response) if response.status().is_success() => {
                ledger.acknowledge_claim(claim.event.id, &owner)?;
                println!(
                    "delivered  id={} key={}",
                    claim.event.id, claim.event.event_key
                );
                report.delivered += 1;
            }
            Ok(response) => {
                let error = format!("webhook returned HTTP {}", response.status());
                match record_delivery_failure(ledger, &claim, &owner, &error, now, policy)? {
                    DeliveryFailureOutcome::RetryScheduled {
                        attempts,
                        next_attempt_at,
                    } => {
                        println!(
                            "retry      id={} attempt={} at={} error={error}",
                            claim.event.id, attempts, next_attempt_at
                        );
                        report.retry_scheduled += 1;
                    }
                    DeliveryFailureOutcome::DeadLettered { attempts } => {
                        println!(
                            "dead       id={} attempts={} error={error}",
                            claim.event.id, attempts
                        );
                        report.dead_lettered += 1;
                    }
                }
            }
            Err(error) => {
                let error = format!("webhook request failed: {error}");
                match record_delivery_failure(ledger, &claim, &owner, &error, now, policy)? {
                    DeliveryFailureOutcome::RetryScheduled {
                        attempts,
                        next_attempt_at,
                    } => {
                        println!(
                            "retry      id={} attempt={} at={} error={error}",
                            claim.event.id, attempts, next_attempt_at
                        );
                        report.retry_scheduled += 1;
                    }
                    DeliveryFailureOutcome::DeadLettered { attempts } => {
                        println!(
                            "dead       id={} attempts={} error={error}",
                            claim.event.id, attempts
                        );
                        report.dead_lettered += 1;
                    }
                }
            }
        }
    }
    Ok(report)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options =
        parse_args(env::args().skip(1)).map_err(|error| format!("{error}\n{}", usage()))?;
    let mut ledger = DepositLedger::open(&options.database)?;
    match options.command {
        Command::List { dead_only } => {
            for status in ledger.outbox_statuses(dead_only)? {
                println!("{}", serde_json::to_string(&status)?);
            }
        }
        Command::Ack { id } => {
            ledger.acknowledge_event(id)?;
            println!("acknowledged {id}");
        }
        Command::Requeue { id } => {
            ledger.requeue_dead_letter(id, now_epoch_seconds()?)?;
            println!("requeued    {id}");
        }
        Command::Deliver {
            endpoint,
            limit,
            max_attempts,
            retry_base_seconds,
            retry_max_seconds,
        } => {
            let report = deliver(
                &mut ledger,
                &endpoint,
                limit,
                DeliveryPolicy {
                    max_attempts,
                    retry_base_seconds,
                    retry_max_seconds,
                },
            )
            .await?;
            println!(
                "complete   delivered={} retry={} dead={}",
                report.delivered, report.retry_scheduled, report.dead_lettered
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::State;
    use axum::http::HeaderMap;
    use axum::routing::post;
    use axum::{Json, Router};
    use kaspa_frontier_engine::{ConfirmedDeposit, UtxoAppearance};
    use std::sync::{Arc, Mutex};

    #[test]
    fn endpoint_requires_tls_except_loopback() {
        assert!(safe_endpoint("https://custody.example/events").is_ok());
        assert!(safe_endpoint("http://127.0.0.1:8080/events").is_ok());
        assert!(safe_endpoint("http://localhost:8080/events").is_ok());
        assert!(safe_endpoint("http://192.0.2.1/events").is_err());
        assert!(safe_endpoint("https://user:pass@example.com/events").is_err());
        assert!(safe_endpoint("file:///tmp/events").is_err());
    }

    #[test]
    fn parses_bounded_delivery_options() {
        let options = parse_args([
            "deliver".into(),
            "https://custody.example/events".into(),
            "--limit".into(),
            "5".into(),
            "--database".into(),
            "state.sqlite".into(),
        ])
        .unwrap();
        assert_eq!(options.database, PathBuf::from("state.sqlite"));
        assert!(matches!(options.command, Command::Deliver { limit: 5, .. }));
        assert!(parse_args(["list".into(), "--limit".into(), "1".into()]).is_err());
        let options = parse_args([
            "deliver".into(),
            "https://custody.example/events".into(),
            "--max-attempts".into(),
            "7".into(),
            "--retry-base-seconds".into(),
            "2".into(),
            "--retry-max-seconds".into(),
            "30".into(),
        ])
        .unwrap();
        assert!(matches!(
            options.command,
            Command::Deliver {
                max_attempts: 7,
                retry_base_seconds: 2,
                retry_max_seconds: 30,
                ..
            }
        ));
        assert!(parse_args([
            "deliver".into(),
            "https://custody.example/events".into(),
            "--retry-base-seconds".into(),
            "60".into(),
            "--retry-max-seconds".into(),
            "30".into(),
        ])
        .is_err());
        assert_eq!(retry_delay_seconds(1, 5, 300), 5);
        assert_eq!(retry_delay_seconds(3, 5, 300), 20);
        assert_eq!(retry_delay_seconds(100, 5, 300), 300);
        assert!(matches!(
            parse_args(["dead".into()]).unwrap().command,
            Command::List { dead_only: true }
        ));
        assert!(matches!(
            parse_args(["requeue".into(), "9".into()]).unwrap().command,
            Command::Requeue { id: 9 }
        ));
    }

    #[tokio::test]
    async fn successful_webhook_receives_stable_key_before_ack() {
        type Received = Arc<Mutex<Vec<(String, serde_json::Value)>>>;

        async fn receive(
            State(received): State<Received>,
            headers: HeaderMap,
            Json(body): Json<serde_json::Value>,
        ) {
            let key = headers
                .get("idempotency-key")
                .and_then(|value| value.to_str().ok())
                .unwrap_or_default()
                .to_string();
            received.lock().unwrap().push((key, body));
        }

        let received: Received = Arc::new(Mutex::new(Vec::new()));
        let app = Router::new()
            .route("/events", post(receive))
            .with_state(received.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let mut ledger = DepositLedger::open(":memory:").unwrap();
        let observed = UtxoAppearance {
            tx_id: "tx".into(),
            output_index: 0,
            address: "kaspatest:abc".into(),
            amount_sompi: 10,
            block_daa_score: 100,
            virtual_daa: 160,
            is_coinbase: false,
        };
        let confirmed = ConfirmedDeposit {
            tx_id: "tx".into(),
            output_index: 0,
            address: "kaspatest:abc".into(),
            amount_sompi: 10,
            block_daa_score: 100,
            confirmations: 60,
            is_coinbase: false,
        };
        ledger
            .reconcile(160, &[observed], &[confirmed], &[])
            .unwrap();
        let endpoint = Url::parse(&format!("http://{address}/events")).unwrap();
        assert_eq!(
            deliver(
                &mut ledger,
                &endpoint,
                1,
                DeliveryPolicy {
                    max_attempts: 5,
                    retry_base_seconds: 5,
                    retry_max_seconds: 300,
                },
            )
            .await
            .unwrap(),
            DeliveryReport {
                delivered: 1,
                retry_scheduled: 0,
                dead_lettered: 0,
            }
        );
        assert!(ledger.unacknowledged_events().unwrap().is_empty());
        let received = received.lock().unwrap();
        assert_eq!(received.len(), 1);
        assert_eq!(received[0].0, "credit:tx:0");
        assert_eq!(received[0].1["schemaVersion"], 1);
        assert_eq!(received[0].1["event"]["eventKey"], "credit:tx:0");
        server.abort();
    }

    #[tokio::test]
    async fn failed_event_is_scheduled_without_blocking_later_delivery() {
        use axum::http::StatusCode;

        async fn receive(headers: HeaderMap) -> StatusCode {
            let key = headers
                .get("idempotency-key")
                .and_then(|value| value.to_str().ok())
                .unwrap_or_default();
            if key == "credit:first:0" {
                StatusCode::SERVICE_UNAVAILABLE
            } else {
                StatusCode::NO_CONTENT
            }
        }

        let app = Router::new().route("/events", post(receive));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let mut ledger = DepositLedger::open(":memory:").unwrap();
        let observed = ["first", "second"].map(|tx_id| UtxoAppearance {
            tx_id: tx_id.into(),
            output_index: 0,
            address: "kaspatest:abc".into(),
            amount_sompi: 10,
            block_daa_score: 100,
            virtual_daa: 160,
            is_coinbase: false,
        });
        let confirmed = ["first", "second"].map(|tx_id| ConfirmedDeposit {
            tx_id: tx_id.into(),
            output_index: 0,
            address: "kaspatest:abc".into(),
            amount_sompi: 10,
            block_daa_score: 100,
            confirmations: 60,
            is_coinbase: false,
        });
        ledger.reconcile(160, &observed, &confirmed, &[]).unwrap();
        let endpoint = Url::parse(&format!("http://{address}/events")).unwrap();
        let report = deliver(
            &mut ledger,
            &endpoint,
            2,
            DeliveryPolicy {
                max_attempts: 5,
                retry_base_seconds: 60,
                retry_max_seconds: 300,
            },
        )
        .await
        .unwrap();
        assert_eq!(
            report,
            DeliveryReport {
                delivered: 1,
                retry_scheduled: 1,
                dead_lettered: 0,
            }
        );
        let statuses = ledger.outbox_statuses(false).unwrap();
        assert_eq!(statuses.len(), 1);
        assert_eq!(statuses[0].event.tx_id, "first");
        assert_eq!(statuses[0].attempts, 1);
        assert!(statuses[0].next_attempt_at > now_epoch_seconds().unwrap());
        server.abort();
    }
}
