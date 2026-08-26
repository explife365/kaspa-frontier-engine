//! On-disk TN10 covenant proof written by examples/silverscript/counter.py.

use crate::error::{EngineError, Result};
use crate::kascov::KascovCoin;
use crate::network::{require_tn10, tn10_tx_url};
use crate::rest::ToccataTx;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CovenantProofStep {
    pub step: String,
    pub count: i64,
    pub txid: String,
    pub covenant_id: String,
    #[serde(default)]
    pub output_index: Option<u32>,
    #[serde(default)]
    pub explorer: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CovenantProof {
    pub network: String,
    pub explorer: String,
    pub funding_address: String,
    pub steps: Vec<CovenantProofStep>,
}

pub const EXPECTED_STEPS: [&str; 3] = ["genesis", "add(5)", "subtract(3)"];
const EXPECTED_COUNTS: [i64; 3] = [0, 5, 2];

impl CovenantProof {
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self> {
        let raw = std::fs::read_to_string(path.as_ref())
            .map_err(|e| EngineError::Message(format!("read proof: {e}")))?;
        let proof: Self = serde_json::from_str(&raw)?;
        proof.validate()?;
        Ok(proof)
    }

    pub fn validate(&self) -> Result<()> {
        require_tn10(&self.network)?;
        for (i, step) in self.steps.iter().enumerate() {
            let expected = EXPECTED_STEPS.get(i).copied().unwrap_or("");
            if expected.is_empty() {
                return Err(EngineError::Message(format!(
                    "proof has extra step {}",
                    step.step
                )));
            }
            if step.step != expected {
                return Err(EngineError::Message(format!(
                    "proof step {i} is {}, expected {expected}",
                    step.step
                )));
            }
            if step.count != EXPECTED_COUNTS[i] {
                return Err(EngineError::Message(format!(
                    "proof step {} has count {}, expected {}",
                    step.step, step.count, EXPECTED_COUNTS[i]
                )));
            }
            if step.txid.is_empty() {
                return Err(EngineError::Message(format!(
                    "proof step {} missing txid",
                    step.step
                )));
            }
        }
        Ok(())
    }

    pub fn is_complete(&self) -> bool {
        self.steps.len() == EXPECTED_STEPS.len()
    }

    pub fn explorer_links(&self) -> Vec<String> {
        self.steps
            .iter()
            .map(CovenantProofStep::explorer_url)
            .collect()
    }

    pub fn covenant_id(&self) -> Result<&str> {
        let id = self
            .steps
            .first()
            .map(|s| s.covenant_id.as_str())
            .filter(|id| !id.is_empty())
            .ok_or_else(|| EngineError::Message("proof missing covenant_id".into()))?;
        if self.steps.iter().any(|s| s.covenant_id != id) {
            return Err(EngineError::Message(
                "proof steps do not share one covenant_id".into(),
            ));
        }
        Ok(id)
    }

    /// Check Toccata REST fields the community asked integrators to parse.
    pub fn verify_rest_txs(&self, txs: &[ToccataTx]) -> Result<()> {
        if txs.len() != self.steps.len() {
            return Err(EngineError::Message(format!(
                "REST returned {} txs, proof has {}",
                txs.len(),
                self.steps.len()
            )));
        }
        let cid = self.covenant_id()?;
        for (step, tx) in self.steps.iter().zip(txs.iter()) {
            if tx.transaction_id != step.txid {
                return Err(EngineError::Message(format!(
                    "{} txid mismatch: proof {} REST {}",
                    step.step, step.txid, tx.transaction_id
                )));
            }
            if tx.version != 1 {
                return Err(EngineError::Message(format!(
                    "{} is tx version {}, expected Toccata v1",
                    step.step, tx.version
                )));
            }
            if tx.inputs.iter().any(|input| input.compute_budget.is_none()) {
                return Err(EngineError::Message(format!(
                    "{} is missing input compute_budget (Toccata v1)",
                    step.step
                )));
            }
            if !tx.is_accepted {
                return Err(EngineError::Message(format!(
                    "{} {} is not accepted on TN10",
                    step.step, step.txid
                )));
            }
            if tx.storage_mass.is_none() {
                return Err(EngineError::Message(format!(
                    "{} is missing storage_mass (Toccata v1)",
                    step.step
                )));
            }
            let output_index = step.output_index.ok_or_else(|| {
                EngineError::Message(format!(
                    "{} proof is missing output_index; regenerate the proof",
                    step.step
                ))
            })?;
            let output = tx.outputs.get(output_index as usize).ok_or_else(|| {
                EngineError::Message(format!(
                    "{} output_index {output_index} is outside {} REST outputs",
                    step.step,
                    tx.outputs.len()
                ))
            })?;
            if output.covenant_id.as_deref() != Some(cid) {
                return Err(EngineError::Message(format!(
                    "{} output {output_index} covenant_id {} != proof {cid}",
                    step.step,
                    output.covenant_id.as_deref().unwrap_or("-")
                )));
            }
            let authorizing_index = output.covenant_authorizing_input.ok_or_else(|| {
                EngineError::Message(format!(
                    "{} output {output_index} is missing covenant_authorizing_input",
                    step.step
                ))
            })?;
            let authorizing_input = tx.inputs.get(authorizing_index as usize).ok_or_else(|| {
                EngineError::Message(format!(
                    "{} authorizing input {authorizing_index} is outside {} REST inputs",
                    step.step,
                    tx.inputs.len()
                ))
            })?;
            if step.step != "genesis" && authorizing_input.covenant_id.as_deref() != Some(cid) {
                return Err(EngineError::Message(format!(
                    "{} authorizing input {authorizing_index} covenant_id {} != proof {cid}",
                    step.step,
                    authorizing_input.covenant_id.as_deref().unwrap_or("-")
                )));
            }
        }
        for (step_window, tx_window) in self.steps.windows(2).zip(txs.windows(2)) {
            let previous_step = &step_window[0];
            let current_step = &step_window[1];
            let previous_tx = &tx_window[0];
            let current_tx = &tx_window[1];
            let output_index = current_step.output_index.ok_or_else(|| {
                EngineError::Message(format!(
                    "{} proof is missing output_index",
                    current_step.step
                ))
            })?;
            let authorizing_index = current_tx.outputs[output_index as usize]
                .covenant_authorizing_input
                .expect("validated above");
            let authorizing_input = &current_tx.inputs[authorizing_index as usize];
            let expected_index = previous_step.output_index.ok_or_else(|| {
                EngineError::Message(format!(
                    "{} proof is missing output_index",
                    previous_step.step
                ))
            })?;
            let spent_hash = authorizing_input.previous_outpoint_hash.as_deref();
            let spent_index = authorizing_input.previous_outpoint_index;
            if spent_hash != Some(previous_tx.transaction_id.as_str())
                || spent_index != Some(expected_index)
            {
                return Err(EngineError::Message(format!(
                    "{} authorizing input does not spend {}:{} (REST previous_outpoint={}:{})",
                    current_tx.transaction_id,
                    previous_tx.transaction_id,
                    expected_index,
                    spent_hash.unwrap_or("-"),
                    spent_index
                        .map(|index| index.to_string())
                        .unwrap_or_else(|| "-".into())
                )));
            }
        }
        Ok(())
    }

    pub fn verify_kascov(&self, coin: &KascovCoin) -> Result<()> {
        let cid = self.covenant_id()?;
        if coin.covenant_id != cid {
            return Err(EngineError::Message(format!(
                "kascov covenant_id {} != proof {cid}",
                coin.covenant_id
            )));
        }
        if !coin.lineage_complete {
            return Err(EngineError::Message(
                "kascov lineage_complete=false for this covenant".into(),
            ));
        }
        if coin.events.len() != self.steps.len()
            || coin.event_count != u64::try_from(self.steps.len()).unwrap_or(u64::MAX)
        {
            return Err(EngineError::Message(format!(
                "kascov event summary {}/{} does not exactly match {} proof steps",
                coin.event_count,
                coin.events.len(),
                self.steps.len(),
            )));
        }
        for (index, (step, event)) in self.steps.iter().zip(coin.events.iter()).enumerate() {
            if event.txid != step.txid {
                return Err(EngineError::Message(format!(
                    "kascov {} txid {} != proof {}",
                    event.kind, event.txid, step.txid
                )));
            }
            let expected_kind = if index == 0 { "genesis" } else { "transition" };
            if event.kind != expected_kind || event.seq != index as u64 {
                return Err(EngineError::Message(format!(
                    "kascov event {index} is kind={} seq={}, expected kind={expected_kind} seq={index}",
                    event.kind, event.seq
                )));
            }
        }
        // Stand-in for missing kaspad getUtxosByCovenantId.
        if coin.live_utxos == 0 {
            return Err(EngineError::Message(
                "kascov reports 0 live_utxos for this covenant".into(),
            ));
        }
        Ok(())
    }
}

impl CovenantProofStep {
    pub fn explorer_url(&self) -> String {
        self.explorer
            .clone()
            .filter(|url| !url.is_empty())
            .unwrap_or_else(|| tn10_tx_url(&self.txid))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_partial_and_complete() {
        let mut proof = CovenantProof {
            network: "testnet-10".into(),
            explorer: "https://explorer-tn10.kaspa.org".into(),
            funding_address: "kaspatest:qq".into(),
            steps: vec![CovenantProofStep {
                step: "genesis".into(),
                count: 0,
                txid: "aa".into(),
                covenant_id: "cc".into(),
                output_index: Some(0),
                explorer: None,
            }],
        };
        proof.validate().unwrap();
        assert!(!proof.is_complete());
        proof.steps.push(CovenantProofStep {
            step: "add(5)".into(),
            count: 5,
            txid: "bb".into(),
            covenant_id: "cc".into(),
            output_index: Some(0),
            explorer: None,
        });
        proof.steps.push(CovenantProofStep {
            step: "subtract(3)".into(),
            count: 2,
            txid: "dd".into(),
            covenant_id: "cc".into(),
            output_index: Some(0),
            explorer: None,
        });
        proof.validate().unwrap();
        assert!(proof.is_complete());
        assert_eq!(proof.explorer_links().len(), 3);
        assert_eq!(
            proof.steps[0].explorer_url(),
            "https://explorer-tn10.kaspa.org/txs/aa"
        );
        assert_eq!(proof.covenant_id().unwrap(), "cc");
        let mut wrong_count = proof;
        wrong_count.steps[2].count = 3;
        assert!(wrong_count.validate().is_err());
    }

    #[test]
    fn rejects_wrong_order() {
        let proof = CovenantProof {
            network: "testnet-10".into(),
            explorer: "https://explorer-tn10.kaspa.org".into(),
            funding_address: "kaspatest:qq".into(),
            steps: vec![CovenantProofStep {
                step: "add(5)".into(),
                count: 5,
                txid: "aa".into(),
                covenant_id: "cc".into(),
                output_index: Some(0),
                explorer: None,
            }],
        };
        assert!(proof.validate().is_err());
    }

    fn sample_tx(
        txid: &str,
        version: u32,
        accepted: bool,
        in_cid: Option<&str>,
        out_cid: &str,
        prev: Option<&str>,
    ) -> ToccataTx {
        serde_json::from_value(serde_json::json!({
            "transaction_id": txid,
            "version": version,
            "is_accepted": accepted,
            "storageMass": "1781",
            "inputs": [{
                "compute_budget": 10,
                "covenant_id": in_cid,
                "previous_outpoint_hash": prev,
                "previous_outpoint_index": if prev.is_some() { Some(0) } else { None }
            }],
            "outputs": [{
                "amount": 1,
                "covenant_id": out_cid,
                "covenant_authorizing_input": 0,
                "script_public_key_type": "scripthash"
            }]
        }))
        .unwrap()
    }

    #[test]
    fn rest_lineage_requires_v1_accepted_and_covenant_id() {
        let proof = CovenantProof {
            network: "testnet-10".into(),
            explorer: "https://explorer-tn10.kaspa.org".into(),
            funding_address: "kaspatest:qq".into(),
            steps: vec![
                CovenantProofStep {
                    step: "genesis".into(),
                    count: 0,
                    txid: "aa".into(),
                    covenant_id: "cc".into(),
                    output_index: Some(0),
                    explorer: None,
                },
                CovenantProofStep {
                    step: "add(5)".into(),
                    count: 5,
                    txid: "bb".into(),
                    covenant_id: "cc".into(),
                    output_index: Some(0),
                    explorer: None,
                },
                CovenantProofStep {
                    step: "subtract(3)".into(),
                    count: 2,
                    txid: "dd".into(),
                    covenant_id: "cc".into(),
                    output_index: Some(0),
                    explorer: None,
                },
            ],
        };
        let ok = vec![
            sample_tx("aa", 1, true, None, "cc", None),
            sample_tx("bb", 1, true, Some("cc"), "cc", Some("aa")),
            sample_tx("dd", 1, true, Some("cc"), "cc", Some("bb")),
        ];
        proof.verify_rest_txs(&ok).unwrap();
        let mut no_budget = ok.clone();
        no_budget[0].inputs[0].compute_budget = None;
        assert!(proof.verify_rest_txs(&no_budget).is_err());
        let unlinked = vec![
            sample_tx("aa", 1, true, None, "cc", None),
            sample_tx("bb", 1, true, Some("cc"), "cc", Some("aa")),
            sample_tx("dd", 1, true, Some("cc"), "cc", Some("aa")),
        ];
        assert!(proof.verify_rest_txs(&unlinked).is_err());
        proof.verify_rest_txs(&ok).unwrap();
        let mut bad = ok.clone();
        bad[0].version = 0;
        assert!(proof.verify_rest_txs(&bad).is_err());
    }

    #[test]
    fn rest_lineage_uses_selected_output_and_nonzero_authorizing_input() {
        let proof = CovenantProof {
            network: "testnet-10".into(),
            explorer: "https://explorer-tn10.kaspa.org".into(),
            funding_address: "kaspatest:qq".into(),
            steps: vec![
                CovenantProofStep {
                    step: "genesis".into(),
                    count: 0,
                    txid: "aa".into(),
                    covenant_id: "cc".into(),
                    output_index: Some(1),
                    explorer: None,
                },
                CovenantProofStep {
                    step: "add(5)".into(),
                    count: 5,
                    txid: "bb".into(),
                    covenant_id: "cc".into(),
                    output_index: Some(1),
                    explorer: None,
                },
            ],
        };
        let genesis: ToccataTx = serde_json::from_value(serde_json::json!({
            "transaction_id": "aa",
            "version": 1,
            "is_accepted": true,
            "storageMass": 100,
            "inputs": [
                {"compute_budget": 1},
                {"compute_budget": 1}
            ],
            "outputs": [
                {"covenant_id": "unrelated", "covenant_authorizing_input": 0},
                {"covenant_id": "cc", "covenant_authorizing_input": 1}
            ]
        }))
        .unwrap();
        let transition: ToccataTx = serde_json::from_value(serde_json::json!({
            "transaction_id": "bb",
            "version": 1,
            "is_accepted": true,
            "storage_mass": 100,
            "inputs": [
                {
                    "compute_budget": 1,
                    "covenant_id": "unrelated",
                    "previous_outpoint_hash": "wrong",
                    "previous_outpoint_index": 0
                },
                {
                    "compute_budget": 1,
                    "covenant_id": "cc",
                    "previous_outpoint_hash": "aa",
                    "previous_outpoint_index": 1
                }
            ],
            "outputs": [
                {"covenant_id": "unrelated", "covenant_authorizing_input": 0},
                {"covenant_id": "cc", "covenant_authorizing_input": 1}
            ]
        }))
        .unwrap();
        proof
            .verify_rest_txs(&[genesis.clone(), transition.clone()])
            .unwrap();
        let mut wrong_index = transition;
        wrong_index.inputs[1].previous_outpoint_index = Some(0);
        assert!(proof.verify_rest_txs(&[genesis, wrong_index]).is_err());
    }

    #[test]
    fn kascov_must_match_proof_txids() {
        let proof = CovenantProof {
            network: "testnet-10".into(),
            explorer: "https://explorer-tn10.kaspa.org".into(),
            funding_address: "kaspatest:qq".into(),
            steps: vec![CovenantProofStep {
                step: "genesis".into(),
                count: 0,
                txid: "aa".into(),
                covenant_id: "cc".into(),
                output_index: Some(0),
                explorer: None,
            }],
        };
        let coin = KascovCoin {
            covenant_id: "cc".into(),
            network: "testnet-10".into(),
            status: "active".into(),
            lineage_complete: true,
            event_count: 1,
            live_utxos: 1,
            live_value: 1,
            genesis_txid: "aa".into(),
            name: "x".into(),
            events: vec![crate::kascov::KascovEvent {
                kind: "genesis".into(),
                txid: "aa".into(),
                seq: 0,
                accepting_daa: None,
                tx_index: None,
            }],
            utxos: Vec::new(),
        };
        proof.verify_kascov(&coin).unwrap();
        let mut broken = coin.clone();
        broken.lineage_complete = false;
        assert!(proof.verify_kascov(&broken).is_err());
        let mut empty = coin.clone();
        empty.live_utxos = 0;
        assert!(proof.verify_kascov(&empty).is_err());
        let mut wrong_sequence = coin.clone();
        wrong_sequence.events[0].seq = 1;
        assert!(proof.verify_kascov(&wrong_sequence).is_err());
        let mut extra = coin;
        extra.event_count = 2;
        extra.events.push(crate::kascov::KascovEvent {
            kind: "transition".into(),
            txid: "bb".into(),
            seq: 1,
            accepting_daa: None,
            tx_index: None,
        });
        assert!(proof.verify_kascov(&extra).is_err());
    }

    #[test]
    fn checked_in_public_tn10_fixture_verifies_offline() {
        let proof: CovenantProof =
            serde_json::from_str(include_str!("../fixtures/tn10-counter-proof.json")).unwrap();
        let txs: Vec<ToccataTx> =
            serde_json::from_str(include_str!("../fixtures/tn10-counter-rest.json")).unwrap();
        let coin: KascovCoin =
            serde_json::from_str(include_str!("../fixtures/tn10-counter-kascov.json")).unwrap();

        proof.validate().unwrap();
        assert!(proof.is_complete());
        proof.verify_rest_txs(&txs).unwrap();
        proof.verify_kascov(&coin).unwrap();
        assert_eq!(coin.utxos.len(), 1);
        assert_eq!(coin.utxos[0].outpoint, format!("{}:0", proof.steps[2].txid));
    }
}
