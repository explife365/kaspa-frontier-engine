//! Local JSON-RPC for integrators (TN10 REST + kascov). Not kaspad.

use axum::body::to_bytes;
use axum::error_handling::HandleErrorLayer;
use axum::extract::{Request, State};
use axum::http::header::CONTENT_TYPE;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::post;
use axum::{BoxError, Json, Router};
use kaspa_frontier_engine::covenant_rpc::{dispatch, jsonrpc_error, IMPLEMENTATION};
use kaspa_frontier_engine::kascov::KascovClient;
use kaspa_frontier_engine::network::{self, KASCOV_TN10, TESTNET_10_REST, TN10_INTEGRATOR_RPC};
use kaspa_frontier_engine::rest::Tn10RestClient;
use kaspa_frontier_engine::rpc::{JsonRpcRequest, JsonRpcResponse};
use std::net::SocketAddr;
use std::time::Duration;
use tokio::net::TcpListener;
use tower::limit::ConcurrencyLimitLayer;
use tower::timeout::TimeoutLayer;
use tower::ServiceBuilder;

const MAX_BODY_BYTES: usize = 64 * 1024;
const MAX_CONCURRENT_REQUESTS: usize = 32;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone)]
struct AppState {
    rest: Tn10RestClient,
    kascov: KascovClient,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter("tn10_covenant_rpc=info,kaspa_frontier_engine=info")
        .init();

    let (bind, allow_public) = parse_args()?;
    let address: SocketAddr = bind.parse()?;
    validate_bind(address, allow_public)?;
    if !address.ip().is_loopback() {
        eprintln!(
            "WARNING: public covenant RPC bind has no TLS or authentication; isolate it behind trusted access controls"
        );
    }
    let rest = Tn10RestClient::new(TESTNET_10_REST)?;
    let kascov = KascovClient::new(KASCOV_TN10)?;
    let listener = TcpListener::bind(address).await?;
    println!("tn10-covenant-rpc  {IMPLEMENTATION}");
    println!("listen             http://{bind}");
    println!("kascov             {KASCOV_TN10}");
    println!("REST               {TESTNET_10_REST}");
    println!(
        "Circle USDC        not listed on Galleon {} or Igra mainnet {}",
        network::GALLEON_TEST_USDC,
        network::IGRA_MAINNET_HYPERLANE_USDC
    );
    println!("gTEST              {}", network::GALLEON_GTEST);
    println!("methods            getInfo getBlockDagInfo getUtxosByAddresses getUtxosByCovenantId getCovenant");
    println!("this is not kaspad — do not use for consensus");

    axum::serve(listener, app(rest, kascov)).await?;
    Ok(())
}

fn validate_bind(
    address: SocketAddr,
    allow_public: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if !address.ip().is_loopback() && !allow_public {
        return Err(
            "refusing non-loopback bind; pass --allow-public only behind trusted access controls"
                .into(),
        );
    }
    Ok(())
}

fn parse_args() -> Result<(String, bool), Box<dyn std::error::Error>> {
    let mut bind = None;
    let mut allow_public = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--bind" => bind = Some(args.next().ok_or("--bind needs an address")?),
            "--allow-public" => allow_public = true,
            value if !value.starts_with('-') && bind.is_none() => bind = Some(value.to_string()),
            other => return Err(format!("unknown argument {other}").into()),
        }
    }
    Ok((
        bind.unwrap_or_else(|| TN10_INTEGRATOR_RPC.to_string()),
        allow_public,
    ))
}

fn app(rest: Tn10RestClient, kascov: KascovClient) -> Router {
    Router::new()
        .route("/", post(handle_rpc))
        .with_state(AppState { rest, kascov })
        .layer(
            ServiceBuilder::new()
                .layer(HandleErrorLayer::new(handle_service_error))
                .layer(TimeoutLayer::new(REQUEST_TIMEOUT))
                .layer(ConcurrencyLimitLayer::new(MAX_CONCURRENT_REQUESTS)),
        )
}

async fn handle_rpc(
    State(state): State<AppState>,
    request: Request,
) -> (StatusCode, Json<JsonRpcResponse>) {
    let headers = request.headers().clone();
    if !is_json_content_type(&headers) {
        return (
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            Json(jsonrpc_error(
                None,
                -32600,
                "Content-Type must be application/json".into(),
            )),
        );
    }
    let body = match to_bytes(request.into_body(), MAX_BODY_BYTES).await {
        Ok(body) => body,
        Err(error) => {
            return (
                StatusCode::OK,
                Json(jsonrpc_error(
                    None,
                    -32600,
                    format!("request body exceeds {MAX_BODY_BYTES} bytes: {error}"),
                )),
            )
        }
    };
    let req = match serde_json::from_slice::<JsonRpcRequest>(&body) {
        Ok(req) => req,
        Err(err) => {
            return (
                StatusCode::OK,
                Json(jsonrpc_error(None, -32700, format!("parse error: {err}"))),
            )
        }
    };
    (
        StatusCode::OK,
        Json(dispatch(&state.rest, &state.kascov, req).await),
    )
}

async fn handle_service_error(error: BoxError) -> (StatusCode, Json<JsonRpcResponse>) {
    (
        StatusCode::OK,
        Json(jsonrpc_error(
            None,
            -32001,
            format!("request timed out or exceeded service capacity: {error}"),
        )),
    )
}

fn is_json_content_type(headers: &HeaderMap) -> bool {
    headers
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .is_some_and(|media_type| media_type.trim().eq_ignore_ascii_case("application/json"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{to_bytes, Body};
    use axum::http::Request;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tower::ServiceExt;

    fn test_app() -> Router {
        app(
            Tn10RestClient::new("https://example.invalid").unwrap(),
            KascovClient::new("https://example.invalid").unwrap(),
        )
    }

    #[tokio::test]
    async fn parses_jsonrpc_and_rejects_oversized_body() {
        let request = Request::post("/")
            .header(CONTENT_TYPE, "application/json; charset=utf-8")
            .body(Body::from(
                r#"{"jsonrpc":"2.0","id":1,"method":"getInfo","params":[]}"#,
            ))
            .unwrap();
        let response = test_app().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), MAX_BODY_BYTES)
            .await
            .unwrap();
        assert!(String::from_utf8_lossy(&bytes).contains("\"notKaspad\":true"));

        let oversized = Request::post("/")
            .header(CONTENT_TYPE, "application/json")
            .body(Body::from(vec![b'x'; MAX_BODY_BYTES + 1]))
            .unwrap();
        let response = test_app().oneshot(oversized).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), MAX_BODY_BYTES)
            .await
            .unwrap();
        assert!(String::from_utf8_lossy(&bytes).contains("\"code\":-32600"));

        let wrong_type = Request::post("/")
            .header(CONTENT_TYPE, "text/plain")
            .body(Body::from(
                r#"{"jsonrpc":"2.0","id":1,"method":"getInfo","params":[]}"#,
            ))
            .unwrap();
        let response = test_app().oneshot(wrong_type).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }

    #[tokio::test]
    async fn hyper_handles_fragmented_and_pipelined_keep_alive_requests() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, test_app()).await.unwrap();
        });
        let mut stream = tokio::net::TcpStream::connect(address).await.unwrap();
        let body = r#"{"jsonrpc":"2.0","id":1,"method":"getInfo","params":[]}"#;
        let request = format!(
            "POST / HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: keep-alive\r\n\r\n{body}",
            body.len()
        );
        for chunk in request.as_bytes().chunks(3) {
            stream.write_all(chunk).await.unwrap();
        }
        stream.write_all(request.as_bytes()).await.unwrap();

        let mut received = Vec::new();
        tokio::time::timeout(Duration::from_secs(2), async {
            let mut buffer = [0_u8; 2048];
            loop {
                let read = stream.read(&mut buffer).await.unwrap();
                assert!(read > 0);
                received.extend_from_slice(&buffer[..read]);
                if String::from_utf8_lossy(&received)
                    .matches("\"notKaspad\":true")
                    .count()
                    == 2
                {
                    break;
                }
            }
        })
        .await
        .unwrap();
        server.abort();
    }

    #[test]
    fn public_bind_requires_explicit_opt_in() {
        assert!(validate_bind("127.0.0.1:16110".parse().unwrap(), false).is_ok());
        assert!(validate_bind("0.0.0.0:16110".parse().unwrap(), false).is_err());
        assert!(validate_bind("0.0.0.0:16110".parse().unwrap(), true).is_ok());
    }
}
