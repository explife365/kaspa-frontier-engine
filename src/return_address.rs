//! Deposit return-address resolution for integrators (rusty-kaspa#435 rehearsal).
//!
//! Until kaspad exposes a native RPC, this walks TN10 REST transaction data:
//! first input of the funding transaction → previous outpoint → sender address.
//! Matches the spec note: return only the first input's address.

use crate::error::{EngineError, Result};
use crate::network::is_valid_testnet_address;
use crate::rest::{Tn10RestClient, ToccataTx, ToccataTxInput};

const MAX_CHAIN_WALK: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReturnAddressReport {
    pub deposit_tx_id: String,
    pub deposit_output_index: u32,
    pub return_address: Option<String>,
    pub method: String,
    pub hops: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TxFeeReport {
    pub transaction_id: String,
    pub fee_sompi: Option<u64>,
    pub input_total_sompi: Option<u64>,
    pub output_total_sompi: Option<u64>,
    pub enriched_inputs: usize,
    pub note: String,
}

pub async fn resolve_return_address(
    rest: &Tn10RestClient,
    deposit_tx_id: &str,
    deposit_output_index: u32,
) -> Result<ReturnAddressReport> {
    let _ = deposit_output_index;
    let mut hops = 0usize;
    let mut tx_id = deposit_tx_id.to_string();
    while hops < MAX_CHAIN_WALK {
        hops += 1;
        let tx = rest
            .toccata_tx(&tx_id)
            .await?
            .ok_or_else(|| EngineError::Message(format!("transaction not found: {tx_id}")))?;
        let input = tx
            .inputs
            .first()
            .ok_or_else(|| EngineError::Message(format!("transaction has no inputs: {tx_id}")))?;
        if let Some(address) = input_previous_outpoint_address(input) {
            validate_testnet_address(&address)?;
            return Ok(ReturnAddressReport {
                deposit_tx_id: deposit_tx_id.to_string(),
                deposit_output_index,
                return_address: Some(address),
                method: "rest.previous_outpoint_address".into(),
                hops,
            });
        }
        let (parent_tx, parent_index) = input_previous_outpoint(input)?;
        let parent = rest
            .toccata_tx(&parent_tx)
            .await?
            .ok_or_else(|| {
                EngineError::Message(format!(
                    "parent transaction pruned or missing: {parent_tx} (rusty-kaspa#435 needs node UTXO diff)"
                ))
            })?;
        if let Some(output) = parent.outputs.get(parent_index as usize) {
            if let Some(address) = output.script_public_key_address.as_deref() {
                validate_testnet_address(address)?;
                return Ok(ReturnAddressReport {
                    deposit_tx_id: deposit_tx_id.to_string(),
                    deposit_output_index,
                    return_address: Some(address.to_string()),
                    method: "rest.parent_output.script_public_key_address".into(),
                    hops,
                });
            }
        }
        tx_id = parent_tx;
    }
    Ok(ReturnAddressReport {
        deposit_tx_id: deposit_tx_id.to_string(),
        deposit_output_index,
        return_address: None,
        method: "rest.chain_walk_exhausted".into(),
        hops,
    })
}

pub fn estimate_tx_fee_from_toccata(tx: &ToccataTx) -> TxFeeReport {
    let enriched = tx
        .inputs
        .iter()
        .filter(|input| input.previous_outpoint_amount.is_some())
        .count();
    let mut input_total = Some(0u64);
    for input in &tx.inputs {
        match input.previous_outpoint_amount {
            Some(amount) => {
                input_total = input_total.and_then(|acc| acc.checked_add(amount));
            }
            None => {
                input_total = None;
                break;
            }
        }
    }
    let mut output_total = Some(0u64);
    for output in &tx.outputs {
        match output.amount {
            Some(amount) => {
                output_total = output_total.and_then(|acc| acc.checked_add(amount));
            }
            None => {
                output_total = None;
                break;
            }
        }
    }
    let fee_sompi = match (input_total, output_total) {
        (Some(inputs), Some(outputs)) if inputs >= outputs => Some(inputs - outputs),
        _ => None,
    };
    let note = if enriched == tx.inputs.len() && enriched > 0 {
        "fee from REST-enriched input amounts (rusty-kaspa#615 partial; GetBlocksV2 on owned node when merged)"
    } else if enriched > 0 {
        "partial enrichment; fee may be incomplete until GetBlocksV2 / input UTXO data"
    } else {
        "inputs lack previous_outpoint_amount; need GetBlocksV2 or owned-node RPC (PR #906)"
    };
    TxFeeReport {
        transaction_id: tx.transaction_id.clone(),
        fee_sompi,
        input_total_sompi: input_total,
        output_total_sompi: output_total,
        enriched_inputs: enriched,
        note: note.into(),
    }
}

fn input_previous_outpoint_address(input: &ToccataTxInput) -> Option<String> {
    input
        .previous_outpoint_address
        .as_deref()
        .filter(|addr| !addr.is_empty())
        .map(str::to_string)
}

fn input_previous_outpoint(input: &ToccataTxInput) -> Result<(String, u32)> {
    let tx = input
        .previous_outpoint_hash
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| EngineError::Message("input missing previous_outpoint_hash".into()))?;
    let index = input
        .previous_outpoint_index
        .ok_or_else(|| EngineError::Message("input missing previous_outpoint_index".into()))?;
    Ok((tx.to_string(), index))
}

fn validate_testnet_address(address: &str) -> Result<()> {
    if !is_valid_testnet_address(address) {
        return Err(EngineError::NotTestnetAddress(address.to_string()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rest::{ToccataTx, ToccataTxInput, ToccataTxOutput};

    #[test]
    fn fee_estimate_when_inputs_enriched() {
        let tx = ToccataTx {
            transaction_id: "aa".into(),
            version: 1,
            is_accepted: true,
            storage_mass: None,
            inputs: vec![ToccataTxInput {
                compute_budget: None,
                covenant_id: None,
                previous_outpoint_hash: Some("bb".into()),
                previous_outpoint_index: Some(0),
                previous_outpoint_address: None,
                previous_outpoint_amount: Some(1_000),
            }],
            outputs: vec![ToccataTxOutput {
                amount: Some(900),
                covenant_id: None,
                covenant_authorizing_input: None,
                script_public_key_type: None,
                script_public_key_address: None,
                script_public_key: None,
            }],
        };
        let report = estimate_tx_fee_from_toccata(&tx);
        assert_eq!(report.enriched_inputs, 1);
        assert_eq!(report.fee_sompi, Some(100));
        assert!(report.note.contains("615"));
    }

    #[test]
    fn parses_output_address_field() {
        let output: ToccataTxOutput = serde_json::from_str(
            r#"{"amount":1,"script_public_key_address":"kaspatest:qptv6u8kel95drh2p2z492cyksk8lpetep286fngqu5j9nk57g642lzf748kt"}"#,
        )
        .unwrap();
        assert_eq!(
            output.script_public_key_address.as_deref(),
            Some("kaspatest:qptv6u8kel95drh2p2z492cyksk8lpetep286fngqu5j9nk57g642lzf748kt")
        );
    }
}
