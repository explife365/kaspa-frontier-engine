//! CEX-facing view of kaspad REST.
//!
//! `Kaspa to do.pdf` (Gemini paste) ships a `CexUtxoIndexer` that credits
//! `Hash::zero()` and a `CexRpcAdapter` that reads an account `StateDB`.
//! Kaspa is UTXO. Exchanges need `getUtxosByAddresses` + DAA depth.
//! This module is that wrapper. It is not a node, not an AMM, not a bridge.

use crate::network::{COINBASE_MATURITY_DAA, DEFAULT_DEPOSIT_CONFIRMATIONS};
use crate::rest::AddressUtxo;
use crate::{EngineError, Result};
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CexSpendable {
    pub tx_id: String,
    pub index: u32,
    pub amount_sompi: u64,
    pub confirmations: u64,
    pub is_coinbase: bool,
}

/// Exchange-style split of one address’s REST UTXOs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CexAddressSnapshot {
    pub address: String,
    pub virtual_daa: u64,
    pub confirmed_sompi: u64,
    pub pending_sompi: u64,
    pub immature_coinbase_sompi: u64,
    pub spendable: Vec<CexSpendable>,
}

fn confirmations(virtual_daa: u64, block_daa: u64) -> u64 {
    virtual_daa.saturating_sub(block_daa)
}

fn required_depth(is_coinbase: bool, deposit_confirmations: u64) -> u64 {
    if is_coinbase {
        deposit_confirmations.max(COINBASE_MATURITY_DAA)
    } else {
        deposit_confirmations.max(1)
    }
}

/// Map explorer `/addresses/{}/utxos` to what a CEX credits / can spend.
/// Rejects foreign addresses and duplicate outpoints before calculating balances.
pub fn snapshot_address(
    address: &str,
    virtual_daa: u64,
    utxos: &[AddressUtxo],
    deposit_confirmations: u64,
) -> Result<CexAddressSnapshot> {
    validate_snapshot(address, utxos)?;
    let deposit_confirmations = deposit_confirmations.max(1);
    let mut confirmed_sompi = 0u64;
    let mut pending_sompi = 0u64;
    let mut immature_coinbase_sompi = 0u64;
    let mut spendable = Vec::new();

    for utxo in utxos {
        let conf = confirmations(virtual_daa, utxo.utxo_entry.block_daa_score);
        let need = required_depth(utxo.utxo_entry.is_coinbase, deposit_confirmations);
        let amount = utxo.utxo_entry.amount;
        if conf < need {
            pending_sompi = pending_sompi
                .checked_add(amount)
                .ok_or_else(|| EngineError::Message("pending CEX balance overflow".into()))?;
            if utxo.utxo_entry.is_coinbase {
                immature_coinbase_sompi = immature_coinbase_sompi
                    .checked_add(amount)
                    .ok_or_else(|| EngineError::Message("coinbase CEX balance overflow".into()))?;
            }
            continue;
        }
        confirmed_sompi = confirmed_sompi
            .checked_add(amount)
            .ok_or_else(|| EngineError::Message("confirmed CEX balance overflow".into()))?;
        spendable.push(CexSpendable {
            tx_id: utxo.outpoint.transaction_id.clone(),
            index: utxo.outpoint.index,
            amount_sompi: amount,
            confirmations: conf,
            is_coinbase: utxo.utxo_entry.is_coinbase,
        });
    }

    Ok(CexAddressSnapshot {
        address: address.to_string(),
        virtual_daa,
        confirmed_sompi,
        pending_sompi,
        immature_coinbase_sompi,
        spendable,
    })
}

fn validate_snapshot(address: &str, utxos: &[AddressUtxo]) -> Result<()> {
    let mut outpoints = HashSet::new();
    for utxo in utxos {
        if utxo.address != address {
            return Err(EngineError::Message(format!(
                "CEX snapshot contains foreign address {} while evaluating {address}",
                utxo.address
            )));
        }
        if !outpoints.insert((utxo.outpoint.transaction_id.clone(), utxo.outpoint.index)) {
            return Err(EngineError::Message(
                "CEX snapshot contains duplicate outpoint".into(),
            ));
        }
    }
    Ok(())
}

/// Local mempool double-spend guard for watched outpoints. Not kaspad's mempool.
#[derive(Debug, Default, Clone)]
pub struct OutpointSpendGuard {
    spent: std::collections::HashSet<(String, u32)>,
}

impl OutpointSpendGuard {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns false if any input was already reserved.
    pub fn reserve(&mut self, inputs: &[(String, u32)]) -> bool {
        if inputs.iter().any(|op| self.spent.contains(op)) {
            return false;
        }
        for op in inputs {
            self.spent.insert(op.clone());
        }
        true
    }

    pub fn release(&mut self, inputs: &[(String, u32)]) {
        for op in inputs {
            self.spent.remove(op);
        }
    }
}

pub fn snapshot_address_default(
    address: &str,
    virtual_daa: u64,
    utxos: &[AddressUtxo],
) -> Result<CexAddressSnapshot> {
    snapshot_address(address, virtual_daa, utxos, DEFAULT_DEPOSIT_CONFIRMATIONS)
}

/// DAG once + concurrent per-address UTXOs, then the same credit split as `snapshot_address`.
pub async fn fetch_address_snapshots(
    rest: &crate::rest::Tn10RestClient,
    addrs: &[String],
    deposit_confirmations: u64,
) -> crate::error::Result<Vec<CexAddressSnapshot>> {
    if addrs.is_empty() {
        return Err(crate::error::EngineError::Message(
            "addresses array is empty".into(),
        ));
    }
    let (dag, utxos) = tokio::join!(rest.block_dag_info(), rest.utxos_for_addresses(addrs));
    let dag = dag?;
    let utxos = utxos?;
    let requested: HashSet<_> = addrs.iter().map(String::as_str).collect();
    let mut seen = HashSet::new();
    for utxo in &utxos {
        if !requested.contains(utxo.address.as_str()) {
            return Err(EngineError::Message(format!(
                "REST returned unrequested CEX address {}",
                utxo.address
            )));
        }
        if !seen.insert((utxo.outpoint.transaction_id.clone(), utxo.outpoint.index)) {
            return Err(EngineError::Message(
                "REST returned duplicate CEX outpoint".into(),
            ));
        }
    }
    addrs
        .iter()
        .map(|addr| {
            let rows: Vec<_> = utxos
                .iter()
                .filter(|utxo| utxo.address == *addr)
                .cloned()
                .collect();
            snapshot_address(addr, dag.virtual_daa_score, &rows, deposit_confirmations)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rest::{AddressUtxo, Outpoint, ScriptPublicKey, UtxoEntry};

    fn row(
        address: &str,
        tx: &str,
        index: u32,
        amount: u64,
        block_daa: u64,
        is_coinbase: bool,
    ) -> AddressUtxo {
        AddressUtxo {
            address: address.to_string(),
            outpoint: Outpoint {
                transaction_id: tx.to_string(),
                index,
            },
            utxo_entry: UtxoEntry {
                amount,
                script_public_key: ScriptPublicKey {
                    script_public_key: None,
                    version: 0,
                },
                block_daa_score: block_daa,
                is_coinbase,
                covenant_id: None,
                storage_mass: None,
            },
        }
    }

    #[test]
    fn rejects_foreign_rows_and_keeps_fresh_utxos_pending() {
        let addr = "kaspatest:qqexample";
        let foreign = [
            row(addr, "aa", 0, 1_000, 100, false),
            row("kaspatest:other", "bb", 0, 9_000, 1, false),
        ];
        assert!(snapshot_address(addr, 160, &foreign, 60).is_err());

        let utxos = [
            row(addr, "aa", 0, 1_000, 100, false),
            row(addr, "cc", 1, 50, 159, false),
        ];
        let snap = snapshot_address(addr, 160, &utxos, 60).unwrap();
        assert_eq!(snap.confirmed_sompi, 1_000);
        assert_eq!(snap.pending_sompi, 50);
        assert_eq!(snap.spendable.len(), 1);
        assert_eq!(snap.spendable[0].tx_id, "aa");
    }

    #[test]
    fn coinbase_needs_maturity_not_deposit_depth() {
        let addr = "kaspatest:qqminer";
        let utxos = [row(addr, "cb", 0, 500_000_000, 100, true)];
        let snap = snapshot_address(addr, 160, &utxos, 60).unwrap();
        assert_eq!(snap.confirmed_sompi, 0);
        assert_eq!(snap.immature_coinbase_sompi, 500_000_000);
        let mature = snapshot_address(addr, 1100, &utxos, 60).unwrap();
        assert_eq!(mature.confirmed_sompi, 500_000_000);
        assert_eq!(mature.immature_coinbase_sompi, 0);
    }

    #[test]
    fn pdf_zero_hash_indexer_would_fail_these_checks() {
        let snap = snapshot_address_default("kaspatest:qqreal", 100, &[]).unwrap();
        assert_eq!(snap.confirmed_sompi, 0);
        assert!(snap.spendable.is_empty());
    }

    #[test]
    fn outpoint_guard_rejects_double_spend() {
        let mut guard = OutpointSpendGuard::new();
        assert!(guard.reserve(&[("aa".into(), 0)]));
        assert!(!guard.reserve(&[("aa".into(), 0)]));
        guard.release(&[("aa".into(), 0)]);
        assert!(guard.reserve(&[("aa".into(), 0)]));
    }

    #[test]
    fn rejects_duplicate_outpoints_before_summing() {
        let addr = "kaspatest:qqexample";
        let row = row(addr, "aa", 0, 1_000, 100, false);
        assert!(snapshot_address(addr, 160, &[row.clone(), row], 60).is_err());
    }

    #[test]
    fn empty_fetch_snapshots_errors_without_http() {
        let rest = crate::rest::Tn10RestClient::new("https://example.invalid").unwrap();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let err = rt
            .block_on(fetch_address_snapshots(&rest, &[], 60))
            .unwrap_err();
        assert!(err.to_string().contains("empty"));
    }
}
