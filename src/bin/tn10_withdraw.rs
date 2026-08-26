use kaspa_frontier_engine::network::{
    self, tn10_tx_url, DEFAULT_DEPOSIT_CONFIRMATIONS, TARGET_BPS,
};
use kaspa_frontier_engine::{
    poll_durable_withdrawal, EngineError, Tn10RestClient, WithdrawalExpectation, WithdrawalLedger,
    WithdrawalState,
};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

struct Options {
    expected: WithdrawalExpectation,
    required: u64,
    database: PathBuf,
}

fn usage() -> &'static str {
    "usage: tn10-withdraw <kaspatest:dest> <txid> <vout> <amount-sompi> [confirmations] [--database PATH]"
}

fn parse_args(arguments: impl IntoIterator<Item = String>) -> Result<Options, String> {
    let mut args = arguments.into_iter();
    let dest = args.next().ok_or_else(|| usage().to_string())?;
    let tx_id = args.next().ok_or_else(|| usage().to_string())?;
    let output_index = args
        .next()
        .ok_or("tn10-withdraw requires the expected vout")?
        .parse::<u32>()
        .map_err(|_| "withdrawal vout must be a u32")?;
    let amount_sompi = args
        .next()
        .ok_or("tn10-withdraw requires the expected amount in sompi")?
        .parse::<u64>()
        .map_err(|_| "withdrawal amount must be a u64")?;
    let mut required = DEFAULT_DEPOSIT_CONFIRMATIONS;
    let mut database = PathBuf::from(".local/tn10-withdrawals.sqlite");
    let remaining: Vec<_> = args.collect();
    let mut index = 0usize;
    if remaining
        .first()
        .is_some_and(|value| !value.starts_with('-'))
    {
        required = remaining[0]
            .parse::<u64>()
            .map_err(|_| "confirmations must be a u64")?
            .max(1);
        index = 1;
    }
    while index < remaining.len() {
        match remaining[index].as_str() {
            "--database" => {
                index += 1;
                database = PathBuf::from(
                    remaining
                        .get(index)
                        .ok_or("--database needs a path")?
                        .as_str(),
                );
            }
            flag => return Err(format!("unknown argument {flag}")),
        }
        index += 1;
    }
    Ok(Options {
        expected: WithdrawalExpectation {
            tx_id,
            dest,
            amount_sompi,
            output_index,
        },
        required,
        database,
    })
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter("tn10_withdraw=info,kaspa_frontier_engine=info")
        .init();

    let options =
        parse_args(env::args().skip(1)).map_err(|error| format!("{error}\n{}", usage()))?;
    if let Some(parent) = options.database.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    let mut ledger = WithdrawalLedger::open(&options.database)?;
    let registered = ledger.register(&options.expected, options.required)?;

    network::print_dev_sig();
    println!("Waiting for withdrawal {}", options.expected.tx_id);
    println!("  dest {}", options.expected.dest);
    println!(
        "  exact vout {} amount {} sompi",
        options.expected.output_index, options.expected.amount_sompi
    );
    println!(
        "  required {required} DAA (~{:.1}s at {TARGET_BPS} BPS)",
        options.required as f64 / TARGET_BPS,
        required = options.required
    );
    println!("  database {}", options.database.display());
    println!("  resumed state {}", registered.state.as_str());
    println!("  explorer {}", tn10_tx_url(&options.expected.tx_id));

    let wait_secs =
        (((options.required as f64) / TARGET_BPS) * 4.0 + 60.0).clamp(90.0, 600.0) as u64;
    let client = Tn10RestClient::new(network::TESTNET_10_REST)?;
    let deadline = Instant::now() + Duration::from_secs(wait_secs);
    let mut delay = Duration::from_secs(1);
    let mut last_state = registered.state;
    loop {
        match poll_durable_withdrawal(&client, &mut ledger, &options.expected).await {
            Ok(record) if record.state == WithdrawalState::Confirmed => {
                let block_daa = record
                    .observed_block_daa
                    .ok_or("confirmed withdrawal is missing observed block DAA")?;
                let confirmations = record.last_checked_daa.saturating_sub(block_daa);
                println!(
                    "CONFIRMED vout {}  {} sompi  {} conf  blockDAA {}",
                    record.expected.output_index,
                    record.expected.amount_sompi,
                    confirmations,
                    block_daa
                );
                println!("  {}", tn10_tx_url(&record.expected.tx_id));
                return Ok(());
            }
            Ok(record) if record.state == WithdrawalState::Rejected => {
                return Err(record
                    .rejection_reason
                    .unwrap_or_else(|| "withdrawal was rejected".into())
                    .into());
            }
            Ok(record) => {
                if record.state != last_state {
                    println!("  state {}", record.state.as_str());
                    last_state = record.state;
                }
                if Instant::now() >= deadline {
                    return Err(format!(
                        "withdrawal remained {} after {wait_secs}s; durable state retained",
                        record.state.as_str()
                    )
                    .into());
                }
                delay = Duration::from_secs(2);
            }
            Err(err @ (EngineError::Transport(_) | EngineError::Json(_))) => {
                eprintln!("poll error: {err}");
                delay = (delay * 2).min(Duration::from_secs(15));
                if Instant::now() >= deadline {
                    return Err(err.into());
                }
            }
            Err(error) => return Err(error.into()),
        }
        tokio::time::sleep(delay).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ADDRESS: &str = "kaspatest:qptv6u8kel95drh2p2z492cyksk8lpetep286fngqu5j9nk57g642lzf748kt";

    #[test]
    fn parses_durable_database_and_confirmations() {
        let options = parse_args([
            ADDRESS.into(),
            "a".repeat(64),
            "1".into(),
            "25000000".into(),
            "120".into(),
            "--database".into(),
            "state.sqlite".into(),
        ])
        .unwrap();
        assert_eq!(options.required, 120);
        assert_eq!(options.database, PathBuf::from("state.sqlite"));
        assert_eq!(options.expected.output_index, 1);
        assert!(parse_args([ADDRESS.into(), "bad".into()]).is_err());
        assert!(parse_args([
            ADDRESS.into(),
            "a".repeat(64),
            "1".into(),
            "1".into(),
            "--unknown".into(),
        ])
        .is_err());
    }
}
