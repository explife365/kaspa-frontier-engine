//! Bounded health checks for an owned, loopback TN10 kaspad.

use crate::error::{EngineError, Result};
use crate::network::require_tn10;
use crate::wrpc::{
    decode_block_dag_info_response, decode_server_info_response, encode_get_block_dag_info,
    encode_get_server_info, WrpcBlockDagInfo, WrpcServerInfo, MAX_WRPC_FRAME_BYTES,
};
use futures_util::{SinkExt, StreamExt};
use std::net::IpAddr;
use std::time::Duration;
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
use tokio_tungstenite::tungstenite::Message;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(10);
const MIN_SERVER_VERSION: (u64, u64, u64) = (2, 0, 1);
const MAX_INTERNAL_DAA_DELTA: u64 = 100;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnedNodeHealth {
    pub server: WrpcServerInfo,
    pub dag: WrpcBlockDagInfo,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnedNodeAssessment {
    pub local_daa: u64,
    pub public_daa: u64,
    pub behind_public_daa: u64,
    pub server_dag_delta: u64,
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

    let mut server = None;
    let mut dag = None;
    while server.is_none() || dag.is_none() {
        let raw = receive_text(&mut socket).await?;
        let value: serde_json::Value = serde_json::from_str(&raw)?;
        match value.get("id").and_then(serde_json::Value::as_u64) {
            Some(1) if server.is_none() => {
                server = Some(decode_server_info_response(&raw, 1)?);
            }
            Some(2) if dag.is_none() => {
                dag = Some(decode_block_dag_info_response(&raw, 2)?);
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
    Ok(OwnedNodeHealth {
        server: server.expect("server response checked"),
        dag: dag.expect("DAG response checked"),
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
    if !health.server.is_synced {
        return Err(EngineError::Message(
            "owned TN10 node reports isSynced=false".into(),
        ));
    }
    if !health.server.has_utxo_index {
        return Err(EngineError::Message(
            "owned TN10 node must run with --utxoindex".into(),
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
            },
        }
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
    fn version_parser_accepts_release_suffixes_only() {
        assert_eq!(parse_version("v2.0.1").unwrap(), (2, 0, 1));
        assert_eq!(parse_version("2.1.0-rc1").unwrap(), (2, 1, 0));
        assert!(parse_version("2.0").is_err());
        assert!(parse_version("2.0.1.4").is_err());
    }
}
