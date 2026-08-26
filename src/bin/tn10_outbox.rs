//! Durable deposit outbox inspection and at-least-once webhook delivery.

use kaspa_frontier_engine::{DepositLedger, LedgerEvent};
use reqwest::{Client, Url};
use serde::Serialize;
use std::env;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const DEFAULT_DATABASE: &str = ".local/tn10-wrpc-live.sqlite";
const DEFAULT_LIMIT: usize = 100;
const LEASE_SECONDS: u64 = 60;

enum Command {
    List,
    Ack { id: i64 },
    Deliver { endpoint: Url, limit: usize },
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

fn usage() -> &'static str {
    "usage: tn10-outbox <list|ack ID|deliver URL> [--database PATH] [--limit N]"
}

fn parse_args(arguments: impl IntoIterator<Item = String>) -> Result<Options, String> {
    let mut args = arguments.into_iter();
    let action = args.next().ok_or_else(|| usage().to_string())?;
    let mut command = match action.as_str() {
        "list" => Command::List,
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
        "deliver" => {
            let raw = args.next().ok_or("deliver requires a webhook URL")?;
            Command::Deliver {
                endpoint: safe_endpoint(&raw)?,
                limit: DEFAULT_LIMIT,
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
            _ => return Err(format!("unknown argument {argument}")),
        }
    }
    Ok(Options { database, command })
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

fn release_delivery<T>(
    ledger: &mut DepositLedger,
    event: &LedgerEvent,
    owner: &str,
    error: &str,
) -> Result<T, Box<dyn std::error::Error>> {
    let stored_error = if error.len() <= 1_024 {
        error
    } else {
        "webhook delivery failed; details exceeded storage limit"
    };
    ledger.release_claim(event.id, owner, stored_error)?;
    Err(error.to_string().into())
}

async fn deliver(
    ledger: &mut DepositLedger,
    endpoint: &Url,
    limit: usize,
) -> Result<usize, Box<dyn std::error::Error>> {
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
    let mut delivered = 0usize;
    while delivered < limit {
        let now = now_epoch_seconds()?;
        let Some(event) = ledger.claim_next_event(&owner, now, LEASE_SECONDS)? else {
            break;
        };
        let response = client
            .post(endpoint.clone())
            .header("Idempotency-Key", &event.event_key)
            .json(&DeliveryBody {
                schema_version: 1,
                event: &event,
            })
            .send()
            .await;
        match response {
            Ok(response) if response.status().is_success() => {
                ledger.acknowledge_claim(event.id, &owner)?;
                println!("delivered  id={} key={}", event.id, event.event_key);
                delivered += 1;
            }
            Ok(response) => {
                return release_delivery(
                    ledger,
                    &event,
                    &owner,
                    &format!("webhook returned HTTP {}", response.status()),
                );
            }
            Err(error) => {
                return release_delivery(
                    ledger,
                    &event,
                    &owner,
                    &format!("webhook request failed: {error}"),
                );
            }
        }
    }
    Ok(delivered)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options =
        parse_args(env::args().skip(1)).map_err(|error| format!("{error}\n{}", usage()))?;
    let mut ledger = DepositLedger::open(&options.database)?;
    match options.command {
        Command::List => {
            for event in ledger.unacknowledged_events()? {
                println!("{}", serde_json::to_string(&event)?);
            }
        }
        Command::Ack { id } => {
            ledger.acknowledge_event(id)?;
            println!("acknowledged {id}");
        }
        Command::Deliver { endpoint, limit } => {
            let delivered = deliver(&mut ledger, &endpoint, limit).await?;
            println!("complete   delivered={delivered}");
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
        assert_eq!(deliver(&mut ledger, &endpoint, 1).await.unwrap(), 1);
        assert!(ledger.unacknowledged_events().unwrap().is_empty());
        let received = received.lock().unwrap();
        assert_eq!(received.len(), 1);
        assert_eq!(received[0].0, "credit:tx:0");
        assert_eq!(received[0].1["schemaVersion"], 1);
        assert_eq!(received[0].1["event"]["eventKey"], "credit:tx:0");
        server.abort();
    }
}
