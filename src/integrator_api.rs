//! Production integrator HTTP API for CEX custody rehearsal.
//!
//! Auth, gate fail-closed, deposit/outbox/withdraw reads, return-address, evidence.

use crate::deposit_ledger::{DepositLedger, DepositRecord, OutboxEventStatus};
use crate::outbox_receiver::OutboxReceiverStore;
use crate::error::EngineError;
use crate::network::is_valid_testnet_address;
use crate::owned_node_gate::{
    apply_gate_env_defaults, evaluate_owned_node_gate, gate_summary_to_json, summarize_gate,
    OwnedNodeGateOptions, OwnedNodeGateSummary,
};
use crate::return_address::{
    estimate_tx_fee_from_toccata, resolve_return_address, ReturnAddressReport, TxFeeReport,
};
use crate::rest::Tn10RestClient;
use crate::withdrawal_ledger::{WithdrawalLedger, WithdrawalRecordView};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::net::SocketAddr;
use std::path::{Path as StdPath, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;
use tower::limit::ConcurrencyLimitLayer;
use tower::ServiceBuilder;

const DEFAULT_BIND: &str = "127.0.0.1:8787";
const DEFAULT_DEPOSIT_DB: &str = ".local/tn10-wrpc-live.sqlite";
const DEFAULT_WITHDRAW_DB: &str = ".local/tn10-withdrawals.sqlite";
const DEFAULT_WATCHLIST: &str = ".local/integrator_watchlist.json";
const DEFAULT_EVIDENCE_DIR: &str = ".local/evidence";
const DEFAULT_RECEIVER_DB: &str = ".local/tn10-outbox-receiver.sqlite";
const MAX_CONCURRENT: usize = 64;
const EXPORT_MAX_ROWS: u64 = 10_000;

#[derive(Clone)]
pub struct IntegratorConfig {
    pub bind: SocketAddr,
    pub deposit_database: PathBuf,
    pub withdrawal_database: PathBuf,
    pub watchlist_path: PathBuf,
    pub evidence_dir: PathBuf,
    pub receiver_database: PathBuf,
    pub api_keys: HashMap<String, String>,
    pub webhook_secret: Option<String>,
    pub confirmation_daa: u64,
    pub network: String,
    pub require_gate: bool,
    pub gate: OwnedNodeGateOptions,
    pub not_consensus: bool,
}

#[derive(Clone)]
pub struct AppState {
    pub config: IntegratorConfig,
    pub deposit_ledger: Arc<Mutex<DepositLedger>>,
    pub rest: Arc<Tn10RestClient>,
}

#[derive(Debug, Deserialize)]
pub struct PaginationQuery {
    pub limit: Option<u64>,
    pub offset: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct DepositListQuery {
    pub address: Option<String>,
    pub state: Option<String>,
    pub limit: Option<u64>,
    pub offset: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct DepositExportQuery {
    pub address: Option<String>,
    pub state: Option<String>,
    pub format: Option<String>,
    pub limit: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct WebhookVerifyBody {
    pub body: String,
    pub signature: String,
}

#[derive(Debug, Deserialize)]
pub struct WithdrawalListQuery {
    pub state: Option<String>,
    pub limit: Option<u64>,
    pub offset: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct OutboxQuery {
    pub dead_only: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct ReturnAddressQuery {
    pub vout: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct WatchlistBody {
    pub addresses: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct DeliverBody {
    pub url: String,
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct WebhookTestBody {
    pub url: String,
}

#[derive(Serialize)]
struct ErrorBody {
    ok: bool,
    error: String,
}

#[derive(Serialize)]
struct HealthBody {
    ok: bool,
    service: &'static str,
    version: &'static str,
    network: String,
    not_consensus: bool,
}

#[derive(Serialize, Deserialize)]
struct WatchlistFile {
    addresses: Vec<String>,
    updated_at: u64,
}

type EngineResult<T> = std::result::Result<T, EngineError>;

pub fn load_config() -> EngineResult<IntegratorConfig> {
    let bind = std::env::var("INTEGRATOR_API_BIND")
        .unwrap_or_else(|_| DEFAULT_BIND.to_string())
        .parse()
        .map_err(|_| EngineError::Message("INTEGRATOR_API_BIND must be a socket address".into()))?;
    let deposit_database = PathBuf::from(
        std::env::var("TN10_DEPOSIT_DATABASE")
            .unwrap_or_else(|_| DEFAULT_DEPOSIT_DB.to_string()),
    );
    let withdrawal_database = PathBuf::from(
        std::env::var("TN10_WITHDRAWAL_DATABASE")
            .unwrap_or_else(|_| DEFAULT_WITHDRAW_DB.to_string()),
    );
    let watchlist_path = PathBuf::from(
        std::env::var("INTEGRATOR_WATCHLIST_PATH")
            .unwrap_or_else(|_| DEFAULT_WATCHLIST.to_string()),
    );
    let evidence_dir = PathBuf::from(
        std::env::var("INTEGRATOR_EVIDENCE_DIR")
            .unwrap_or_else(|_| DEFAULT_EVIDENCE_DIR.to_string()),
    );
    let receiver_database = PathBuf::from(
        std::env::var("INTEGRATOR_RECEIVER_DATABASE")
            .unwrap_or_else(|_| DEFAULT_RECEIVER_DB.to_string()),
    );
    let confirmation_daa = std::env::var("INTEGRATOR_CONFIRMATION_DAA")
        .ok()
        .and_then(|raw| raw.parse().ok())
        .unwrap_or(10);
    let network = std::env::var("INTEGRATOR_NETWORK")
        .unwrap_or_else(|_| "testnet-10".to_string());
    let require_gate = std::env::var("INTEGRATOR_REQUIRE_GATE")
        .map(|raw| !matches!(raw.as_str(), "0" | "false" | "no"))
        .unwrap_or(true);
    let webhook_secret = std::env::var("INTEGRATOR_WEBHOOK_SECRET")
        .ok()
        .filter(|value| !value.trim().is_empty());
    let api_keys = parse_api_keys(
        std::env::var("INTEGRATOR_API_KEYS")
            .unwrap_or_else(|_| "pilot:change-me-before-go-live".to_string()),
    )?;
    let mut gate = OwnedNodeGateOptions::default();
    apply_gate_env_defaults(&mut gate);
    if gate.min_healthy == 0 {
        gate.min_healthy = 2;
    }
    Ok(IntegratorConfig {
        bind,
        deposit_database,
        withdrawal_database,
        watchlist_path,
        evidence_dir,
        receiver_database,
        api_keys,
        webhook_secret,
        confirmation_daa,
        network,
        require_gate,
        gate,
        not_consensus: true,
    })
}

fn parse_api_keys(raw: String) -> EngineResult<HashMap<String, String>> {
    let mut keys = HashMap::new();
    for part in raw.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let (tenant, key) = part
            .split_once(':')
            .ok_or_else(|| EngineError::Message("INTEGRATOR_API_KEYS entries must be tenant:key".into()))?;
        if tenant.trim().is_empty() || key.trim().is_empty() {
            return Err(EngineError::Message("INTEGRATOR_API_KEYS tenant and key must be non-empty".into()));
        }
        keys.insert(key.trim().to_string(), tenant.trim().to_string());
    }
    if keys.is_empty() {
        return Err(EngineError::Message("INTEGRATOR_API_KEYS must contain at least one tenant:key".into()));
    }
    Ok(keys)
}

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/openapi.json", get(openapi))
        .route("/v1/gate", get(gate_status))
        .route("/v1/deposits", get(list_deposits))
        .route("/v1/deposits/{txid}/{vout}", get(get_deposit))
        .route("/v1/outbox", get(list_outbox))
        .route("/v1/outbox/{id}/ack", post(ack_outbox))
        .route("/v1/outbox/deliver", post(deliver_outbox))
        .route("/v1/withdrawals", get(list_withdrawals))
        .route("/v1/withdrawals/{txid}/{vout}", get(get_withdrawal))
        .route("/v1/return-address/{txid}", get(return_address))
        .route("/v1/fee-estimate/{txid}", get(fee_estimate))
        .route("/v1/evidence/latest", get(evidence_latest))
        .route("/v1/watchlist", get(get_watchlist).post(set_watchlist))
        .route("/v1/webhooks/test", post(webhook_test))
        .route("/v1/pilot/summary", get(pilot_summary))
        .route("/v1/pilot/selftest", get(pilot_selftest))
        .route("/v1/deposits/export", get(export_deposits))
        .route("/v1/receiver/stats", get(receiver_stats))
        .route("/v1/webhooks/verify", post(webhook_verify))
        .with_state(state)
        .layer(ServiceBuilder::new().layer(ConcurrencyLimitLayer::new(MAX_CONCURRENT)))
}

pub async fn serve(config: IntegratorConfig) -> EngineResult<()> {
    if !config.bind.ip().is_loopback() {
        return Err(EngineError::Message(
            "integrator API must bind loopback unless fronted by authenticated reverse proxy".into(),
        ));
    }
    let deposit_ledger = DepositLedger::open(&config.deposit_database)?;
    let rest = Tn10RestClient::new(crate::network::TESTNET_10_REST)?;
    let bind = config.bind;
    let state = AppState {
        config,
        deposit_ledger: Arc::new(Mutex::new(deposit_ledger)),
        rest: Arc::new(rest),
    };
    let app = build_router(state);
    let listener = tokio::net::TcpListener::bind(bind)
        .await
        .map_err(|err| EngineError::Message(err.to_string()))?;
    eprintln!(
        "tn10-integrator-api listening http://{} (auth required on /v1/*)",
        bind
    );
    axum::serve(listener, app)
        .await
        .map_err(|err| EngineError::Message(err.to_string()))?;
    Ok(())
}

fn auth_tenant(state: &AppState, headers: &HeaderMap) -> EngineResult<String> {
    let key = headers
        .get("x-integrator-key")
        .or_else(|| headers.get("authorization"))
        .and_then(|value| value.to_str().ok())
        .map(|value| value.trim().strip_prefix("Bearer ").unwrap_or(value).trim())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| EngineError::Message("missing X-Integrator-Key or Authorization Bearer".into()))?;
    state
        .config
        .api_keys
        .get(key)
        .cloned()
        .ok_or_else(|| EngineError::Message("invalid integrator API key".into()))
}

async fn fetch_gate_summary(state: &AppState) -> EngineResult<OwnedNodeGateSummary> {
    let public = state.rest.block_dag_info().await?;
    let reports = evaluate_owned_node_gate(
        &state.config.gate.urls,
        public.virtual_daa_score,
        state.config.gate.max_daa_lag,
    )
    .await;
    Ok(summarize_gate(
        reports,
        &state.config.gate.urls,
        public.virtual_daa_score,
        state.config.gate.max_daa_lag,
        state.config.gate.min_healthy,
    ))
}

async fn require_gate(state: &AppState) -> EngineResult<()> {
    if !state.config.require_gate {
        return Ok(());
    }
    let summary = fetch_gate_summary(state).await?;
    if !summary.gate_healthy {
        return Err(EngineError::Message("owned-node gate is red".into()));
    }
    Ok(())
}

fn api_error(status: StatusCode, error: impl Into<String>) -> Response {
    (
        status,
        Json(ErrorBody {
            ok: false,
            error: error.into(),
        }),
    )
        .into_response()
}

fn map_err(err: EngineError) -> Response {
    let status = match &err {
        EngineError::Message(message) if message.contains("gate is red") => StatusCode::SERVICE_UNAVAILABLE,
        EngineError::Message(message) if message.contains("invalid integrator") || message.contains("missing X-Integrator") => {
            StatusCode::UNAUTHORIZED
        }
        EngineError::Message(message) if message.contains("not found") => StatusCode::NOT_FOUND,
        _ => StatusCode::BAD_REQUEST,
    };
    api_error(status, err.to_string())
}

async fn health(State(state): State<AppState>) -> Json<HealthBody> {
    Json(HealthBody {
        ok: true,
        service: "tn10-integrator-api",
        version: "1.0.0",
        network: state.config.network.clone(),
        not_consensus: state.config.not_consensus,
    })
}

async fn openapi() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "openapi": "3.0.3",
        "info": {
            "title": "Kaspa TN10 Integrator API",
            "version": "1.0.0",
            "description": "CEX custody rehearsal. Not consensus. Fail-closed when gate is red."
        },
        "paths": {
            "/health": { "get": { "summary": "Liveness" } },
            "/v1/gate": { "get": { "summary": "Owned-node N-of-M gate", "security": [{"integratorKey": []}] } },
            "/v1/deposits": { "get": { "summary": "List deposits", "security": [{"integratorKey": []}] } },
            "/v1/deposits/{txid}/{vout}": { "get": { "summary": "Get deposit", "security": [{"integratorKey": []}] } },
            "/v1/outbox": { "get": { "summary": "List outbox events", "security": [{"integratorKey": []}] } },
            "/v1/outbox/{id}/ack": { "post": { "summary": "Acknowledge outbox event", "security": [{"integratorKey": []}] } },
            "/v1/outbox/deliver": { "post": { "summary": "Deliver outbox to webhook URL", "security": [{"integratorKey": []}] } },
            "/v1/withdrawals": { "get": { "summary": "List withdrawals", "security": [{"integratorKey": []}] } },
            "/v1/withdrawals/{txid}/{vout}": { "get": { "summary": "Get withdrawal", "security": [{"integratorKey": []}] } },
            "/v1/return-address/{txid}": { "get": { "summary": "Deposit return address", "security": [{"integratorKey": []}] } },
            "/v1/fee-estimate/{txid}": { "get": { "summary": "TX fee estimate", "security": [{"integratorKey": []}] } },
            "/v1/evidence/latest": { "get": { "summary": "Latest evidence JSON", "security": [{"integratorKey": []}] } },
            "/v1/watchlist": { "get": { "summary": "Watched deposit addresses" }, "post": { "summary": "Set watchlist" } },
            "/v1/webhooks/test": { "post": { "summary": "Send test webhook payload" } },
            "/v1/pilot/summary": { "get": { "summary": "CEX pilot week-1 handoff snapshot", "security": [{"integratorKey": []}] } },
            "/v1/pilot/selftest": { "get": { "summary": "Pre-flight readiness checks", "security": [{"integratorKey": []}] } },
            "/v1/deposits/export": { "get": { "summary": "Export deposit journal (ndjson or csv)", "security": [{"integratorKey": []}] } },
            "/v1/receiver/stats": { "get": { "summary": "Webhook receiver inbox stats", "security": [{"integratorKey": []}] } },
            "/v1/webhooks/verify": { "post": { "summary": "Verify HMAC webhook signature", "security": [{"integratorKey": []}] } }
        },
        "components": {
            "securitySchemes": {
                "integratorKey": {
                    "type": "apiKey",
                    "in": "header",
                    "name": "X-Integrator-Key"
                }
            }
        }
    }))
}

async fn gate_status(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let tenant = match auth_tenant(&state, &headers) {
        Ok(value) => value,
        Err(err) => return map_err(err),
    };
    let summary = match fetch_gate_summary(&state).await {
        Ok(value) => value,
        Err(err) => return map_err(err),
    };
    let body = gate_summary_to_json(&summary);
    if state.config.require_gate && !summary.gate_healthy {
        return api_error(StatusCode::SERVICE_UNAVAILABLE, "owned-node gate is red");
    }
    Json(serde_json::json!({
        "ok": summary.gate_healthy,
        "tenant": tenant,
        "confirmationDaa": state.config.confirmation_daa,
        "gate": body,
        "notConsensus": true
    }))
    .into_response()
}

async fn list_deposits(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<DepositListQuery>,
) -> Result<Json<serde_json::Value>, Response> {
    auth_tenant(&state, &headers).map_err(map_err)?;
    require_gate(&state).await.map_err(map_err)?;
    let ledger = state.deposit_ledger.lock().await;
    let items = ledger
        .list_deposits(
            query.address.as_deref(),
            query.state.as_deref(),
            query.limit.unwrap_or(100),
            query.offset.unwrap_or(0),
        )
        .map_err(map_err)?;
    Ok(Json(serde_json::json!({
        "ok": true,
        "count": items.len(),
        "items": items,
        "confirmationDaa": state.config.confirmation_daa
    })))
}

async fn get_deposit(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((txid, vout)): Path<(String, u32)>,
) -> Result<Json<serde_json::Value>, Response> {
    auth_tenant(&state, &headers).map_err(map_err)?;
    require_gate(&state).await.map_err(map_err)?;
    let ledger = state.deposit_ledger.lock().await;
    let item = ledger
        .get_deposit(&txid.to_lowercase(), vout)
        .map_err(map_err)?
        .ok_or_else(|| EngineError::Message("deposit not found".into()))
        .map_err(map_err)?;
    Ok(Json(serde_json::json!({ "ok": true, "deposit": item })))
}

async fn list_outbox(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<OutboxQuery>,
) -> Result<Json<serde_json::Value>, Response> {
    auth_tenant(&state, &headers).map_err(map_err)?;
    require_gate(&state).await.map_err(map_err)?;
    let ledger = state.deposit_ledger.lock().await;
    let items: Vec<OutboxEventStatus> = ledger
        .outbox_statuses(query.dead_only.unwrap_or(false))
        .map_err(map_err)?;
    Ok(Json(serde_json::json!({ "ok": true, "count": items.len(), "items": items })))
}

async fn ack_outbox(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, Response> {
    auth_tenant(&state, &headers).map_err(map_err)?;
    require_gate(&state).await.map_err(map_err)?;
    let mut ledger = state.deposit_ledger.lock().await;
    ledger.acknowledge_event(id).map_err(map_err)?;
    Ok(Json(serde_json::json!({ "ok": true, "acknowledged": id })))
}

async fn deliver_outbox(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<DeliverBody>,
) -> Result<Json<serde_json::Value>, Response> {
    auth_tenant(&state, &headers).map_err(map_err)?;
    require_gate(&state).await.map_err(map_err)?;
    let limit = body.limit.unwrap_or(10).clamp(1, 100);
    let output = tokio::process::Command::new(resolve_outbox_bin())
        .arg("deliver")
        .arg(&body.url)
        .arg("--database")
        .arg(&state.config.deposit_database)
        .arg("--limit")
        .arg(limit.to_string())
        .arg("--dual")
        .arg("--min-healthy")
        .arg(state.config.gate.min_healthy.to_string())
        .output()
        .await
        .map_err(|err| EngineError::Message(err.to_string()))
        .map_err(map_err)?;
    Ok(Json(serde_json::json!({
        "ok": output.status.success(),
        "exitCode": output.status.code(),
        "stdout": String::from_utf8_lossy(&output.stdout),
        "stderr": String::from_utf8_lossy(&output.stderr)
    })))
}

fn resolve_outbox_bin() -> String {
    std::env::var("TN10_OUTBOX_BIN").unwrap_or_else(|_| "tn10-outbox".to_string())
}

async fn list_withdrawals(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<WithdrawalListQuery>,
) -> Result<Json<serde_json::Value>, Response> {
    auth_tenant(&state, &headers).map_err(map_err)?;
    require_gate(&state).await.map_err(map_err)?;
    let ledger = WithdrawalLedger::open(&state.config.withdrawal_database).map_err(map_err)?;
    let items = ledger
        .list_withdrawals(
            query.state.as_deref(),
            query.limit.unwrap_or(100),
            query.offset.unwrap_or(0),
        )
        .map_err(map_err)?;
    Ok(Json(serde_json::json!({ "ok": true, "count": items.len(), "items": items })))
}

async fn get_withdrawal(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((txid, vout)): Path<(String, u32)>,
) -> Result<Json<serde_json::Value>, Response> {
    auth_tenant(&state, &headers).map_err(map_err)?;
    require_gate(&state).await.map_err(map_err)?;
    let ledger = WithdrawalLedger::open(&state.config.withdrawal_database).map_err(map_err)?;
    let item: WithdrawalRecordView = ledger
        .get_view(&txid.to_lowercase(), vout)
        .map_err(map_err)?
        .ok_or_else(|| EngineError::Message("withdrawal not found".into()))
        .map_err(map_err)?;
    Ok(Json(serde_json::json!({ "ok": true, "withdrawal": item })))
}

async fn return_address(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(txid): Path<String>,
    Query(query): Query<ReturnAddressQuery>,
) -> Result<Json<ReturnAddressReport>, Response> {
    auth_tenant(&state, &headers).map_err(map_err)?;
    let report = resolve_return_address(&state.rest, &txid.to_lowercase(), query.vout.unwrap_or(0))
        .await
        .map_err(map_err)?;
    Ok(Json(report))
}

async fn fee_estimate(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(txid): Path<String>,
) -> Result<Json<TxFeeReport>, Response> {
    auth_tenant(&state, &headers).map_err(map_err)?;
    let tx = state
        .rest
        .toccata_tx(&txid.to_lowercase())
        .await
        .map_err(map_err)?
        .ok_or_else(|| EngineError::Message("transaction not found".into()))
        .map_err(map_err)?;
    Ok(Json(estimate_tx_fee_from_toccata(&tx)))
}

async fn evidence_latest(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, Response> {
    auth_tenant(&state, &headers).map_err(map_err)?;
    let path = latest_evidence_path(&state.config.evidence_dir).map_err(map_err)?;
    let body = fs::read_to_string(&path).map_err(|err| map_err(EngineError::Message(err.to_string())))?;
    Ok((
        StatusCode::OK,
        [(axum::http::header::CONTENT_TYPE, "application/json")],
        body,
    )
        .into_response())
}

fn latest_evidence_path(dir: &StdPath) -> EngineResult<PathBuf> {
    let mut newest: Option<(u64, PathBuf)> = None;
    for entry in fs::read_dir(dir).map_err(|err| EngineError::Message(err.to_string()))? {
        let entry = entry.map_err(|err| EngineError::Message(err.to_string()))?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let modified = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs())
            .unwrap_or(0);
        if newest.as_ref().map(|(ts, _)| modified > *ts).unwrap_or(true) {
            newest = Some((modified, path));
        }
    }
    newest
        .map(|(_, path)| path)
        .ok_or_else(|| EngineError::Message("no evidence JSON found".into()))
}

async fn get_watchlist(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<WatchlistFile>, Response> {
    auth_tenant(&state, &headers).map_err(map_err)?;
    Ok(Json(read_watchlist(&state.config.watchlist_path).map_err(map_err)?))
}

async fn set_watchlist(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<WatchlistBody>,
) -> Result<Json<WatchlistFile>, Response> {
    auth_tenant(&state, &headers).map_err(map_err)?;
    if body.addresses.is_empty() || body.addresses.len() > 100 {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "watchlist requires 1-100 addresses",
        ));
    }
    let mut unique = std::collections::HashSet::new();
    for address in &body.addresses {
        if !is_valid_testnet_address(address) {
            return Err(api_error(StatusCode::BAD_REQUEST, format!("invalid TN10 address {address}")));
        }
        if !unique.insert(address.clone()) {
            return Err(api_error(StatusCode::BAD_REQUEST, format!("duplicate address {address}")));
        }
    }
    let file = WatchlistFile {
        addresses: body.addresses,
        updated_at: now_secs(),
    };
    write_watchlist(&state.config.watchlist_path, &file).map_err(map_err)?;
    Ok(Json(file))
}

async fn pilot_summary(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, Response> {
    let tenant = auth_tenant(&state, &headers).map_err(map_err)?;
    let gate = fetch_gate_summary(&state).await.map_err(map_err)?;
    let ledger = state.deposit_ledger.lock().await;
    let state_counts = ledger.deposit_state_counts().map_err(map_err)?;
    let outbox_stats = ledger.outbox_stats().map_err(map_err)?;
    let watchlist = read_watchlist(&state.config.watchlist_path).map_err(map_err)?;
    let receiver_inbox = receiver_inbox_count(&state.config.receiver_database);
    let evidence = latest_evidence_path(&state.config.evidence_dir)
        .ok()
        .and_then(|path| path.file_name().map(|name| name.to_string_lossy().to_string()));
    Ok(Json(serde_json::json!({
        "ok": true,
        "tenant": tenant,
        "pilotWeek": 1,
        "notConsensus": true,
        "network": state.config.network,
        "confirmationDaa": state.config.confirmation_daa,
        "gate": gate_summary_to_json(&gate),
        "deposits": state_counts,
        "outbox": outbox_stats,
        "receiverInbox": receiver_inbox,
        "watchlist": watchlist.addresses,
        "evidenceFile": evidence,
        "repo": "https://github.com/explife365/kaspa-frontier-engine",
        "openapi": "/openapi.json",
        "gist": "https://gist.github.com/explife365/477afea386ddba43574c7cb841ad4c73"
    })))
}

async fn pilot_selftest(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, Response> {
    let tenant = auth_tenant(&state, &headers).map_err(map_err)?;
    let mut checks: Vec<serde_json::Value> = Vec::new();
    checks.push(json_check(
        "deposit_database",
        state.config.deposit_database.is_file(),
        state.config.deposit_database.display().to_string(),
    ));
    checks.push(json_check(
        "withdrawal_database",
        state.config.withdrawal_database.is_file(),
        state.config.withdrawal_database.display().to_string(),
    ));
    checks.push(json_check(
        "webhook_secret",
        state.config.webhook_secret.is_some(),
        if state.config.webhook_secret.is_some() {
            "configured"
        } else {
            "missing INTEGRATOR_WEBHOOK_SECRET"
        },
    ));
    let ledger_ok = state.deposit_ledger.lock().await.deposit_state_counts().is_ok();
    checks.push(json_check("deposit_ledger", ledger_ok, "sqlite readable"));
    let gate = fetch_gate_summary(&state).await;
    match gate {
        Ok(summary) => {
            checks.push(json_check(
                "owned_node_gate",
                summary.gate_healthy,
                format!(
                    "{}/{} healthy",
                    summary.healthy_nodes, summary.required_healthy
                ),
            ));
        }
        Err(err) => {
            checks.push(json_check("owned_node_gate", false, err.to_string()));
        }
    }
    let receiver = receiver_inbox_count(&state.config.receiver_database);
    checks.push(json_check(
        "receiver_inbox",
        receiver.get("ok").and_then(|v| v.as_bool()).unwrap_or(false),
        receiver
            .get("count")
            .map(|v| v.to_string())
            .unwrap_or_else(|| "n/a".into()),
    ));
    let watchlist = read_watchlist(&state.config.watchlist_path).map_err(map_err)?;
    checks.push(json_check(
        "watchlist",
        !watchlist.addresses.is_empty(),
        format!("{} addresses", watchlist.addresses.len()),
    ));
    let evidence_ok = latest_evidence_path(&state.config.evidence_dir).is_ok();
    checks.push(json_check("evidence_pack", evidence_ok, "latest JSON present"));
    let ok = checks.iter().all(|check| check["ok"].as_bool().unwrap_or(false));
    Ok(Json(serde_json::json!({
        "ok": ok,
        "tenant": tenant,
        "notConsensus": true,
        "checks": checks
    })))
}

async fn export_deposits(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<DepositExportQuery>,
) -> Result<Response, Response> {
    auth_tenant(&state, &headers).map_err(map_err)?;
    require_gate(&state).await.map_err(map_err)?;
    let format = query.format.as_deref().unwrap_or("ndjson").to_lowercase();
    if format != "ndjson" && format != "csv" {
        return Err(api_error(StatusCode::BAD_REQUEST, "format must be ndjson or csv"));
    }
    let limit = query.limit.unwrap_or(1000).clamp(1, EXPORT_MAX_ROWS);
    let ledger = state.deposit_ledger.lock().await;
    let items = ledger
        .list_deposits(
            query.address.as_deref(),
            query.state.as_deref(),
            limit,
            0,
        )
        .map_err(map_err)?;
    if format == "csv" {
        let body = deposits_to_csv(&items);
        return Ok((
            StatusCode::OK,
            [
                (axum::http::header::CONTENT_TYPE, "text/csv"),
                (
                    axum::http::header::CONTENT_DISPOSITION,
                    "attachment; filename=deposits.csv",
                ),
            ],
            body,
        )
            .into_response());
    }
    let mut body = String::new();
    for item in &items {
        let line = serde_json::to_string(item).map_err(|err| map_err(EngineError::Message(err.to_string())))?;
        body.push_str(&line);
        body.push('\n');
    }
    Ok((
        StatusCode::OK,
        [
            (axum::http::header::CONTENT_TYPE, "application/x-ndjson"),
            (
                axum::http::header::CONTENT_DISPOSITION,
                "attachment; filename=deposits.ndjson",
            ),
        ],
        body,
    )
        .into_response())
}

async fn receiver_stats(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, Response> {
    auth_tenant(&state, &headers).map_err(map_err)?;
    Ok(Json(receiver_inbox_count(&state.config.receiver_database)))
}

async fn webhook_verify(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<WebhookVerifyBody>,
) -> Result<Json<serde_json::Value>, Response> {
    auth_tenant(&state, &headers).map_err(map_err)?;
    let secret = state
        .config
        .webhook_secret
        .as_deref()
        .ok_or_else(|| api_error(StatusCode::BAD_REQUEST, "INTEGRATOR_WEBHOOK_SECRET not configured"))?;
    let valid = verify_hmac_sha256_hex(secret, &body.body, &body.signature);
    Ok(Json(serde_json::json!({ "ok": valid, "algorithm": "sha256" })))
}

fn json_check(name: &str, ok: bool, detail: impl Into<String>) -> serde_json::Value {
    serde_json::json!({
        "name": name,
        "ok": ok,
        "detail": detail.into()
    })
}

fn receiver_inbox_count(path: &StdPath) -> serde_json::Value {
    if !path.is_file() {
        return serde_json::json!({
            "ok": false,
            "count": 0,
            "database": path.display().to_string(),
            "error": "receiver database missing"
        });
    }
    match OutboxReceiverStore::open(path) {
        Ok(store) => match store.count() {
            Ok(count) => serde_json::json!({
                "ok": true,
                "count": count,
                "database": path.display().to_string()
            }),
            Err(err) => serde_json::json!({
                "ok": false,
                "count": 0,
                "database": path.display().to_string(),
                "error": err.to_string()
            }),
        },
        Err(err) => serde_json::json!({
            "ok": false,
            "count": 0,
            "database": path.display().to_string(),
            "error": err.to_string()
        }),
    }
}

fn deposits_to_csv(items: &[DepositRecord]) -> String {
    let mut out = String::from(
        "txId,outputIndex,address,amountSompi,state,blockDaaScore,isCoinbase,firstSeenDaa,lastSeenDaa,creditedDaa,reversedDaa,reversalReason\n",
    );
    for item in items {
        out.push_str(&csv_field(&item.tx_id));
        out.push(',');
        out.push_str(&item.output_index.to_string());
        out.push(',');
        out.push_str(&csv_field(&item.address));
        out.push(',');
        out.push_str(&item.amount_sompi.to_string());
        out.push(',');
        out.push_str(&csv_field(&item.state));
        out.push(',');
        out.push_str(&item.block_daa_score.to_string());
        out.push(',');
        out.push_str(if item.is_coinbase { "true" } else { "false" });
        out.push(',');
        out.push_str(&item.first_seen_daa.to_string());
        out.push(',');
        out.push_str(&item.last_seen_daa.to_string());
        out.push(',');
        out.push_str(
            &item
                .credited_daa
                .map(|value| value.to_string())
                .unwrap_or_default(),
        );
        out.push(',');
        out.push_str(
            &item
                .reversed_daa
                .map(|value| value.to_string())
                .unwrap_or_default(),
        );
        out.push(',');
        out.push_str(&csv_field(
            item.reversal_reason.as_deref().unwrap_or(""),
        ));
        out.push('\n');
    }
    out
}

fn csv_field(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn verify_hmac_sha256_hex(secret: &str, body: &str, signature: &str) -> bool {
    let normalized = signature.trim().strip_prefix("sha256=").unwrap_or(signature.trim());
    let expected = hmac_sha256_hex(secret, body);
    normalized.eq_ignore_ascii_case(&expected)
}

async fn webhook_test(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<WebhookTestBody>,
) -> Result<Json<serde_json::Value>, Response> {
    auth_tenant(&state, &headers).map_err(map_err)?;
    let payload = serde_json::json!({
        "schemaVersion": 1,
        "event": {
            "id": 0,
            "eventKey": "test:integrator-api",
            "kind": "credit",
            "txId": "0000000000000000000000000000000000000000000000000000000000000000",
            "outputIndex": 0,
            "amountSompi": 1,
            "address": "kaspatest:qtest"
        }
    });
    let client = reqwest::Client::new();
    let mut request = client
        .post(&body.url)
        .header("Content-Type", "application/json")
        .header("Idempotency-Key", "test:integrator-api")
        .json(&payload);
    if let Some(secret) = &state.config.webhook_secret {
        let signature = hmac_sha256_hex(secret, &payload.to_string());
        request = request.header("X-Integrator-Signature", format!("sha256={signature}"));
    }
    let response = request
        .send()
        .await
        .map_err(|err| map_err(EngineError::Transport(err)))?;
    Ok(Json(serde_json::json!({
        "ok": response.status().is_success(),
        "status": response.status().as_u16()
    })))
}

fn hmac_sha256_hex(secret: &str, body: &str) -> String {
    use ring::hmac;
    let key = hmac::Key::new(hmac::HMAC_SHA256, secret.as_bytes());
    let tag = hmac::sign(&key, body.as_bytes());
    hex_encode(tag.as_ref())
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{:02x}", byte)).collect()
}

fn read_watchlist(path: &StdPath) -> EngineResult<WatchlistFile> {
    if !path.exists() {
        return Ok(WatchlistFile {
            addresses: Vec::new(),
            updated_at: 0,
        });
    }
    let raw = fs::read_to_string(path).map_err(|err| EngineError::Message(err.to_string()))?;
    serde_json::from_str(&raw).map_err(|err| EngineError::Message(err.to_string()))
}

fn write_watchlist(path: &StdPath, file: &WatchlistFile) -> EngineResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| EngineError::Message(err.to_string()))?;
    }
    let raw = serde_json::to_string_pretty(file).map_err(|err| EngineError::Message(err.to_string()))?;
    fs::write(path, raw).map_err(|err| EngineError::Message(err.to_string()))
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{to_bytes, Body};
    use axum::http::{Request, StatusCode};
    use std::collections::HashMap;
    use tempfile::TempDir;
    use tower::ServiceExt;

    const TEST_ADDRESS: &str =
        "kaspatest:qptv6u8kel95drh2p2z492cyksk8lpetep286fngqu5j9nk57g642lzf748kt";

    fn test_state(dir: &TempDir) -> AppState {
        let deposit_database = dir.path().join("deposits.sqlite");
        let withdrawal_database = dir.path().join("withdrawals.sqlite");
        let watchlist_path = dir.path().join("watchlist.json");
        let evidence_dir = dir.path().join("evidence");
        std::fs::create_dir_all(&evidence_dir).unwrap();
        let deposit_ledger = DepositLedger::open(&deposit_database).unwrap();
        let mut api_keys = HashMap::new();
        api_keys.insert("test-secret-key".into(), "pilot".into());
        AppState {
            config: IntegratorConfig {
                bind: "127.0.0.1:8787".parse().unwrap(),
                deposit_database,
                withdrawal_database,
                watchlist_path,
                evidence_dir,
                receiver_database: dir.path().join("receiver.sqlite"),
                api_keys,
                webhook_secret: Some("webhook-test-secret".into()),
                confirmation_daa: 10,
                network: "testnet-10".to_string(),
                require_gate: false,
                gate: OwnedNodeGateOptions::default(),
                not_consensus: true,
            },
            deposit_ledger: Arc::new(Mutex::new(deposit_ledger)),
            rest: Arc::new(Tn10RestClient::new(crate::network::TESTNET_10_REST).unwrap()),
        }
    }

    #[test]
    fn parse_api_keys_accepts_tenant_pairs() {
        let keys = parse_api_keys("pilot:abc,cex:def".into()).unwrap();
        assert_eq!(keys.get("abc"), Some(&"pilot".to_string()));
        assert_eq!(keys.get("def"), Some(&"cex".to_string()));
        assert!(parse_api_keys("invalid".into()).is_err());
    }

    #[tokio::test]
    async fn health_is_public() {
        let dir = tempfile::tempdir().unwrap();
        let app = build_router(test_state(&dir));
        let response = app
            .oneshot(Request::get("/health").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["service"], "tn10-integrator-api");
    }

    #[tokio::test]
    async fn custody_routes_require_auth() {
        let dir = tempfile::tempdir().unwrap();
        let app = build_router(test_state(&dir));
        let response = app
            .oneshot(Request::get("/v1/deposits").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn deposits_list_returns_empty_journal() {
        let dir = tempfile::tempdir().unwrap();
        let app = build_router(test_state(&dir));
        let response = app
            .oneshot(
                Request::get("/v1/deposits")
                    .header("x-integrator-key", "test-secret-key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["ok"].as_bool().unwrap());
        assert_eq!(json["count"], 0);
    }

    #[tokio::test]
    async fn pilot_summary_returns_journal_counts() {
        let dir = tempfile::tempdir().unwrap();
        let app = build_router(test_state(&dir));
        let response = app
            .oneshot(
                Request::get("/v1/pilot/summary")
                    .header("x-integrator-key", "test-secret-key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["pilotWeek"], 1);
        assert_eq!(json["tenant"], "pilot");
    }

    #[test]
    fn verify_hmac_accepts_sha256_prefix() {
        let secret = "webhook-test-secret";
        let body = r#"{"schemaVersion":1}"#;
        let sig = format!("sha256={}", hmac_sha256_hex(secret, body));
        assert!(verify_hmac_sha256_hex(secret, body, &sig));
        assert!(!verify_hmac_sha256_hex(secret, body, "deadbeef"));
    }

    #[test]
    fn deposits_csv_escapes_commas() {
        let record = DepositRecord {
            tx_id: "tx".into(),
            output_index: 0,
            address: "kaspatest:abc".into(),
            amount_sompi: 1,
            block_daa_score: 1,
            is_coinbase: false,
            state: "credited".into(),
            first_seen_daa: 1,
            last_seen_daa: 2,
            credited_daa: Some(2),
            reversed_daa: None,
            reversal_reason: Some("orphan, proof".into()),
        };
        let csv = deposits_to_csv(&[record]);
        assert!(csv.contains("\"orphan, proof\""));
    }

    #[tokio::test]
    async fn selftest_lists_checks() {
        let dir = tempfile::tempdir().unwrap();
        let state = test_state(&dir);
        let evidence = dir.path().join("evidence/evidence_test.json");
        std::fs::write(&evidence, "{}").unwrap();
        let app = build_router(state);
        let response = app
            .oneshot(
                Request::get("/v1/pilot/selftest")
                    .header("x-integrator-key", "test-secret-key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["checks"].as_array().unwrap().len() >= 5);
    }

    #[tokio::test]
    async fn webhook_verify_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let app = build_router(test_state(&dir));
        let payload = r#"{"event":"test"}"#;
        let signature = format!("sha256={}", hmac_sha256_hex("webhook-test-secret", payload));
        let response = app
            .oneshot(
                Request::post("/v1/webhooks/verify")
                    .header("x-integrator-key", "test-secret-key")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({ "body": payload, "signature": signature }).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["ok"].as_bool().unwrap());
    }

    #[tokio::test]
    async fn watchlist_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let app = build_router(test_state(&dir));
        let payload = serde_json::json!({ "addresses": [TEST_ADDRESS] });
        let response = app
            .oneshot(
                Request::post("/v1/watchlist")
                    .header("x-integrator-key", "test-secret-key")
                    .header("content-type", "application/json")
                    .body(Body::from(payload.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["addresses"][0], TEST_ADDRESS);
    }
}
