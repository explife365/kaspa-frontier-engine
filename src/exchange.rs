//! Deposit confirmation tracker.
//!
//! Gemini's listener emitted a deposit only when `virtual_daa - utxo_daa >= 60`
//! on the same UTXO-added event. At add time that delta is ~0, so credits
//! never fire. Track pending UTXOs and confirm on later DAA updates.

use crate::network::{COINBASE_MATURITY_DAA, DEFAULT_DEPOSIT_CONFIRMATIONS};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingDeposit {
    pub tx_id: String,
    pub output_index: u32,
    pub address: String,
    pub amount_sompi: u64,
    pub block_daa_score: u64,
    pub is_coinbase: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfirmedDeposit {
    pub tx_id: String,
    pub output_index: u32,
    pub address: String,
    pub amount_sompi: u64,
    pub block_daa_score: u64,
    pub confirmations: u64,
    pub is_coinbase: bool,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct DepositKey {
    tx_id: String,
    output_index: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UtxoAppearance {
    pub tx_id: String,
    pub output_index: u32,
    pub address: String,
    pub amount_sompi: u64,
    pub block_daa_score: u64,
    pub virtual_daa: u64,
    pub is_coinbase: bool,
}

#[derive(Debug)]
pub struct DepositTracker {
    required_confirmations: u64,
    pending: HashMap<DepositKey, PendingDeposit>,
}

impl DepositTracker {
    pub fn new(required_confirmations: u64) -> Self {
        Self {
            required_confirmations: required_confirmations.max(1),
            pending: HashMap::new(),
        }
    }

    pub fn with_default_confirmations() -> Self {
        Self::new(DEFAULT_DEPOSIT_CONFIRMATIONS)
    }

    pub fn required_confirmations(&self) -> u64 {
        self.required_confirmations
    }

    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    pub fn required_for(&self, is_coinbase: bool) -> u64 {
        if is_coinbase {
            self.required_confirmations.max(COINBASE_MATURITY_DAA)
        } else {
            self.required_confirmations
        }
    }

    /// Record a newly appeared UTXO. Returns Some only if it is already deep
    /// enough (reorg catch-up / indexer lag). Coinbase uses 1000 DAA maturity.
    pub fn on_utxo_added(&mut self, seen: UtxoAppearance) -> Option<ConfirmedDeposit> {
        let pending = PendingDeposit {
            tx_id: seen.tx_id,
            output_index: seen.output_index,
            address: seen.address,
            amount_sompi: seen.amount_sompi,
            block_daa_score: seen.block_daa_score,
            is_coinbase: seen.is_coinbase,
        };
        let required = self.required_for(seen.is_coinbase);
        let confirmations = seen.virtual_daa.saturating_sub(seen.block_daa_score);
        if confirmations >= required {
            return Some(ConfirmedDeposit {
                tx_id: pending.tx_id,
                output_index: pending.output_index,
                address: pending.address,
                amount_sompi: pending.amount_sompi,
                block_daa_score: pending.block_daa_score,
                confirmations,
                is_coinbase: pending.is_coinbase,
            });
        }
        let key = DepositKey {
            tx_id: pending.tx_id.clone(),
            output_index: pending.output_index,
        };
        self.pending.insert(key, pending);
        None
    }

    /// Drop a spent / reorged UTXO so it cannot confirm later.
    pub fn on_utxo_removed(&mut self, tx_id: &str, output_index: u32) -> bool {
        self.pending
            .remove(&DepositKey {
                tx_id: tx_id.to_string(),
                output_index,
            })
            .is_some()
    }

    pub fn on_virtual_daa(&mut self, virtual_daa: u64) -> Vec<ConfirmedDeposit> {
        let deposit_required = self.required_confirmations;
        let mut confirmed = Vec::new();
        self.pending.retain(|_, pending| {
            let required = if pending.is_coinbase {
                deposit_required.max(COINBASE_MATURITY_DAA)
            } else {
                deposit_required
            };
            let confirmations = virtual_daa.saturating_sub(pending.block_daa_score);
            if confirmations >= required {
                confirmed.push(ConfirmedDeposit {
                    tx_id: pending.tx_id.clone(),
                    output_index: pending.output_index,
                    address: pending.address.clone(),
                    amount_sompi: pending.amount_sompi,
                    block_daa_score: pending.block_daa_score,
                    confirmations,
                    is_coinbase: pending.is_coinbase,
                });
                false
            } else {
                true
            }
        });
        confirmed
    }
}

/// Outbound payment an exchange would not mark complete until DAA depth.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfirmedWithdrawal {
    pub tx_id: String,
    pub dest: String,
    pub amount_sompi: u64,
    pub output_index: u32,
    pub block_daa_score: u64,
    pub confirmations: u64,
}

/// Destination UTXO facts needed to confirm a withdrawal (no REST types here).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WithdrawalUtxo {
    pub tx_id: String,
    pub output_index: u32,
    pub amount_sompi: u64,
    pub block_daa_score: u64,
    /// REST dest address. Empty skips the dest check (unit tests).
    pub address: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WithdrawalExpectation {
    pub tx_id: String,
    pub dest: String,
    pub amount_sompi: u64,
    pub output_index: u32,
}

pub fn daa_confirmations(virtual_daa: u64, block_daa_score: u64) -> u64 {
    virtual_daa.saturating_sub(block_daa_score)
}

/// `None` until the dest UTXO exists *and* `virtual_daa - block_daa >= required`.
/// Submitting a tx is not enough — that was Gemini's deposit bug in reverse.
pub fn confirm_withdrawal(
    expected: &WithdrawalExpectation,
    virtual_daa: u64,
    dest_utxos: &[WithdrawalUtxo],
    required: u64,
) -> Option<ConfirmedWithdrawal> {
    let required = required.max(1);
    dest_utxos.iter().find_map(|utxo| {
        if utxo.tx_id != expected.tx_id
            || utxo.output_index != expected.output_index
            || utxo.amount_sompi != expected.amount_sompi
            || utxo.address != expected.dest
        {
            return None;
        }
        let confirmations = daa_confirmations(virtual_daa, utxo.block_daa_score);
        if confirmations < required {
            return None;
        }
        Some(ConfirmedWithdrawal {
            tx_id: utxo.tx_id.clone(),
            dest: expected.dest.clone(),
            amount_sompi: utxo.amount_sompi,
            output_index: utxo.output_index,
            block_daa_score: utxo.block_daa_score,
            confirmations,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn appear(
        tx_id: &str,
        output_index: u32,
        address: &str,
        amount_sompi: u64,
        block_daa_score: u64,
        virtual_daa: u64,
        is_coinbase: bool,
    ) -> UtxoAppearance {
        UtxoAppearance {
            tx_id: tx_id.into(),
            output_index,
            address: address.into(),
            amount_sompi,
            block_daa_score,
            virtual_daa,
            is_coinbase,
        }
    }

    #[test]
    fn gemini_bug_would_drop_fresh_utxo_this_confirms_later() {
        let mut tracker = DepositTracker::new(60);
        let none = tracker.on_utxo_added(appear(
            "tx1",
            0,
            "kaspatest:abc",
            100_000_000,
            1000,
            1000,
            false,
        ));
        assert!(none.is_none());
        assert_eq!(tracker.pending_count(), 1);

        assert!(tracker.on_virtual_daa(1050).is_empty());
        let confirmed = tracker.on_virtual_daa(1060);
        assert_eq!(confirmed.len(), 1);
        assert_eq!(confirmed[0].confirmations, 60);
        assert_eq!(confirmed[0].output_index, 0);
        assert_eq!(tracker.pending_count(), 0);
    }

    #[test]
    fn already_deep_utxo_confirms_immediately() {
        let mut tracker = DepositTracker::new(10);
        let hit = tracker.on_utxo_added(appear("tx2", 0, "kaspatest:abc", 1, 100, 200, false));
        assert_eq!(hit.unwrap().confirmations, 100);
        assert_eq!(tracker.pending_count(), 0);
    }

    #[test]
    fn two_outputs_same_tx_confirm_independently() {
        let mut tracker = DepositTracker::new(2);
        tracker.on_utxo_added(appear("tx", 0, "kaspatest:a", 1, 10, 10, false));
        tracker.on_utxo_added(appear("tx", 1, "kaspatest:a", 2, 10, 10, false));
        assert_eq!(tracker.pending_count(), 2);
        let confirmed = tracker.on_virtual_daa(12);
        assert_eq!(confirmed.len(), 2);
    }

    #[test]
    fn spent_utxo_does_not_confirm() {
        let mut tracker = DepositTracker::new(5);
        tracker.on_utxo_added(appear("tx3", 0, "kaspatest:abc", 1, 1, 1, false));
        assert!(tracker.on_utxo_removed("tx3", 0));
        assert!(!tracker.on_utxo_removed("tx3", 0));
        assert!(tracker.on_virtual_daa(100).is_empty());
    }

    #[test]
    fn default_confirmations_is_at_least_one() {
        let tracker = DepositTracker::new(0);
        assert_eq!(tracker.required_confirmations(), 1);
        let defaults = DepositTracker::with_default_confirmations();
        assert_eq!(
            defaults.required_confirmations(),
            DEFAULT_DEPOSIT_CONFIRMATIONS
        );
    }

    #[test]
    fn coinbase_waits_for_maturity_not_deposit_depth() {
        let mut tracker = DepositTracker::new(60);
        assert_eq!(tracker.required_for(true), COINBASE_MATURITY_DAA);
        let none = tracker.on_utxo_added(appear("cb", 0, "kaspatest:miner", 5, 100, 200, true));
        assert!(none.is_none());
        assert!(tracker.on_virtual_daa(159).is_empty());
        let confirmed = tracker.on_virtual_daa(1100);
        assert_eq!(confirmed.len(), 1);
        assert!(confirmed[0].is_coinbase);
        assert_eq!(confirmed[0].confirmations, 1000);
    }

    #[test]
    fn withdrawal_waits_for_daa_not_just_appearance() {
        let utxos = [WithdrawalUtxo {
            tx_id: "w1".into(),
            output_index: 0,
            amount_sompi: 25_000_000,
            block_daa_score: 1000,
            address: "kaspatest:bob".into(),
        }];
        let expected = WithdrawalExpectation {
            tx_id: "w1".into(),
            dest: "kaspatest:bob".into(),
            amount_sompi: 25_000_000,
            output_index: 0,
        };
        assert!(confirm_withdrawal(&expected, 1000, &utxos, 60).is_none());
        assert!(confirm_withdrawal(&expected, 1059, &utxos, 60).is_none());
        let hit = confirm_withdrawal(&expected, 1060, &utxos, 60).unwrap();
        assert_eq!(hit.confirmations, 60);
        assert_eq!(hit.amount_sompi, 25_000_000);
        let mut mismatch = expected.clone();
        mismatch.tx_id = "other".into();
        assert!(confirm_withdrawal(&mismatch, 2000, &utxos, 1).is_none());
        mismatch = expected.clone();
        mismatch.amount_sompi += 1;
        assert!(confirm_withdrawal(&mismatch, 2000, &utxos, 1).is_none());
        mismatch = expected.clone();
        mismatch.output_index = 1;
        assert!(confirm_withdrawal(&mismatch, 2000, &utxos, 1).is_none());
        let wrong_dest = [WithdrawalUtxo {
            tx_id: "w1".into(),
            output_index: 0,
            amount_sompi: 25_000_000,
            block_daa_score: 1000,
            address: "kaspatest:eve".into(),
        }];
        assert!(confirm_withdrawal(&expected, 2000, &wrong_dest, 1).is_none());
    }
}
