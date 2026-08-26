use kaspa_frontier_engine::network::{self, KASCOV_TN10, TESTNET_10_REST};
use kaspa_frontier_engine::{CovenantProof, KascovClient, Tn10RestClient};
use std::env;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("fixtures/tn10-counter-proof.json"));

    if !path.is_file() {
        return Err(format!(
            "no proof at {} — use the checked-in fixture or provide a generated proof",
            path.display()
        )
        .into());
    }

    let proof = CovenantProof::from_path(&path)?;
    network::print_dev_sig();
    println!("proof      {}", path.display());
    println!("network    {}", proof.network);
    println!("funding    {}", proof.funding_address);
    println!(
        "steps      {}/{}  complete={}",
        proof.steps.len(),
        kaspa_frontier_engine::proof::EXPECTED_STEPS.len(),
        proof.is_complete()
    );
    println!("covenant   {}", proof.covenant_id()?);

    if !proof.is_complete() {
        return Err(format!(
            "proof incomplete: {}/{} steps (need genesis, add(5), subtract(3))",
            proof.steps.len(),
            kaspa_frontier_engine::proof::EXPECTED_STEPS.len()
        )
        .into());
    }

    let client = Tn10RestClient::new(TESTNET_10_REST)?;
    let kascov = KascovClient::new(KASCOV_TN10)?;
    let cid = proof.covenant_id()?.to_string();
    let (a, b, c, snap) = tokio::join!(
        client.toccata_tx(&proof.steps[0].txid),
        client.toccata_tx(&proof.steps[1].txid),
        client.toccata_tx(&proof.steps[2].txid),
        kascov.snapshot(&cid),
    );
    let mut txs = Vec::with_capacity(3);
    let mut missing = 0usize;
    for (step, found) in proof.steps.iter().zip([a?, b?, c?]) {
        let link = step.explorer_url();
        match found {
            Some(tx) => {
                println!(
                    "  OK  {}  v{} accepted={} mass={} in_cid={} out_cid={}  {link}",
                    step.step,
                    tx.version,
                    tx.is_accepted,
                    tx.storage_mass
                        .map(|m| m.to_string())
                        .unwrap_or_else(|| "-".into()),
                    tx.input_covenant_id().unwrap_or("-"),
                    tx.output_covenant_id().unwrap_or("-"),
                );
                txs.push(tx);
            }
            None => {
                missing += 1;
                println!("  MISSING  {}  {}  {link}", step.step, step.txid);
            }
        }
    }
    if missing > 0 {
        return Err(format!("{missing} proof txid(s) not found on TN10 REST").into());
    }
    proof.verify_rest_txs(&txs)?;
    println!(
        "REST Toccata fields match (v1, accepted, selected output, authorizing input, exact previous outpoint, covenant_id)."
    );

    let (coin, utxos) = snap?;
    proof.verify_kascov(&coin)?;
    println!(
        "kascov     {}  status={} lineage_complete={} events={} live_utxos={} live_sompi={}",
        coin.name,
        coin.status,
        coin.lineage_complete,
        coin.event_count,
        coin.live_utxos,
        coin.live_value
    );
    println!(
        "           kascov live utxos {} (typed community indexer data; not consensus evidence)",
        utxos.len()
    );
    println!("           {}", kascov.coin_url(proof.covenant_id()?));
    Ok(())
}
