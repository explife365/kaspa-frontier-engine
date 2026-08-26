//! Off-chain KRC-20 / Kasplex-style payload state machine.
//! This is not L1 consensus. Kasplex (or equivalent) is the live indexer.
//! Do not treat balances here as chain truth until a txid is accepted on TN10.

use crate::network::{classify_address, AddressNetwork};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use thiserror::Error;

#[derive(Error, Debug, PartialEq, Eq)]
pub enum Krc20Error {
    #[error("invalid protocol tag")]
    InvalidProtocol,
    #[error("token already registered: {0}")]
    AlreadyExists(String),
    #[error("token not found: {0}")]
    NotFound(String),
    #[error("mint {requested} exceeds per-mint limit {limit}")]
    MintLimitExceeded { requested: u128, limit: u128 },
    #[error("supply cap exceeded: requested {requested}, remaining {remaining}")]
    SupplyCapExceeded { requested: u128, remaining: u128 },
    #[error("insufficient balance for {address}: needed {needed}, have {available}")]
    InsufficientBalance {
        address: String,
        needed: u128,
        available: u128,
    },
    #[error("invalid numeric field")]
    InvalidAmount,
    #[error("invalid ticker")]
    InvalidTick,
    #[error("invalid address")]
    InvalidAddress,
    #[error("txid required")]
    MissingTxId,
    #[error("duplicate txid: {0}")]
    DuplicateTx(String),
    #[error("JSON: {0}")]
    Parse(String),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "op", rename_all = "lowercase")]
pub enum Krc20Operation {
    Deploy {
        tick: String,
        max: String,
        lim: String,
        #[serde(default)]
        dec: Option<u8>,
    },
    Mint {
        tick: String,
        /// Kasplex compact mint omits `amt` and uses the deploy `lim`.
        #[serde(default)]
        amt: Option<String>,
    },
    Transfer {
        tick: String,
        amt: String,
        to: String,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Krc20Envelope {
    pub p: String,
    #[serde(flatten)]
    pub op: Krc20Operation,
}

#[derive(Debug, Clone)]
pub struct TokenState {
    pub ticker: String,
    pub max_supply: u128,
    pub minted_supply: u128,
    pub limit_per_mint: u128,
    pub decimals: u8,
    pub balances: HashMap<String, u128>,
}

#[derive(Default, Debug)]
pub struct Krc20StateEngine {
    tokens: HashMap<String, TokenState>,
    seen_txids: HashSet<String>,
}

fn parse_u128(raw: &str) -> Result<u128, Krc20Error> {
    if raw.is_empty() || !raw.bytes().all(|b| b.is_ascii_digit()) {
        return Err(Krc20Error::InvalidAmount);
    }
    raw.parse::<u128>().map_err(|_| Krc20Error::InvalidAmount)
}

fn normalize_tick(tick: &str) -> Result<String, Krc20Error> {
    let ticker = tick.to_uppercase();
    if ticker.is_empty() || ticker.len() > 10 || !ticker.chars().all(|c| c.is_ascii_alphanumeric())
    {
        return Err(Krc20Error::InvalidTick);
    }
    Ok(ticker)
}

/// Kasplex inscription tick: 4–6 alphanumeric, lowercase, compact JSON.
pub fn kasplex_tick(tick: &str) -> Result<String, Krc20Error> {
    let ticker = tick.trim().to_ascii_lowercase();
    if !(4..=6).contains(&ticker.len()) || !ticker.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(Krc20Error::InvalidTick);
    }
    Ok(ticker)
}

fn digits_only(raw: &str) -> Result<&str, Krc20Error> {
    if raw.is_empty() || !raw.bytes().all(|b| b.is_ascii_digit()) {
        return Err(Krc20Error::InvalidAmount);
    }
    Ok(raw)
}

/// Compact lowercase JSON Kasplex requires in the reveal envelope. No spaces.
pub fn kasplex_mint_inscription(tick: &str) -> Result<String, Krc20Error> {
    let tick = kasplex_tick(tick)?;
    Ok(format!(r#"{{"p":"krc-20","op":"mint","tick":"{tick}"}}"#))
}

pub fn kasplex_transfer_inscription(tick: &str, amt: &str, to: &str) -> Result<String, Krc20Error> {
    let tick = kasplex_tick(tick)?;
    let amt = digits_only(amt)?;
    let to = to.trim().to_ascii_lowercase();
    if classify_address(&to) != AddressNetwork::Testnet {
        return Err(Krc20Error::InvalidAddress);
    }
    Ok(format!(
        r#"{{"p":"krc-20","op":"transfer","tick":"{tick}","amt":"{amt}","to":"{to}"}}"#
    ))
}

pub fn kasplex_deploy_inscription(tick: &str, max: &str, lim: &str) -> Result<String, Krc20Error> {
    let tick = kasplex_tick(tick)?;
    let max = digits_only(max)?;
    let lim = digits_only(lim)?;
    Ok(format!(
        r#"{{"p":"krc-20","op":"deploy","tick":"{tick}","max":"{max}","lim":"{lim}"}}"#
    ))
}

fn reject_cross_network(sender: &str, dest: &str) -> Result<(), Krc20Error> {
    let sender_net = classify_address(sender);
    let dest_net = classify_address(dest);
    match (sender_net, dest_net) {
        (AddressNetwork::Testnet, AddressNetwork::Mainnet)
        | (AddressNetwork::Mainnet, AddressNetwork::Testnet) => Err(Krc20Error::InvalidAddress),
        _ => Ok(()),
    }
}

impl Krc20StateEngine {
    pub fn new() -> Self {
        Self::default()
    }

    /// Apply one accepted payload. `tx_id` is the L1 txid; replays are rejected.
    /// This is still not Kasplex — balances are only as good as the feed you give it.
    pub fn apply(
        &mut self,
        tx_id: &str,
        sender: &str,
        raw_script_json: &str,
    ) -> Result<(), Krc20Error> {
        if tx_id.is_empty() {
            return Err(Krc20Error::MissingTxId);
        }
        if self.seen_txids.contains(tx_id) {
            return Err(Krc20Error::DuplicateTx(tx_id.to_string()));
        }
        let envelope: Krc20Envelope =
            serde_json::from_str(raw_script_json).map_err(|e| Krc20Error::Parse(e.to_string()))?;
        if envelope.p != "krc-20" {
            return Err(Krc20Error::InvalidProtocol);
        }
        match envelope.op {
            Krc20Operation::Deploy {
                tick,
                max,
                lim,
                dec,
            } => {
                let ticker = normalize_tick(&tick)?;
                if self.tokens.contains_key(&ticker) {
                    return Err(Krc20Error::AlreadyExists(ticker));
                }
                self.tokens.insert(
                    ticker.clone(),
                    TokenState {
                        ticker,
                        max_supply: parse_u128(&max)?,
                        minted_supply: 0,
                        limit_per_mint: parse_u128(&lim)?,
                        decimals: dec.unwrap_or(8),
                        balances: HashMap::new(),
                    },
                );
            }
            Krc20Operation::Mint { tick, amt } => {
                let ticker = normalize_tick(&tick)?;
                let token = self
                    .tokens
                    .get_mut(&ticker)
                    .ok_or_else(|| Krc20Error::NotFound(ticker.clone()))?;
                let amount = match amt.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
                    Some(raw) => parse_u128(raw)?,
                    None => token.limit_per_mint,
                };
                if amount == 0 {
                    return Err(Krc20Error::InvalidAmount);
                }
                if amount > token.limit_per_mint {
                    return Err(Krc20Error::MintLimitExceeded {
                        requested: amount,
                        limit: token.limit_per_mint,
                    });
                }
                let remaining = token.max_supply.saturating_sub(token.minted_supply);
                if amount > remaining {
                    return Err(Krc20Error::SupplyCapExceeded {
                        requested: amount,
                        remaining,
                    });
                }
                token.minted_supply = token
                    .minted_supply
                    .checked_add(amount)
                    .ok_or(Krc20Error::InvalidAmount)?;
                let balance = token.balances.entry(sender.to_string()).or_insert(0);
                *balance = balance
                    .checked_add(amount)
                    .ok_or(Krc20Error::InvalidAmount)?;
            }
            Krc20Operation::Transfer { tick, amt, to } => {
                if to.is_empty() {
                    return Err(Krc20Error::InvalidAddress);
                }
                reject_cross_network(sender, &to)?;
                let ticker = normalize_tick(&tick)?;
                let amount = parse_u128(&amt)?;
                if amount == 0 {
                    return Err(Krc20Error::InvalidAmount);
                }
                let token = self
                    .tokens
                    .get_mut(&ticker)
                    .ok_or_else(|| Krc20Error::NotFound(ticker.clone()))?;
                let sender_balance = token.balances.get(sender).copied().unwrap_or(0);
                if sender_balance < amount {
                    return Err(Krc20Error::InsufficientBalance {
                        address: sender.to_string(),
                        needed: amount,
                        available: sender_balance,
                    });
                }
                token
                    .balances
                    .insert(sender.to_string(), sender_balance - amount);
                let dest = token.balances.entry(to).or_insert(0);
                *dest = dest.checked_add(amount).ok_or(Krc20Error::InvalidAmount)?;
            }
        }
        self.seen_txids.insert(tx_id.to_string());
        Ok(())
    }

    pub fn get_balance(&self, ticker: &str, address: &str) -> u128 {
        self.tokens
            .get(&ticker.to_uppercase())
            .and_then(|t| t.balances.get(address))
            .copied()
            .unwrap_or(0)
    }

    pub fn minted(&self, ticker: &str) -> Option<u128> {
        self.tokens
            .get(&ticker.to_uppercase())
            .map(|t| t.minted_supply)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn go(
        engine: &mut Krc20StateEngine,
        tx: &str,
        sender: &str,
        json: &str,
    ) -> Result<(), Krc20Error> {
        engine.apply(tx, sender, json)
    }

    #[test]
    fn deploy_mint_transfer() {
        let mut engine = Krc20StateEngine::new();
        go(
            &mut engine,
            "tx1",
            "kaspatest:issuer",
            r#"{"p":"krc-20","op":"deploy","tick":"KUSDT","max":"1000000","lim":"1000"}"#,
        )
        .unwrap();
        go(
            &mut engine,
            "tx2",
            "kaspatest:alice",
            r#"{"p":"krc-20","op":"mint","tick":"KUSDT","amt":"1000"}"#,
        )
        .unwrap();
        go(
            &mut engine,
            "tx3",
            "kaspatest:alice",
            r#"{"p":"krc-20","op":"transfer","tick":"KUSDT","amt":"250","to":"kaspatest:bob"}"#,
        )
        .unwrap();
        assert_eq!(engine.get_balance("kusdt", "kaspatest:alice"), 750);
        assert_eq!(engine.get_balance("KUSDT", "kaspatest:bob"), 250);
        assert_eq!(engine.minted("KUSDT"), Some(1000));
        assert_eq!(
            go(
                &mut engine,
                "tx3",
                "kaspatest:alice",
                r#"{"p":"krc-20","op":"transfer","tick":"KUSDT","amt":"1","to":"kaspatest:bob"}"#,
            ),
            Err(Krc20Error::DuplicateTx("tx3".into()))
        );
    }

    #[test]
    fn rejects_overmint() {
        let mut engine = Krc20StateEngine::new();
        go(
            &mut engine,
            "d",
            "a",
            r#"{"p":"krc-20","op":"deploy","tick":"X","max":"10","lim":"5"}"#,
        )
        .unwrap();
        let err = go(
            &mut engine,
            "m",
            "a",
            r#"{"p":"krc-20","op":"mint","tick":"X","amt":"6"}"#,
        )
        .unwrap_err();
        assert!(matches!(err, Krc20Error::MintLimitExceeded { .. }));
    }

    #[test]
    fn rejects_overflow_as_invalid_amount() {
        let mut engine = Krc20StateEngine::new();
        go(
            &mut engine,
            "d",
            "a",
            r#"{"p":"krc-20","op":"deploy","tick":"X","max":"10","lim":"10"}"#,
        )
        .unwrap();
        let err = go(
            &mut engine,
            "m",
            "a",
            r#"{"p":"krc-20","op":"mint","tick":"X","amt":"-1"}"#,
        )
        .unwrap_err();
        assert_eq!(err, Krc20Error::InvalidAmount);
    }

    #[test]
    fn rejects_bad_tick_zero_mint_and_cap() {
        let mut engine = Krc20StateEngine::new();
        assert!(matches!(
            go(
                &mut engine,
                "bad",
                "a",
                r#"{"p":"krc-20","op":"deploy","tick":"","max":"10","lim":"10"}"#
            ),
            Err(Krc20Error::InvalidTick)
        ));
        go(
            &mut engine,
            "d",
            "a",
            r#"{"p":"krc-20","op":"deploy","tick":"X","max":"10","lim":"10"}"#,
        )
        .unwrap();
        assert!(matches!(
            go(
                &mut engine,
                "d2",
                "a",
                r#"{"p":"krc-20","op":"deploy","tick":"x","max":"1","lim":"1"}"#
            ),
            Err(Krc20Error::AlreadyExists(_))
        ));
        assert!(matches!(
            go(
                &mut engine,
                "z",
                "a",
                r#"{"p":"krc-20","op":"mint","tick":"X","amt":"0"}"#
            ),
            Err(Krc20Error::InvalidAmount)
        ));
        go(
            &mut engine,
            "m",
            "a",
            r#"{"p":"krc-20","op":"mint","tick":"X","amt":"10"}"#,
        )
        .unwrap();
        let err = go(
            &mut engine,
            "m2",
            "a",
            r#"{"p":"krc-20","op":"mint","tick":"X","amt":"1"}"#,
        )
        .unwrap_err();
        assert!(matches!(
            err,
            Krc20Error::SupplyCapExceeded { remaining: 0, .. }
        ));
        let err = go(
            &mut engine,
            "t",
            "a",
            r#"{"p":"krc-20","op":"transfer","tick":"X","amt":"11","to":"b"}"#,
        )
        .unwrap_err();
        assert!(matches!(err, Krc20Error::InsufficientBalance { .. }));
        assert_eq!(
            go(
                &mut engine,
                "",
                "a",
                r#"{"p":"krc-20","op":"mint","tick":"X","amt":"1"}"#
            ),
            Err(Krc20Error::MissingTxId)
        );
        assert_eq!(
            go(
                &mut engine,
                "empty-to",
                "a",
                r#"{"p":"krc-20","op":"transfer","tick":"X","amt":"1","to":""}"#,
            ),
            Err(Krc20Error::InvalidAddress)
        );
    }

    #[test]
    fn rejects_testnet_to_mainnet_transfer() {
        let mut engine = Krc20StateEngine::new();
        go(
            &mut engine,
            "d",
            "kaspatest:alice",
            r#"{"p":"krc-20","op":"deploy","tick":"X","max":"10","lim":"10"}"#,
        )
        .unwrap();
        go(
            &mut engine,
            "m",
            "kaspatest:alice",
            r#"{"p":"krc-20","op":"mint","tick":"X","amt":"5"}"#,
        )
        .unwrap();
        assert_eq!(
            engine.apply(
                "cross-net",
                "kaspatest:alice",
                r#"{"p":"krc-20","op":"transfer","tick":"X","amt":"1","to":"kaspa:qpxdemlyx445kt5xteux0qhadaw8lh5m0vnqvcy8fh483t70usgkkeulsx9cm"}"#,
            ),
            Err(Krc20Error::InvalidAddress)
        );
        assert_eq!(engine.get_balance("X", "kaspatest:alice"), 5);
    }

    #[test]
    fn rejects_wrong_protocol() {
        let mut engine = Krc20StateEngine::new();
        let err = go(
            &mut engine,
            "p",
            "a",
            r#"{"p":"krc-21","op":"deploy","tick":"X","max":"1","lim":"1"}"#,
        )
        .unwrap_err();
        assert_eq!(err, Krc20Error::InvalidProtocol);
    }

    #[test]
    fn kasplex_inscriptions_are_compact_lowercase() {
        assert_eq!(
            kasplex_mint_inscription("TMBMN").unwrap(),
            r#"{"p":"krc-20","op":"mint","tick":"tmbmn"}"#
        );
        assert!(kasplex_mint_inscription("abc").is_err());
        assert!(kasplex_mint_inscription("toolong").is_err());
        let xfer = kasplex_transfer_inscription(
            "tmbmn",
            "1",
            "kaspatest:qptv6u8kel95drh2p2z492cyksk8lpetep286fngqu5j9nk57g642lzf748kt",
        )
        .unwrap();
        assert!(!xfer.contains(' '));
        assert!(xfer.contains("\"op\":\"transfer\""));
        assert!(kasplex_transfer_inscription("tmbmn", "1", "kaspa:qq").is_err());
        assert_eq!(
            kasplex_deploy_inscription("aaaa", "100", "10").unwrap(),
            r#"{"p":"krc-20","op":"deploy","tick":"aaaa","max":"100","lim":"10"}"#
        );
    }

    #[test]
    fn compact_kasplex_mint_uses_deploy_lim() {
        let mut engine = Krc20StateEngine::new();
        go(
            &mut engine,
            "d",
            "kaspatest:alice",
            r#"{"p":"krc-20","op":"deploy","tick":"TMBM","max":"1000","lim":"25"}"#,
        )
        .unwrap();
        go(
            &mut engine,
            "m",
            "kaspatest:alice",
            &kasplex_mint_inscription("tmbm").unwrap(),
        )
        .unwrap();
        assert_eq!(engine.get_balance("TMBM", "kaspatest:alice"), 25);
        assert_eq!(engine.minted("TMBM"), Some(25));
    }
}
