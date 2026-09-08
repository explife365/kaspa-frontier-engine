//! Supervisor-ready health gate for an owned TN10 kaspad.

use kaspa_frontier_engine::network::TESTNET_10_REST;
use kaspa_frontier_engine::{
    evaluate_owned_node_gate, finish_gate_options, gate_summary_to_json, summarize_gate,
    try_parse_gate_flag, OwnedNodeGateOptions, OwnedNodeGateReport, Tn10RestClient,
    GATE_DEFAULT_MAX_DAA_LAG,
};
use std::env;

struct Options {
    gate: OwnedNodeGateOptions,
    rest: String,
    json: bool,
}

fn usage() -> &'static str {
    "usage: tn10-node-health [--url ws://127.0.0.1:18210]... [--dual] [--min-healthy N] [--rest HTTPS] [--max-daa-lag 100] [--json]"
}

fn parse_args(arguments: impl IntoIterator<Item = String>) -> Result<Options, String> {
    let mut gate = OwnedNodeGateOptions {
        urls: Vec::new(),
        max_daa_lag: GATE_DEFAULT_MAX_DAA_LAG,
        min_healthy: 1,
    };
    let mut rest = TESTNET_10_REST.to_string();
    let mut json = false;
    let mut dual = false;
    let mut args = arguments.into_iter();
    while let Some(argument) = args.next() {
        if try_parse_gate_flag(&mut gate, &mut dual, &argument, &mut args)? {
            continue;
        }
        match argument.as_str() {
            "--rest" => rest = args.next().ok_or("--rest needs a value")?,
            "--json" => json = true,
            _ => return Err(format!("unknown argument {argument}")),
        }
    }
    let gate = finish_gate_options(gate, dual)?;
    if gate.min_healthy == 0 || gate.min_healthy > gate.urls.len() {
        return Err("--min-healthy must be 1 through the number of node URLs".into());
    }
    Ok(Options { gate, rest, json })
}

fn print_human_report(report: &OwnedNodeGateReport) {
    println!(
        "node         {} {} [{}]{}",
        report.url,
        if report.healthy { "healthy" } else { "unhealthy" },
        report.stage_label,
        report
            .error
            .as_deref()
            .map(|error| format!(" ({error})"))
            .unwrap_or_default()
    );
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options =
        parse_args(env::args().skip(1)).map_err(|error| format!("{error}\n{}", usage()))?;
    let rest = Tn10RestClient::new(&options.rest)?;
    let public = rest.block_dag_info().await?;
    let reports = evaluate_owned_node_gate(
        &options.gate.urls,
        public.virtual_daa_score,
        options.gate.max_daa_lag,
    )
    .await;
    let summary = summarize_gate(
        reports,
        &options.gate.urls,
        public.virtual_daa_score,
        options.gate.max_daa_lag,
        options.gate.min_healthy,
    );

    if options.json {
        println!("{}", gate_summary_to_json(&summary));
    } else {
        for report in &summary.reports {
            print_human_report(report);
        }
        println!("public DAA   {}", summary.public_daa);
        println!(
            "redundancy   {}/{} healthy",
            summary.healthy_nodes,
            summary.required_healthy
        );
        println!(
            "selected     {}",
            summary.selected_url.as_deref().unwrap_or("none")
        );
        println!(
            "status       {}",
            if summary.gate_healthy { "healthy" } else { "unhealthy" }
        );
    }
    if !summary.gate_healthy {
        return Err(format!(
            "owned-node redundancy gate failed: {} healthy, {} required",
            summary.healthy_nodes,
            summary.required_healthy
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
        assert_eq!(options.gate.urls, vec!["ws://localhost:18210"]);
        assert_eq!(options.gate.max_daa_lag, 500);
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
        assert_eq!(options.gate.urls.len(), 2);
        assert_eq!(options.gate.min_healthy, 2);
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
        let dual = parse_args(["--dual".into(), "--min-healthy".into(), "2".into()]).unwrap();
        assert_eq!(dual.gate.urls.len(), 2);
        assert_eq!(dual.gate.min_healthy, 2);
        assert!(parse_args([
            "--dual".into(),
            "--url".into(),
            "ws://127.0.0.1:18210".into(),
        ])
        .is_err());
    }
}
