use kaspa_frontier_engine::network::{
    self, is_testnet_address, tn10_tx_url, COINBASE_MATURITY_DAA, TN10_EXPLORER,
};
use kaspa_frontier_engine::{DepositLedger, DepositWatch, Tn10RestClient};
use std::collections::HashSet;
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

fn parse_credit_line(line: &str) -> Option<(String, u32)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let split = line.find(|c: char| c == ':' || c.is_whitespace())?;
    let tx = line[..split].trim();
    let vout = line[split + 1..].trim().parse().ok()?;
    if tx.is_empty() {
        return None;
    }
    Some((tx.to_string(), vout))
}

fn load_ledger(path: &Path) -> io::Result<Vec<(String, u32)>> {
    let raw = fs::read_to_string(path)?;
    let mut credits = Vec::new();
    for (index, line) in raw.lines().enumerate() {
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        let credit = parse_credit_line(line).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("invalid legacy credit at {}:{}", path.display(), index + 1),
            )
        })?;
        credits.push(credit);
    }
    Ok(credits)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter("tn10_deposits=info,kaspa_frontier_engine=info")
        .init();

    let mut address = None;
    let mut ledger = PathBuf::from(".local/tn10-deposits.sqlite");
    let mut import_text: Option<PathBuf> = None;
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--ledger" {
            ledger = PathBuf::from(args.next().ok_or("--ledger needs a path")?);
        } else if arg == "--import-text" {
            import_text = Some(PathBuf::from(
                args.next().ok_or("--import-text needs a path")?,
            ));
        } else if arg.starts_with('-') {
            return Err(format!("unknown flag {arg}").into());
        } else if address.is_none() {
            address = Some(arg);
        }
    }
    let address = address
        .ok_or("usage: tn10-deposits <kaspatest:address> [--ledger PATH] [--import-text PATH]")?;
    if !is_testnet_address(&address) {
        return Err(format!("TN10 watcher refuses non-testnet address: {address}").into());
    }

    network::print_dev_sig();
    println!("Watching {address}");
    println!("Explorer {TN10_EXPLORER}/addresses/{address}");
    println!(
        "Confirmations: {} DAA (~{:.1}s at 10 BPS); coinbase maturity {} DAA",
        network::DEFAULT_DEPOSIT_CONFIRMATIONS,
        network::DEFAULT_DEPOSIT_CONFIRMATIONS as f64 / network::TARGET_BPS,
        COINBASE_MATURITY_DAA
    );

    let client = Tn10RestClient::new(network::TESTNET_10_REST)?;
    let mut watch = DepositWatch::new(network::DEFAULT_DEPOSIT_CONFIRMATIONS);
    if let Some(parent) = ledger.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    let mut deposit_ledger = DepositLedger::open(&ledger)?;
    if let Some(path) = import_text {
        let prior = load_ledger(&path)?;
        for (tx_id, output_index) in &prior {
            deposit_ledger.import_legacy_credit(tx_id, *output_index)?;
        }
        println!(
            "imported  {} prior text credits from {}",
            prior.len(),
            path.display()
        );
    }
    println!(
        "ledger    {} (SQLite WAL, synchronous=FULL)",
        ledger.display()
    );
    println!("outbox    delivery requires tn10-outbox; watcher never auto-acknowledges");
    let mut delay = Duration::from_secs(2);
    let mut warned_lag = false;
    let mut printed_outbox = HashSet::new();
    loop {
        match watch.poll(&client, &address).await {
            Ok(tick) => {
                delay = Duration::from_secs(2);
                if let Some(balance) = tick.rest_balance_sompi {
                    if balance > tick.utxo_sum_sompi {
                        if !warned_lag {
                            eprintln!(
                                "REST indexer lag: balance {balance} sompi > UTXO sum {}",
                                tick.utxo_sum_sompi
                            );
                            warned_lag = true;
                        }
                    } else {
                        warned_lag = false;
                    }
                }
                deposit_ledger.reconcile(
                    tick.virtual_daa,
                    &tick.observed,
                    &tick.confirmed,
                    &tick.disappeared_outpoints,
                )?;
                if tick.newly_seen > 0 || tick.disappeared > 0 || !tick.confirmed.is_empty() {
                    println!(
                        "DAA {}  seen+{} gone-{} pending {} confirmed {}",
                        tick.virtual_daa,
                        tick.newly_seen,
                        tick.disappeared,
                        deposit_ledger.pending_count()?,
                        tick.confirmed.len()
                    );
                }
                for event in deposit_ledger.unacknowledged_events()? {
                    if !printed_outbox.insert(event.id) {
                        continue;
                    }
                    println!(
                        "  {} outbox={} key={} {} vout {}  {} sompi  address={}  {}",
                        event.kind.to_uppercase(),
                        event.id,
                        event.event_key,
                        event.tx_id,
                        event.output_index,
                        event.amount_sompi,
                        event.address,
                        tn10_tx_url(&event.tx_id)
                    );
                    io::stdout().flush()?;
                }
            }
            Err(err) => {
                eprintln!("poll error: {err}");
                delay = (delay * 2).min(Duration::from_secs(30));
            }
        }
        tokio::time::sleep(delay).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_import_rejects_missing_and_malformed_files() {
        let directory = tempfile::tempdir().unwrap();
        assert!(load_ledger(&directory.path().join("missing.txt")).is_err());

        let malformed = directory.path().join("malformed.txt");
        fs::write(&malformed, "tx:0\nnot-an-outpoint\n").unwrap();
        let error = load_ledger(&malformed).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(error.to_string().contains(":2"));
    }

    #[test]
    fn legacy_import_accepts_comments_and_both_separators() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("credits.txt");
        fs::write(&path, "# old ledger\ntx-a:1\ntx-b 2\n").unwrap();
        assert_eq!(
            load_ledger(&path).unwrap(),
            vec![("tx-a".into(), 1), ("tx-b".into(), 2)]
        );
    }
}
