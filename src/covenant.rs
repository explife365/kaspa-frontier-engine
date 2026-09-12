//! Off-chain covenant *policy* checks (timelock + destination).
//!
//! Toccata covenants execute inside kaspad. Gemini's `verify_zk_groth16`
//! returned `true` for every proof — that is a backdoor, not verification.
//! This engine never claims a ZK proof is valid.

use std::collections::HashMap;
use thiserror::Error;

#[derive(Error, Debug, PartialEq, Eq)]
pub enum CovenantError {
    #[error("covenant not registered: {0}")]
    Missing(String),
    #[error("timelock active: need DAA {expected}, current {current}")]
    TimelockActive { expected: u64, current: u64 },
    #[error("destination hash does not match covenant policy")]
    DestinationMismatch,
    #[error("hashlock preimage does not match payment_hash")]
    HashlockMismatch,
    #[error("HTLC already settled (status {status})")]
    HtlcSettled { status: u8 },
    #[error("claim window closed: current DAA {current} >= refund DAA {refund_daa}")]
    ClaimWindowClosed { refund_daa: u64, current: u64 },
    #[error("refund before timelock: need DAA {refund_daa}, current {current}")]
    RefundBeforeTimelock { refund_daa: u64, current: u64 },
    #[error("escrow already settled (status {status})")]
    EscrowSettled { status: u8 },
    #[error("escrow release requires seller + arbiter actor hashes")]
    EscrowReleaseActors,
    #[error("escrow buyer refund requires buyer + arbiter actor hashes")]
    EscrowBuyerRefundActors,
    #[error("ZK proofs must be verified by kaspad OpZkPrecompile, not this client")]
    ZkMustUseNode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeCovenantUtxo {
    pub utxo_id: String,
    pub amount_sompi: u64,
    pub covenant_script_hash: [u8; 32],
    pub min_daa_timelock: u64,
    pub allowed_destination_hash: Option<[u8; 32]>,
}

#[derive(Default, Debug)]
pub struct CovenantPolicyEngine {
    covenants: HashMap<String, NativeCovenantUtxo>,
}

/// Off-chain HTLC policy (int hashlock demo; not SHA256 verification).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HtlcUtxo {
    pub utxo_id: String,
    pub amount_sompi: u64,
    pub payment_hash: u64,
    pub refund_daa: u64,
    pub status: u8,
}

#[derive(Default, Debug)]
pub struct HtlcPolicyEngine {
    utxos: HashMap<String, HtlcUtxo>,
}

/// Off-chain 2-of-3 escrow policy (int actor tags; not pubkey verification).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Escrow2of3Utxo {
    pub utxo_id: String,
    pub amount_sompi: u64,
    pub buyer_hash: u64,
    pub seller_hash: u64,
    pub arbiter_hash: u64,
    pub refund_daa: u64,
    pub status: u8,
}

#[derive(Default, Debug)]
pub struct Escrow2of3PolicyEngine {
    utxos: HashMap<String, Escrow2of3Utxo>,
}

impl Escrow2of3PolicyEngine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, utxo: Escrow2of3Utxo) {
        self.utxos.insert(utxo.utxo_id.clone(), utxo);
    }

    fn pair_ok(a: u64, b: u64, left: u64, right: u64) -> bool {
        (a == left && b == right) || (a == right && b == left)
    }

    pub fn release_to_seller(
        &mut self,
        utxo_id: &str,
        current_daa: u64,
        actor1: u64,
        actor2: u64,
    ) -> Result<Escrow2of3Utxo, CovenantError> {
        let utxo = self
            .utxos
            .get(utxo_id)
            .ok_or_else(|| CovenantError::Missing(utxo_id.to_string()))?;
        if utxo.status != 0 {
            return Err(CovenantError::EscrowSettled { status: utxo.status });
        }
        if current_daa >= utxo.refund_daa {
            return Err(CovenantError::ClaimWindowClosed {
                refund_daa: utxo.refund_daa,
                current: current_daa,
            });
        }
        if !Self::pair_ok(actor1, actor2, utxo.seller_hash, utxo.arbiter_hash) {
            return Err(CovenantError::EscrowReleaseActors);
        }
        let mut spent = self
            .utxos
            .remove(utxo_id)
            .ok_or_else(|| CovenantError::Missing(utxo_id.to_string()))?;
        spent.status = 1;
        Ok(spent)
    }

    pub fn refund_to_buyer(
        &mut self,
        utxo_id: &str,
        current_daa: u64,
        actor1: u64,
        actor2: u64,
    ) -> Result<Escrow2of3Utxo, CovenantError> {
        let utxo = self
            .utxos
            .get(utxo_id)
            .ok_or_else(|| CovenantError::Missing(utxo_id.to_string()))?;
        if utxo.status != 0 {
            return Err(CovenantError::EscrowSettled { status: utxo.status });
        }
        if current_daa >= utxo.refund_daa {
            return Err(CovenantError::ClaimWindowClosed {
                refund_daa: utxo.refund_daa,
                current: current_daa,
            });
        }
        if !Self::pair_ok(actor1, actor2, utxo.buyer_hash, utxo.arbiter_hash) {
            return Err(CovenantError::EscrowBuyerRefundActors);
        }
        let mut spent = self
            .utxos
            .remove(utxo_id)
            .ok_or_else(|| CovenantError::Missing(utxo_id.to_string()))?;
        spent.status = 2;
        Ok(spent)
    }

    pub fn timeout_to_buyer(
        &mut self,
        utxo_id: &str,
        current_daa: u64,
    ) -> Result<Escrow2of3Utxo, CovenantError> {
        let utxo = self
            .utxos
            .get(utxo_id)
            .ok_or_else(|| CovenantError::Missing(utxo_id.to_string()))?;
        if utxo.status != 0 {
            return Err(CovenantError::EscrowSettled { status: utxo.status });
        }
        if current_daa < utxo.refund_daa {
            return Err(CovenantError::RefundBeforeTimelock {
                refund_daa: utxo.refund_daa,
                current: current_daa,
            });
        }
        let mut spent = self
            .utxos
            .remove(utxo_id)
            .ok_or_else(|| CovenantError::Missing(utxo_id.to_string()))?;
        spent.status = 3;
        Ok(spent)
    }
}

impl HtlcPolicyEngine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, utxo: HtlcUtxo) {
        self.utxos.insert(utxo.utxo_id.clone(), utxo);
    }

    pub fn claim(
        &mut self,
        utxo_id: &str,
        current_daa: u64,
        preimage_hash: u64,
    ) -> Result<HtlcUtxo, CovenantError> {
        let utxo = self
            .utxos
            .get(utxo_id)
            .ok_or_else(|| CovenantError::Missing(utxo_id.to_string()))?;
        if utxo.status != 0 {
            return Err(CovenantError::HtlcSettled { status: utxo.status });
        }
        if current_daa >= utxo.refund_daa {
            return Err(CovenantError::ClaimWindowClosed {
                refund_daa: utxo.refund_daa,
                current: current_daa,
            });
        }
        if preimage_hash != utxo.payment_hash {
            return Err(CovenantError::HashlockMismatch);
        }
        let mut spent = self
            .utxos
            .remove(utxo_id)
            .ok_or_else(|| CovenantError::Missing(utxo_id.to_string()))?;
        spent.status = 1;
        Ok(spent)
    }

    pub fn refund(&mut self, utxo_id: &str, current_daa: u64) -> Result<HtlcUtxo, CovenantError> {
        let utxo = self
            .utxos
            .get(utxo_id)
            .ok_or_else(|| CovenantError::Missing(utxo_id.to_string()))?;
        if utxo.status != 0 {
            return Err(CovenantError::HtlcSettled { status: utxo.status });
        }
        if current_daa < utxo.refund_daa {
            return Err(CovenantError::RefundBeforeTimelock {
                refund_daa: utxo.refund_daa,
                current: current_daa,
            });
        }
        let mut spent = self
            .utxos
            .remove(utxo_id)
            .ok_or_else(|| CovenantError::Missing(utxo_id.to_string()))?;
        spent.status = 2;
        Ok(spent)
    }
}

impl CovenantPolicyEngine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, utxo: NativeCovenantUtxo) {
        self.covenants.insert(utxo.utxo_id.clone(), utxo);
    }

    pub fn contains(&self, utxo_id: &str) -> bool {
        self.covenants.contains_key(utxo_id)
    }

    pub fn inspect(&self, utxo_id: &str) -> Option<&NativeCovenantUtxo> {
        self.covenants.get(utxo_id)
    }

    /// Local policy check only. Pass `zk_attached = true` to refuse (honest).
    /// Toccata ZK runs in kaspad OpZkPrecompile, not here.
    pub fn verify_and_spend(
        &mut self,
        utxo_id: &str,
        current_daa_score: u64,
        destination_script_hash: &[u8; 32],
        zk_attached: bool,
    ) -> Result<NativeCovenantUtxo, CovenantError> {
        if zk_attached {
            return Err(CovenantError::ZkMustUseNode);
        }
        let covenant = self
            .covenants
            .get(utxo_id)
            .ok_or_else(|| CovenantError::Missing(utxo_id.to_string()))?;
        if current_daa_score < covenant.min_daa_timelock {
            return Err(CovenantError::TimelockActive {
                expected: covenant.min_daa_timelock,
                current: current_daa_score,
            });
        }
        if let Some(allowed) = covenant.allowed_destination_hash {
            if &allowed != destination_script_hash {
                return Err(CovenantError::DestinationMismatch);
            }
        }
        self.covenants
            .remove(utxo_id)
            .ok_or_else(|| CovenantError::Missing(utxo_id.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(lock: u64, dest: Option<[u8; 32]>) -> NativeCovenantUtxo {
        NativeCovenantUtxo {
            utxo_id: "utxo-1".into(),
            amount_sompi: 100_000_000,
            covenant_script_hash: [1u8; 32],
            min_daa_timelock: lock,
            allowed_destination_hash: dest,
        }
    }

    #[test]
    fn timelock_and_spend() {
        let mut engine = CovenantPolicyEngine::new();
        engine.register(sample(50, None));
        assert!(engine.inspect("utxo-1").is_some());
        assert_eq!(
            engine.verify_and_spend("utxo-1", 49, &[0u8; 32], false),
            Err(CovenantError::TimelockActive {
                expected: 50,
                current: 49
            })
        );
        assert!(engine
            .verify_and_spend("utxo-1", 50, &[0u8; 32], false)
            .is_ok());
        assert!(!engine.contains("utxo-1"));
    }

    #[test]
    fn refuses_stub_zk() {
        let mut engine = CovenantPolicyEngine::new();
        engine.register(sample(0, None));
        assert_eq!(
            engine.verify_and_spend("utxo-1", 1, &[0u8; 32], true),
            Err(CovenantError::ZkMustUseNode)
        );
    }

    #[test]
    fn destination_restriction() {
        let mut engine = CovenantPolicyEngine::new();
        engine.register(sample(0, Some([9u8; 32])));
        assert_eq!(
            engine.verify_and_spend("utxo-1", 1, &[8u8; 32], false),
            Err(CovenantError::DestinationMismatch)
        );
        assert!(engine
            .verify_and_spend("utxo-1", 1, &[9u8; 32], false)
            .is_ok());
    }

    #[test]
    fn missing_covenant() {
        let mut engine = CovenantPolicyEngine::new();
        assert_eq!(
            engine.verify_and_spend("nope", 1, &[0u8; 32], false),
            Err(CovenantError::Missing("nope".into()))
        );
    }

    fn sample_htlc(refund_daa: u64, payment_hash: u64) -> HtlcUtxo {
        HtlcUtxo {
            utxo_id: "htlc-1".into(),
            amount_sompi: 50_000_000,
            payment_hash,
            refund_daa,
            status: 0,
        }
    }

    #[test]
    fn htlc_claim_and_refund() {
        let mut engine = HtlcPolicyEngine::new();
        engine.register(sample_htlc(100, 0x4854_4C43));
        assert_eq!(
            engine.claim("htlc-1", 50, 0xDEAD),
            Err(CovenantError::HashlockMismatch)
        );
        assert_eq!(
            engine.claim("htlc-1", 100, 0x4854_4C43),
            Err(CovenantError::ClaimWindowClosed {
                refund_daa: 100,
                current: 100,
            })
        );
        assert!(engine.claim("htlc-1", 50, 0x4854_4C43).is_ok());
        engine.register(sample_htlc(100, 0x4854_4C43));
        assert_eq!(
            engine.refund("htlc-1", 50),
            Err(CovenantError::RefundBeforeTimelock {
                refund_daa: 100,
                current: 50,
            })
        );
        assert!(engine.refund("htlc-1", 100).is_ok());
    }

    fn sample_escrow(refund_daa: u64) -> Escrow2of3Utxo {
        Escrow2of3Utxo {
            utxo_id: "escrow-1".into(),
            amount_sompi: 50_000_000,
            buyer_hash: 0x4255_5945,
            seller_hash: 0x5345_4C4C,
            arbiter_hash: 0x4152_4220,
            refund_daa,
            status: 0,
        }
    }

    #[test]
    fn escrow_2of3_release_and_timeout() {
        let mut engine = Escrow2of3PolicyEngine::new();
        engine.register(sample_escrow(100));
        assert_eq!(
            engine.release_to_seller("escrow-1", 50, 0x5345_4C4C, 0xDEAD),
            Err(CovenantError::EscrowReleaseActors)
        );
        assert!(engine
            .release_to_seller("escrow-1", 50, 0x5345_4C4C, 0x4152_4220)
            .is_ok());
        engine.register(sample_escrow(100));
        assert_eq!(
            engine.timeout_to_buyer("escrow-1", 50),
            Err(CovenantError::RefundBeforeTimelock {
                refund_daa: 100,
                current: 50,
            })
        );
        assert!(engine.timeout_to_buyer("escrow-1", 100).is_ok());
    }
}
