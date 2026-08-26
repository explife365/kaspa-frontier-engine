//! Public explorer REST — works without a local kaspad.
//! TN10: https://api-tn10.kaspa.org

use crate::error::{EngineError, Result};
use crate::network::{require_tn10, TESTNET_10_REST};
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, ACCEPT_ENCODING};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use std::time::Duration;

const HTTP_TIMEOUT: Duration = Duration::from_secs(12);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const HTTP_ATTEMPTS: u32 = 3;
const HTTP_RETRY_BASE: Duration = Duration::from_millis(150);
pub const MAX_ADDRESS_BATCH: usize = 100;
const ADDRESS_CONCURRENCY: usize = 8;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockDagInfo {
    pub network_name: String,
    #[serde(deserialize_with = "de_u64_from_string_or_number")]
    pub block_count: u64,
    #[serde(deserialize_with = "de_u64_from_string_or_number")]
    pub header_count: u64,
    pub tip_hashes: Vec<String>,
    pub difficulty: f64,
    #[serde(deserialize_with = "de_u64_from_string_or_number")]
    pub past_median_time: u64,
    pub virtual_parent_hashes: Vec<String>,
    pub pruning_point_hash: String,
    #[serde(deserialize_with = "de_u64_from_string_or_number")]
    pub virtual_daa_score: u64,
    pub sink: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Outpoint {
    pub transaction_id: String,
    pub index: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptPublicKey {
    #[serde(alias = "script")]
    pub script_public_key: Option<String>,
    #[serde(default)]
    pub version: u16,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UtxoEntry {
    #[serde(deserialize_with = "de_u64_from_string_or_number")]
    pub amount: u64,
    pub script_public_key: ScriptPublicKey,
    #[serde(deserialize_with = "de_u64_from_string_or_number")]
    pub block_daa_score: u64,
    pub is_coinbase: bool,
    #[serde(default, alias = "covenant_id")]
    pub covenant_id: Option<String>,
    #[serde(
        default,
        alias = "storage_mass",
        deserialize_with = "de_opt_u64_from_string_or_number"
    )]
    pub storage_mass: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HashrateInfo {
    /// Kaspa REST returns terahashes per second.
    pub hashrate: f64,
}

impl HashrateInfo {
    pub fn hashes_per_second(&self) -> f64 {
        self.hashrate * 1e12
    }
}

/// Explorer `/info/fee-estimate` — what pools/exchanges were asked to rehearse.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeeBucket {
    pub feerate: f64,
    pub estimated_seconds: f64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeeEstimate {
    pub priority_bucket: FeeBucket,
    #[serde(default)]
    pub normal_buckets: Vec<FeeBucket>,
    #[serde(default)]
    pub low_buckets: Vec<FeeBucket>,
}

impl FeeEstimate {
    pub fn priority_feerate(&self) -> f64 {
        self.priority_bucket.feerate
    }

    /// Minimum standard mempool/RPC policy target; not a consensus rule.
    pub fn meets_standard_relay_rate(&self) -> bool {
        self.priority_feerate() + 1e-9 >= 100.0
    }

    /// Named buckets under the standard 100 sompi/gram relay-policy target.
    pub fn buckets_below_standard_rate(&self) -> Vec<(&'static str, f64)> {
        let mut out = Vec::new();
        if self.priority_feerate() + 1e-9 < 100.0 {
            out.push(("priority", self.priority_feerate()));
        }
        if let Some(normal) = self.normal_buckets.first() {
            if normal.feerate + 1e-9 < 100.0 {
                out.push(("normal", normal.feerate));
            }
        }
        if let Some(low) = self.low_buckets.first() {
            if low.feerate + 1e-9 < 100.0 {
                out.push(("low", low.feerate));
            }
        }
        out
    }
}

/// Concurrent DAG + hashrate + fee snapshot for `tn10-status`.
#[derive(Debug, Clone)]
pub struct StatusSnapshot {
    pub dag: BlockDagInfo,
    pub hashrate: Option<HashrateInfo>,
    pub fee: Option<FeeEstimate>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddressUtxo {
    pub address: String,
    pub outpoint: Outpoint,
    pub utxo_entry: UtxoEntry,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AddressBalance {
    pub address: String,
    #[serde(deserialize_with = "de_u64_from_string_or_number")]
    pub balance: u64,
}

/// Explorer REST transaction after Toccata (v1 fields integrators were asked to parse).
/// Live explorer uses snake_case; some other Kaspa REST payloads use camelCase.
#[derive(Debug, Clone, Deserialize)]
pub struct ToccataTx {
    #[serde(alias = "transactionId")]
    pub transaction_id: String,
    #[serde(default)]
    pub version: u32,
    #[serde(default, alias = "isAccepted")]
    pub is_accepted: bool,
    #[serde(
        default,
        alias = "mass",
        alias = "storageMass",
        deserialize_with = "de_opt_u64_from_string_or_number"
    )]
    pub storage_mass: Option<u64>,
    #[serde(default)]
    pub inputs: Vec<ToccataTxInput>,
    #[serde(default)]
    pub outputs: Vec<ToccataTxOutput>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ToccataTxInput {
    #[serde(default, alias = "computeBudget")]
    pub compute_budget: Option<u64>,
    #[serde(default, alias = "covenantId")]
    pub covenant_id: Option<String>,
    #[serde(default, alias = "previousOutpointHash")]
    pub previous_outpoint_hash: Option<String>,
    #[serde(
        default,
        alias = "previousOutpointIndex",
        deserialize_with = "de_opt_u32_from_string_or_number"
    )]
    pub previous_outpoint_index: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ToccataTxOutput {
    #[serde(default, deserialize_with = "de_opt_u64_from_string_or_number")]
    pub amount: Option<u64>,
    #[serde(default, alias = "covenantId")]
    pub covenant_id: Option<String>,
    #[serde(default, alias = "covenantAuthorizingInput")]
    pub covenant_authorizing_input: Option<u32>,
    #[serde(default, alias = "scriptPublicKeyType")]
    pub script_public_key_type: Option<String>,
}

impl ToccataTx {
    pub fn output_covenant_id(&self) -> Option<&str> {
        self.outputs.iter().find_map(|o| o.covenant_id.as_deref())
    }

    pub fn input_covenant_id(&self) -> Option<&str> {
        self.inputs.iter().find_map(|i| i.covenant_id.as_deref())
    }
}

pub(crate) fn de_u64_from_string_or_number<'de, D>(
    deserializer: D,
) -> std::result::Result<u64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    parse_u64_value(serde_json::Value::deserialize(deserializer)?)
}

pub(crate) fn de_opt_u64_from_string_or_number<'de, D>(
    deserializer: D,
) -> std::result::Result<Option<u64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    if value.is_null() {
        return Ok(None);
    }
    parse_u64_value(value).map(Some)
}

fn de_opt_u32_from_string_or_number<'de, D>(
    deserializer: D,
) -> std::result::Result<Option<u32>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    de_opt_u64_from_string_or_number(deserializer)?
        .map(|value| u32::try_from(value).map_err(serde::de::Error::custom))
        .transpose()
}

fn parse_u64_value<E: serde::de::Error>(value: serde_json::Value) -> std::result::Result<u64, E> {
    match value {
        serde_json::Value::Number(n) => n
            .as_u64()
            .ok_or_else(|| E::custom("numeric field out of u64 range")),
        serde_json::Value::String(s) => s.parse::<u64>().map_err(E::custom),
        other => Err(E::custom(format!("expected u64, got {other}"))),
    }
}

/// Encode a path segment so `kaspa:` / `kaspatest:` colons are not treated as URL syntax.
pub fn encode_path_segment(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for b in value.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn should_retry_unencoded(status: u16) -> bool {
    matches!(status, 400 | 403 | 404)
}

fn is_transient_status(status: u16) -> bool {
    matches!(status, 408 | 425 | 429 | 500 | 502 | 503 | 504)
}

/// Honor Retry-After on 429/503, but cap at 2s so CEX snapshots cannot stall.
pub(crate) fn retry_after_wait(retry_after: Option<&str>, attempt: u32) -> Duration {
    let backoff = HTTP_RETRY_BASE * 2u32.pow(attempt);
    if let Some(text) = retry_after {
        if let Ok(secs) = text.trim().parse::<u64>() {
            return Duration::from_secs(secs.min(2)).max(backoff);
        }
    }
    backoff
}

pub(crate) fn require_https(base: &str) -> Result<()> {
    if base.starts_with("https://") {
        Ok(())
    } else {
        Err(EngineError::InsecureTransport(base.to_string()))
    }
}

pub(crate) fn https_json_client() -> Result<reqwest::Client> {
    let mut headers = HeaderMap::new();
    headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
    headers.insert(ACCEPT_ENCODING, HeaderValue::from_static("identity"));
    Ok(reqwest::Client::builder()
        .user_agent(concat!("kaspa-frontier-engine/", env!("CARGO_PKG_VERSION")))
        .default_headers(headers)
        // TN10 / explorer nodes often mishandle Content-Encoding:gzip
        // (truncated or double-compressed bodies). Prefer identity.
        .gzip(false)
        .http2_adaptive_window(true)
        .tcp_nodelay(true)
        .tcp_keepalive(Duration::from_secs(30))
        .pool_idle_timeout(Duration::from_secs(45))
        .pool_max_idle_per_host(8)
        .timeout(HTTP_TIMEOUT)
        .connect_timeout(CONNECT_TIMEOUT)
        .build()?)
}

pub(crate) async fn http_send(
    http: &reqwest::Client,
    method: reqwest::Method,
    url: &str,
    json_body: Option<&serde_json::Value>,
) -> Result<reqwest::Response> {
    let mut last: Option<EngineError> = None;
    for attempt in 0..HTTP_ATTEMPTS {
        let mut req = http.request(method.clone(), url);
        if let Some(body) = json_body {
            req = req.json(body);
        }
        match req.send().await {
            Ok(resp) => {
                let code = resp.status().as_u16();
                if resp.status().is_success() || code == 404 || !is_transient_status(code) {
                    return Ok(resp);
                }
                let wait = retry_after_wait(
                    resp.headers()
                        .get("retry-after")
                        .and_then(|v| v.to_str().ok()),
                    attempt,
                );
                last = Some(EngineError::Message(format!("HTTP {code} for {url}")));
                drop(resp);
                if attempt + 1 == HTTP_ATTEMPTS {
                    break;
                }
                tokio::time::sleep(wait).await;
            }
            Err(err) => {
                let retry = err.is_timeout() || err.is_connect() || err.is_request();
                if !retry || attempt + 1 == HTTP_ATTEMPTS {
                    return Err(err.into());
                }
                last = Some(err.into());
                tokio::time::sleep(HTTP_RETRY_BASE * 2u32.pow(attempt)).await;
            }
        }
    }
    Err(last.unwrap_or_else(|| EngineError::Message(format!("{method} failed for {url}"))))
}

pub(crate) async fn http_send_get(http: &reqwest::Client, url: &str) -> Result<reqwest::Response> {
    http_send(http, reqwest::Method::GET, url, None).await
}

/// Same Retry-After cap and reconnect budget as GET. Rebuilds the JSON body per attempt.
pub(crate) async fn http_send_post(
    http: &reqwest::Client,
    url: &str,
    body: &serde_json::Value,
) -> Result<reqwest::Response> {
    http_send(http, reqwest::Method::POST, url, Some(body)).await
}

#[derive(Clone)]
pub struct Tn10RestClient {
    http: reqwest::Client,
    base: String,
}

impl Default for Tn10RestClient {
    fn default() -> Self {
        Self::new(TESTNET_10_REST).expect("reqwest TLS client")
    }
}

impl Tn10RestClient {
    pub fn new(base: impl Into<String>) -> Result<Self> {
        let base = base.into().trim_end_matches('/').to_string();
        require_https(&base)?;
        Ok(Self {
            http: https_json_client()?,
            base,
        })
    }

    pub fn from_http(base: impl Into<String>, http: reqwest::Client) -> Result<Self> {
        let base = base.into().trim_end_matches('/').to_string();
        require_https(&base)?;
        Ok(Self { http, base })
    }

    pub fn base_url(&self) -> &str {
        &self.base
    }

    async fn send_get(&self, url: &str) -> Result<reqwest::Response> {
        http_send_get(&self.http, url).await
    }

    async fn get_json<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = format!("{}{path}", self.base);
        Ok(self
            .send_get(&url)
            .await?
            .error_for_status()?
            .json()
            .await?)
    }

    async fn get_json_encoded_or_raw<T: DeserializeOwned>(
        &self,
        encoded_url: String,
        raw_url: String,
    ) -> Result<T> {
        let encoded_resp = self.send_get(&encoded_url).await?;
        if encoded_resp.status().is_success() {
            return Ok(encoded_resp.json().await?);
        }
        if should_retry_unencoded(encoded_resp.status().as_u16()) {
            return Ok(self
                .send_get(&raw_url)
                .await?
                .error_for_status()?
                .json()
                .await?);
        }
        encoded_resp.error_for_status()?;
        Err(EngineError::Message(format!(
            "GET failed for {encoded_url}"
        )))
    }

    /// Two observational DAA samples. This rate does not identify consensus protocol.
    pub async fn sample_daa_rate(&self, wait: std::time::Duration) -> Result<(u64, u64, f64)> {
        let first = self.block_dag_info().await?;
        let started = std::time::Instant::now();
        tokio::time::sleep(wait).await;
        let second = self.block_dag_info().await?;
        let dt = started.elapsed().as_secs_f64().max(0.001);
        let delta = second
            .virtual_daa_score
            .saturating_sub(first.virtual_daa_score) as f64;
        Ok((
            first.virtual_daa_score,
            second.virtual_daa_score,
            delta / dt,
        ))
    }

    pub async fn block_dag_info(&self) -> Result<BlockDagInfo> {
        let info: BlockDagInfo = self.get_json("/info/blockdag").await?;
        require_tn10(&info.network_name)?;
        Ok(info)
    }

    pub async fn hashrate(&self) -> Result<HashrateInfo> {
        self.get_json("/info/hashrate").await
    }

    pub async fn fee_estimate(&self) -> Result<FeeEstimate> {
        self.get_json("/info/fee-estimate").await
    }

    /// DAG info, hashrate, and fee estimate in one round of concurrent requests.
    pub async fn status_snapshot(&self) -> Result<StatusSnapshot> {
        let (dag, hr, fee) =
            tokio::join!(self.block_dag_info(), self.hashrate(), self.fee_estimate());
        Ok(StatusSnapshot {
            dag: dag?,
            hashrate: hr.ok(),
            fee: fee.ok(),
        })
    }

    /// Returns Ok(None) when the explorer has no such tx (404).
    pub async fn transaction(&self, txid: &str) -> Result<Option<serde_json::Value>> {
        let encoded = format!("{}/transactions/{}", self.base, encode_path_segment(txid));
        let resp = self.send_get(&encoded).await?;
        if resp.status().as_u16() == 404 {
            return Ok(None);
        }
        if resp.status().is_success() {
            return Ok(Some(resp.json().await?));
        }
        if should_retry_unencoded(resp.status().as_u16()) {
            let raw = format!("{}/transactions/{txid}", self.base);
            let fallback = self.send_get(&raw).await?;
            if fallback.status().as_u16() == 404 {
                return Ok(None);
            }
            return Ok(Some(fallback.error_for_status()?.json().await?));
        }
        resp.error_for_status()?;
        Err(EngineError::Message(format!("tx fetch failed for {txid}")))
    }

    pub async fn toccata_tx(&self, txid: &str) -> Result<Option<ToccataTx>> {
        match self.transaction(txid).await? {
            None => Ok(None),
            Some(value) => Ok(Some(serde_json::from_value(value)?)),
        }
    }

    /// REST stand-in for kaspad `getUtxosByAddresses`. Confirm with
    /// `utxo_entry.block_daa_score` vs virtual DAA — not a simulated indexer.
    pub async fn utxos_for_address(&self, address: &str) -> Result<Vec<AddressUtxo>> {
        self.get_json_encoded_or_raw(
            format!(
                "{}/addresses/{}/utxos",
                self.base,
                encode_path_segment(address)
            ),
            format!("{}/addresses/{}/utxos", self.base, address),
        )
        .await
    }

    /// Concurrent REST lookups for CEX-style multi-address snapshots.
    pub async fn utxos_for_addresses(&self, addrs: &[String]) -> Result<Vec<AddressUtxo>> {
        if addrs.len() > MAX_ADDRESS_BATCH {
            return Err(EngineError::Message(format!(
                "addresses exceeds limit {MAX_ADDRESS_BATCH}"
            )));
        }
        if addrs.len() <= 1 {
            let Some(addr) = addrs.first() else {
                return Err(EngineError::Message("addresses array is empty".into()));
            };
            return self.utxos_for_address(addr).await;
        }
        let n = addrs.len();
        let mut slots: Vec<Option<Result<Vec<AddressUtxo>>>> = (0..n).map(|_| None).collect();
        for (chunk_index, chunk) in addrs.chunks(ADDRESS_CONCURRENCY).enumerate() {
            let mut set = tokio::task::JoinSet::new();
            for (offset, addr) in chunk.iter().cloned().enumerate() {
                let i = chunk_index * ADDRESS_CONCURRENCY + offset;
                let rest = self.clone();
                set.spawn(async move { (i, rest.utxos_for_address(&addr).await) });
            }
            while let Some(joined) = set.join_next().await {
                match joined {
                    Ok((i, result)) => {
                        if let Some(slot) = slots.get_mut(i) {
                            *slot = Some(result);
                        }
                    }
                    Err(e) => return Err(EngineError::Message(format!("address join: {e}"))),
                }
            }
        }
        let mut out = Vec::new();
        for slot in slots {
            match slot {
                Some(Ok(utxos)) => out.extend(utxos),
                Some(Err(e)) => return Err(e),
                None => return Err(EngineError::Message("missing address UTXO result".into())),
            }
        }
        Ok(out)
    }

    pub async fn balance_for_address(&self, address: &str) -> Result<AddressBalance> {
        self.get_json_encoded_or_raw(
            format!(
                "{}/addresses/{}/balance",
                self.base,
                encode_path_segment(address)
            ),
            format!("{}/addresses/{}/balance", self.base, address),
        )
        .await
    }

    /// Concurrent DAG + UTXO + balance snapshot for one address.
    pub async fn address_snapshot(
        &self,
        address: &str,
    ) -> Result<(BlockDagInfo, Vec<AddressUtxo>, Option<u64>)> {
        let (dag, utxos, balance) = tokio::join!(
            self.block_dag_info(),
            self.utxos_for_address(address),
            self.balance_for_address(address)
        );
        Ok((dag?, utxos?, balance.ok().map(|b| b.balance)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network::{is_tn10_network_name, is_unsupported_testnet_name};

    const LIVE_TN10_BLOCKDAG: &str = r#"{
        "networkName":"kaspa-testnet-10",
        "blockCount":"1394032",
        "headerCount":"1394032",
        "tipHashes":["8610c73bb03581ecad146c5085a7c26ce2177bcffa6ef07381e48f5149708e18"],
        "difficulty":962114.0452079439,
        "pastMedianTime":"1787419193272",
        "virtualParentHashes":["8610c73bb03581ecad146c5085a7c26ce2177bcffa6ef07381e48f5149708e18"],
        "pruningPointHash":"eefb043341b87b92ed11c59601f6b1fa564a5ee89488554c8f14a2d224ce3c11",
        "virtualDaaScore":"550603357",
        "sink":"8610c73bb03581ecad146c5085a7c26ce2177bcffa6ef07381e48f5149708e18"
    }"#;

    #[test]
    fn parses_live_tn10_stringy_u64s() {
        let info: BlockDagInfo = serde_json::from_str(LIVE_TN10_BLOCKDAG).unwrap();
        assert_eq!(info.network_name, "kaspa-testnet-10");
        assert_eq!(info.block_count, 1_394_032);
        assert_eq!(info.virtual_daa_score, 550_603_357);
        assert!((info.difficulty - 962_114.045_2).abs() < 0.01);
    }

    #[test]
    fn parses_numeric_utxo_amount() {
        let raw = r#"{
            "address":"kaspatest:qq",
            "outpoint":{"transactionId":"aa","index":1},
            "utxoEntry":{
                "amount":100000000,
                "scriptPublicKey":{"scriptPublicKey":"00","version":0},
                "blockDaaScore":"9",
                "isCoinbase":false,
                "covenantId":"cc",
                "storageMass":"1234"
            }
        }"#;
        let utxo: AddressUtxo = serde_json::from_str(raw).unwrap();
        assert_eq!(utxo.outpoint.index, 1);
        assert_eq!(utxo.utxo_entry.amount, 100_000_000);
        assert_eq!(utxo.utxo_entry.block_daa_score, 9);
        assert_eq!(utxo.utxo_entry.covenant_id.as_deref(), Some("cc"));
        assert_eq!(utxo.utxo_entry.storage_mass, Some(1234));
    }

    #[test]
    fn live_utxo_omits_script_version() {
        let raw = r#"{
            "address":"kaspatest:qq",
            "outpoint":{"transactionId":"dae7","index":0},
            "utxoEntry":{
                "amount":"25000000",
                "scriptPublicKey":{"scriptPublicKey":"20aa"},
                "blockDaaScore":"550700268",
                "isCoinbase":false
            }
        }"#;
        let utxo: AddressUtxo = serde_json::from_str(raw).unwrap();
        assert_eq!(utxo.utxo_entry.amount, 25_000_000);
        assert_eq!(utxo.utxo_entry.script_public_key.version, 0);
    }

    #[test]
    fn encodes_colon_in_address() {
        assert_eq!(encode_path_segment("kaspatest:abc"), "kaspatest%3Aabc");
    }

    #[test]
    fn retries_unencoded_on_waf_and_not_found() {
        assert!(should_retry_unencoded(400));
        assert!(should_retry_unencoded(403));
        assert!(should_retry_unencoded(404));
        assert!(!should_retry_unencoded(500));
        assert!(is_transient_status(429));
        assert!(is_transient_status(503));
        assert!(!is_transient_status(400));
        assert_eq!(retry_after_wait(None, 0), Duration::from_millis(150));
        assert_eq!(retry_after_wait(Some("1"), 0), Duration::from_secs(1));
        assert_eq!(retry_after_wait(Some("30"), 0), Duration::from_secs(2));
        assert_eq!(
            retry_after_wait(Some("nope"), 1),
            Duration::from_millis(300)
        );
    }

    #[test]
    fn rejects_cleartext_rest() {
        assert!(matches!(
            Tn10RestClient::new("http://127.0.0.1:8080"),
            Err(EngineError::InsecureTransport(_))
        ));
    }

    #[test]
    fn empty_addresses_utxos_error_without_http() {
        let rest = Tn10RestClient::new("https://example.invalid").unwrap();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let err = rt.block_on(rest.utxos_for_addresses(&[])).unwrap_err();
        assert!(err.to_string().contains("empty"));
    }

    #[test]
    fn address_batch_limit_errors_without_http() {
        let rest = Tn10RestClient::new("https://example.invalid").unwrap();
        let addresses = vec!["kaspatest:abc".to_string(); MAX_ADDRESS_BATCH + 1];
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let err = rt
            .block_on(rest.utxos_for_addresses(&addresses))
            .unwrap_err();
        assert!(err.to_string().contains("limit"));
    }

    #[test]
    fn parses_address_balance_string_or_number() {
        let n: AddressBalance =
            serde_json::from_str(r#"{"address":"kaspatest:qq","balance":200000000}"#).unwrap();
        assert_eq!(n.balance, 200_000_000);
        let s: AddressBalance =
            serde_json::from_str(r#"{"address":"kaspatest:qq","balance":"9"}"#).unwrap();
        assert_eq!(s.balance, 9);
    }

    #[test]
    fn rejects_mainnet_dag_name() {
        let mut info: BlockDagInfo = serde_json::from_str(LIVE_TN10_BLOCKDAG).unwrap();
        info.network_name = "kaspa-mainnet".into();
        assert!(!is_tn10_network_name(&info.network_name));
    }

    #[test]
    fn rejects_tn12_dag_name() {
        let mut info: BlockDagInfo = serde_json::from_str(LIVE_TN10_BLOCKDAG).unwrap();
        info.network_name = "kaspa-testnet-12".into();
        assert!(!is_tn10_network_name(&info.network_name));
        assert!(is_unsupported_testnet_name(&info.network_name));
        match require_tn10(&info.network_name) {
            Err(EngineError::UnsupportedTestnet { found }) => {
                assert!(found.contains("12"));
            }
            other => panic!("expected UnsupportedTestnet, got {other:?}"),
        }
    }

    #[tokio::test]
    #[ignore = "hits api-tn10.kaspa.org"]
    async fn live_tn10_blockdag() {
        let client = Tn10RestClient::default();
        let info = client.block_dag_info().await.unwrap();
        assert!(is_tn10_network_name(&info.network_name));
        assert!(info.virtual_daa_score > 0);
        let hr = client.hashrate().await.unwrap();
        assert!(hr.hashrate.is_finite());
        assert!(hr.hashrate >= 0.0);
        let fee = client.fee_estimate().await.unwrap();
        assert!(fee.priority_feerate().is_finite());
        assert!(fee.priority_feerate() >= 0.0);
    }

    #[test]
    fn parses_toccata_v1_fields() {
        let genesis = r#"{
            "transaction_id":"6d0acd6fcbaf68bca1568a3cbbafe0f3c1d72c4f6ea0edc6f6c013a59cb5d591",
            "version":1,
            "is_accepted":true,
            "mass":"1671",
            "inputs":[{"compute_budget":10,"covenant_id":null}],
            "outputs":[{
                "amount":198363600,
                "covenant_id":"4a95a59dc79c3f46f35db91453f26785750450606836d82c48c1affdd71ed70a",
                "covenant_authorizing_input":0,
                "script_public_key_type":"scripthash"
            }]
        }"#;
        let tx: ToccataTx = serde_json::from_str(genesis).unwrap();
        assert_eq!(tx.version, 1);
        assert!(tx.is_accepted);
        assert_eq!(tx.storage_mass, Some(1671));
        assert_eq!(tx.inputs[0].compute_budget, Some(10));
        assert!(tx.input_covenant_id().is_none());
        assert_eq!(
            tx.output_covenant_id(),
            Some("4a95a59dc79c3f46f35db91453f26785750450606836d82c48c1affdd71ed70a")
        );
        let camel: ToccataTx = serde_json::from_str(
            r#"{
            "transactionId":"bb",
            "version":1,
            "isAccepted":true,
            "mass":1671,
            "inputs":[{"computeBudget":10,"covenantId":"cc","previousOutpointHash":"aa"}],
            "outputs":[{"covenantId":"cc","covenantAuthorizingInput":0,"scriptPublicKeyType":"scripthash"}]
        }"#,
        )
        .unwrap();
        assert_eq!(camel.transaction_id, "bb");
        assert!(camel.is_accepted);
        assert_eq!(camel.inputs[0].compute_budget, Some(10));
        assert_eq!(camel.input_covenant_id(), Some("cc"));
        assert_eq!(
            camel.inputs[0].previous_outpoint_hash.as_deref(),
            Some("aa")
        );
    }

    #[test]
    fn parses_fee_estimate() {
        let raw = r#"{
            "priorityBucket":{"feerate":100,"estimatedSeconds":0.0008},
            "normalBuckets":[{"feerate":100,"estimatedSeconds":0.0008}],
            "lowBuckets":[{"feerate":100,"estimatedSeconds":0.0008}]
        }"#;
        let fee: FeeEstimate = serde_json::from_str(raw).unwrap();
        assert_eq!(fee.priority_feerate(), 100.0);
        assert!(fee.meets_standard_relay_rate());
        assert!(fee.buckets_below_standard_rate().is_empty());
        let mut cheap = fee.clone();
        cheap.low_buckets[0].feerate = 50.0;
        assert_eq!(cheap.buckets_below_standard_rate(), vec![("low", 50.0)]);
        assert_eq!(fee.normal_buckets.len(), 1);
    }
}
