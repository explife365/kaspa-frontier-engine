//! On-disk TN10 covenant proof written by examples/silverscript/counter.py.

use crate::error::{EngineError, Result};
use crate::kascov::{KascovClient, KascovCoin};
use crate::network::{require_tn10, tn10_tx_url};
use crate::rest::{Tn10RestClient, ToccataTx};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProofStepReport {
    pub step: String,
    pub txid: String,
    pub found_on_rest: bool,
    pub version: Option<u32>,
    pub is_accepted: Option<bool>,
    pub storage_mass: Option<String>,
    pub input_covenant_id: Option<String>,
    pub output_covenant_id: Option<String>,
    pub explorer: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KascovProofSummary {
    pub name: String,
    pub status: String,
    pub lineage_complete: bool,
    pub event_count: u64,
    pub live_utxos: u64,
    pub live_value: u64,
    pub fetched_live_utxos: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProofVerificationReport {
    pub network: String,
    pub covenant_id: String,
    pub funding_address: String,
    pub complete: bool,
    pub rest_verified: bool,
    pub kascov_verified: bool,
    pub steps: Vec<ProofStepReport>,
    pub kascov: Option<KascovProofSummary>,
    pub kascov_url: String,
}

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
    #[serde(default = "default_proof_app")]
    pub app: String,
    pub network: String,
    pub explorer: String,
    pub funding_address: String,
    pub steps: Vec<CovenantProofStep>,
    #[serde(default)]
    pub unlock_daa: Option<u64>,
    #[serde(default)]
    pub allowed_recipient_hash: Option<i64>,
}

fn default_proof_app() -> String {
    "counter".into()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProofApp {
    Counter,
    TimelockVault,
    RestrictedSwap,
}

impl ProofApp {
    pub fn steps(self) -> &'static [&'static str] {
        match self {
            ProofApp::Counter => &["genesis", "add(5)", "subtract(3)"],
            ProofApp::TimelockVault => &["genesis", "release"],
            ProofApp::RestrictedSwap => &["genesis", "swap"],
        }
    }

    pub fn counts(self) -> &'static [i64] {
        match self {
            ProofApp::Counter => &[0, 5, 2],
            ProofApp::TimelockVault => &[0, 1],
            ProofApp::RestrictedSwap => &[0, 1],
        }
    }

    pub fn fixture_prefix(self) -> &'static str {
        match self {
            ProofApp::Counter => "tn10-counter",
            ProofApp::TimelockVault => "tn10-vault",
            ProofApp::RestrictedSwap => "tn10-swap",
        }
    }
}

pub const EXPECTED_STEPS: [&str; 3] = ["genesis", "add(5)", "subtract(3)"];

impl CovenantProof {
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self> {
        let raw = std::fs::read_to_string(path.as_ref())
            .map_err(|e| EngineError::Message(format!("read proof: {e}")))?;
        let proof: Self = serde_json::from_str(&raw)?;
        proof.validate()?;
        Ok(proof)
    }

    pub fn app_kind(&self) -> ProofApp {
        match self.app.as_str() {
            "timelock_vault" => ProofApp::TimelockVault,
            "restricted_swap" => ProofApp::RestrictedSwap,
            _ => ProofApp::Counter,
        }
    }

    pub fn expected_steps(&self) -> &'static [&'static str] {
        self.app_kind().steps()
    }

    pub fn expected_counts(&self) -> &'static [i64] {
        self.app_kind().counts()
    }

    pub fn validate(&self) -> Result<()> {
        require_tn10(&self.network)?;
        let expected_steps = self.expected_steps();
        let expected_counts = self.expected_counts();
        for (i, step) in self.steps.iter().enumerate() {
            let expected = expected_steps.get(i).copied().unwrap_or("");
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
            if step.count != expected_counts[i] {
                return Err(EngineError::Message(format!(
                    "proof step {} has count {}, expected {}",
                    step.step, step.count, expected_counts[i]
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
        self.steps.len() == self.expected_steps().len()
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

    pub async fn verify_online(
        &self,
        rest: &Tn10RestClient,
        kascov: &KascovClient,
    ) -> Result<ProofVerificationReport> {
        if !self.is_complete() {
            return Err(EngineError::Message(format!(
                "proof incomplete: {}/{} steps (need genesis, add(5), subtract(3))",
                self.steps.len(),
                self.expected_steps().len()
            )));
        }
        let covenant_id = self.covenant_id()?.to_string();
        let mut step_reports = Vec::with_capacity(self.steps.len());
        let mut txs = Vec::with_capacity(self.steps.len());
        let mut missing = 0usize;
        for step in &self.steps {
            match rest.toccata_tx(&step.txid).await? {
                Some(tx) => {
                    step_reports.push(ProofStepReport {
                        step: step.step.clone(),
                        txid: step.txid.clone(),
                        found_on_rest: true,
                        version: Some(tx.version),
                        is_accepted: Some(tx.is_accepted),
                        storage_mass: tx
                            .storage_mass
                            .map(|mass| mass.to_string()),
                        input_covenant_id: tx.input_covenant_id().map(str::to_string),
                        output_covenant_id: tx.output_covenant_id().map(str::to_string),
                        explorer: step.explorer_url(),
                    });
                    txs.push(tx);
                }
                None => {
                    missing += 1;
                    step_reports.push(ProofStepReport {
                        step: step.step.clone(),
                        txid: step.txid.clone(),
                        found_on_rest: false,
                        version: None,
                        is_accepted: None,
                        storage_mass: None,
                        input_covenant_id: None,
                        output_covenant_id: None,
                        explorer: step.explorer_url(),
                    });
                }
            }
        }
        if missing > 0 {
            return Err(EngineError::Message(format!(
                "{missing} proof txid(s) not found on TN10 REST"
            )));
        }
        self.verify_rest_txs(&txs)?;
        let (coin, utxos) = kascov.snapshot(&covenant_id).await?;
        self.verify_kascov(&coin)?;
        Ok(self.build_report(covenant_id, step_reports, true, true, coin, utxos.len(), kascov))
    }

    /// Fetch live REST + kascov JSON for offline `--offline` replay after broadcast.
    pub async fn capture_fixture_files(
        &self,
        proof_path: &Path,
        rest: &Tn10RestClient,
        kascov: &KascovClient,
    ) -> Result<(std::path::PathBuf, std::path::PathBuf)> {
        use std::path::PathBuf;
        if !self.is_complete() {
            return Err(EngineError::Message(format!(
                "proof incomplete: {}/{} steps",
                self.steps.len(),
                self.expected_steps().len()
            )));
        }
        let mut txs = Vec::with_capacity(self.steps.len());
        for step in &self.steps {
            let raw = rest.transaction(&step.txid).await?.ok_or_else(|| {
                EngineError::Message(format!(
                    "REST missing txid {} — wait for indexer or verify with --kascov-only",
                    step.txid
                ))
            })?;
            txs.push(raw);
        }
        let covenant_id = self.covenant_id()?;
        let coin = kascov.coin(&covenant_id).await?;
        let prefix = self.app_kind().fixture_prefix();
        let fixture_dir = proof_path
            .parent()
            .filter(|dir| dir.ends_with("fixtures"))
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("fixtures"));
        let rest_path = fixture_dir.join(format!("{prefix}-rest.json"));
        let kascov_path = fixture_dir.join(format!("{prefix}-kascov.json"));
        std::fs::write(&rest_path, serde_json::to_string_pretty(&txs)?).map_err(|error| {
            EngineError::Message(format!("write {}: {error}", rest_path.display()))
        })?;
        std::fs::write(&kascov_path, serde_json::to_string_pretty(&coin)?).map_err(|error| {
            EngineError::Message(format!("write {}: {error}", kascov_path.display()))
        })?;
        Ok((rest_path, kascov_path))
    }

    /// Live kascov verification when TN10 REST no longer serves historical txids.
    /// REST steps are best-effort; success requires kascov lineage + live_utxos only.
    pub async fn verify_kascov_online(
        &self,
        rest: &Tn10RestClient,
        kascov: &KascovClient,
    ) -> Result<ProofVerificationReport> {
        if !self.is_complete() {
            return Err(EngineError::Message(format!(
                "proof incomplete: {}/{} steps (need genesis, add(5), subtract(3))",
                self.steps.len(),
                self.expected_steps().len()
            )));
        }
        let covenant_id = self.covenant_id()?.to_string();
        let mut step_reports = Vec::with_capacity(self.steps.len());
        let mut txs = Vec::new();
        for step in &self.steps {
            match rest.toccata_tx(&step.txid).await? {
                Some(tx) => {
                    step_reports.push(ProofStepReport {
                        step: step.step.clone(),
                        txid: step.txid.clone(),
                        found_on_rest: true,
                        version: Some(tx.version),
                        is_accepted: Some(tx.is_accepted),
                        storage_mass: tx.storage_mass.map(|mass| mass.to_string()),
                        input_covenant_id: tx.input_covenant_id().map(str::to_string),
                        output_covenant_id: tx.output_covenant_id().map(str::to_string),
                        explorer: step.explorer_url(),
                    });
                    txs.push(tx);
                }
                None => {
                    step_reports.push(ProofStepReport {
                        step: step.step.clone(),
                        txid: step.txid.clone(),
                        found_on_rest: false,
                        version: None,
                        is_accepted: None,
                        storage_mass: None,
                        input_covenant_id: None,
                        output_covenant_id: None,
                        explorer: step.explorer_url(),
                    });
                }
            }
        }
        let rest_verified = if txs.len() == self.steps.len() {
            self.verify_rest_txs(&txs).is_ok()
        } else {
            false
        };
        let (coin, utxos) = kascov.snapshot(&covenant_id).await?;
        self.verify_kascov(&coin)?;
        Ok(self.build_report(
            covenant_id,
            step_reports,
            rest_verified,
            true,
            coin,
            utxos.len(),
            kascov,
        ))
    }

    fn build_report(
        &self,
        covenant_id: String,
        steps: Vec<ProofStepReport>,
        rest_verified: bool,
        kascov_verified: bool,
        coin: KascovCoin,
        fetched_live_utxos: usize,
        kascov: &KascovClient,
    ) -> ProofVerificationReport {
        ProofVerificationReport {
            network: self.network.clone(),
            covenant_id: covenant_id.clone(),
            funding_address: self.funding_address.clone(),
            complete: true,
            rest_verified,
            kascov_verified,
            steps,
            kascov: Some(KascovProofSummary {
                name: coin.name.clone(),
                status: coin.status.clone(),
                lineage_complete: coin.lineage_complete,
                event_count: coin.event_count,
                live_utxos: coin.live_utxos,
                live_value: coin.live_value,
                fetched_live_utxos,
            }),
            kascov_url: kascov.coin_url(&covenant_id),
        }
    }

    /// Verify against checked-in REST + kascov fixture snapshots (no network).
    pub fn verify_offline_fixture(
        &self,
        txs: &[ToccataTx],
        coin: &KascovCoin,
        kascov_url: &str,
    ) -> Result<ProofVerificationReport> {
        if !self.is_complete() {
            return Err(EngineError::Message(format!(
                "proof incomplete: {}/{} steps (need genesis, add(5), subtract(3))",
                self.steps.len(),
                self.expected_steps().len()
            )));
        }
        self.verify_rest_txs(txs)?;
        self.verify_kascov(coin)?;
        let covenant_id = self.covenant_id()?.to_string();
        let steps = self
            .steps
            .iter()
            .zip(txs.iter())
            .map(|(step, tx)| ProofStepReport {
                step: step.step.clone(),
                txid: step.txid.clone(),
                found_on_rest: true,
                version: Some(tx.version),
                is_accepted: Some(tx.is_accepted),
                storage_mass: tx.storage_mass.map(|mass| mass.to_string()),
                input_covenant_id: tx.input_covenant_id().map(str::to_string),
                output_covenant_id: tx.output_covenant_id().map(str::to_string),
                explorer: step.explorer_url(),
            })
            .collect();
        Ok(ProofVerificationReport {
            network: self.network.clone(),
            covenant_id,
            funding_address: self.funding_address.clone(),
            complete: true,
            rest_verified: true,
            kascov_verified: true,
            steps,
            kascov: Some(KascovProofSummary {
                name: coin.name.clone(),
                status: coin.status.clone(),
                lineage_complete: coin.lineage_complete,
                event_count: coin.event_count,
                live_utxos: coin.live_utxos,
                live_value: coin.live_value,
                fetched_live_utxos: coin.utxos.len(),
            }),
            kascov_url: kascov_url.to_string(),
        })
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
            app: "counter".into(),
            unlock_daa: None,
            allowed_recipient_hash: None,
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
            app: "counter".into(),
            unlock_daa: None,
            allowed_recipient_hash: None,
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
            app: "counter".into(),
            unlock_daa: None,
            allowed_recipient_hash: None,
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
            app: "counter".into(),
            unlock_daa: None,
            allowed_recipient_hash: None,
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
            app: "counter".into(),
            unlock_daa: None,
            allowed_recipient_hash: None,
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
    fn timelock_vault_profile_validates_two_steps() {
        let proof = CovenantProof {
            app: "timelock_vault".into(),
            unlock_daa: Some(1_000_000),
            allowed_recipient_hash: None,
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
                    step: "release".into(),
                    count: 1,
                    txid: "bb".into(),
                    covenant_id: "cc".into(),
                    output_index: Some(0),
                    explorer: None,
                },
            ],
        };
        proof.validate().unwrap();
        assert!(proof.is_complete());
        assert_eq!(proof.app_kind(), ProofApp::TimelockVault);
    }

    #[test]
    fn restricted_swap_profile_validates_two_steps() {
        let proof = CovenantProof {
            app: "restricted_swap".into(),
            unlock_daa: None,
            allowed_recipient_hash: Some(0x5357_4150),
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
                    step: "swap".into(),
                    count: 1,
                    txid: "bb".into(),
                    covenant_id: "cc".into(),
                    output_index: Some(0),
                    explorer: None,
                },
            ],
        };
        proof.validate().unwrap();
        assert!(proof.is_complete());
        assert_eq!(proof.app_kind(), ProofApp::RestrictedSwap);
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
