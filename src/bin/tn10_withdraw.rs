use kaspa_frontier_engine::network::{
    self, is_testnet_address, tn10_tx_url, DEFAULT_DEPOSIT_CONFIRMATIONS, TARGET_BPS,
};
use kaspa_frontier_engine::watch::poll_withdrawal;
use kaspa_frontier_engine::{Tn10RestClient, WithdrawalExpectation};
use std::env;
use std::time::{Duration, Instant};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter("tn10_withdraw=info,kaspa_frontier_engine=info")
        .init();

    let dest = env::args().nth(1).ok_or(
        "usage: tn10-withdraw <kaspatest:dest> <txid> <vout> <amount-sompi> [confirmations]",
    )?;
    let tx_id = env::args().nth(2).ok_or(
        "usage: tn10-withdraw <kaspatest:dest> <txid> <vout> <amount-sompi> [confirmations]",
    )?;
    let output_index = env::args()
        .nth(3)
        .ok_or("tn10-withdraw requires the expected vout")?
        .parse::<u32>()?;
    let amount_sompi = env::args()
        .nth(4)
        .ok_or("tn10-withdraw requires the expected amount in sompi")?
        .parse::<u64>()?;
    let required = env::args()
        .nth(5)
        .map(|s| s.parse::<u64>())
        .transpose()?
        .unwrap_or(DEFAULT_DEPOSIT_CONFIRMATIONS)
        .max(1);

    if !is_testnet_address(&dest) {
        return Err(format!("TN10 withdrawal watcher refuses non-testnet dest: {dest}").into());
    }

    network::print_dev_sig();
    println!("Waiting for withdrawal {tx_id}");
    println!("  dest {dest}");
    println!("  exact vout {output_index} amount {amount_sompi} sompi");
    println!(
        "  required {required} DAA (~{:.1}s at {TARGET_BPS} BPS)",
        required as f64 / TARGET_BPS
    );
    println!("  explorer {}", tn10_tx_url(&tx_id));

    let wait_secs = (((required as f64) / TARGET_BPS) * 4.0 + 60.0).clamp(90.0, 600.0) as u64;
    let client = Tn10RestClient::new(network::TESTNET_10_REST)?;
    let expected = WithdrawalExpectation {
        tx_id: tx_id.clone(),
        dest: dest.clone(),
        amount_sompi,
        output_index,
    };
    let deadline = Instant::now() + Duration::from_secs(wait_secs);
    let mut delay = Duration::from_secs(1);
    loop {
        match poll_withdrawal(&client, &expected, required).await {
            Ok(Some(hit)) => {
                println!(
                    "CONFIRMED vout {}  {} sompi  {} conf  blockDAA {}",
                    hit.output_index, hit.amount_sompi, hit.confirmations, hit.block_daa_score
                );
                println!("  {}", tn10_tx_url(&hit.tx_id));
                return Ok(());
            }
            Ok(None) => {
                if Instant::now() >= deadline {
                    return Err(format!("withdrawal not confirmed within {wait_secs}s").into());
                }
                delay = Duration::from_secs(2);
            }
            Err(err) => {
                eprintln!("poll error: {err}");
                delay = (delay * 2).min(Duration::from_secs(15));
                if Instant::now() >= deadline {
                    return Err(err.into());
                }
            }
        }
        tokio::time::sleep(delay).await;
    }
}
