//! Poll explorer UTXOs and drive [`DepositTracker`].
//! The tracker was previously only unit-tested; nothing called REST.

use crate::error::{EngineError, Result};
use crate::exchange::{
    confirm_withdrawal, ConfirmedDeposit, ConfirmedWithdrawal, DepositTracker, UtxoAppearance,
    WithdrawalExpectation, WithdrawalUtxo,
};
use crate::network::is_testnet_address;
use crate::rest::{AddressUtxo, Tn10RestClient};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Default)]
pub struct WatchTick {
    pub virtual_daa: u64,
    pub confirmed: Vec<ConfirmedDeposit>,
    pub newly_seen: usize,
    pub disappeared: usize,
    pub disappeared_outpoints: Vec<(String, u32)>,
    pub observed: Vec<UtxoAppearance>,
    pub pending: usize,
    pub utxo_sum_sompi: u64,
    pub rest_balance_sompi: Option<u64>,
}

pub struct DepositWatch {
    tracker: DepositTracker,
    seen: HashSet<(String, u32)>,
    credited: HashSet<(String, u32)>,
}

impl DepositWatch {
    pub fn new(required_confirmations: u64) -> Self {
        Self {
            tracker: DepositTracker::new(required_confirmations),
            seen: HashSet::new(),
            credited: HashSet::new(),
        }
    }

    pub fn pending_count(&self) -> usize {
        self.tracker.pending_count()
    }

    /// Replay credits from a previous process so REST catch-up does not double-pay.
    pub fn import_credited(&mut self, entries: impl IntoIterator<Item = (String, u32)>) {
        self.credited.extend(entries);
    }

    pub fn export_credited(&self) -> Vec<(String, u32)> {
        let mut keys: Vec<_> = self.credited.iter().cloned().collect();
        keys.sort();
        keys
    }

    /// Diff a UTXO snapshot against the last tick. First call treats every
    /// current UTXO as newly seen (indexer catch-up).
    pub fn apply(&mut self, virtual_daa: u64, utxos: &[AddressUtxo]) -> WatchTick {
        let current: HashMap<(String, u32), &AddressUtxo> = utxos
            .iter()
            .map(|u| ((u.outpoint.transaction_id.clone(), u.outpoint.index), u))
            .collect();

        let mut newly_seen = 0;
        let mut immediate = Vec::new();
        for (key, utxo) in &current {
            if self.seen.contains(key) {
                continue;
            }
            newly_seen += 1;
            if let Some(hit) = self.tracker.on_utxo_added(UtxoAppearance {
                tx_id: utxo.outpoint.transaction_id.clone(),
                output_index: utxo.outpoint.index,
                address: utxo.address.clone(),
                amount_sompi: utxo.utxo_entry.amount,
                block_daa_score: utxo.utxo_entry.block_daa_score,
                virtual_daa,
                is_coinbase: utxo.utxo_entry.is_coinbase,
            }) {
                immediate.push(hit);
            }
        }

        let mut disappeared = 0;
        let mut disappeared_outpoints = Vec::new();
        for key in self.seen.iter() {
            if !current.contains_key(key) {
                disappeared += 1;
                disappeared_outpoints.push(key.clone());
                self.tracker.on_utxo_removed(&key.0, key.1);
            }
        }

        let mut confirmed = self.tracker.on_virtual_daa(virtual_daa);
        confirmed.extend(immediate);
        confirmed.retain(|hit| self.credited.insert((hit.tx_id.clone(), hit.output_index)));

        self.seen = current.keys().cloned().collect();
        WatchTick {
            virtual_daa,
            confirmed,
            newly_seen,
            disappeared,
            disappeared_outpoints,
            observed: current
                .values()
                .map(|utxo| UtxoAppearance {
                    tx_id: utxo.outpoint.transaction_id.clone(),
                    output_index: utxo.outpoint.index,
                    address: utxo.address.clone(),
                    amount_sompi: utxo.utxo_entry.amount,
                    block_daa_score: utxo.utxo_entry.block_daa_score,
                    virtual_daa,
                    is_coinbase: utxo.utxo_entry.is_coinbase,
                })
                .collect(),
            pending: self.tracker.pending_count(),
            utxo_sum_sompi: utxos.iter().map(|u| u.utxo_entry.amount).sum(),
            rest_balance_sompi: None,
        }
    }

    pub async fn poll(&mut self, client: &Tn10RestClient, address: &str) -> Result<WatchTick> {
        if !is_testnet_address(address) {
            return Err(EngineError::NotTestnetAddress(address.to_string()));
        }
        let (dag, utxos, balance) = client.address_snapshot(address).await?;
        validate_snapshot(address, &utxos)?;
        let mut tick = self.apply(dag.virtual_daa_score, &utxos);
        tick.rest_balance_sompi = balance;
        Ok(tick)
    }
}

fn validate_snapshot(address: &str, utxos: &[AddressUtxo]) -> Result<()> {
    let mut outpoints = HashSet::new();
    for utxo in utxos {
        if utxo.address != address {
            return Err(EngineError::Message(format!(
                "REST returned foreign address {} while watching {address}",
                utxo.address
            )));
        }
        let key = (utxo.outpoint.transaction_id.clone(), utxo.outpoint.index);
        if !outpoints.insert(key) {
            return Err(EngineError::Message(
                "REST returned a duplicate outpoint".into(),
            ));
        }
    }
    Ok(())
}

pub fn withdrawal_utxos(utxos: &[AddressUtxo]) -> Vec<WithdrawalUtxo> {
    utxos
        .iter()
        .map(|utxo| WithdrawalUtxo {
            tx_id: utxo.outpoint.transaction_id.clone(),
            output_index: utxo.outpoint.index,
            amount_sompi: utxo.utxo_entry.amount,
            block_daa_score: utxo.utxo_entry.block_daa_score,
            address: utxo.address.clone(),
        })
        .collect()
}

/// REST poll: dest UTXO must exist *and* have `required` DAA confirmations.
pub async fn poll_withdrawal(
    client: &Tn10RestClient,
    expected: &WithdrawalExpectation,
    required: u64,
) -> Result<Option<ConfirmedWithdrawal>> {
    if !is_testnet_address(&expected.dest) {
        return Err(EngineError::NotTestnetAddress(expected.dest.clone()));
    }
    let (dag, utxos) = tokio::join!(
        client.block_dag_info(),
        client.utxos_for_address(&expected.dest)
    );
    Ok(confirm_withdrawal(
        expected,
        dag?.virtual_daa_score,
        &withdrawal_utxos(&utxos?),
        required,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rest::{AddressUtxo, Outpoint, ScriptPublicKey, UtxoEntry};

    fn utxo(txid: &str, index: u32, amount: u64, daa: u64, coinbase: bool) -> AddressUtxo {
        AddressUtxo {
            address: "kaspatest:abc".into(),
            outpoint: Outpoint {
                transaction_id: txid.into(),
                index,
            },
            utxo_entry: UtxoEntry {
                amount,
                script_public_key: ScriptPublicKey {
                    script_public_key: Some("00".into()),
                    version: 0,
                },
                block_daa_score: daa,
                is_coinbase: coinbase,
                covenant_id: None,
                storage_mass: None,
            },
        }
    }

    #[test]
    fn snapshot_diff_confirms_then_drops_spent() {
        let mut watch = DepositWatch::new(10);
        let first = watch.apply(100, &[utxo("tx", 0, 1, 100, false)]);
        assert_eq!(first.newly_seen, 1);
        assert!(first.confirmed.is_empty());
        assert_eq!(first.pending, 1);
        assert_eq!(first.utxo_sum_sompi, 1);

        let later = watch.apply(110, &[utxo("tx", 0, 1, 100, false)]);
        assert_eq!(later.newly_seen, 0);
        assert_eq!(later.confirmed.len(), 1);

        let spent = watch.apply(120, &[]);
        assert_eq!(spent.disappeared, 1);
        assert_eq!(spent.pending, 0);
    }

    #[test]
    fn restart_does_not_double_credit() {
        let mut first = DepositWatch::new(10);
        let credited = first.apply(110, &[utxo("tx", 0, 1, 100, false)]);
        assert_eq!(credited.confirmed.len(), 1);

        let mut restarted = DepositWatch::new(10);
        restarted.import_credited(first.export_credited());
        let again = restarted.apply(120, &[utxo("tx", 0, 1, 100, false)]);
        assert!(again.confirmed.is_empty());
        assert_eq!(again.newly_seen, 1);
    }

    #[test]
    fn coinbase_does_not_confirm_at_normal_depth() {
        let mut watch = DepositWatch::new(10);
        let tick = watch.apply(50, &[utxo("cb", 0, 5, 40, true)]);
        assert!(tick.confirmed.is_empty());
        assert_eq!(tick.pending, 1);
    }

    #[test]
    fn rejects_foreign_and_duplicate_snapshot_rows() {
        let row = utxo("tx", 0, 1, 100, false);
        assert!(validate_snapshot("kaspatest:abc", &[row.clone(), row]).is_err());
        let foreign = AddressUtxo {
            address: "kaspatest:other".into(),
            ..utxo("tx", 0, 1, 100, false)
        };
        assert!(validate_snapshot("kaspatest:abc", &[foreign]).is_err());
    }

    #[test]
    fn rest_utxos_confirm_withdrawal_only_after_daa() {
        let rows = [utxo("w1", 0, 25_000_000, 1000, false)];
        let mapped = withdrawal_utxos(&rows);
        let expected = WithdrawalExpectation {
            tx_id: "w1".into(),
            dest: "kaspatest:abc".into(),
            amount_sompi: 25_000_000,
            output_index: 0,
        };
        assert!(confirm_withdrawal(&expected, 1059, &mapped, 60).is_none());
        let hit = confirm_withdrawal(&expected, 1060, &mapped, 60).unwrap();
        assert_eq!(hit.output_index, 0);
        assert_eq!(hit.amount_sompi, 25_000_000);
    }
}
