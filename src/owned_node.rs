//! Bounded health checks for an owned, loopback TN10 kaspad.

use crate::error::{EngineError, Result};
use crate::network::require_tn10;
use crate::wrpc::{
    decode_block_dag_info_response, decode_connected_peer_info_response,
    decode_server_info_response, encode_get_block_dag_info, encode_get_connected_peer_info,
    encode_get_server_info, WrpcBlockDagInfo, WrpcServerInfo, MAX_WRPC_FRAME_BYTES,
};
use futures_util::{SinkExt, StreamExt};
use std::collections::HashSet;
use std::net::IpAddr;
use std::time::Duration;
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
use tokio_tungstenite::tungstenite::Message;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(10);
const MIN_SERVER_VERSION: (u64, u64, u64) = (2, 0, 1);
const MAX_INTERNAL_DAA_DELTA: u64 = 100;
const MAX_HEADER_BODY_GAP: u64 = 100;
pub const MAX_OWNED_NODE_URLS: usize = 8;

/// Machine-readable owned-node sync stage for supervisors (not consensus).
pub const STAGE_HEALTHY: &str = "healthy";
pub const STAGE_UTXO_COMMIT: &str = "utxo_commit";
pub const STAGE_DAG_INCOMPLETE: &str = "dag_incomplete";
pub const STAGE_BODY_SYNC: &str = "body_sync";
pub const STAGE_IBD_PEERS: &str = "ibd_peers";
pub const STAGE_NO_PEERS: &str = "no_peers";
pub const STAGE_FINISHING_SYNC: &str = "finishing_sync";
pub const STAGE_MISSING_UTXOINDEX: &str = "missing_utxoindex";
pub const STAGE_UNREACHABLE: &str = "unreachable";

/// Classify probe data without applying the full health gate (monitoring only).
pub fn owned_node_stage(health: &OwnedNodeHealth) -> &'static str {
    if !health.server.has_utxo_index {
        return STAGE_MISSING_UTXOINDEX;
    }
    if health.server.virtual_daa_score == 0 || health.dag.virtual_daa_score == 0 {
        return STAGE_UTXO_COMMIT;
    }
    if health.dag.virtual_daa_score > 0 && health.dag.header_count == 0 {
        return STAGE_DAG_INCOMPLETE;
    }
    let header_body_gap = health
        .dag
        .header_count
        .saturating_sub(health.dag.block_count);
    if header_body_gap > MAX_HEADER_BODY_GAP {
        return STAGE_BODY_SYNC;
    }
    if health.ibd_peers > 0 {
        return STAGE_IBD_PEERS;
    }
    if health.connected_peers == 0 {
        return STAGE_NO_PEERS;
    }
    if !health.server.is_synced {
        return STAGE_FINISHING_SYNC;
    }
    STAGE_HEALTHY
}

pub fn owned_node_stage_label(stage: &str) -> &'static str {
    match stage {
        STAGE_HEALTHY => "healthy",
        STAGE_UTXO_COMMIT => "UTXO commit (DAA 0)",
        STAGE_DAG_INCOMPLETE => "DAG incomplete (header_count 0)",
        STAGE_BODY_SYNC => "body sync (header/body gap)",
        STAGE_IBD_PEERS => "IBD peers connected",
        STAGE_NO_PEERS => "no connected peers",
        STAGE_FINISHING_SYNC => "finishing sync (isSynced=false)",
        STAGE_MISSING_UTXOINDEX => "missing --utxoindex",
        STAGE_UNREACHABLE => "wRPC unreachable",
        _ => "unknown",
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnedNodeHealth {
    pub server: WrpcServerInfo,
    pub dag: WrpcBlockDagInfo,
    pub connected_peers: u64,
    pub ibd_peers: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnedNodeAssessment {
    pub local_daa: u64,
    pub public_daa: u64,
    pub behind_public_daa: u64,
    pub server_dag_delta: u64,
    pub header_body_gap: u64,
    pub connected_peers: u64,
    pub ibd_peers: u64,
}

pub fn require_loopback_wrpc_url(raw: &str) -> Result<()> {
    let url = reqwest::Url::parse(raw)
        .map_err(|error| EngineError::Message(format!("invalid owned-node URL: {error}")))?;
    if url.scheme() != "ws" {
        return Err(EngineError::Message(
            "owned-node URL must use cleartext ws:// on loopback".into(),
        ));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(EngineError::Message(
            "owned-node URL must not contain credentials".into(),
        ));
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err(EngineError::Message(
            "owned-node URL must not contain a query or fragment".into(),
        ));
    }
    let host = url
        .host_str()
        .ok_or_else(|| EngineError::Message("owned-node URL is missing a host".into()))?;
    let normalized_host = host.trim_start_matches('[').trim_end_matches(']');
    let loopback = normalized_host.eq_ignore_ascii_case("localhost")
        || normalized_host
            .parse::<IpAddr>()
            .map(|address| address.is_loopback())
            .unwrap_or(false);
    if !loopback {
        return Err(EngineError::Message(
            "cleartext wRPC is restricted to a loopback owned node".into(),
        ));
    }
    if url.port().is_none() {
        return Err(EngineError::Message(
            "owned-node URL requires an explicit TCP port".into(),
        ));
    }
    Ok(())
}

pub fn validate_owned_node_urls(urls: &[String]) -> Result<()> {
    if urls.is_empty() || urls.len() > MAX_OWNED_NODE_URLS {
        return Err(EngineError::Message(format!(
            "owned-node pool requires 1-{MAX_OWNED_NODE_URLS} URLs"
        )));
    }
    let mut unique = HashSet::with_capacity(urls.len());
    for url in urls {
        require_loopback_wrpc_url(url)?;
        if !unique.insert(url) {
            return Err(EngineError::Message(format!(
                "duplicate owned-node URL {url}"
            )));
        }
    }
    Ok(())
}

/// Prefer the lowest public-DAA lag, then the earliest configured URL.
pub fn select_primary(candidates: &[(usize, OwnedNodeAssessment)]) -> Option<usize> {
    candidates
        .iter()
        .min_by_key(|(index, assessment)| (assessment.behind_public_daa, *index))
        .map(|(index, _)| *index)
}

pub fn next_failover_index(failed_index: usize, count: usize) -> Result<usize> {
    if count == 0 || failed_index >= count {
        return Err(EngineError::Message(
            "owned-node failover index is out of range".into(),
        ));
    }
    Ok((failed_index + 1) % count)
}

/// Choose the healthiest remaining replica, otherwise rotate in configured order.
pub fn choose_failover_index(
    failed_index: usize,
    count: usize,
    healthy: &[(usize, OwnedNodeAssessment)],
) -> Result<usize> {
    if let Some(index) = select_primary(healthy) {
        return Ok(index);
    }
    next_failover_index(failed_index, count)
}

pub async fn probe_owned_node(url: &str) -> Result<OwnedNodeHealth> {
    require_loopback_wrpc_url(url)?;
    let mut config = WebSocketConfig::default();
    config.max_message_size = Some(MAX_WRPC_FRAME_BYTES);
    config.max_frame_size = Some(MAX_WRPC_FRAME_BYTES);
    let connected = tokio::time::timeout(
        CONNECT_TIMEOUT,
        tokio_tungstenite::connect_async_with_config(url, Some(config), false),
    )
    .await
    .map_err(|_| EngineError::Message("owned-node wRPC connect timeout".into()))?;
    let (mut socket, _) = connected.map_err(|error| {
        EngineError::Message(format!("owned-node wRPC connect failed: {error}"))
    })?;

    socket
        .send(Message::Text(encode_get_server_info(1)?.into()))
        .await
        .map_err(|error| EngineError::Message(format!("wRPC send failed: {error}")))?;
    socket
        .send(Message::Text(encode_get_block_dag_info(2)?.into()))
        .await
        .map_err(|error| EngineError::Message(format!("wRPC send failed: {error}")))?;
    socket
        .send(Message::Text(encode_get_connected_peer_info(3)?.into()))
        .await
        .map_err(|error| EngineError::Message(format!("wRPC send failed: {error}")))?;

    let mut server = None;
    let mut dag = None;
    let mut peers = None;
    while server.is_none() || dag.is_none() || peers.is_none() {
        let raw = receive_text(&mut socket).await?;
        let value: serde_json::Value = serde_json::from_str(&raw)?;
        match value.get("id").and_then(serde_json::Value::as_u64) {
            Some(1) if server.is_none() => {
                server = Some(decode_server_info_response(&raw, 1)?);
            }
            Some(2) if dag.is_none() => {
                dag = Some(decode_block_dag_info_response(&raw, 2)?);
            }
            Some(3) if peers.is_none() => {
                peers = Some(decode_connected_peer_info_response(&raw, 3)?);
            }
            Some(id) => {
                return Err(EngineError::Message(format!(
                    "unexpected or duplicate owned-node response id {id}"
                )));
            }
            None => {
                return Err(EngineError::Message(
                    "owned-node health probe received a notification".into(),
                ));
            }
        }
    }
    let _ = socket.close(None).await;
    let peers = peers.expect("peer response checked");
    let connected_peers = u64::try_from(peers.infos.len()).unwrap_or(u64::MAX);
    let ibd_peers = u64::try_from(peers.infos.iter().filter(|peer| peer.is_ibd_peer).count())
        .unwrap_or(u64::MAX);
    Ok(OwnedNodeHealth {
        server: server.expect("server response checked"),
        dag: dag.expect("DAG response checked"),
        connected_peers,
        ibd_peers,
    })
}

pub fn assess_owned_node(
    health: &OwnedNodeHealth,
    public_daa: u64,
    max_daa_lag: u64,
) -> Result<OwnedNodeAssessment> {
    if max_daa_lag == 0 || max_daa_lag > 100_000 {
        return Err(EngineError::Message(
            "maximum owned-node DAA lag must be 1-100000".into(),
        ));
    }
    require_tn10(&health.server.network_id)?;
    require_tn10(&health.dag.network)?;
    if !health.server.has_utxo_index {
        return Err(EngineError::Message(
            "owned TN10 node must run with --utxoindex".into(),
        ));
    }
    if health.server.virtual_daa_score == 0 || health.dag.virtual_daa_score == 0 {
        return Err(EngineError::Message(
            "owned TN10 node is still importing the pruning-point UTXO set (DAA 0)".into(),
        ));
    }
    if health.dag.virtual_daa_score > 0 && health.dag.header_count == 0 {
        return Err(EngineError::Message(
            "owned TN10 node DAG response has zero header_count while DAA > 0".into(),
        ));
    }
    let header_body_gap = health
        .dag
        .header_count
        .saturating_sub(health.dag.block_count);
    if header_body_gap > MAX_HEADER_BODY_GAP {
        return Err(EngineError::Message(format!(
            "owned TN10 node has {header_body_gap} headers without bodies; maximum is {MAX_HEADER_BODY_GAP}"
        )));
    }
    if health.ibd_peers > 0 {
        return Err(EngineError::Message(format!(
            "owned TN10 node still has {} IBD peer(s); kaspad isSynced is not enough",
            health.ibd_peers
        )));
    }
    if health.connected_peers == 0 {
        return Err(EngineError::Message(
            "owned TN10 node has no connected peers".into(),
        ));
    }
    if !health.server.is_synced {
        return Err(EngineError::Message(
            "owned TN10 node reports isSynced=false".into(),
        ));
    }
    let version = parse_version(&health.server.server_version)?;
    if version < MIN_SERVER_VERSION {
        return Err(EngineError::Message(format!(
            "owned TN10 node {} is older than required 2.0.1",
            health.server.server_version
        )));
    }
    let server_dag_delta = health
        .server
        .virtual_daa_score
        .abs_diff(health.dag.virtual_daa_score);
    if server_dag_delta > MAX_INTERNAL_DAA_DELTA {
        return Err(EngineError::Message(format!(
            "owned-node wRPC responses disagree by {server_dag_delta} DAA"
        )));
    }
    let local_daa = health
        .server
        .virtual_daa_score
        .max(health.dag.virtual_daa_score);
    let behind_public_daa = public_daa.saturating_sub(local_daa);
    if behind_public_daa > max_daa_lag {
        return Err(EngineError::Message(format!(
            "owned TN10 node is {behind_public_daa} DAA behind public TN10; maximum is {max_daa_lag}"
        )));
    }
    Ok(OwnedNodeAssessment {
        local_daa,
        public_daa,
        behind_public_daa,
        server_dag_delta,
        header_body_gap,
        connected_peers: health.connected_peers,
        ibd_peers: health.ibd_peers,
    })
}

fn parse_version(raw: &str) -> Result<(u64, u64, u64)> {
    let core = raw
        .trim()
        .trim_start_matches('v')
        .split_once('-')
        .map(|(version, _)| version)
        .unwrap_or_else(|| raw.trim().trim_start_matches('v'));
    let mut parts = core.split('.');
    let major = parts.next().and_then(|value| value.parse().ok());
    let minor = parts.next().and_then(|value| value.parse().ok());
    let patch = parts.next().and_then(|value| value.parse().ok());
    if parts.next().is_some() {
        return Err(EngineError::Message(format!(
            "invalid owned-node version {raw}"
        )));
    }
    match (major, minor, patch) {
        (Some(major), Some(minor), Some(patch)) => Ok((major, minor, patch)),
        _ => Err(EngineError::Message(format!(
            "invalid owned-node version {raw}"
        ))),
    }
}

async fn receive_text(
    socket: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
) -> Result<String> {
    loop {
        let next = tokio::time::timeout(RESPONSE_TIMEOUT, socket.next())
            .await
            .map_err(|_| EngineError::Message("owned-node wRPC response timeout".into()))?;
        let message = next
            .ok_or_else(|| EngineError::Message("owned-node wRPC connection closed".into()))?
            .map_err(|error| EngineError::Message(format!("wRPC receive failed: {error}")))?;
        match message {
            Message::Text(text) => return Ok(text.to_string()),
            Message::Ping(payload) => socket
                .send(Message::Pong(payload))
                .await
                .map_err(|error| EngineError::Message(format!("wRPC pong failed: {error}")))?,
            Message::Pong(_) => {}
            Message::Close(frame) => {
                return Err(EngineError::Message(format!(
                    "owned-node wRPC close frame: {frame:?}"
                )));
            }
            Message::Binary(_) => {
                return Err(EngineError::Message(
                    "owned-node JSON endpoint sent a binary frame".into(),
                ));
            }
            Message::Frame(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn healthy() -> OwnedNodeHealth {
        OwnedNodeHealth {
            server: WrpcServerInfo {
                has_utxo_index: true,
                is_synced: true,
                network_id: "testnet-10".into(),
                rpc_api_revision: 0,
                rpc_api_version: 1,
                server_version: "2.0.1".into(),
                virtual_daa_score: 1_000,
            },
            dag: WrpcBlockDagInfo {
                network: "testnet-10".into(),
                virtual_daa_score: 1_001,
                block_count: 1_000,
                header_count: 1_000,
            },
            connected_peers: 3,
            ibd_peers: 0,
        }
    }

    #[test]
    fn owned_node_stage_labels_ibd_progression() {
        assert_eq!(owned_node_stage(&healthy()), STAGE_HEALTHY);
        let mut utxo = healthy();
        utxo.dag.virtual_daa_score = 0;
        assert_eq!(owned_node_stage(&utxo), STAGE_UTXO_COMMIT);
        let mut bodies = healthy();
        bodies.dag.block_count = 1;
        bodies.dag.header_count = 10_000;
        assert_eq!(owned_node_stage(&bodies), STAGE_BODY_SYNC);
        let mut ibd = healthy();
        ibd.ibd_peers = 1;
        assert_eq!(owned_node_stage(&ibd), STAGE_IBD_PEERS);
        assert_eq!(
            owned_node_stage_label(STAGE_BODY_SYNC),
            "body sync (header/body gap)"
        );
    }

    #[test]
    fn loopback_wrpc_validation_fails_closed() {
        assert!(require_loopback_wrpc_url("ws://127.0.0.1:18210").is_ok());
        assert!(require_loopback_wrpc_url("ws://[::1]:18210").is_ok());
        assert!(require_loopback_wrpc_url("ws://localhost:18210").is_ok());
        assert!(require_loopback_wrpc_url("ws://192.0.2.1:18210").is_err());
        assert!(require_loopback_wrpc_url("wss://127.0.0.1:18210").is_err());
        assert!(require_loopback_wrpc_url("ws://user@localhost:18210").is_err());
        assert!(require_loopback_wrpc_url("ws://localhost").is_err());
    }

    #[test]
    fn health_assessment_checks_sync_index_version_and_lag() {
        let assessment = assess_owned_node(&healthy(), 1_010, 10).unwrap();
        assert_eq!(assessment.local_daa, 1_001);
        assert_eq!(assessment.behind_public_daa, 9);

        let mut unhealthy = healthy();
        unhealthy.server.is_synced = false;
        assert!(assess_owned_node(&unhealthy, 1_010, 10).is_err());
        let mut unhealthy = healthy();
        unhealthy.server.has_utxo_index = false;
        assert!(assess_owned_node(&unhealthy, 1_010, 10).is_err());
        let mut unhealthy = healthy();
        unhealthy.server.server_version = "2.0.0".into();
        assert!(assess_owned_node(&unhealthy, 1_010, 10).is_err());
        assert!(assess_owned_node(&healthy(), 1_100, 10).is_err());
    }

    #[test]
    fn health_assessment_rejects_ibd_header_gap_and_isolation() {
        let mut unhealthy = healthy();
        unhealthy.ibd_peers = 1;
        assert!(assess_owned_node(&unhealthy, 1_010, 10)
            .unwrap_err()
            .to_string()
            .contains("IBD peer"));
        let mut unhealthy = healthy();
        unhealthy.connected_peers = 0;
        assert!(assess_owned_node(&unhealthy, 1_010, 10)
            .unwrap_err()
            .to_string()
            .contains("no connected peers"));
        let mut unhealthy = healthy();
        unhealthy.dag.header_count = 1_437_924;
        unhealthy.dag.block_count = 1;
        assert!(assess_owned_node(&unhealthy, 1_010, 10)
            .unwrap_err()
            .to_string()
            .contains("headers without bodies"));
        let mut unhealthy = healthy();
        unhealthy.dag.virtual_daa_score = 0;
        assert!(assess_owned_node(&unhealthy, 1_010, 10)
            .unwrap_err()
            .to_string()
            .contains("DAA 0"));
        let mut unhealthy = healthy();
        unhealthy.dag.header_count = 0;
        unhealthy.dag.virtual_daa_score = 1_001;
        assert!(assess_owned_node(&unhealthy, 1_010, 10)
            .unwrap_err()
            .to_string()
            .contains("header_count"));
    }

    #[test]
    fn version_parser_accepts_release_suffixes_only() {
        assert_eq!(parse_version("v2.0.1").unwrap(), (2, 0, 1));
        assert_eq!(parse_version("2.1.0-rc1").unwrap(), (2, 1, 0));
        assert!(parse_version("2.0").is_err());
        assert!(parse_version("2.0.1.4").is_err());
    }

    fn assessment(behind: u64) -> OwnedNodeAssessment {
        OwnedNodeAssessment {
            local_daa: 100 - behind,
            public_daa: 100,
            behind_public_daa: behind,
            server_dag_delta: 0,
            header_body_gap: 0,
            connected_peers: 3,
            ibd_peers: 0,
        }
    }

    #[test]
    fn owned_node_urls_must_be_bounded_unique_loopback() {
        assert!(validate_owned_node_urls(&["ws://127.0.0.1:18210".into()]).is_ok());
        assert!(validate_owned_node_urls(&[
            "ws://127.0.0.1:18210".into(),
            "ws://127.0.0.1:28210".into(),
        ])
        .is_ok());
        assert!(validate_owned_node_urls(&[]).is_err());
        assert!(validate_owned_node_urls(&[
            "ws://127.0.0.1:18210".into(),
            "ws://127.0.0.1:18210".into(),
        ])
        .is_err());
        assert!(validate_owned_node_urls(&["ws://192.0.2.1:18210".into()]).is_err());
        let too_many: Vec<String> = (0..=MAX_OWNED_NODE_URLS)
            .map(|port| format!("ws://127.0.0.1:{}", 18210 + port))
            .collect();
        assert!(validate_owned_node_urls(&too_many).is_err());
    }

    #[test]
    fn failover_prefers_healthy_replica_then_rotates() {
        let lagging = vec![(0, assessment(10)), (1, assessment(1))];
        assert_eq!(select_primary(&lagging), Some(1));
        assert_eq!(select_primary(&[]), None);
        assert_eq!(next_failover_index(0, 2).unwrap(), 1);
        assert_eq!(next_failover_index(1, 2).unwrap(), 0);
        assert!(next_failover_index(0, 0).is_err());
        assert_eq!(
            choose_failover_index(0, 2, &[(1, assessment(4))]).unwrap(),
            1
        );
        assert_eq!(choose_failover_index(1, 2, &[]).unwrap(), 0);
        assert_eq!(choose_failover_index(0, 1, &[]).unwrap(), 0);
    }
}
