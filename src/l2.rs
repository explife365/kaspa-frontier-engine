//! EVM L2s where Kaspa DeFi actually lives. Not kaspad and not a native USD stable.

use crate::error::{EngineError, Result};
use crate::network::{
    IGRA_GALLEON_CHAIN_ID, IGRA_GALLEON_RPC, IGRA_MAINNET_CHAIN_ID, IGRA_MAINNET_RPC,
    KASPLEX_L2_CHAIN_ID, KASPLEX_L2_RPC,
};
use crate::rest::{http_send_post, https_json_client, require_https};
use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct EvmChainProbe {
    pub name: &'static str,
    pub rpc: &'static str,
    pub expected_chain_id: u64,
    pub chain_id: u64,
    pub block_number: Option<u64>,
}

impl EvmChainProbe {
    pub fn matches_expected(&self) -> bool {
        self.chain_id == self.expected_chain_id
    }
}

#[derive(Clone)]
pub struct EvmRpcClient {
    http: reqwest::Client,
    url: String,
}

impl EvmRpcClient {
    pub fn new(url: impl Into<String>) -> Result<Self> {
        let url = url.into();
        require_https(&url)?;
        Ok(Self {
            http: https_json_client()?,
            url,
        })
    }

    pub async fn chain_id(&self) -> Result<u64> {
        self.rpc_hex("eth_chainId").await
    }

    pub async fn block_number(&self) -> Result<u64> {
        self.rpc_hex("eth_blockNumber").await
    }

    pub(crate) async fn post_rpc(&self, body: &serde_json::Value) -> Result<String> {
        let parsed: JsonRpcHexResult = self.post_rpc_value(body).await?;
        hex_result(&parsed)
    }

    async fn post_rpc_value<T: for<'de> Deserialize<'de>>(
        &self,
        body: &serde_json::Value,
    ) -> Result<T> {
        let resp = http_send_post(&self.http, &self.url, body)
            .await?
            .error_for_status()?;
        Ok(resp.json().await?)
    }

    async fn rpc_hex(&self, method: &str) -> Result<u64> {
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": []
        });
        parse_hex_u64(&self.post_rpc(&body).await?)
    }

    /// One POST for chainId + blockNumber. Falls back to two concurrent calls
    /// if the endpoint rejects JSON-RPC batch arrays.
    pub async fn chain_id_and_block_number(&self) -> Result<(u64, Option<u64>)> {
        let batch = serde_json::json!([
            {"jsonrpc": "2.0", "id": 1, "method": "eth_chainId", "params": []},
            {"jsonrpc": "2.0", "id": 2, "method": "eth_blockNumber", "params": []},
        ]);
        if let Ok(items) = self.post_rpc_value::<Vec<JsonRpcHexResult>>(&batch).await {
            if let Some(pair) = parse_chain_and_block(&items) {
                return Ok(pair);
            }
        }
        let (chain_id, block_number) = tokio::join!(self.chain_id(), self.block_number());
        Ok((chain_id?, block_number.ok()))
    }

    /// One POST of `eth_call`s. Falls back to concurrent single calls.
    pub async fn eth_call_batch(&self, calls: &[(&str, &str)]) -> Result<Vec<String>> {
        if calls.is_empty() {
            return Ok(Vec::new());
        }
        let batch: Vec<serde_json::Value> = calls
            .iter()
            .enumerate()
            .map(|(i, (to, data))| {
                serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": i + 1,
                    "method": "eth_call",
                    "params": [{ "to": *to, "data": *data }, "latest"]
                })
            })
            .collect();
        if let Ok(items) = self
            .post_rpc_value::<Vec<JsonRpcHexResult>>(&serde_json::Value::Array(batch))
            .await
        {
            if let Some(out) = ordered_hex_results(&items, calls.len()) {
                return Ok(out);
            }
        }
        let n = calls.len();
        let mut set = tokio::task::JoinSet::new();
        for (i, (to, data)) in calls.iter().enumerate() {
            let client = self.clone();
            let to = (*to).to_string();
            let data = (*data).to_string();
            set.spawn(async move { (i, client.eth_call(&to, &data).await) });
        }
        let mut slots: Vec<Option<Result<String>>> = (0..n).map(|_| None).collect();
        while let Some(joined) = set.join_next().await {
            match joined {
                Ok((i, result)) => {
                    if let Some(slot) = slots.get_mut(i) {
                        *slot = Some(result);
                    }
                }
                Err(e) => return Err(EngineError::Message(format!("eth_call join: {e}"))),
            }
        }
        let mut out = Vec::with_capacity(n);
        for slot in slots {
            match slot {
                Some(Ok(hex)) => out.push(hex),
                Some(Err(e)) => return Err(e),
                None => return Err(EngineError::Message("missing eth_call result".into())),
            }
        }
        Ok(out)
    }
}

#[derive(Deserialize)]
struct JsonRpcHexResult {
    #[serde(default)]
    id: Option<u64>,
    result: Option<String>,
    error: Option<serde_json::Value>,
}

fn hex_result(parsed: &JsonRpcHexResult) -> Result<String> {
    if let Some(err) = parsed.error.as_ref() {
        return Err(EngineError::Message(format!("eth rpc: {err}")));
    }
    parsed
        .result
        .clone()
        .ok_or_else(|| EngineError::Message("eth rpc missing result".into()))
}

fn ordered_hex_results(items: &[JsonRpcHexResult], n: usize) -> Option<Vec<String>> {
    let mut slots: Vec<Option<String>> = (0..n).map(|_| None).collect();
    for item in items {
        let Ok(hex) = hex_result(item) else {
            continue;
        };
        let Some(id) = item.id else {
            continue;
        };
        let idx = usize::try_from(id.checked_sub(1)?).ok()?;
        if let Some(slot) = slots.get_mut(idx) {
            *slot = Some(hex);
        }
    }
    slots.into_iter().collect()
}

fn parse_chain_and_block(items: &[JsonRpcHexResult]) -> Option<(u64, Option<u64>)> {
    let mut chain_id = None;
    let mut block_number = None;
    for item in items {
        if item.error.is_some() {
            continue;
        }
        let Some(hex) = item.result.as_deref() else {
            continue;
        };
        let Ok(value) = parse_hex_u64(hex) else {
            continue;
        };
        match item.id {
            Some(1) => chain_id = Some(value),
            Some(2) => block_number = Some(value),
            _ => {}
        }
    }
    Some((chain_id?, block_number))
}

pub fn parse_hex_u64(value: &str) -> Result<u64> {
    let s = value.trim();
    let hex = s
        .strip_prefix("0x")
        .or_else(|| s.strip_prefix("0X"))
        .unwrap_or(s);
    u64::from_str_radix(hex, 16)
        .map_err(|e| EngineError::Message(format!("bad chain id {value}: {e}")))
}

pub async fn probe_named(
    name: &'static str,
    rpc: &'static str,
    expected: u64,
) -> Result<EvmChainProbe> {
    let client = EvmRpcClient::new(rpc)?;
    let (chain_id, block_number) = client.chain_id_and_block_number().await?;
    Ok(EvmChainProbe {
        name,
        rpc,
        expected_chain_id: expected,
        chain_id,
        block_number,
    })
}

pub async fn probe_igra_galleon() -> Result<EvmChainProbe> {
    probe_named("Igra Galleon", IGRA_GALLEON_RPC, IGRA_GALLEON_CHAIN_ID).await
}

pub async fn probe_igra_mainnet() -> Result<EvmChainProbe> {
    probe_named("Igra Mainnet", IGRA_MAINNET_RPC, IGRA_MAINNET_CHAIN_ID).await
}

pub async fn probe_kasplex_l2() -> Result<EvmChainProbe> {
    probe_named("Kasplex L2", KASPLEX_L2_RPC, KASPLEX_L2_CHAIN_ID).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network::{IGRA_GALLEON_CHAIN_ID, IGRA_MAINNET_CHAIN_ID, KASPLEX_L2_CHAIN_ID};

    #[test]
    fn parses_galleon_and_kasplex_chain_ids() {
        assert_eq!(parse_hex_u64("0x97b4").unwrap(), IGRA_GALLEON_CHAIN_ID);
        assert_eq!(parse_hex_u64("0x97b1").unwrap(), IGRA_MAINNET_CHAIN_ID);
        assert_eq!(parse_hex_u64("0x28C64").unwrap(), KASPLEX_L2_CHAIN_ID);
        assert!(parse_hex_u64("nope").is_err());
    }

    #[test]
    fn rejects_cleartext_evm_rpc() {
        assert!(matches!(
            EvmRpcClient::new("http://127.0.0.1:8545"),
            Err(EngineError::InsecureTransport(_))
        ));
    }

    #[test]
    fn parses_batched_chain_and_block() {
        let items = vec![
            JsonRpcHexResult {
                id: Some(2),
                result: Some("0x11a1d14".into()),
                error: None,
            },
            JsonRpcHexResult {
                id: Some(1),
                result: Some("0x97b4".into()),
                error: None,
            },
        ];
        let (chain, block) = parse_chain_and_block(&items).unwrap();
        assert_eq!(chain, IGRA_GALLEON_CHAIN_ID);
        assert_eq!(block, Some(parse_hex_u64("0x11a1d14").unwrap()));
        assert!(parse_chain_and_block(&[JsonRpcHexResult {
            id: Some(2),
            result: Some("0x1".into()),
            error: None,
        }])
        .is_none());
        let hexes = ordered_hex_results(
            &[
                JsonRpcHexResult {
                    id: Some(3),
                    result: Some("0x06".into()),
                    error: None,
                },
                JsonRpcHexResult {
                    id: Some(1),
                    result: Some("0xaa".into()),
                    error: None,
                },
                JsonRpcHexResult {
                    id: Some(2),
                    result: Some("0xbb".into()),
                    error: None,
                },
            ],
            3,
        )
        .unwrap();
        assert_eq!(hexes, ["0xaa", "0xbb", "0x06"]);
    }
}
