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
}
