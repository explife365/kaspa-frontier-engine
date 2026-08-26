//! GHOSTDAG telemetry from live node/explorer data.
//! DAGKnight is roadmap, not current consensus. Gemini's 100 BPS / AAA grades
//! and 51% USD costs were not measured.

use crate::network::TARGET_BPS;
use crate::rest::{BlockDagInfo, HashrateInfo};
use crate::roadmap::PROTOCOL_LABEL;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct GhostdagTelemetry {
    pub protocol: &'static str,
    pub network: String,
    pub virtual_daa_score: u64,
    pub difficulty: f64,
    pub block_count: u64,
    pub header_count: u64,
    pub target_bps: f64,
    /// `difficulty × BPS` — a local estimate. Prefer `rest_hashrate_ths`.
    pub estimated_hashrate_hs: f64,
    /// Explorer `/info/hashrate` in TH/s when fetched.
    pub rest_hashrate_ths: Option<f64>,
    pub sink: String,
}

impl GhostdagTelemetry {
    pub fn from_block_dag(info: &BlockDagInfo) -> Self {
        Self {
            protocol: PROTOCOL_LABEL,
            network: info.network_name.clone(),
            virtual_daa_score: info.virtual_daa_score,
            difficulty: info.difficulty,
            block_count: info.block_count,
            header_count: info.header_count,
            target_bps: TARGET_BPS,
            estimated_hashrate_hs: info.difficulty * TARGET_BPS,
            rest_hashrate_ths: None,
            sink: info.sink.clone(),
        }
    }

    pub fn with_rest_hashrate(mut self, hr: &HashrateInfo) -> Self {
        self.rest_hashrate_ths = Some(hr.hashrate);
        self
    }

    /// Observational rate anomaly only; DAA/s cannot identify the consensus protocol.
    pub fn high_daa_rate_anomaly(measured_daa_per_sec: f64) -> bool {
        measured_daa_per_sec >= 40.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rest::BlockDagInfo;

    #[test]
    fn labels_ghostdag_not_dagknight() {
        let info = BlockDagInfo {
            network_name: "kaspa-testnet-10".into(),
            block_count: 1,
            header_count: 1,
            tip_hashes: vec![],
            difficulty: 100.0,
            past_median_time: 0,
            virtual_parent_hashes: vec![],
            pruning_point_hash: "00".into(),
            virtual_daa_score: 9,
            sink: "aa".into(),
        };
        let t = GhostdagTelemetry::from_block_dag(&info);
        assert!(t.protocol.contains("GHOSTDAG"));
        assert!(!t.protocol.contains("activated DAGKnight"));
        assert_eq!(t.estimated_hashrate_hs, 1000.0);
        assert!(t.rest_hashrate_ths.is_none());
        assert!(!GhostdagTelemetry::high_daa_rate_anomaly(10.0));
        assert!(GhostdagTelemetry::high_daa_rate_anomaly(100.0));
    }
}
