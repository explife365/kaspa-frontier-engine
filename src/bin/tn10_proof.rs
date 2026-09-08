//! Verify a TN10 SilverScript covenant proof bundle (REST + kascov).

use kaspa_frontier_engine::network::{self, KASCOV_TN10, TESTNET_10_REST};
use kaspa_frontier_engine::proof::ProofApp;
use kaspa_frontier_engine::{CovenantProof, KascovClient, Tn10RestClient};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

struct Options {
    path: PathBuf,
    json: bool,
    offline: bool,
    kascov_only: bool,
}

fn usage() -> &'static str {
    "usage: tn10-proof [fixtures/tn10-counter-proof.json] [--json] [--offline] [--kascov-only]"
}

fn parse_args(arguments: impl IntoIterator<Item = String>) -> Result<Options, String> {
    let mut path = PathBuf::from("fixtures/tn10-counter-proof.json");
    let mut json = false;
    let mut offline = false;
    let mut kascov_only = false;
    for argument in arguments {
        if argument == "--json" {
            json = true;
        } else if argument == "--offline" {
            offline = true;
        } else if argument == "--kascov-only" {
            kascov_only = true;
        } else if argument.starts_with('-') {
            return Err(format!("unknown flag {argument}"));
        } else {
            path = PathBuf::from(argument);
        }
    }
    if offline && kascov_only {
        return Err("--offline and --kascov-only are mutually exclusive".into());
    }
    Ok(Options {
        path,
        json,
        offline,
        kascov_only,
    })
}

fn load_offline_snapshots(
    proof: &CovenantProof,
    proof_path: &Path,
) -> Result<
    (
        Vec<kaspa_frontier_engine::rest::ToccataTx>,
        kaspa_frontier_engine::kascov::KascovCoin,
        String,
    ),
    Box<dyn std::error::Error>,
> {
    let prefix = match proof.app_kind() {
        ProofApp::TimelockVault => "tn10-vault",
        ProofApp::RestrictedSwap => "tn10-swap",
        ProofApp::Counter => "tn10-counter",
    };
    let fixture_dir = proof_path
        .parent()
        .filter(|dir| dir.ends_with("fixtures"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("fixtures"));
    let rest_path = fixture_dir.join(format!("{prefix}-rest.json"));
    let kascov_path = fixture_dir.join(format!("{prefix}-kascov.json"));
    let rest_raw = if prefix == "tn10-counter" {
        include_str!("../../fixtures/tn10-counter-rest.json").to_string()
    } else {
        fs::read_to_string(&rest_path).map_err(|error| {
            format!(
                "read {}: {error} (publish fixtures after broadcast; see fixtures/tn10-*-proof.PENDING.md)",
                rest_path.display()
            )
        })?
    };
    let kascov_raw = if prefix == "tn10-counter" {
        include_str!("../../fixtures/tn10-counter-kascov.json").to_string()
    } else {
        fs::read_to_string(&kascov_path).map_err(|error| {
            format!(
                "read {}: {error} (publish fixtures after broadcast; see fixtures/tn10-*-proof.PENDING.md)",
                kascov_path.display()
            )
        })?
    };
    let txs: Vec<kaspa_frontier_engine::rest::ToccataTx> = serde_json::from_str(&rest_raw)?;
    let coin: kaspa_frontier_engine::kascov::KascovCoin = serde_json::from_str(&kascov_raw)?;
    let cid = proof.covenant_id()?.to_string();
    let kascov_url = format!("https://kascov.io/data/testnet-10/c/{cid}.json");
    Ok((txs, coin, kascov_url))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options =
        parse_args(env::args().skip(1)).map_err(|error| format!("{error}\n{}", usage()))?;
    if !options.path.is_file() {
        return Err(format!(
            "no proof at {} — use the checked-in fixture or provide a generated proof",
            options.path.display()
        )
        .into());
    }

    let proof = CovenantProof::from_path(&options.path)?;
    let report = if options.offline {
        let (txs, coin, kascov_url) = load_offline_snapshots(&proof, &options.path)?;
        proof.verify_offline_fixture(&txs, &coin, &kascov_url)?
    } else if options.kascov_only {
        let client = Tn10RestClient::new(TESTNET_10_REST)?;
        let kascov = KascovClient::new(KASCOV_TN10)?;
        proof.verify_kascov_online(&client, &kascov).await?
    } else {
        let client = Tn10RestClient::new(TESTNET_10_REST)?;
        let kascov = KascovClient::new(KASCOV_TN10)?;
        proof.verify_online(&client, &kascov).await?
    };

    if options.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    network::print_dev_sig();
    println!("proof      {}", options.path.display());
    println!("network    {}", report.network);
    println!("funding    {}", report.funding_address);
    println!(
        "steps      {}/{}  complete={}",
        report.steps.len(),
        kaspa_frontier_engine::proof::EXPECTED_STEPS.len(),
        report.complete
    );
    println!("covenant   {}", report.covenant_id);
    for step in &report.steps {
        println!(
            "  OK  {}  v{} accepted={} mass={} in_cid={} out_cid={}  {}",
            step.step,
            step.version.unwrap_or(0),
            step.is_accepted.unwrap_or(false),
            step.storage_mass.as_deref().unwrap_or("-"),
            step.input_covenant_id.as_deref().unwrap_or("-"),
            step.output_covenant_id.as_deref().unwrap_or("-"),
            step.explorer
        );
    }
    println!(
        "REST Toccata fields match (v1, accepted, selected output, authorizing input, exact previous outpoint, covenant_id)."
    );
    if !report.rest_verified {
        println!("REST       partial or unavailable (use --offline for fixture replay or --kascov-only for live kascov)");
    }
    if let Some(coin) = &report.kascov {
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
            coin.fetched_live_utxos
        );
    }
    println!("           {}", report.kascov_url);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_json_flag() {
        let options = parse_args(["custom.json".into(), "--json".into()]).unwrap();
        assert_eq!(options.path, PathBuf::from("custom.json"));
        assert!(options.json);
    }

    #[test]
    fn rejects_offline_and_kascov_only() {
        assert!(parse_args(["--offline".into(), "--kascov-only".into()]).is_err());
    }
}
