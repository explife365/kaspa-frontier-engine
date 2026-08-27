//! Loopback receiver proving atomic idempotency-key persistence.

use axum::error_handling::HandleErrorLayer;
use axum::extract::{DefaultBodyLimit, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use axum::{BoxError, Json, Router};
use kaspa_frontier_engine::{DeliveryEnvelope, EngineError, InboxOutcome, OutboxReceiverStore};
use serde::Serialize;
use std::env;
use std::fs;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::net::TcpListener;
use tower::limit::ConcurrencyLimitLayer;
use tower::timeout::TimeoutLayer;
use tower::ServiceBuilder;

const DEFAULT_BIND: &str = "127.0.0.1:18320";
const DEFAULT_DATABASE: &str = ".local/tn10-outbox-receiver.sqlite";
const MAX_BODY_BYTES: usize = 16 * 1024;
const MAX_CONCURRENT_REQUESTS: usize = 32;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone)]
struct AppState {
    store: Arc<Mutex<OutboxReceiverStore>>,
}

struct Options {
    bind: SocketAddr,
    database: PathBuf,
    tls_cert: Option<PathBuf>,
    tls_key: Option<PathBuf>,
    tls_client_ca: Option<PathBuf>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReceiverResponse {
    status: InboxOutcome,
    event_key: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HealthResponse {
    healthy: bool,
    inbox_events: u64,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

fn usage() -> &'static str {
    "usage: tn10-outbox-receiver [--bind 127.0.0.1:18320] [--database PATH] [--tls-cert PEM] [--tls-key PEM] [--tls-client-ca PEM]"
}

fn parse_args(arguments: impl IntoIterator<Item = String>) -> Result<Options, String> {
    let mut bind = DEFAULT_BIND
        .parse::<SocketAddr>()
        .expect("valid default bind");
    let mut database = PathBuf::from(DEFAULT_DATABASE);
    let mut tls_cert = None;
    let mut tls_key = None;
    let mut tls_client_ca = None;
    let mut args = arguments.into_iter();
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--bind" => {
                bind = args
                    .next()
                    .ok_or("--bind needs an address")?
                    .parse()
                    .map_err(|_| "--bind must be an IP socket address")?;
            }
            "--database" => database = PathBuf::from(args.next().ok_or("--database needs a path")?),
            "--tls-cert" => {
                tls_cert = Some(PathBuf::from(args.next().ok_or("--tls-cert needs a path")?))
            }
            "--tls-key" => {
                tls_key = Some(PathBuf::from(args.next().ok_or("--tls-key needs a path")?))
            }
            "--tls-client-ca" => {
                tls_client_ca = Some(PathBuf::from(
                    args.next().ok_or("--tls-client-ca needs a path")?,
                ))
            }
            _ => return Err(format!("unknown argument {argument}")),
        }
    }
    if !bind.ip().is_loopback() {
        return Err(
            "receiver must bind to loopback; place authenticated mTLS on this bind, not a public socket"
                .into(),
        );
    }
    match (
        tls_cert.is_some(),
        tls_key.is_some(),
        tls_client_ca.is_some(),
    ) {
        (false, false, false) | (true, true, true) => {}
        _ => {
            return Err("mTLS requires --tls-cert, --tls-key, and --tls-client-ca together".into())
        }
    }
    Ok(Options {
        bind,
        database,
        tls_cert,
        tls_key,
        tls_client_ca,
    })
}

fn app(store: Arc<Mutex<OutboxReceiverStore>>) -> Router {
    Router::new()
        .route("/events", post(receive))
        .route("/kaspa-events", post(receive))
        .route("/health", get(health))
        .with_state(AppState { store })
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
        .layer(
            ServiceBuilder::new()
                .layer(HandleErrorLayer::new(handle_service_error))
                .layer(TimeoutLayer::new(REQUEST_TIMEOUT))
                .layer(ConcurrencyLimitLayer::new(MAX_CONCURRENT_REQUESTS)),
        )
}

async fn receive(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(delivery): Json<DeliveryEnvelope>,
) -> (StatusCode, Json<serde_json::Value>) {
    let Some(idempotency_key) = headers
        .get("idempotency-key")
        .and_then(|value| value.to_str().ok())
    else {
        return json_error(StatusCode::BAD_REQUEST, "missing valid Idempotency-Key");
    };
    let received_at = match epoch_seconds() {
        Ok(received_at) => received_at,
        Err(_) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, "clock unavailable"),
    };
    let outcome = match state.store.lock() {
        Ok(mut store) => store.accept(idempotency_key, &delivery, received_at),
        Err(_) => {
            return json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "receiver state unavailable",
            )
        }
    };
    match outcome {
        Ok(InboxOutcome::Accepted) => (
            StatusCode::CREATED,
            Json(
                serde_json::to_value(ReceiverResponse {
                    status: InboxOutcome::Accepted,
                    event_key: delivery.event.event_key,
                })
                .expect("receiver response serializes"),
            ),
        ),
        Ok(InboxOutcome::Duplicate) => (
            StatusCode::OK,
            Json(
                serde_json::to_value(ReceiverResponse {
                    status: InboxOutcome::Duplicate,
                    event_key: delivery.event.event_key,
                })
                .expect("receiver response serializes"),
            ),
        ),
        Ok(InboxOutcome::Conflict) => json_error(
            StatusCode::CONFLICT,
            "idempotency key already exists with different facts",
        ),
        Err(EngineError::Message(message) | EngineError::NotTestnetAddress(message)) => {
            json_error(StatusCode::BAD_REQUEST, &message)
        }
        Err(_) => json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "receiver persistence failed",
        ),
    }
}

async fn health(State(state): State<AppState>) -> (StatusCode, Json<HealthResponse>) {
    match state.store.lock() {
        Ok(store) => match store.count() {
            Ok(inbox_events) => (
                StatusCode::OK,
                Json(HealthResponse {
                    healthy: true,
                    inbox_events,
                }),
            ),
            Err(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(HealthResponse {
                    healthy: false,
                    inbox_events: 0,
                }),
            ),
        },
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(HealthResponse {
                healthy: false,
                inbox_events: 0,
            }),
        ),
    }
}

fn json_error(status: StatusCode, message: &str) -> (StatusCode, Json<serde_json::Value>) {
    (
        status,
        Json(
            serde_json::to_value(ErrorResponse {
                error: message.into(),
            })
            .expect("error response serializes"),
        ),
    )
}

async fn handle_service_error(error: BoxError) -> (StatusCode, Json<serde_json::Value>) {
    eprintln!("receiver request rejected: {error}");
    json_error(StatusCode::SERVICE_UNAVAILABLE, "request unavailable")
}

fn epoch_seconds() -> Result<u64, std::time::SystemTimeError> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter("tn10_outbox_receiver=info,kaspa_frontier_engine=info")
        .init();
    let options =
        parse_args(env::args().skip(1)).map_err(|error| format!("{error}\n{}", usage()))?;
    if let Some(parent) = options.database.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    let store = Arc::new(Mutex::new(OutboxReceiverStore::open(&options.database)?));
    match (
        options.tls_cert.as_deref(),
        options.tls_key.as_deref(),
        options.tls_client_ca.as_deref(),
    ) {
        (Some(cert), Some(key), Some(client_ca)) => {
            let config = kaspa_frontier_engine::mtls::server_config(cert, key, client_ca)?;
            let listener = TcpListener::bind(options.bind).await?;
            println!("listen      https://{}", options.bind);
            println!("endpoint    /kaspa-events");
            println!("database    {}", options.database.display());
            println!("security    loopback mTLS; client certificate required");
            axum::serve(
                kaspa_frontier_engine::mtls::TlsIncoming::new(listener, config),
                app(store),
            )
            .await?;
        }
        _ => {
            let listener = TcpListener::bind(options.bind).await?;
            println!("listen      http://{}", options.bind);
            println!("endpoint    /kaspa-events");
            println!("database    {}", options.database.display());
            println!(
                "security    loopback HTTP; pass --tls-cert --tls-key --tls-client-ca for mTLS"
            );
            axum::serve(listener, app(store)).await?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{to_bytes, Body};
    use axum::http::Request;
    use tower::ServiceExt;

    #[path = "../../mtls_test_support.rs"]
    mod mtls_test_support;

    const ADDRESS: &str = "kaspatest:qptv6u8kel95drh2p2z492cyksk8lpetep286fngqu5j9nk57g642lzf748kt";

    fn request(amount: u64, include_key: bool) -> Request<Body> {
        let tx_id = "a".repeat(64);
        let key = format!("credit:{tx_id}:0");
        let body = serde_json::json!({
            "schemaVersion": 1,
            "event": {
                "id": 1,
                "eventKey": key,
                "kind": "credit",
                "txId": tx_id,
                "outputIndex": 0,
                "amountSompi": amount,
                "address": ADDRESS
            }
        });
        let mut builder = Request::builder()
            .method("POST")
            .uri("/kaspa-events")
            .header("content-type", "application/json");
        if include_key {
            builder = builder.header("idempotency-key", format!("credit:{}:0", "a".repeat(64)));
        }
        builder.body(Body::from(body.to_string())).unwrap()
    }

    #[test]
    fn parser_refuses_public_bind() {
        assert!(parse_args(["--bind".into(), "127.0.0.1:19000".into()]).is_ok());
        assert!(parse_args(["--bind".into(), "0.0.0.0:19000".into()]).is_err());
        assert!(parse_args(["--tls-cert".into(), "server.pem".into()]).is_err());
        let tls = parse_args([
            "--tls-cert".into(),
            "server.pem".into(),
            "--tls-key".into(),
            "server.key".into(),
            "--tls-client-ca".into(),
            "ca.pem".into(),
        ])
        .unwrap();
        assert!(tls.tls_cert.is_some() && tls.tls_key.is_some() && tls.tls_client_ca.is_some());
    }

    #[tokio::test]
    async fn http_contract_accepts_duplicate_and_rejects_conflict() {
        let store = Arc::new(Mutex::new(OutboxReceiverStore::open(":memory:").unwrap()));
        let router = app(store);
        let accepted = router.clone().oneshot(request(10, true)).await.unwrap();
        assert_eq!(accepted.status(), StatusCode::CREATED);
        let duplicate = router.clone().oneshot(request(10, true)).await.unwrap();
        assert_eq!(duplicate.status(), StatusCode::OK);
        let conflict = router.clone().oneshot(request(11, true)).await.unwrap();
        assert_eq!(conflict.status(), StatusCode::CONFLICT);
        let missing = router.oneshot(request(10, false)).await.unwrap();
        assert_eq!(missing.status(), StatusCode::BAD_REQUEST);
        let body = to_bytes(missing.into_body(), MAX_BODY_BYTES).await.unwrap();
        assert!(String::from_utf8(body.to_vec())
            .unwrap()
            .contains("Idempotency-Key"));
    }

    #[tokio::test]
    async fn mtls_health_requires_client_certificate() {
        let dir = tempfile::tempdir().unwrap();
        let pems = mtls_test_support::write_into(dir.path());
        let config = kaspa_frontier_engine::mtls::server_config(
            &pems.server_cert,
            &pems.server_key,
            &pems.ca,
        )
        .unwrap();
        let store = Arc::new(Mutex::new(OutboxReceiverStore::open(":memory:").unwrap()));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(
                kaspa_frontier_engine::mtls::TlsIncoming::new(listener, config),
                app(store),
            )
            .await
            .unwrap();
        });

        let url = format!("https://127.0.0.1:{}/health", address.port());
        let ca = reqwest::Certificate::from_pem(&std::fs::read(&pems.ca).unwrap()).unwrap();
        let unsigned = reqwest::Client::builder()
            .add_root_certificate(ca.clone())
            .https_only(true)
            .http1_only()
            .build()
            .unwrap();
        assert!(unsigned.get(&url).send().await.is_err());

        let mut identity = std::fs::read(&pems.client_cert).unwrap();
        identity.extend(std::fs::read(&pems.client_key).unwrap());
        let signed = reqwest::Client::builder()
            .add_root_certificate(ca)
            .identity(reqwest::Identity::from_pem(&identity).unwrap())
            .https_only(true)
            .http1_only()
            .build()
            .unwrap();
        let response = signed.get(&url).send().await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert!(response.text().await.unwrap().contains("\"healthy\":true"));
        server.abort();
    }
}
