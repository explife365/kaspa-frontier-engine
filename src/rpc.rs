//! kaspad JSON-RPC *shapes* for `getUtxosByAddresses`.
//!
//! `Kaspa to do 2.pdf` (Gemini) then ships a fake gRPC marshaller (JSON length-prefix),
//! a GHOSTDAG rewrite, compact-bits DAA, XOR “UTXO root”, and a Schnorr check that
//! returns Ok. Those do not belong here. Talk to a real node: REST (this crate) or
//! kaspad wrpc-json (`--rpclisten-json=default`, TN10 18210).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: u64,
    pub method: String,
    pub params: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    #[serde(default)]
    pub jsonrpc: Option<String>,
    pub id: Option<u64>,
    pub result: Option<serde_json::Value>,
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GetUtxosByAddressesParams {
    pub addresses: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RpcOutpoint {
    pub transaction_id: String,
    pub index: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RpcScriptPublicKey {
    pub script_public_key: Option<String>,
    #[serde(default)]
    pub version: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RpcUtxoEntry {
    #[serde(deserialize_with = "crate::rest::de_u64_from_string_or_number")]
    pub amount: u64,
    pub script_public_key: RpcScriptPublicKey,
    #[serde(deserialize_with = "crate::rest::de_u64_from_string_or_number")]
    pub block_daa_score: u64,
    pub is_coinbase: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub covenant_id: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::rest::de_opt_u64_from_string_or_number"
    )]
    pub storage_mass: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RpcUtxoEntryRef {
    #[serde(default)]
    pub address: Option<String>,
    pub outpoint: RpcOutpoint,
    pub utxo_entry: RpcUtxoEntry,
}

pub fn encode_get_utxos_by_addresses(
    id: u64,
    addresses: &[String],
) -> Result<String, serde_json::Error> {
    let params = GetUtxosByAddressesParams {
        addresses: addresses.to_vec(),
    };
    let request = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id,
        method: "getUtxosByAddresses".to_string(),
        params: serde_json::to_value(params)?,
    };
    serde_json::to_string(&request)
}

/// Accepts `result` as `{ "entries": [...] }` or a bare array (explorer vs wrpc).
pub fn decode_utxos_by_addresses(raw: &str) -> Result<Vec<RpcUtxoEntryRef>, String> {
    let response: JsonRpcResponse =
        serde_json::from_str(raw).map_err(|e| format!("JSON-RPC parse failed: {e}"))?;
    if let Some(err) = response.error {
        return Err(format!("node error [{}]: {}", err.code, err.message));
    }
    let value = response
        .result
        .ok_or_else(|| "empty JSON-RPC result".to_string())?;
    if let Some(entries) = value.get("entries") {
        serde_json::from_value(entries.clone()).map_err(|e| format!("entries mapping failed: {e}"))
    } else {
        serde_json::from_value(value).map_err(|e| format!("UTXO array mapping failed: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_get_utxos_by_addresses() {
        let body = encode_get_utxos_by_addresses(7, &["kaspatest:qqexample".into()]).unwrap();
        assert!(body.contains("getUtxosByAddresses"));
        assert!(body.contains("kaspatest:qqexample"));
        assert!(body.contains("\"id\":7"));
    }

    #[test]
    fn decodes_wrapped_entries() {
        let raw = r#"{
            "jsonrpc":"2.0","id":1,
            "result":{"entries":[{
                "address":"kaspatest:qqexample",
                "outpoint":{"transactionId":"aa","index":0},
                "utxoEntry":{
                    "amount":"1000",
                    "scriptPublicKey":{"scriptPublicKey":"20ab","version":0},
                    "blockDaaScore":"9",
                    "isCoinbase":false,
                    "covenantId":"cc",
                    "storageMass":"1234"
                }
            }]}
        }"#;
        let rows = decode_utxos_by_addresses(raw).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].utxo_entry.amount, 1000);
        assert_eq!(rows[0].outpoint.index, 0);
        assert_eq!(rows[0].utxo_entry.covenant_id.as_deref(), Some("cc"));
        assert_eq!(rows[0].utxo_entry.storage_mass, Some(1234));
    }

    #[test]
    fn surfaces_node_error() {
        let raw = r#"{"id":1,"error":{"code":-32000,"message":"not synced"}}"#;
        let err = decode_utxos_by_addresses(raw).unwrap_err();
        assert!(err.contains("not synced"));
    }

    #[test]
    fn request_rejects_unknown_fields() {
        let raw = r#"{"jsonrpc":"2.0","id":1,"method":"getInfo","params":[],"unexpected":true}"#;
        assert!(serde_json::from_str::<JsonRpcRequest>(raw).is_err());
    }
}
