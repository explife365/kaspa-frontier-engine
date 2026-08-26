//! Supervisor-ready health gate for an owned TN10 kaspad.

use kaspa_frontier_engine::network::{TESTNET_10_REST, TN10_WRPC_JSON};
use kaspa_frontier_engine::{assess_owned_node, probe_owned_node, Tn10RestClient};
use std::env;

const DEFAULT_MAX_DAA_LAG: u64 = 100;

struct Options {
    url: String,
    rest: String,
    max_daa_lag: u64,
    json: bool,
}

fn usage() -> &'static str {
    "usage: tn10-node-health [--url ws://127.0.0.1:18210] [--rest HTTPS] [--max-daa-lag 100] [--json]"
}

fn parse_args(arguments: impl IntoIterator<Item = String>) -> Result<Options, String> {
    let mut url = format!("ws://127.0.0.1:{TN10_WRPC_JSON}");
    let mut rest = TESTNET_10_REST.to_string();
    let mut max_daa_lag = DEFAULT_MAX_DAA_LAG;
    let mut json = false;
    let mut args = arguments.into_iter();
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--url" => url = args.next().ok_or("--url needs a value")?,
            "--rest" => rest = args.next().ok_or("--rest needs a value")?,
            "--max-daa-lag" => {
                max_daa_lag = args
                    .next()
                    .ok_or("--max-daa-lag needs a value")?
                    .parse()
                    .map_err(|_| "--max-daa-lag must be an integer")?;
                if !(1..=100_000).contains(&max_daa_lag) {
                    return Err("--max-daa-lag must be 1-100000".into());
                }
            }
            "--json" => json = true,
            _ => return Err(format!("unknown argument {argument}")),
        }
    }
    Ok(Options {
        url,
        rest,
        max_daa_lag,
        json,
    })
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options =
        parse_args(env::args().skip(1)).map_err(|error| format!("{error}\n{}", usage()))?;
    let rest = Tn10RestClient::new(&options.rest)?;
    let (owned, public) = tokio::join!(probe_owned_node(&options.url), rest.block_dag_info());
    let owned = owned?;
    let public = public?;
    let assessment = assess_owned_node(&owned, public.virtual_daa_score, options.max_daa_lag)?;

    if options.json {
        println!(
            "{}",
            serde_json::json!({
                "healthy": true,
                "url": options.url,
                "network": owned.server.network_id,
                "serverVersion": owned.server.server_version,
                "rpcApiVersion": owned.server.rpc_api_version,
                "rpcApiRevision": owned.server.rpc_api_revision,
                "isSynced": owned.server.is_synced,
                "hasUtxoIndex": owned.server.has_utxo_index,
                "localDaa": assessment.local_daa,
                "publicDaa": assessment.public_daa,
                "behindPublicDaa": assessment.behind_public_daa,
                "serverDagDelta": assessment.server_dag_delta,
                "maxDaaLag": options.max_daa_lag,
            })
        );
    } else {
        println!("owned node   {}", options.url);
        println!("network      {}", owned.server.network_id);
        println!("version      {}", owned.server.server_version);
        println!(
            "RPC          {}.{}",
            owned.server.rpc_api_version, owned.server.rpc_api_revision
        );
        println!("synced       {}", owned.server.is_synced);
        println!("UTXO index   {}", owned.server.has_utxo_index);
        println!("local DAA    {}", assessment.local_daa);
        println!("public DAA   {}", assessment.public_daa);
        println!(
            "lag          {} DAA (maximum {})",
            assessment.behind_public_daa, options.max_daa_lag
        );
        println!("status       healthy");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bounded_health_options() {
        let options = parse_args([
            "--url".into(),
            "ws://localhost:18210".into(),
            "--max-daa-lag".into(),
            "500".into(),
            "--json".into(),
        ])
        .unwrap();
        assert_eq!(options.url, "ws://localhost:18210");
        assert_eq!(options.max_daa_lag, 500);
        assert!(options.json);
        assert!(parse_args(["--max-daa-lag".into(), "0".into()]).is_err());
        assert!(parse_args(["--unknown".into()]).is_err());
    }
}
