//! Supervisor-ready health gate for an owned TN10 kaspad.

use kaspa_frontier_engine::network::{TESTNET_10_REST, TN10_WRPC_JSON};
use kaspa_frontier_engine::{
    assess_owned_node, probe_owned_node, select_primary, validate_owned_node_urls, Tn10RestClient,
};
use std::env;

const DEFAULT_MAX_DAA_LAG: u64 = 100;

struct Options {
    urls: Vec<String>,
    rest: String,
    max_daa_lag: u64,
    min_healthy: usize,
    json: bool,
}

fn usage() -> &'static str {
    "usage: tn10-node-health [--url ws://127.0.0.1:18210]... [--min-healthy N] [--rest HTTPS] [--max-daa-lag 100] [--json]"
}

fn parse_args(arguments: impl IntoIterator<Item = String>) -> Result<Options, String> {
    let mut urls = Vec::new();
    let mut rest = TESTNET_10_REST.to_string();
    let mut max_daa_lag = DEFAULT_MAX_DAA_LAG;
    let mut min_healthy = 1usize;
    let mut json = false;
    let mut args = arguments.into_iter();
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--url" => urls.push(args.next().ok_or("--url needs a value")?),
            "--rest" => rest = args.next().ok_or("--rest needs a value")?,
            "--min-healthy" => {
                min_healthy = args
                    .next()
                    .ok_or("--min-healthy needs a value")?
                    .parse()
                    .map_err(|_| "--min-healthy must be an integer")?;
            }
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
    if urls.is_empty() {
        urls.push(format!("ws://127.0.0.1:{TN10_WRPC_JSON}"));
    }
    validate_owned_node_urls(&urls).map_err(|error| error.to_string())?;
    if min_healthy == 0 || min_healthy > urls.len() {
        return Err("--min-healthy must be 1 through the number of node URLs".into());
    }
    Ok(Options {
        urls,
        rest,
        max_daa_lag,
        min_healthy,
        json,
    })
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options =
        parse_args(env::args().skip(1)).map_err(|error| format!("{error}\n{}", usage()))?;
    let rest = Tn10RestClient::new(&options.rest)?;
    let public = rest.block_dag_info().await?;
    let probes =
        futures_util::future::join_all(options.urls.iter().map(|url| probe_owned_node(url))).await;
    let mut reports = Vec::with_capacity(options.urls.len());
    let mut healthy = Vec::new();
    for (index, (url, probe)) in options.urls.iter().zip(probes).enumerate() {
        match probe.and_then(|health| {
            assess_owned_node(&health, public.virtual_daa_score, options.max_daa_lag)
                .map(|assessment| (health, assessment))
        }) {
            Ok((health, assessment)) => {
                healthy.push((index, assessment.clone()));
                reports.push(serde_json::json!({
                    "url": url,
                    "healthy": true,
                    "network": health.server.network_id,
                    "serverVersion": health.server.server_version,
                    "rpcApiVersion": health.server.rpc_api_version,
                    "rpcApiRevision": health.server.rpc_api_revision,
                    "isSynced": health.server.is_synced,
                    "hasUtxoIndex": health.server.has_utxo_index,
                    "localDaa": assessment.local_daa,
                    "publicDaa": assessment.public_daa,
                    "behindPublicDaa": assessment.behind_public_daa,
                    "serverDagDelta": assessment.server_dag_delta,
                }));
            }
            Err(error) => reports.push(serde_json::json!({
                "url": url,
                "healthy": false,
                "error": error.to_string(),
            })),
        }
    }
    let selected_index = select_primary(&healthy);
    let selected_url = selected_index.map(|index| options.urls[index].as_str());
    let gate_healthy = healthy.len() >= options.min_healthy;

    if options.json {
        println!(
            "{}",
            serde_json::json!({
                "healthy": gate_healthy,
                "healthyNodes": healthy.len(),
                "requiredHealthyNodes": options.min_healthy,
                "selectedUrl": selected_url,
                "publicDaa": public.virtual_daa_score,
                "maxDaaLag": options.max_daa_lag,
                "nodes": reports,
            })
        );
    } else {
        for report in &reports {
            println!(
                "node         {} {}{}",
                report["url"].as_str().unwrap_or("<invalid>"),
                if report["healthy"].as_bool().unwrap_or(false) {
                    "healthy"
                } else {
                    "unhealthy"
                },
                report["error"]
                    .as_str()
                    .map(|error| format!(" ({error})"))
                    .unwrap_or_default()
            );
        }
        println!("public DAA   {}", public.virtual_daa_score);
        println!(
            "redundancy   {}/{} healthy",
            healthy.len(),
            options.min_healthy
        );
        println!("selected     {}", selected_url.unwrap_or("none"));
        println!(
            "status       {}",
            if gate_healthy { "healthy" } else { "unhealthy" }
        );
    }
    if !gate_healthy {
        return Err(format!(
            "owned-node redundancy gate failed: {} healthy, {} required",
            healthy.len(),
            options.min_healthy
        )
        .into());
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
        assert_eq!(options.urls, vec!["ws://localhost:18210"]);
        assert_eq!(options.max_daa_lag, 500);
        assert!(options.json);
        assert!(parse_args(["--max-daa-lag".into(), "0".into()]).is_err());
        assert!(parse_args(["--unknown".into()]).is_err());
        let options = parse_args([
            "--url".into(),
            "ws://127.0.0.1:18211".into(),
            "--url".into(),
            "ws://127.0.0.1:18210".into(),
            "--min-healthy".into(),
            "2".into(),
        ])
        .unwrap();
        assert_eq!(options.urls.len(), 2);
        assert_eq!(options.min_healthy, 2);
        assert!(parse_args([
            "--url".into(),
            "ws://127.0.0.1:18210".into(),
            "--min-healthy".into(),
            "2".into(),
        ])
        .is_err());
        assert!(parse_args([
            "--url".into(),
            "ws://127.0.0.1:18210".into(),
            "--url".into(),
            "ws://127.0.0.1:18210".into(),
        ])
        .is_err());
    }
}
