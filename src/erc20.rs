//! Generic ERC-20 `eth_call` helpers for Galleon / Kasplex L2.
//!
//! Galleon `USDC` is Igra's test token (`GALLEON_TEST_USDC`). Igra mainnet
//! `USDC` is Hyperlane HypSynthetic (`IGRA_MAINNET_HYPERLANE_USDC`). Neither
//! is Circle-issued cash USDC.

use crate::error::{EngineError, Result};
use crate::l2::EvmRpcClient;
use crate::network::{
    GALLEON_TEST_USDC as GALLEON_USDC, IGRA_MAINNET_HYPERLANE_USDC as IGRA_HYPERLANE_USDC,
};

/// `decimals()`
pub const SELECTOR_DECIMALS: &str = "0x313ce567";
/// `symbol()`
pub const SELECTOR_SYMBOL: &str = "0x95d89b41";
/// `name()`
pub const SELECTOR_NAME: &str = "0x06fdde03";
/// `balanceOf(address)` prefix (no padded address).
pub const SELECTOR_BALANCE_OF: &str = "70a08231";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Erc20Meta {
    pub address: String,
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
}

impl Erc20Meta {
    pub fn is_galleon_test_usdc(&self) -> bool {
        self.address.eq_ignore_ascii_case(GALLEON_USDC)
    }

    pub fn is_hyperlane_igra_usdc(&self) -> bool {
        self.address.eq_ignore_ascii_case(IGRA_HYPERLANE_USDC)
    }

    /// Circle does not list Igra. Do not treat Galleon or Hyperlane USDC as cash.
    pub fn is_circle_issued(&self) -> bool {
        false
    }
}

pub fn balance_of_calldata(holder: &str) -> Result<String> {
    let hex = holder
        .trim()
        .strip_prefix("0x")
        .or_else(|| holder.trim().strip_prefix("0X"))
        .unwrap_or(holder.trim());
    if hex.len() != 40 {
        return Err(EngineError::Message("holder must be 20-byte hex".into()));
    }
    Ok(format!("0x{SELECTOR_BALANCE_OF}{}{hex}", "0".repeat(24)))
}

pub fn decode_uint256(hex_result: &str) -> Result<u128> {
    let hex = strip_0x(hex_result);
    if hex.is_empty() {
        return Err(EngineError::Message("empty eth_call result".into()));
    }
    u128::from_str_radix(hex, 16).map_err(|e| EngineError::Message(format!("bad uint {e}")))
}

pub fn decode_abi_string(hex_result: &str) -> Result<String> {
    let hex = strip_0x(hex_result);
    let raw = hex::decode_loose(hex)?;
    if raw.len() < 64 {
        return Err(EngineError::Message("ABI string too short".into()));
    }
    let offset = usize::try_from(u64::from_be_bytes(
        raw[24..32]
            .try_into()
            .map_err(|_| EngineError::Message("ABI offset".into()))?,
    ))
    .map_err(|_| EngineError::Message("ABI offset".into()))?;
    if offset + 32 > raw.len() {
        return Err(EngineError::Message("ABI string offset".into()));
    }
    let len = usize::try_from(u64::from_be_bytes(
        raw[offset + 24..offset + 32]
            .try_into()
            .map_err(|_| EngineError::Message("ABI length".into()))?,
    ))
    .map_err(|_| EngineError::Message("ABI length".into()))?;
    let start = offset + 32;
    let end = start
        .checked_add(len)
        .ok_or_else(|| EngineError::Message("ABI length overflow".into()))?;
    if end > raw.len() {
        return Err(EngineError::Message("ABI string truncated".into()));
    }
    String::from_utf8(raw[start..end].to_vec())
        .map_err(|_| EngineError::Message("ABI string is not UTF-8".into()))
}

fn strip_0x(s: &str) -> &str {
    s.trim()
        .strip_prefix("0x")
        .or_else(|| s.trim().strip_prefix("0X"))
        .unwrap_or(s.trim())
}

mod hex {
    use crate::error::{EngineError, Result};

    pub fn decode_loose(hex: &str) -> Result<Vec<u8>> {
        if hex.len() % 2 != 0 {
            return Err(EngineError::Message("odd hex length".into()));
        }
        (0..hex.len())
            .step_by(2)
            .map(|i| {
                u8::from_str_radix(&hex[i..i + 2], 16)
                    .map_err(|_| EngineError::Message("bad hex".into()))
            })
            .collect()
    }
}

impl EvmRpcClient {
    pub async fn eth_call(&self, to: &str, data: &str) -> Result<String> {
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "eth_call",
            "params": [
                { "to": to, "data": data },
                "latest"
            ]
        });
        let resp = self.post_rpc(&body).await?;
        Ok(resp)
    }

    pub async fn erc20_meta(&self, token: &str) -> Result<Erc20Meta> {
        let hexes = self
            .eth_call_batch(&[
                (token, SELECTOR_DECIMALS),
                (token, SELECTOR_SYMBOL),
                (token, SELECTOR_NAME),
            ])
            .await?;
        let decimals_hex = hexes
            .first()
            .ok_or_else(|| EngineError::Message("erc20_meta missing decimals".into()))?;
        let symbol_hex = hexes
            .get(1)
            .ok_or_else(|| EngineError::Message("erc20_meta missing symbol".into()))?;
        let name_hex = hexes
            .get(2)
            .ok_or_else(|| EngineError::Message("erc20_meta missing name".into()))?;
        let decimals_u = decode_uint256(decimals_hex)?;
        if decimals_u > 255 {
            return Err(EngineError::Message("decimals out of range".into()));
        }
        Ok(Erc20Meta {
            address: token.to_string(),
            name: decode_abi_string(name_hex)?,
            symbol: decode_abi_string(symbol_hex)?,
            decimals: decimals_u as u8,
        })
    }

    pub async fn erc20_balance(&self, token: &str, holder: &str) -> Result<u128> {
        let data = balance_of_calldata(holder)?;
        let hex = self.eth_call(token, &data).await?;
        decode_uint256(&hex)
    }

    pub async fn galleon_test_usdc_meta(&self) -> Result<Erc20Meta> {
        self.erc20_meta(GALLEON_USDC).await
    }

    pub async fn igra_hyperlane_usdc_meta(&self) -> Result<Erc20Meta> {
        self.erc20_meta(IGRA_HYPERLANE_USDC).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn balance_selector_pads_address() {
        let data = balance_of_calldata("0x00000000000000000000000000000000000000aa").unwrap();
        assert!(data.starts_with("0x70a08231"));
        assert!(data.ends_with("00000000000000000000000000000000000000aa"));
        assert_eq!(data.len(), 2 + 8 + 64);
    }

    #[test]
    fn decodes_abi_string_usdc() {
        // offset 32, len 4, "USDC"
        let hex = concat!(
            "0x",
            "0000000000000000000000000000000000000000000000000000000000000020",
            "0000000000000000000000000000000000000000000000000000000000000004",
            "5553444300000000000000000000000000000000000000000000000000000000"
        );
        assert_eq!(decode_abi_string(hex).unwrap(), "USDC");
        assert_eq!(decode_uint256("0x06").unwrap(), 6);
        assert!(!Erc20Meta {
            address: GALLEON_USDC.into(),
            name: "USD Coin".into(),
            symbol: "USDC".into(),
            decimals: 6,
        }
        .is_circle_issued());
        assert!(Erc20Meta {
            address: IGRA_HYPERLANE_USDC.into(),
            name: "USD Coin".into(),
            symbol: "USDC".into(),
            decimals: 6,
        }
        .is_hyperlane_igra_usdc());
        assert!(!Erc20Meta {
            address: IGRA_HYPERLANE_USDC.into(),
            name: "USD Coin".into(),
            symbol: "USDC".into(),
            decimals: 6,
        }
        .is_circle_issued());
    }
}
