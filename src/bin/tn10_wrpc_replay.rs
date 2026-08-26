//! Offline rehearsal for durable wRPC notification replay.
//! No WebSocket or network call is made; live owned-node transport is a later step.

use kaspa_frontier_engine::network::is_valid_testnet_address;
use kaspa_frontier_engine::{decode_notification, replay_into_ledger, DepositLedger, WrpcJournal};
use std::env;
use std::fs;
use std::path::PathBuf;

const DEFAULT_ADDRESS: &str =
    "kaspatest:qptv6u8kel95drh2p2z492cyksk8lpetep286fngqu5j9nk57g642lzf748kt";

fn parse_fixture(raw: &str) -> Result<Vec<&str>, String> {
    let mut frames = Vec::new();
    for (line_index, raw_line) in raw.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        decode_notification(line)
            .map_err(|error| format!("invalid fixture line {}: {error}", line_index + 1))?;
        frames.push(line);
    }
    if frames.is_empty() {
        return Err("wRPC replay fixture contains no frames".into());
    }
    Ok(frames)
}

fn usage() -> &'static str {
    "usage: tn10-wrpc-replay [ADDRESS] [--fixture PATH] [--database PATH] [--source NAME]"
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut address = DEFAULT_ADDRESS.to_string();
    let mut fixture = PathBuf::from("fixtures/wrpc-utxos-replay.jsonl");
    let mut database = PathBuf::from(".local/tn10-wrpc-replay.sqlite");
    let mut source = "fixture:wrpc-utxos-replay-v1".to_string();
    let mut address_set = false;
    let mut args = env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--fixture" => {
                fixture = PathBuf::from(args.next().ok_or("--fixture needs a path")?);
            }
            "--database" => {
                database = PathBuf::from(args.next().ok_or("--database needs a path")?);
            }
            "--source" => {
                source = args.next().ok_or("--source needs a name")?;
            }
            _ if argument.starts_with('-') => return Err(format!("unknown flag {argument}").into()),
            _ if !address_set => {
                address = argument;
                address_set = true;
            }
            _ => return Err(usage().into()),
        }
    }
    if !is_valid_testnet_address(&address) {
        return Err(format!("replay requires a checksummed kaspatest address: {address}").into());
    }
    if let Some(parent) = database.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }

    let raw = fs::read_to_string(&fixture)?;
    let frames = parse_fixture(&raw)?;
    let mut journal = WrpcJournal::open(&database)?;
    for (index, frame) in frames.iter().enumerate() {
        let sequence = u64::try_from(index + 1)?;
        let _ = journal.record(&source, sequence, frame)?;
    }

    let mut ledger = DepositLedger::open(&database)?;
    let report = replay_into_ledger(&mut journal, &mut ledger, &source, &address)?;

    println!("fixture     {}", fixture.display());
    println!("database    {}", database.display());
    println!("source      {source}");
    println!("checkpoint  {}/{}", report.checkpoint, report.frame_count);
    println!("live UTXOs  {}", report.live_utxos);
    println!("pending     {}", ledger.pending_count()?);
    for event in ledger.unacknowledged_events()? {
        println!(
            "outbox      {} key={} {}:{} {} sompi {}",
            event.kind,
            event.event_key,
            event.tx_id,
            event.output_index,
            event.amount_sompi,
            event.address
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_in_fixture_is_strict_and_complete() {
        let raw = include_str!("../../fixtures/wrpc-utxos-replay.jsonl");
        let frames = parse_fixture(raw).unwrap();
        assert_eq!(frames.len(), 4);
    }

    #[test]
    fn fixture_parser_reports_poison_line() {
        let error = parse_fixture("{}\n").unwrap_err();
        assert!(error.contains("line 1"));
    }
}
