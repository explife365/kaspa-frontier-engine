//! Community covenant indexer (kascov). kaspad still has no getUtxosByCovenantId.

use crate::error::{EngineError, Result};
use crate::network::{require_tn10, KASCOV_TN10};
use crate::rest::{encode_path_segment, http_send_get, https_json_client, require_https};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KascovEvent {
    pub kind: String,
    pub txid: String,
    #[serde(default)]
    pub seq: u64,
    #[serde(default)]
    pub accepting_daa: Option<u64>,
    #[serde(default)]
    pub tx_index: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KascovStateField {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KascovUtxo {
    pub outpoint: String,
    #[serde(default)]
    pub created_daa: u64,
    pub live: bool,
    #[serde(default)]
    pub value: u64,
    #[serde(default)]
    pub script_hex: String,
    #[serde(default)]
    pub script_asm: Vec<String>,
    #[serde(default)]
    pub state_fields: Vec<KascovStateField>,
    #[serde(default)]
    pub template: String,
    #[serde(default)]
    pub uses_covenant_ops: bool,
    #[serde(default)]
    pub uses_zk_ops: bool,
    #[serde(default)]
    pub spent_txid: Option<String>,
    #[serde(default)]
    pub spent_budget: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KascovCoin {
    pub covenant_id: String,
    pub network: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub lineage_complete: bool,
    #[serde(default)]
    pub event_count: u64,
    #[serde(default)]
    pub live_utxos: u64,
    #[serde(default)]
    pub live_value: u64,
    #[serde(default)]
    pub genesis_txid: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub events: Vec<KascovEvent>,
    #[serde(default)]
    pub utxos: Vec<KascovUtxo>,
}

#[derive(Clone)]
pub struct KascovClient {
    http: reqwest::Client,
    base: String,
}

impl Default for KascovClient {
    fn default() -> Self {
        Self::new(KASCOV_TN10).expect("reqwest TLS client")
    }
}

impl KascovClient {
    pub fn new(base: impl Into<String>) -> Result<Self> {
        let base = base.into().trim_end_matches('/').to_string();
        require_https(&base)?;
        Ok(Self {
            http: https_json_client()?,
            base,
        })
    }

    pub fn coin_url(&self, covenant_id: &str) -> String {
        format!("{}/c/{}.json", self.base, encode_path_segment(covenant_id))
    }

    /// Kascov embeds typed UTXOs in the covenant document.
    pub fn utxos_url(&self, covenant_id: &str) -> String {
        self.coin_url(covenant_id)
    }

    pub async fn coin(&self, covenant_id: &str) -> Result<KascovCoin> {
        let url = self.coin_url(covenant_id);
        let resp = http_send_get(&self.http, &url).await?.error_for_status()?;
        let coin: KascovCoin = resp.json().await?;
        require_tn10(&coin.network)?;
        validate_coin(covenant_id, &coin)?;
        Ok(coin)
    }

    pub async fn utxos(&self, covenant_id: &str) -> Result<Vec<KascovUtxo>> {
        Ok(self
            .coin(covenant_id)
            .await?
            .utxos
            .into_iter()
            .filter(|utxo| utxo.live)
            .collect())
    }

    /// Coin metadata and validated live UTXOs from one community-indexer document.
    pub async fn snapshot(&self, covenant_id: &str) -> Result<(KascovCoin, Vec<KascovUtxo>)> {
        let coin = self.coin(covenant_id).await?;
        let live = coin
            .utxos
            .iter()
            .filter(|utxo| utxo.live)
            .cloned()
            .collect();
        Ok((coin, live))
    }
}

fn validate_coin(requested_id: &str, coin: &KascovCoin) -> Result<()> {
    if coin.covenant_id != requested_id {
        return Err(EngineError::Message(format!(
            "kascov covenant_id {} does not match requested {requested_id}",
            coin.covenant_id
        )));
    }
    for utxo in &coin.utxos {
        let (txid, index) = utxo.outpoint.rsplit_once(':').ok_or_else(|| {
            EngineError::Message(format!("invalid kascov outpoint {}", utxo.outpoint))
        })?;
        if txid.len() != 64
            || !txid.bytes().all(|byte| byte.is_ascii_hexdigit())
            || index.parse::<u32>().is_err()
        {
            return Err(EngineError::Message(format!(
                "invalid kascov outpoint {}",
                utxo.outpoint
            )));
        }
    }
    let live: Vec<_> = coin.utxos.iter().filter(|utxo| utxo.live).collect();
    let live_count = u64::try_from(live.len())
        .map_err(|_| EngineError::Message("kascov live UTXO count overflow".into()))?;
    let live_value = live
        .iter()
        .try_fold(0_u64, |sum, utxo| sum.checked_add(utxo.value))
        .ok_or_else(|| EngineError::Message("kascov live UTXO value overflow".into()))?;
    if live_count != coin.live_utxos || live_value != coin.live_value {
        return Err(EngineError::Message(format!(
            "kascov live UTXO summary mismatch: rows={live_count}/{live_value}, summary={}/{}",
            coin.live_utxos, coin.live_value
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::EngineError;

    #[test]
    fn parses_kascov_coin() {
        let raw = r#"{
            "covenant_id":"4a95a59dc79c3f46f35db91453f26785750450606836d82c48c1affdd71ed70a",
            "network":"testnet-10",
            "status":"active",
            "lineage_complete":true,
            "event_count":3,
            "live_utxos":1,
            "live_value":197961400,
            "genesis_txid":"6d0acd6fcbaf68bca1568a3cbbafe0f3c1d72c4f6ea0edc6f6c013a59cb5d591",
            "name":"jolly-amber-tapir",
            "events":[
                {"kind":"genesis","seq":0,"txid":"6d0acd6fcbaf68bca1568a3cbbafe0f3c1d72c4f6ea0edc6f6c013a59cb5d591"},
                {"kind":"transition","seq":1,"txid":"796445b99363dee7540d7dd75287c5a980d110bab007cee8d913ae254541410d"},
                {"kind":"transition","seq":2,"txid":"480fc61819f73dc476cbc5b12fb974bd711457f07e4d07b074ca19bdb77a5dcd"}
            ]
        }"#;
        let coin: KascovCoin = serde_json::from_str(raw).unwrap();
        assert!(require_tn10(&coin.network).is_ok());
        assert!(coin.lineage_complete);
        assert_eq!(coin.events.len(), 3);
        assert_eq!(coin.events[0].kind, "genesis");
        assert_eq!(coin.live_value, 197_961_400);
    }

    #[test]
    fn rejects_cleartext_kascov() {
        assert!(matches!(
            KascovClient::new("http://127.0.0.1:9"),
            Err(EngineError::InsecureTransport(_))
        ));
    }

    #[test]
    fn encodes_covenant_id_in_utxos_url() {
        let client = KascovClient::new("https://kascov.io/data/testnet-10").unwrap();
        assert!(client.utxos_url("ab:cd").ends_with("/c/ab%3Acd.json"));
    }

    #[test]
    fn validates_typed_embedded_utxos() {
        let covenant_id = "a".repeat(64);
        let txid = "b".repeat(64);
        let raw = serde_json::json!({
            "covenant_id": covenant_id,
            "network": "testnet-10",
            "status": "active",
            "lineage_complete": true,
            "event_count": 1,
            "live_utxos": 1,
            "live_value": 25,
            "genesis_txid": txid.clone(),
            "name": "typed",
            "events": [{"kind": "genesis", "txid": txid.clone(), "seq": 0}],
            "utxos": [{
                "outpoint": format!("{txid}:2"),
                "created_daa": 10,
                "live": true,
                "value": 25,
                "script_hex": "20aa",
                "state_fields": [{"name": "owner", "value": "aa"}]
            }]
        });
        let coin: KascovCoin = serde_json::from_value(raw).unwrap();
        validate_coin(&covenant_id, &coin).unwrap();
        assert_eq!(coin.utxos[0].state_fields[0].name, "owner");

        let mut inconsistent = coin;
        inconsistent.live_value = 26;
        assert!(validate_coin(&covenant_id, &inconsistent).is_err());
    }
}
