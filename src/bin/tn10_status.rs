use kaspa_frontier_engine::l2::{probe_igra_galleon, probe_kasplex_l2};
use kaspa_frontier_engine::roadmap;
use kaspa_frontier_engine::{network, GhostdagTelemetry, KasplexClient, Tn10RestClient};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter("tn10_status=info,kaspa_frontier_engine=info")
        .init();

    println!("Kaspa frontier — testnet-10 live status");
    println!("REST: {}", network::TESTNET_10_REST);
    println!("Local node: {}", network::local_kaspad_cmd());
    println!("Faucet: {}", network::TN10_FAUCET);
    println!("Kasplex KRC-20: {}", network::KASPLEX_TN10);
    println!("kascov: {}", network::KASCOV_TN10);
    println!(
        "Igra Galleon L2: {}  chain {}",
        network::IGRA_GALLEON_RPC,
        network::IGRA_GALLEON_CHAIN_ID
    );
    println!(
        "Kasplex L2: {}  chain {}",
        network::KASPLEX_L2_RPC,
        network::KASPLEX_L2_CHAIN_ID
    );
    network::print_dev_sig();
    println!();

    let client = Tn10RestClient::new(network::TESTNET_10_REST)?;
    let kasplex = KasplexClient::new(network::KASPLEX_TN10)?;
    let usdc_meta = async {
        let rpc = kaspa_frontier_engine::l2::EvmRpcClient::new(network::IGRA_GALLEON_RPC)?;
        rpc.galleon_test_usdc_meta().await
    };
    let (snap, krc, page, igra, kasplex_l2, usdc) = tokio::join!(
        client.status_snapshot(),
        kasplex.info(),
        kasplex.tokenlist(None),
        probe_igra_galleon(),
        probe_kasplex_l2(),
        usdc_meta
    );
    let snap = snap?;
    let dag = snap.dag;
    let hr = snap.hashrate;
    let mut telemetry = GhostdagTelemetry::from_block_dag(&dag);
    if let Some(ref measured) = hr {
        telemetry = telemetry.with_rest_hashrate(measured);
    }

    println!("network            {}", telemetry.network);
    println!("protocol           {}", telemetry.protocol);
    println!("virtual DAA        {}", telemetry.virtual_daa_score);
    println!("blocks             {}", telemetry.block_count);
    println!("difficulty         {:.4}", telemetry.difficulty);
    println!(
        "est. hashrate      {:.4} H/s  (difficulty × {} BPS, not pool data)",
        telemetry.estimated_hashrate_hs, telemetry.target_bps
    );
    match hr {
        Some(measured) => println!(
            "REST hashrate      {:.6} TH/s  ({:.4} H/s)",
            measured.hashrate,
            measured.hashes_per_second()
        ),
        None => println!("REST hashrate      unavailable"),
    }
    match snap.fee {
        Some(fee) => {
            println!(
                "fee estimate       priority {:.0} sompi/gram  (~{:.4}s)  standard relay policy target is 100{}",
                fee.priority_feerate(),
                fee.priority_bucket.estimated_seconds,
                if fee.meets_standard_relay_rate() {
                    ""
                } else {
                    "  BELOW FLOOR"
                }
            );
            if let Some(normal) = fee.normal_buckets.first() {
                println!(
                    "                    normal {:.0} sompi/gram  (~{:.4}s)",
                    normal.feerate, normal.estimated_seconds
                );
            }
            if let Some(low) = fee.low_buckets.first() {
                println!(
                    "                    low {:.0} sompi/gram  (~{:.4}s)",
                    low.feerate, low.estimated_seconds
                );
            }
            for (name, rate) in fee.buckets_below_standard_rate() {
                println!("                    {name} {rate:.0} is below Toccata 100 sompi/gram");
            }
        }
        None => println!("fee estimate       unavailable"),
    }
    match krc {
        Ok(status) => {
            let synced = if status.is_synced() {
                "synced"
            } else {
                status.message.as_str()
            };
            let gap_note = match status.info.daa_gap() {
                Some(gap) if gap.abs() > 50 => "  INDEXER LAG",
                _ => "",
            };
            println!(
                "KRC-20 indexer     {synced}  tokens={}  daa_gap={}{gap_note}  (Kasplex inscriptions, not USD)",
                status.info.token_total,
                status.info.daa_score_gap
            );
        }
        Err(err) => println!("KRC-20 indexer     unavailable ({err})"),
    }
    match page {
        Ok(tokens) => {
            let open = tokens.open_mints().count();
            println!(
                "KRC-20 tokenlist   {} on first page, {} open mints  (not USD)",
                tokens.result.len(),
                open
            );
        }
        Err(err) => println!("KRC-20 tokenlist   unavailable ({err})"),
    }
    print_l2("Igra Galleon L2   ", igra);
    print_l2("Kasplex L2        ", kasplex_l2);
    match usdc {
        Ok(meta) => println!(
            "Galleon ERC-20     {} {} decimals={}  {}  (Igra test USDC, not Circle cash)",
            meta.symbol, meta.address, meta.decimals, meta.name
        ),
        Err(err) => println!("Galleon ERC-20     USDC eth_call failed ({err})"),
    }
    println!("sink               {}", telemetry.sink);
    println!("pruning point      {}", dag.pruning_point_hash);
    match client
        .sample_daa_rate(std::time::Duration::from_secs(5))
        .await
    {
        Ok((a, b, rate)) => {
            let high_rate = GhostdagTelemetry::high_daa_rate_anomaly(rate);
            println!(
                "measured DAA/s      {rate:.2}  (DAA {a} → {b} over 5s)  target {} BPS",
                network::TARGET_BPS
            );
            println!(
                "high-rate anomaly?  {}  (observational only; does not identify DAGKnight)",
                if high_rate {
                    "yes — investigate"
                } else {
                    "no"
                }
            );
        }
        Err(err) => println!("measured DAA/s      unavailable ({err})"),
    }
    println!();
    roadmap::print_tracks();
    roadmap::print_l1_gaps();
    roadmap::print_integrator_next();
    roadmap::print_community_asks();
    kaspa_frontier_engine::kip2_sat::print_round1();
    println!();
    println!("TN10 is reachable. Next:");
    println!("  python examples/tn10_transfer.py --print-wallets");
    println!("  cargo run --release --bin tn10-kasplex");
    println!("  python examples/kasplex_krc20.py --token TMBMN");
    println!("  python examples/kasplex_krc20.py --distribute --from alice --amt 10000000000");
    println!("  Stables/contracts live on Igra Galleon / Kasplex L2, not kaspad.");
    Ok(())
}

fn print_l2(
    label: &str,
    probe: Result<kaspa_frontier_engine::EvmChainProbe, kaspa_frontier_engine::EngineError>,
) {
    match probe {
        Ok(p) if p.matches_expected() => {
            let block = p
                .block_number
                .map(|n| format!("  block {n}"))
                .unwrap_or_default();
            println!(
                "{label} chain {} reachable{block}  (EVM DeFi, not kaspad)",
                p.chain_id
            );
        }
        Ok(p) => println!(
            "{label} chain {} (expected {}) — do not treat as Galleon/Kasplex",
            p.chain_id, p.expected_chain_id
        ),
        Err(err) => println!("{label} unavailable ({err})"),
    }
}
