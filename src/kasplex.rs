//! Kasplex KRC-20 indexer on TN10. Not L1 consensus and not a USD stable.

use crate::error::{EngineError, Result};
use crate::network::{is_valid_testnet_address, KASPLEX_TN10};
use crate::rest::{encode_path_segment, http_send_get, https_json_client, require_https};
use serde::de::DeserializeOwned;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KasplexInfo {
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub daa_score: String,
    #[serde(default)]
    pub daa_score_gap: String,
    #[serde(default)]
    pub op_total: String,
    #[serde(default)]
    pub token_total: String,
}

#[derive(Debug, Clone, Deserialize)]
struct KasplexInfoEnvelope {
    #[serde(default)]
    message: String,
    result: KasplexInfo,
}

#[derive(Debug, Clone)]
pub struct KasplexStatus {
    pub message: String,
    pub info: KasplexInfo,
}

impl KasplexStatus {
    pub fn is_synced(&self) -> bool {
        self.message.eq_ignore_ascii_case("synced")
    }
}

impl KasplexInfo {
    pub fn token_count(&self) -> Option<u64> {
        self.token_total.parse().ok()
    }

    pub fn daa_gap(&self) -> Option<i64> {
        self.daa_score_gap.parse().ok()
    }
}

/// One row from `/krc20/tokenlist` or `/krc20/token/{tick}`.
#[derive(Debug, Clone, Deserialize)]
pub struct KasplexToken {
    #[serde(default)]
    pub tick: Option<String>,
    #[serde(default)]
    pub ca: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub max: String,
    #[serde(default)]
    pub lim: String,
    #[serde(default)]
    pub minted: String,
    #[serde(default)]
    pub burned: String,
    #[serde(default)]
    pub state: String,
    #[serde(rename = "mod", default)]
    pub mode: String,
    #[serde(default)]
    pub dec: String,
    #[serde(rename = "hashRev", default)]
    pub hash_rev: String,
    #[serde(rename = "holderTotal", default)]
    pub holder_total: String,
}

impl KasplexToken {
    pub fn ticker(&self) -> Option<&str> {
        self.tick
            .as_deref()
            .filter(|t| !t.is_empty())
            .or(self.name.as_deref().filter(|t| !t.is_empty()))
    }

    pub fn remaining(&self) -> Option<u128> {
        let max: u128 = self.max.parse().ok()?;
        let minted: u128 = self.minted.parse().ok()?;
        Some(max.saturating_sub(minted))
    }

    pub fn is_open_mint(&self) -> bool {
        let limit = self.lim.parse::<u128>().ok();
        let maximum = self.max.parse::<u128>().ok();
        self.mode.eq_ignore_ascii_case("mint")
            && self.state.eq_ignore_ascii_case("deployed")
            && self.remaining().map(|r| r > 0).unwrap_or(false)
            && matches!((limit, maximum), (Some(lim), Some(max)) if lim > 0 && lim <= max)
            && self.tick.as_deref().is_some_and(valid_tick)
    }

    pub fn is_unused_tick(&self) -> bool {
        self.state.eq_ignore_ascii_case("unused")
            || (self.state.is_empty() && self.hash_rev.is_empty())
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct KasplexTokenPage {
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub prev: Option<String>,
    #[serde(default)]
    pub next: Option<String>,
    #[serde(default)]
    pub result: Vec<KasplexToken>,
}

impl KasplexTokenPage {
    pub fn open_mints(&self) -> impl Iterator<Item = &KasplexToken> {
        self.result.iter().filter(|t| t.is_open_mint())
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct KasplexBalance {
    #[serde(default)]
    pub tick: Option<String>,
    #[serde(default)]
    pub ca: Option<String>,
    #[serde(default)]
    pub balance: String,
    #[serde(default)]
    pub locked: String,
    #[serde(default)]
    pub dec: String,
}

#[derive(Debug, Clone, Deserialize)]
struct KasplexBalancePage {
    #[serde(default)]
    pub result: Vec<KasplexBalance>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct KasplexOp {
    #[serde(default)]
    pub p: String,
    #[serde(default)]
    pub op: String,
    #[serde(default)]
    pub tick: Option<String>,
    #[serde(rename = "hashRev", default)]
    pub hash_rev: String,
    #[serde(rename = "txAccept", default)]
    pub tx_accept: String,
    #[serde(rename = "opAccept", default)]
    pub op_accept: String,
    #[serde(rename = "opError", default)]
    pub op_error: String,
}

impl KasplexOp {
    pub fn accepted(&self) -> bool {
        self.op_accept == "1" || self.op_accept.eq_ignore_ascii_case("true")
    }
}

#[derive(Debug, Clone, Deserialize)]
struct KasplexOpPage {
    #[serde(default)]
    result: Vec<KasplexOp>,
}

#[derive(Debug, Clone, Deserialize)]
struct KasplexTokenInfoPage {
    #[serde(default)]
    result: Vec<KasplexToken>,
}

#[derive(Clone)]
pub struct KasplexClient {
    http: reqwest::Client,
    base: String,
}

impl Default for KasplexClient {
    fn default() -> Self {
        Self::new(KASPLEX_TN10).expect("reqwest TLS client")
    }
}

impl KasplexClient {
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

    async fn get_json<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = format!("{}{path}", self.base);
        Ok(http_send_get(&self.http, &url)
            .await?
            .error_for_status()?
            .json()
            .await?)
    }

    pub async fn info(&self) -> Result<KasplexStatus> {
        let body: KasplexInfoEnvelope = self.get_json("/info").await?;
        Ok(KasplexStatus {
            message: body.message,
            info: body.result,
        })
    }

    pub async fn tokenlist(&self, next: Option<&str>) -> Result<KasplexTokenPage> {
        let path = match next.filter(|c| !c.is_empty()) {
            Some(cursor) => format!("/krc20/tokenlist?next={}", encode_path_segment(cursor)),
            None => "/krc20/tokenlist".to_string(),
        };
        self.get_json(&path).await
    }

    pub async fn token(&self, tick: &str) -> Result<Option<KasplexToken>> {
        if !valid_tick(tick) {
            return Err(EngineError::Message(
                "Kasplex ticker must be 4-6 ASCII alphanumeric characters".into(),
            ));
        }
        let path = format!("/krc20/token/{}", encode_path_segment(tick));
        let page: KasplexTokenInfoPage = self.get_json(&path).await?;
        Ok(page.result.into_iter().next())
    }

    pub async fn address_tokenlist(&self, address: &str) -> Result<Vec<KasplexBalance>> {
        if !is_valid_testnet_address(address) {
            return Err(EngineError::NotTestnetAddress(address.to_string()));
        }
        let encoded = format!("/krc20/address/{}/tokenlist", encode_path_segment(address));
        let page: KasplexBalancePage = self.get_json(&encoded).await?;
        Ok(page.result)
    }

    pub async fn op(&self, reveal_txid: &str) -> Result<Option<KasplexOp>> {
        let path = format!("/krc20/op/{}", encode_path_segment(reveal_txid));
        let url = format!("{}{path}", self.base);
        let resp = http_send_get(&self.http, &url).await?;
        let code = resp.status().as_u16();
        // Indexer often 403/404 until the reveal is visible.
        if matches!(code, 403 | 404) {
            return Ok(None);
        }
        let page: KasplexOpPage = resp.error_for_status()?.json().await?;
        Ok(page.result.into_iter().next())
    }
}

fn valid_tick(tick: &str) -> bool {
    (4..=6).contains(&tick.len()) && tick.bytes().all(|byte| byte.is_ascii_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::EngineError;

    #[test]
    fn parses_synced_indexer() {
        let raw = r#"{
            "message":"synced",
            "result":{
                "version":"3.01.260412",
                "daaScore":"550659745",
                "daaScoreGap":"1",
                "opTotal":"14644",
                "tokenTotal":"87"
            }
        }"#;
        let env: KasplexInfoEnvelope = serde_json::from_str(raw).unwrap();
        let status = KasplexStatus {
            message: env.message,
            info: env.result,
        };
        assert!(status.is_synced());
        assert_eq!(status.info.token_count(), Some(87));
        assert_eq!(status.info.daa_gap(), Some(1));
    }

    #[test]
    fn parses_live_tokenlist_row_and_open_mint() {
        let raw = r#"{
            "message":"successful",
            "next":"4771596870000",
            "result":[
                {"tick":"TMBMN","max":"2100000000000000","lim":"100000000000","minted":"0","burned":"0","state":"deployed","mod":"mint","dec":"8","hashRev":"aa","holderTotal":"2"},
                {"ca":"140cb197","name":"AMUEL","max":"1","lim":"0","minted":"1","state":"finished","mod":"issue"},
                {"tick":"FULL","max":"10","lim":"1","minted":"10","state":"deployed","mod":"mint"}
            ]
        }"#;
        let page: KasplexTokenPage = serde_json::from_str(raw).unwrap();
        let open: Vec<_> = page.open_mints().collect();
        assert_eq!(open.len(), 1);
        assert_eq!(open[0].ticker(), Some("TMBMN"));
        assert_eq!(open[0].holder_total, "2");
        let unused: KasplexToken = serde_json::from_str(
            r#"{"tick":"FRONT","max":"0","lim":"0","minted":"0","state":"unused"}"#,
        )
        .unwrap();
        assert!(unused.is_unused_tick());
        assert!(!unused.is_open_mint());
        assert_eq!(open[0].remaining(), Some(2_100_000_000_000_000));
        assert!(!page.result[1].is_open_mint());
        assert!(!page.result[2].is_open_mint());
    }

    #[test]
    fn open_mint_requires_safe_ticker_and_positive_bounded_limit() {
        for raw in [
            r#"{"tick":"ÅBCD","max":"10","lim":"1","minted":"0","state":"deployed","mod":"mint"}"#,
            r#"{"tick":"ABC","max":"10","lim":"1","minted":"0","state":"deployed","mod":"mint"}"#,
            r#"{"tick":"VALID","max":"10","lim":"0","minted":"0","state":"deployed","mod":"mint"}"#,
            r#"{"tick":"VALID","max":"10","lim":"11","minted":"0","state":"deployed","mod":"mint"}"#,
        ] {
            let token: KasplexToken = serde_json::from_str(raw).unwrap();
            assert!(!token.is_open_mint());
        }
    }

    #[test]
    fn rejects_invalid_ticker_and_address_before_http() {
        let client = KasplexClient::new("https://example.invalid").unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        assert!(runtime.block_on(client.token("BAD!")).is_err());
        assert!(runtime
            .block_on(client.address_tokenlist("kaspatest:not-a-checksummed-address"))
            .is_err());
    }

    #[test]
    fn parses_address_tokenlist() {
        let empty: KasplexBalancePage =
            serde_json::from_str(r#"{"message":"successful","result":[]}"#).unwrap();
        assert!(empty.result.is_empty());
        let held: KasplexBalancePage = serde_json::from_str(
            r#"{"message":"successful","result":[{"tick":"KASP","balance":"12","locked":"0","dec":"8"}]}"#,
        )
        .unwrap();
        assert_eq!(held.result[0].balance, "12");
    }

    #[test]
    fn parses_op_accept() {
        let page: KasplexOpPage = serde_json::from_str(
            r#"{"result":[{"p":"krc-20","op":"mint","tick":"tmbmn","hashRev":"aa","txAccept":"1","opAccept":"1","opError":""}]}"#,
        )
        .unwrap();
        assert!(page.result[0].accepted());
    }

    #[test]
    fn rejects_cleartext_kasplex() {
        assert!(matches!(
            KasplexClient::new("http://127.0.0.1:9"),
            Err(EngineError::InsecureTransport(_))
        ));
    }
}
