//! Shared N-of-M owned-node gate for integrator CLIs.

use crate::error::{EngineError, Result};
use crate::network::{default_dual_owned_node_urls, loopback_wrpc_url, TN10_WRPC_JSON};
use crate::owned_node::{
    assess_owned_node, owned_node_stage, owned_node_stage_label, probe_owned_node,
    select_primary, validate_owned_node_urls, OwnedNodeAssessment, OwnedNodeHealth,
    STAGE_UNREACHABLE,
};
use crate::rest::Tn10RestClient;
use serde::Serialize;

type GateParseResult<T> = std::result::Result<T, String>;

pub const DEFAULT_MAX_DAA_LAG: u64 = 100;

#[derive(Debug, Clone)]
pub struct OwnedNodeGateOptions {
    pub urls: Vec<String>,
    pub max_daa_lag: u64,
    pub min_healthy: usize,
}

impl Default for OwnedNodeGateOptions {
    fn default() -> Self {
        Self {
            urls: Vec::new(),
            max_daa_lag: DEFAULT_MAX_DAA_LAG,
            min_healthy: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct OwnedNodeGateReport {
    pub url: String,
    pub healthy: bool,
    pub stage: String,
    pub stage_label: String,
    pub error: Option<String>,
    pub health: Option<OwnedNodeHealth>,
    pub assessment: Option<OwnedNodeAssessment>,
}

#[derive(Debug, Clone)]
pub struct OwnedNodeGateSummary {
    pub public_daa: u64,
    pub max_daa_lag: u64,
    pub required_healthy: usize,
    pub healthy_nodes: usize,
    pub gate_healthy: bool,
    pub selected_url: Option<String>,
    pub reports: Vec<OwnedNodeGateReport>,
}

pub fn parse_owned_node_urls_from_env() -> Vec<String> {
    std::env::var("TN10_OWNED_NODE_URLS")
        .ok()
        .map(|raw| {
            raw.split(',')
                .map(str::trim)
                .filter(|part| !part.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// Apply `TN10_MIN_HEALTHY` and `TN10_OWNED_NODE_URLS` when CLI flags were not set.
pub fn apply_gate_env_defaults(options: &mut OwnedNodeGateOptions) {
    if options.min_healthy == 0 {
        if let Ok(raw) = std::env::var("TN10_MIN_HEALTHY") {
            if let Ok(parsed) = raw.parse::<usize>() {
                options.min_healthy = parsed;
            }
        }
    }
    if options.urls.is_empty() {
        options.urls = parse_owned_node_urls_from_env();
    }
}

pub fn resolve_gate_urls(urls: &[String], dual: bool) -> GateParseResult<Vec<String>> {
    if urls.is_empty() {
        return Ok(if dual {
            default_dual_owned_node_urls()
        } else {
            vec![loopback_wrpc_url(TN10_WRPC_JSON)]
        });
    }
    if dual {
        return Err("--dual cannot be combined with explicit --url values".into());
    }
    Ok(urls.to_vec())
}

pub fn validate_gate_options(urls: &[String], min_healthy: usize) -> GateParseResult<()> {
    validate_owned_node_urls(urls).map_err(|error| error.to_string())?;
    if min_healthy == 0 {
        return Ok(());
    }
    if min_healthy > urls.len() {
        return Err("--min-healthy must be 1 through the number of node URLs".into());
    }
    Ok(())
}

pub fn report_from_probe(
    url: &str,
    probe: Result<OwnedNodeHealth>,
    public_daa: u64,
    max_daa_lag: u64,
) -> OwnedNodeGateReport {
    match probe {
        Ok(health) => {
            let stage = owned_node_stage(&health);
            match assess_owned_node(&health, public_daa, max_daa_lag) {
                Ok(assessment) => OwnedNodeGateReport {
                    url: url.to_string(),
                    healthy: true,
                    stage: stage.to_string(),
                    stage_label: owned_node_stage_label(stage).to_string(),
                    error: None,
                    health: Some(health),
                    assessment: Some(assessment),
                },
                Err(error) => OwnedNodeGateReport {
                    url: url.to_string(),
                    healthy: false,
                    stage: stage.to_string(),
                    stage_label: owned_node_stage_label(stage).to_string(),
                    error: Some(error.to_string()),
                    health: Some(health),
                    assessment: None,
                },
            }
        }
        Err(error) => OwnedNodeGateReport {
            url: url.to_string(),
            healthy: false,
            stage: STAGE_UNREACHABLE.to_string(),
            stage_label: owned_node_stage_label(STAGE_UNREACHABLE).to_string(),
            error: Some(error.to_string()),
            health: None,
            assessment: None,
        },
    }
}

pub async fn evaluate_owned_node_gate(
    urls: &[String],
    public_daa: u64,
    max_daa_lag: u64,
) -> Vec<OwnedNodeGateReport> {
    let probes = futures_util::future::join_all(urls.iter().map(|url| probe_owned_node(url))).await;
    urls.iter()
        .zip(probes)
        .map(|(url, probe)| report_from_probe(url, probe, public_daa, max_daa_lag))
        .collect()
}

pub fn summarize_gate(
    reports: Vec<OwnedNodeGateReport>,
    urls: &[String],
    public_daa: u64,
    max_daa_lag: u64,
    required_healthy: usize,
) -> OwnedNodeGateSummary {
    let healthy_candidates: Vec<(usize, OwnedNodeAssessment)> = reports
        .iter()
        .enumerate()
        .filter_map(|(index, report)| {
            report
                .assessment
                .clone()
                .map(|assessment| (index, assessment))
        })
        .collect();
    let selected_index = select_primary(&healthy_candidates);
    let healthy_nodes = reports.iter().filter(|report| report.healthy).count();
    OwnedNodeGateSummary {
        public_daa,
        max_daa_lag,
        required_healthy,
        healthy_nodes,
        gate_healthy: healthy_nodes >= required_healthy,
        selected_url: selected_index.map(|index| urls[index].clone()),
        reports,
    }
}

pub async fn run_owned_node_gate(
    urls: &[String],
    rest: &Tn10RestClient,
    max_daa_lag: u64,
    required_healthy: usize,
) -> Result<OwnedNodeGateSummary> {
    let public = rest.block_dag_info().await?;
    let reports = evaluate_owned_node_gate(urls, public.virtual_daa_score, max_daa_lag).await;
    let summary = summarize_gate(
        reports,
        urls,
        public.virtual_daa_score,
        max_daa_lag,
        required_healthy,
    );
    if !summary.gate_healthy {
        return Err(EngineError::Message(format!(
            "owned-node redundancy gate failed: {} healthy, {} required",
            summary.healthy_nodes, required_healthy
        )));
    }
    Ok(summary)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GateJson<'a> {
    healthy: bool,
    healthy_nodes: usize,
    required_healthy_nodes: usize,
    selected_url: Option<&'a str>,
    public_daa: u64,
    max_daa_lag: u64,
    nodes: Vec<serde_json::Value>,
}

pub fn gate_summary_to_json(summary: &OwnedNodeGateSummary) -> serde_json::Value {
    let nodes = summary
        .reports
        .iter()
        .map(|report| report_to_json(report, summary.public_daa))
        .collect();
    serde_json::to_value(GateJson {
        healthy: summary.gate_healthy,
        healthy_nodes: summary.healthy_nodes,
        required_healthy_nodes: summary.required_healthy,
        selected_url: summary.selected_url.as_deref(),
        public_daa: summary.public_daa,
        max_daa_lag: summary.max_daa_lag,
        nodes,
    })
    .expect("gate summary serializes")
}

fn report_to_json(report: &OwnedNodeGateReport, public_daa: u64) -> serde_json::Value {
    match (&report.health, &report.assessment) {
        (Some(health), Some(assessment)) => serde_json::json!({
            "url": report.url,
            "healthy": report.healthy,
            "stage": report.stage,
            "stageLabel": report.stage_label,
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
            "blockCount": health.dag.block_count,
            "headerCount": health.dag.header_count,
            "headerBodyGap": assessment.header_body_gap,
            "connectedPeers": assessment.connected_peers,
            "ibdPeers": assessment.ibd_peers,
            "error": report.error,
        }),
        (Some(health), None) => serde_json::json!({
            "url": report.url,
            "healthy": false,
            "stage": report.stage,
            "stageLabel": report.stage_label,
            "error": report.error,
            "network": health.server.network_id,
            "serverVersion": health.server.server_version,
            "isSynced": health.server.is_synced,
            "hasUtxoIndex": health.server.has_utxo_index,
            "localDaa": health.server.virtual_daa_score.max(health.dag.virtual_daa_score),
            "publicDaa": public_daa,
            "blockCount": health.dag.block_count,
            "headerCount": health.dag.header_count,
            "headerBodyGap": health.dag.header_count.saturating_sub(health.dag.block_count),
            "connectedPeers": health.connected_peers,
            "ibdPeers": health.ibd_peers,
        }),
        _ => serde_json::json!({
            "url": report.url,
            "healthy": false,
            "stage": report.stage,
            "stageLabel": report.stage_label,
            "error": report.error,
        }),
    }
}

/// Returns true when `argument` was a recognized gate flag and consumed.
pub fn try_parse_gate_flag(
    options: &mut OwnedNodeGateOptions,
    dual: &mut bool,
    argument: &str,
    args: &mut impl Iterator<Item = String>,
) -> GateParseResult<bool> {
    match argument {
        "--url" => {
            options
                .urls
                .push(args.next().ok_or("--url needs a value")?);
            Ok(true)
        }
        "--dual" => {
            *dual = true;
            Ok(true)
        }
        "--min-healthy" | "--require-healthy" => {
            options.min_healthy = args
                .next()
                .ok_or("--min-healthy needs a value")?
                .parse()
                .map_err(|_| "--min-healthy must be an integer")?;
            Ok(true)
        }
        "--max-daa-lag" => {
            options.max_daa_lag = args
                .next()
                .ok_or("--max-daa-lag needs a value")?
                .parse()
                .map_err(|_| "--max-daa-lag must be an integer")?;
            if !(1..=100_000).contains(&options.max_daa_lag) {
                return Err("--max-daa-lag must be 1-100000".into());
            }
            Ok(true)
        }
        _ => Ok(false),
    }
}

pub fn print_gate_preflight(summary: &OwnedNodeGateSummary) {
    println!(
        "owned gate  {}/{} healthy",
        summary.healthy_nodes,
        summary.required_healthy
    );
    if let Some(url) = &summary.selected_url {
        println!("selected    {}", url);
    }
}

pub fn finish_gate_options(
    mut options: OwnedNodeGateOptions,
    dual: bool,
) -> GateParseResult<OwnedNodeGateOptions> {
    apply_gate_env_defaults(&mut options);
    if dual && !options.urls.is_empty() {
        options.urls.clear();
    }
    let urls = resolve_gate_urls(&options.urls, dual)?;
    if options.min_healthy > 0 {
        validate_gate_options(&urls, options.min_healthy)?;
    } else {
        validate_owned_node_urls(&urls).map_err(|error| error.to_string())?;
    }
    Ok(OwnedNodeGateOptions {
        urls,
        max_daa_lag: options.max_daa_lag,
        min_healthy: options.min_healthy,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn healthy() -> OwnedNodeHealth {
        OwnedNodeHealth {
            server: crate::wrpc::WrpcServerInfo {
                has_utxo_index: true,
                is_synced: true,
                network_id: "testnet-10".into(),
                rpc_api_revision: 0,
                rpc_api_version: 1,
                server_version: "2.0.1".into(),
                virtual_daa_score: 1_000,
            },
            dag: crate::wrpc::WrpcBlockDagInfo {
                network: "testnet-10".into(),
                virtual_daa_score: 1_001,
                block_count: 1_000,
                header_count: 1_000,
            },
            connected_peers: 3,
            ibd_peers: 0,
        }
    }

    #[test]
    fn report_from_probe_marks_healthy_and_unhealthy() {
        let ok = report_from_probe("ws://127.0.0.1:18210", Ok(healthy()), 1_010, 100);
        assert!(ok.healthy);
        assert_eq!(ok.stage, "healthy");
        let mut bad = healthy();
        bad.server.is_synced = false;
        let err = report_from_probe("ws://127.0.0.1:18210", Ok(bad), 1_010, 100);
        assert!(!err.healthy);
        assert!(err.error.is_some());
        let down = report_from_probe(
            "ws://127.0.0.1:28210",
            Err(EngineError::Message("connect failed".into())),
            1_010,
            100,
        );
        assert_eq!(down.stage, STAGE_UNREACHABLE);
    }

    #[test]
    fn dual_and_explicit_urls_conflict() {
        assert!(resolve_gate_urls(&["ws://127.0.0.1:18210".into()], true).is_err());
        assert_eq!(resolve_gate_urls(&[], true).unwrap().len(), 2);
    }

    #[test]
    fn dual_overrides_env_urls() {
        std::env::set_var(
            "TN10_OWNED_NODE_URLS",
            "ws://127.0.0.1:18210,ws://127.0.0.1:28210",
        );
        let finished = finish_gate_options(OwnedNodeGateOptions::default(), true).unwrap();
        assert_eq!(finished.urls.len(), 2);
        assert_eq!(finished.urls[0], loopback_wrpc_url(TN10_WRPC_JSON));
        std::env::remove_var("TN10_OWNED_NODE_URLS");
    }

    #[test]
    fn env_defaults_fill_urls_and_min_healthy() {
        std::env::set_var(
            "TN10_OWNED_NODE_URLS",
            "ws://127.0.0.1:18210,ws://127.0.0.1:28210",
        );
        std::env::set_var("TN10_MIN_HEALTHY", "2");
        let finished = finish_gate_options(OwnedNodeGateOptions::default(), false).unwrap();
        assert_eq!(finished.urls.len(), 2);
        assert_eq!(finished.min_healthy, 2);
        std::env::remove_var("TN10_OWNED_NODE_URLS");
        std::env::remove_var("TN10_MIN_HEALTHY");
    }
}
