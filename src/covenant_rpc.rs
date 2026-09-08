//! JSON-RPC surface for integrators. Backed by TN10 REST + kascov.
//!
//! This is **not** kaspad. kaspad still has no `getUtxosByCovenantId`.
//! Do not point a mainnet wallet at this bind.

use crate::error::{EngineError, Result};
use crate::kascov::KascovClient;
use crate::network::{
    CIRCLE_USDC_ON_GALLEON, CIRCLE_USDC_ON_IGRA_MAINNET, GALLEON_GTEST, GALLEON_TEST_USDC,
    IGRA_GALLEON_CHAIN_ID, IGRA_MAINNET_CHAIN_ID, IGRA_MAINNET_HYPERLANE_USDC, KASCOV_TN10,
    TESTNET_10_REST, TN10_INTEGRATOR_RPC,
};
use crate::rest::{AddressUtxo, BlockDagInfo, Tn10RestClient};
use crate::rpc::{JsonRpcError, JsonRpcRequest, JsonRpcResponse};
use serde_json::{json, Value};

pub const IMPLEMENTATION: &str = "kaspa-frontier-engine (REST+kascov; not kaspad)";
pub const MAX_RPC_ADDRESSES: usize = 100;
pub const MAX_RPC_ADDRESS_BYTES: usize = 128;

pub fn parse_covenant_id(params: &Value) -> Result<String> {
    let object = params.as_object().ok_or_else(|| {
        EngineError::Message("params must be exactly {\"covenantId\":\"<64 hex>\"}".into())
    })?;
    if object.len() != 1 {
        return Err(EngineError::Message(
            "covenant params contain unexpected fields".into(),
        ));
    }
    let id = object
        .get("covenantId")
        .and_then(Value::as_str)
        .ok_or_else(|| EngineError::Message("covenantId must be a string".into()))?;
    exact_covenant_id(id)
}

fn exact_covenant_id(s: &str) -> Result<String> {
    let t = s.trim();
    if t.len() != 64 || !t.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(EngineError::Message(
            "covenantId must be exactly 32 bytes of hex".into(),
        ));
    }
    Ok(t.to_ascii_lowercase())
}

pub fn parse_addresses(params: &Value) -> Result<Vec<String>> {
    let object = params
        .as_object()
        .ok_or_else(|| EngineError::Message("params must be {addresses:[…]}".into()))?;
    if object.len() != 1 {
        return Err(EngineError::Message(
            "address params contain unexpected fields".into(),
        ));
    }
    let arr = object
        .get("addresses")
        .and_then(Value::as_array)
        .ok_or_else(|| EngineError::Message("addresses must be an array".into()))?;
    if arr.is_empty() {
        return Err(EngineError::Message("addresses array is empty".into()));
    }
    if arr.len() > MAX_RPC_ADDRESSES {
        return Err(EngineError::Message(format!(
            "addresses exceeds limit {MAX_RPC_ADDRESSES}"
        )));
    }
    let mut out = Vec::with_capacity(arr.len());
    let mut unique = std::collections::HashSet::with_capacity(arr.len());
    for value in arr {
        let address = value
            .as_str()
            .map(str::trim)
            .filter(|address| !address.is_empty())
            .ok_or_else(|| EngineError::Message("every address must be a string".into()))?;
        if !crate::network::is_testnet_address(address) {
            return Err(EngineError::NotTestnetAddress(address.to_string()));
        }
        if address.len() > MAX_RPC_ADDRESS_BYTES {
            return Err(EngineError::Message(format!(
                "address exceeds {MAX_RPC_ADDRESS_BYTES} bytes"
            )));
        }
        if !unique.insert(address) {
            return Err(EngineError::Message("duplicate address".into()));
        }
        out.push(address.to_string());
    }
    Ok(out)
}

fn require_no_params(params: &Value) -> Result<()> {
    if params.as_array().is_some_and(Vec::is_empty) {
        Ok(())
    } else {
        Err(EngineError::Message("method requires params: []".into()))
    }
}

pub fn info_result() -> Value {
    json!({
        "implementation": IMPLEMENTATION,
        "bind": TN10_INTEGRATOR_RPC,
        "rest": TESTNET_10_REST,
        "kascov": KASCOV_TN10,
        "galleonChainId": IGRA_GALLEON_CHAIN_ID,
        "galleonTestUsdc": GALLEON_TEST_USDC,
        "galleonGtest": GALLEON_GTEST,
        "circleUsdcOnGalleon": CIRCLE_USDC_ON_GALLEON,
        "circleListsGalleon": false,
        "igraMainnetChainId": IGRA_MAINNET_CHAIN_ID,
        "igraMainnetHyperlaneUsdc": IGRA_MAINNET_HYPERLANE_USDC,
        "circleUsdcOnIgraMainnet": CIRCLE_USDC_ON_IGRA_MAINNET,
        "circleListsIgraMainnet": false,
        "circleUsdcEthereum": crate::circle::CIRCLE_USDC_ETHEREUM,
        "notKaspad": true,
        "methods": [
            "getInfo",
            "getBlockDagInfo",
            "getUtxosByAddresses",
            "getUtxosByCovenantId",
            "getCovenant"
        ]
    })
}

pub fn jsonrpc_error(id: Option<u64>, code: i32, message: String) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: Some("2.0".into()),
        id,
        result: None,
        error: Some(JsonRpcError { code, message }),
    }
}

pub fn jsonrpc_ok(id: Option<u64>, result: Value) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: Some("2.0".into()),
        id,
        result: Some(result),
        error: None,
    }
}

pub async fn dispatch(
    rest: &Tn10RestClient,
    kascov: &KascovClient,
    req: JsonRpcRequest,
) -> JsonRpcResponse {
    let id = Some(req.id);
    if req.jsonrpc != "2.0" {
        return jsonrpc_error(id, -32600, "jsonrpc must be exactly \"2.0\"".into());
    }
    match req.method.as_str() {
        "getInfo" => match require_no_params(&req.params) {
            Ok(()) => jsonrpc_ok(id, info_result()),
            Err(error) => jsonrpc_error(id, -32602, error.to_string()),
        },
        "getBlockDagInfo" => match require_no_params(&req.params) {
            Err(error) => jsonrpc_error(id, -32602, error.to_string()),
            Ok(()) => match rest.block_dag_info().await {
                Ok(dag) => jsonrpc_ok(id, dag_json(&dag)),
                Err(e) => jsonrpc_error(id, -32000, e.to_string()),
            },
        },
        "getUtxosByAddresses" => match parse_addresses(&req.params) {
            Err(e) => jsonrpc_error(id, -32602, e.to_string()),
            Ok(addrs) => match rest.utxos_for_addresses(&addrs).await {
                Ok(utxos) => jsonrpc_ok(
                    id,
                    json!({ "entries": utxos.iter().map(utxo_json).collect::<Vec<_>>(), "backend": "tn10-rest" }),
                ),
                Err(e) => jsonrpc_error(id, -32000, e.to_string()),
            },
        },
        "getCovenant" => match parse_covenant_id(&req.params) {
            Err(e) => jsonrpc_error(id, -32602, e.to_string()),
            Ok(cid) => match kascov.snapshot(&cid).await {
                Ok((coin, utxos)) => match serde_json::to_value(&coin) {
                    Ok(mut v) => {
                        if let Some(obj) = v.as_object_mut() {
                            obj.insert("backend".into(), json!("kascov"));
                            obj.insert("coinUrl".into(), json!(kascov.coin_url(&cid)));
                            obj.insert("utxosUrl".into(), json!(kascov.utxos_url(&cid)));
                            obj.insert("utxos".into(), json!(utxos));
                            obj.insert("utxosVerified".into(), json!(false));
                            obj.insert(
                                "utxosTrust".into(),
                                json!("typed community-indexer data; not consensus or custody evidence"),
                            );
                        }
                        jsonrpc_ok(id, v)
                    }
                    Err(e) => jsonrpc_error(id, -32603, e.to_string()),
                },
                Err(e) => jsonrpc_error(id, -32000, e.to_string()),
            },
        },
        "getUtxosByCovenantId" => match parse_covenant_id(&req.params) {
            Err(e) => jsonrpc_error(id, -32602, e.to_string()),
            Ok(cid) => {
                let (utxos, dag) = tokio::join!(kascov.utxos(&cid), rest.block_dag_info());
                match utxos {
                    Ok(utxos) => jsonrpc_ok(
                        id,
                        json!({
                            "covenantId": cid,
                            "utxos": utxos,
                            "virtualDaaScore": dag.as_ref().ok().map(|d| d.virtual_daa_score.to_string()),
                            "backend": "kascov",
                            "verified": false,
                            "trust": "community-indexer; not consensus or custody evidence",
                            "daaBackend": if dag.is_ok() { "tn10-rest" } else { "unavailable" },
                            "utxosUrl": kascov.utxos_url(&cid),
                            "note": "Not kaspad. kaspad has no getUtxosByCovenantId."
                        }),
                    ),
                    Err(e) => jsonrpc_error(id, -32000, e.to_string()),
                }
            }
        },
        other => jsonrpc_error(id, -32601, format!("unknown method {other}")),
    }
}

fn dag_json(dag: &BlockDagInfo) -> Value {
    json!({
        "networkName": dag.network_name,
        "blockCount": dag.block_count,
        "headerCount": dag.header_count,
        "tipHashes": dag.tip_hashes,
        "difficulty": dag.difficulty,
        "pastMedianTime": dag.past_median_time,
        "virtualParentHashes": dag.virtual_parent_hashes,
        "pruningPointHash": dag.pruning_point_hash,
        "virtualDaaScore": dag.virtual_daa_score,
        "sink": dag.sink,
        "backend": "tn10-rest"
    })
}

fn utxo_json(u: &AddressUtxo) -> Value {
    let mut value = json!({
        "address": u.address,
        "outpoint": {
            "transactionId": u.outpoint.transaction_id,
            "index": u.outpoint.index
        },
        "utxoEntry": {
            "amount": u.utxo_entry.amount.to_string(),
            "scriptPublicKey": {
                "scriptPublicKey": u.utxo_entry.script_public_key.script_public_key,
                "version": u.utxo_entry.script_public_key.version
            },
            "blockDaaScore": u.utxo_entry.block_daa_score.to_string(),
            "isCoinbase": u.utxo_entry.is_coinbase
        }
    });
    let entry = value["utxoEntry"]
        .as_object_mut()
        .expect("utxoEntry is constructed as an object");
    if let Some(covenant_id) = &u.utxo_entry.covenant_id {
        entry.insert("covenantId".into(), json!(covenant_id));
    }
    if let Some(storage_mass) = u.utxo_entry.storage_mass {
        entry.insert("storageMass".into(), json!(storage_mass.to_string()));
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn covenant_id_params_are_exact() {
        let id = "a".repeat(64);
        assert_eq!(
            parse_covenant_id(&json!({"covenantId": id.clone()})).unwrap(),
            id
        );
        assert!(parse_covenant_id(&json!("abcd")).is_err());
        assert!(parse_covenant_id(&json!({"covenant_id": id.clone()})).is_err());
        assert!(parse_covenant_id(&json!({"covenantId": id, "extra": 1})).is_err());
        assert!(parse_covenant_id(&json!({})).is_err());
    }

    #[test]
    fn addresses_are_strict_tn10_and_bounded() {
        assert_eq!(
            parse_addresses(&json!({"addresses":["kaspatest:abc"]})).unwrap(),
            vec!["kaspatest:abc"]
        );
        assert!(parse_addresses(&json!(["kaspatest:abc"])).is_err());
        assert!(parse_addresses(&json!({"addresses":["kaspatest:abc"], "extra": 1})).is_err());
        assert!(parse_addresses(&json!({"addresses":["kaspatest:abc", 7]})).is_err());
        assert!(parse_addresses(&json!({"addresses":["kaspa:mainnet"]})).is_err());
        assert!(parse_addresses(&json!({"addresses":["kaspatest:abc", "kaspatest:abc"]})).is_err());
        let oversized = format!("kaspatest:{}", "a".repeat(MAX_RPC_ADDRESS_BYTES));
        assert!(parse_addresses(&json!({"addresses":[oversized]})).is_err());
        let too_many = vec!["kaspatest:abc"; MAX_RPC_ADDRESSES + 1];
        assert!(parse_addresses(&json!({"addresses":too_many})).is_err());
    }

    #[test]
    fn utxo_json_conditionally_preserves_toccata_fields() {
        let with_fields: AddressUtxo = serde_json::from_value(json!({
            "address": "kaspatest:abc",
            "outpoint": {"transactionId": "aa", "index": 0},
            "utxoEntry": {
                "amount": "1",
                "scriptPublicKey": {"scriptPublicKey": "00", "version": 0},
                "blockDaaScore": "2",
                "isCoinbase": false,
                "covenantId": "cc",
                "storageMass": "3"
            }
        }))
        .unwrap();
        let value = utxo_json(&with_fields);
        assert_eq!(value["utxoEntry"]["covenantId"], "cc");
        assert_eq!(value["utxoEntry"]["storageMass"], "3");

        let legacy: AddressUtxo = serde_json::from_value(json!({
            "address": "kaspatest:abc",
            "outpoint": {"transactionId": "aa", "index": 0},
            "utxoEntry": {
                "amount": "1",
                "scriptPublicKey": {"scriptPublicKey": "00", "version": 0},
                "blockDaaScore": "2",
                "isCoinbase": false
            }
        }))
        .unwrap();
        let value = utxo_json(&legacy);
        assert!(value["utxoEntry"].get("covenantId").is_none());
        assert!(value["utxoEntry"].get("storageMass").is_none());
    }

    #[test]
    fn info_says_not_kaspad_and_no_circle() {
        let v = info_result();
        assert_eq!(v["notKaspad"], true);
        assert!(v["circleUsdcOnGalleon"].is_null());
        assert_eq!(v["circleListsGalleon"], false);
        assert!(v["circleUsdcOnIgraMainnet"].is_null());
        assert_eq!(v["circleListsIgraMainnet"], false);
        assert_eq!(v["circleUsdcEthereum"], crate::circle::CIRCLE_USDC_ETHEREUM);
        assert_eq!(v["galleonTestUsdc"], GALLEON_TEST_USDC);
        assert_eq!(v["galleonGtest"], GALLEON_GTEST);
        assert_eq!(v["igraMainnetHyperlaneUsdc"], IGRA_MAINNET_HYPERLANE_USDC);
        assert!(IMPLEMENTATION.contains("not kaspad"));
    }

    #[test]
    fn unknown_method_is_jsonrpc_error() {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: 1,
            method: "submitBlock".into(),
            params: json!([]),
        };
        let rest = Tn10RestClient::new("https://example.invalid").unwrap();
        let kascov = KascovClient::new("https://example.invalid").unwrap();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let resp = rt.block_on(dispatch(&rest, &kascov, req));
        assert!(resp.error.unwrap().message.contains("unknown method"));
    }

    #[test]
    fn rejects_wrong_version_and_unexpected_noarg_params() {
        let rest = Tn10RestClient::new("https://example.invalid").unwrap();
        let kascov = KascovClient::new("https://example.invalid").unwrap();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let wrong_version = JsonRpcRequest {
            jsonrpc: "1.0".into(),
            id: 1,
            method: "getInfo".into(),
            params: json!([]),
        };
        assert_eq!(
            rt.block_on(dispatch(&rest, &kascov, wrong_version))
                .error
                .unwrap()
                .code,
            -32600
        );
        let extra = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: 2,
            method: "getInfo".into(),
            params: json!([1]),
        };
        assert_eq!(
            rt.block_on(dispatch(&rest, &kascov, extra))
                .error
                .unwrap()
                .code,
            -32602
        );
    }

    #[test]
    fn empty_addresses_fetch_errors_without_http() {
        let rest = Tn10RestClient::new("https://example.invalid").unwrap();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let err = rt.block_on(rest.utxos_for_addresses(&[])).unwrap_err();
        assert!(err.to_string().contains("empty"));
    }
}
