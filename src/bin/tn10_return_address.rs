//! Resolve deposit return address (rusty-kaspa#435 integrator rehearsal).
//!
//!   cargo run --release --bin tn10-return-address -- <txid> [output_index]
//!   cargo run --release --bin tn10-return-address -- <txid> --fee

use kaspa_frontier_engine::network::tn10_tx_url;
use kaspa_frontier_engine::{
    estimate_tx_fee_from_toccata, resolve_return_address, Tn10RestClient,
};
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: tn10-return-address <txid> [output_index] [--fee]");
        std::process::exit(2);
    }
    let fee_only = args.iter().any(|arg| arg == "--fee");
    let txid = args
        .first()
        .ok_or("missing txid")?
        .trim()
        .to_lowercase();
    let output_index = args
        .get(1)
        .and_then(|value| {
            if value == "--fee" {
                None
            } else {
                value.parse().ok()
            }
        })
        .unwrap_or(0);
    let rest = Tn10RestClient::new(kaspa_frontier_engine::network::TESTNET_10_REST)?;
    if fee_only {
        let tx = rest
            .toccata_tx(&txid)
            .await?
            .ok_or_else(|| format!("transaction not found: {txid}"))?;
        let report = estimate_tx_fee_from_toccata(&tx);
        println!("tx       {}", report.transaction_id);
        println!("explorer {}", tn10_tx_url(&report.transaction_id));
        println!("fee      {} sompi", report.fee_sompi.map(|v| v.to_string()).unwrap_or_else(|| "unknown".into()));
        println!("inputs   {} sompi ({} enriched)", report.input_total_sompi.map(|v| v.to_string()).unwrap_or_else(|| "?".into()), report.enriched_inputs);
        println!("outputs  {} sompi", report.output_total_sompi.map(|v| v.to_string()).unwrap_or_else(|| "?".into()));
        println!("note     {}", report.note);
        return Ok(());
    }
    let report = resolve_return_address(&rest, &txid, output_index).await?;
    println!("deposit  {}:{}", report.deposit_tx_id, report.deposit_output_index);
    println!("explorer {}", tn10_tx_url(&report.deposit_tx_id));
    println!("method   {}", report.method);
    println!("hops     {}", report.hops);
    match &report.return_address {
        Some(address) => println!("return   {}", address),
        None => {
            println!("return   (unresolved — pruned parent or needs kaspad#435 RPC)");
            std::process::exit(1);
        }
    }
    Ok(())
}
