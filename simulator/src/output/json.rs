use std::fs;
use std::path::Path;

use serde::Serialize;

use crate::config::SimConfig;
use crate::graph::Graph;
use crate::sim::metrics::{PactEventKind, Percentiles, ReadResultRecord};
use crate::types::{FormulaResult, NodeType, ReadTier};

// ── JsonReport ──────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct JsonReport {
    pub config: SimConfig,
    pub formulas: Vec<FormulaResult>,
    pub per_node: PerNodeSummary,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub sample_events: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub activity_weights: Vec<f64>,
}

#[derive(Serialize)]
pub struct PerNodeSummary {
    pub data_availability: Percentiles,
    pub bandwidth_mb_day_full: Percentiles,
    pub bandwidth_mb_day_light: Percentiles,
    pub gossip_received_per_node: Percentiles,
    pub pact_count: Percentiles,
    pub delivery_latency: Percentiles,
    pub pact_churn: PactChurnSummary,
    pub retrieval: RetrievalSummary,
}

#[derive(Debug, Clone, Serialize)]
pub struct PactChurnSummary {
    pub total_formed: u64,
    pub total_dropped: u64,
    pub net_pacts: i64,
    /// Drops per node per day.
    pub churn_rate: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct RetrievalSummary {
    pub total_attempts: u64,
    pub success_rate: f64,
    pub by_tier: RetrievalByTier,
    pub paths_tried_per_request: Percentiles,
}

#[derive(Debug, Clone, Serialize)]
pub struct RetrievalByTier {
    pub instant: TierSummary,
    pub cached_endpoint: TierSummary,
    pub gossip: TierSummary,
    pub relay: TierSummary,
    pub failed: TierSummary,
}

#[derive(Debug, Clone, Serialize)]
pub struct TierSummary {
    pub count: u64,
    pub pct: f64,
    pub latency_ms: Percentiles,
}

// ── write_report ────────────────────────────────────────────────────

/// Serialize a `JsonReport` to pretty JSON and write it to the given path.
///
/// Creates parent directories if they do not exist.
pub fn write_report(report: &JsonReport, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let json = serde_json::to_string_pretty(report)?;
    fs::write(path, json)?;

    Ok(())
}

// ── build_per_node_summary ──────────────────────────────────────────

/// Build a `PerNodeSummary` from collected per-node metrics.
///
/// Each field aggregates a distribution over all nodes.
pub fn build_per_node_summary(
    metrics: &crate::sim::metrics::CollectedMetrics,
    config: &SimConfig,
    graph: &Graph,
) -> PerNodeSummary {
    let duration_days = config.simulation.duration_days as f64;
    let bytes_to_mb_day = |bytes: u64| -> f64 { bytes as f64 / (1024.0 * 1024.0) / duration_days };

    let mut availability_vals = Vec::new();
    let mut bw_full_vals = Vec::new();
    let mut bw_light_vals = Vec::new();
    let mut gossip_vals = Vec::new();
    let mut pact_vals = Vec::new();

    for (id, node_metrics) in &metrics.snapshots {
        // Data availability: fraction of samples where node was online
        let avail = if node_metrics.availability_samples.is_empty() {
            0.0
        } else {
            let online = node_metrics
                .availability_samples
                .iter()
                .filter(|&&s| s)
                .count();
            online as f64 / node_metrics.availability_samples.len() as f64
        };
        availability_vals.push(avail);

        // Bandwidth: total (upload + download) in MB/day
        let total_bytes =
            node_metrics.bandwidth.upload_bytes + node_metrics.bandwidth.download_bytes;
        let mb_day = bytes_to_mb_day(total_bytes);

        // Use graph node_types for classification
        let is_full = graph
            .node_types
            .get(id)
            .map_or(false, |t| *t == NodeType::Full);
        if is_full {
            bw_full_vals.push(mb_day);
        } else {
            bw_light_vals.push(mb_day);
        }

        // Gossip received
        gossip_vals.push(node_metrics.gossip.received as f64);

        // Pact count
        pact_vals.push(node_metrics.pact_count as f64);
    }

    // Pact churn summary from pact_events
    let total_formed = metrics
        .pact_events
        .iter()
        .filter(|e| e.kind == PactEventKind::Formed)
        .count() as u64;
    let total_dropped = metrics
        .pact_events
        .iter()
        .filter(|e| e.kind == PactEventKind::Dropped)
        .count() as u64;
    let net_pacts = total_formed as i64 - total_dropped as i64;
    let node_count = metrics.snapshots.len().max(1) as f64;
    let churn_rate = total_dropped as f64 / node_count / duration_days;

    PerNodeSummary {
        data_availability: Percentiles::from_values(availability_vals),
        bandwidth_mb_day_full: Percentiles::from_values(bw_full_vals),
        bandwidth_mb_day_light: Percentiles::from_values(bw_light_vals),
        gossip_received_per_node: Percentiles::from_values(gossip_vals),
        pact_count: Percentiles::from_values(pact_vals),
        delivery_latency: Percentiles::from_values(metrics.delivery_latencies.clone()),
        pact_churn: PactChurnSummary {
            total_formed,
            total_dropped,
            net_pacts,
            churn_rate,
        },
        retrieval: build_retrieval_summary(&metrics.read_results),
    }
}

// ── build_retrieval_summary ─────────────────────────────────────────

pub fn build_retrieval_summary(read_results: &[ReadResultRecord]) -> RetrievalSummary {
    let total = read_results.len() as u64;
    if total == 0 {
        return RetrievalSummary {
            total_attempts: 0,
            success_rate: 0.0,
            by_tier: RetrievalByTier {
                instant: TierSummary { count: 0, pct: 0.0, latency_ms: Percentiles::from_values(vec![]) },
                cached_endpoint: TierSummary { count: 0, pct: 0.0, latency_ms: Percentiles::from_values(vec![]) },
                gossip: TierSummary { count: 0, pct: 0.0, latency_ms: Percentiles::from_values(vec![]) },
                relay: TierSummary { count: 0, pct: 0.0, latency_ms: Percentiles::from_values(vec![]) },
                failed: TierSummary { count: 0, pct: 0.0, latency_ms: Percentiles::from_values(vec![]) },
            },
            paths_tried_per_request: Percentiles::from_values(vec![]),
        };
    }

    let mut instant_lat = Vec::new();
    let mut cached_lat = Vec::new();
    let mut gossip_lat = Vec::new();
    let mut relay_lat = Vec::new();
    let mut failed_count = 0u64;
    let mut paths_tried = Vec::new();

    for r in read_results {
        let ms = r.latency_secs * 1000.0;
        paths_tried.push(r.paths_tried as f64);
        match r.tier {
            ReadTier::Instant => instant_lat.push(ms),
            ReadTier::CachedEndpoint => cached_lat.push(ms),
            ReadTier::Gossip => gossip_lat.push(ms),
            ReadTier::Relay => relay_lat.push(ms),
            ReadTier::Failed => failed_count += 1,
        }
    }

    let total_f = total as f64;
    let successes = total - failed_count;

    RetrievalSummary {
        total_attempts: total,
        success_rate: successes as f64 / total_f,
        by_tier: RetrievalByTier {
            instant: TierSummary {
                count: instant_lat.len() as u64,
                pct: instant_lat.len() as f64 / total_f,
                latency_ms: Percentiles::from_values(instant_lat),
            },
            cached_endpoint: TierSummary {
                count: cached_lat.len() as u64,
                pct: cached_lat.len() as f64 / total_f,
                latency_ms: Percentiles::from_values(cached_lat),
            },
            gossip: TierSummary {
                count: gossip_lat.len() as u64,
                pct: gossip_lat.len() as f64 / total_f,
                latency_ms: Percentiles::from_values(gossip_lat),
            },
            relay: TierSummary {
                count: relay_lat.len() as u64,
                pct: relay_lat.len() as f64 / total_f,
                latency_ms: Percentiles::from_values(relay_lat),
            },
            failed: TierSummary {
                count: failed_count,
                pct: failed_count as f64 / total_f,
                latency_ms: Percentiles::from_values(vec![]),
            },
        },
        paths_tried_per_request: Percentiles::from_values(paths_tried),
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::metrics::ReadResultRecord;
    use crate::types::{FormulaResult, ReadTier, WotTier};

    #[test]
    fn test_json_serialization() {
        let config = SimConfig::default();
        let formulas = vec![
            FormulaResult::new("F-01", "avg_event_size", 925.0, 925.0),
            FormulaResult::new("F-24", "online_fraction", 0.6875, 0.6875),
        ];

        let per_node = PerNodeSummary {
            data_availability: Percentiles::from_values(vec![0.9, 0.95, 0.99]),
            bandwidth_mb_day_full: Percentiles::from_values(vec![10.0, 20.0, 30.0]),
            bandwidth_mb_day_light: Percentiles::from_values(vec![1.0, 2.0, 3.0]),
            gossip_received_per_node: Percentiles::from_values(vec![100.0, 200.0, 300.0]),
            pact_count: Percentiles::from_values(vec![5.0, 10.0, 15.0]),
            delivery_latency: Percentiles::from_values(vec![0.1, 0.5, 1.0]),
            pact_churn: PactChurnSummary {
                total_formed: 10,
                total_dropped: 3,
                net_pacts: 7,
                churn_rate: 0.1,
            },
            retrieval: build_retrieval_summary(&[]),
        };

        let report = JsonReport {
            config,
            formulas,
            per_node,
            sample_events: Vec::new(),
            activity_weights: Vec::new(),
        };

        let json_str = serde_json::to_string_pretty(&report).expect("serialization should succeed");

        assert!(json_str.contains("F-01"), "JSON should contain formula ID F-01");
        assert!(json_str.contains("avg_event_size"), "JSON should contain formula name");
        assert!(json_str.contains("data_availability"), "JSON should contain per-node field");
        assert!(json_str.contains("bandwidth_mb_day_full"), "JSON should contain bandwidth field");
        assert!(json_str.contains("retrieval"), "JSON should contain retrieval field");
    }

    #[test]
    fn test_retrieval_summary_computes_correctly() {
        let records = vec![
            ReadResultRecord {
                reader: 1,
                target_author: 2,
                request_id: 100,
                tier: ReadTier::Instant,
                wot_tier: WotTier::Orbit,
                latency_secs: 0.001,
                paths_tried: 1,
                time: 10.0,
            },
            ReadResultRecord {
                reader: 1,
                target_author: 3,
                request_id: 101,
                tier: ReadTier::CachedEndpoint,
                wot_tier: WotTier::Orbit,
                latency_secs: 0.05,
                paths_tried: 1,
                time: 20.0,
            },
            ReadResultRecord {
                reader: 2,
                target_author: 4,
                request_id: 102,
                tier: ReadTier::Gossip,
                wot_tier: WotTier::Orbit,
                latency_secs: 0.5,
                paths_tried: 2,
                time: 30.0,
            },
            ReadResultRecord {
                reader: 3,
                target_author: 5,
                request_id: 103,
                tier: ReadTier::Relay,
                wot_tier: WotTier::Orbit,
                latency_secs: 2.0,
                paths_tried: 3,
                time: 40.0,
            },
            ReadResultRecord {
                reader: 4,
                target_author: 6,
                request_id: 104,
                tier: ReadTier::Failed,
                wot_tier: WotTier::Orbit,
                latency_secs: 10.0,
                paths_tried: 3,
                time: 50.0,
            },
        ];

        let summary = build_retrieval_summary(&records);

        // Total attempts
        assert_eq!(summary.total_attempts, 5);

        // Success rate: 4 out of 5 succeeded (1 failed)
        assert!((summary.success_rate - 0.8).abs() < f64::EPSILON,
            "success_rate should be 0.8, got {}", summary.success_rate);

        // Tier counts
        assert_eq!(summary.by_tier.instant.count, 1);
        assert_eq!(summary.by_tier.cached_endpoint.count, 1);
        assert_eq!(summary.by_tier.gossip.count, 1);
        assert_eq!(summary.by_tier.relay.count, 1);
        assert_eq!(summary.by_tier.failed.count, 1);

        // Tier percentages (each 1 out of 5 = 0.2)
        assert!((summary.by_tier.instant.pct - 0.2).abs() < f64::EPSILON);
        assert!((summary.by_tier.cached_endpoint.pct - 0.2).abs() < f64::EPSILON);
        assert!((summary.by_tier.gossip.pct - 0.2).abs() < f64::EPSILON);
        assert!((summary.by_tier.relay.pct - 0.2).abs() < f64::EPSILON);
        assert!((summary.by_tier.failed.pct - 0.2).abs() < f64::EPSILON);

        // Instant latency: 0.001s = 1.0ms
        assert!((summary.by_tier.instant.latency_ms.mean - 1.0).abs() < f64::EPSILON,
            "instant latency mean should be 1.0ms, got {}", summary.by_tier.instant.latency_ms.mean);

        // CachedEndpoint latency: 0.05s = 50.0ms
        assert!((summary.by_tier.cached_endpoint.latency_ms.mean - 50.0).abs() < f64::EPSILON,
            "cached_endpoint latency mean should be 50.0ms, got {}", summary.by_tier.cached_endpoint.latency_ms.mean);

        // Gossip latency: 0.5s = 500.0ms
        assert!((summary.by_tier.gossip.latency_ms.mean - 500.0).abs() < f64::EPSILON,
            "gossip latency mean should be 500.0ms, got {}", summary.by_tier.gossip.latency_ms.mean);

        // Relay latency: 2.0s = 2000.0ms
        assert!((summary.by_tier.relay.latency_ms.mean - 2000.0).abs() < f64::EPSILON,
            "relay latency mean should be 2000.0ms, got {}", summary.by_tier.relay.latency_ms.mean);

        // Failed tier should have empty latency (all zeros)
        assert!((summary.by_tier.failed.latency_ms.mean - 0.0).abs() < f64::EPSILON,
            "failed tier latency should be 0.0, got {}", summary.by_tier.failed.latency_ms.mean);

        // paths_tried_per_request: [1, 1, 2, 3, 3] -> mean = 2.0
        assert!((summary.paths_tried_per_request.mean - 2.0).abs() < f64::EPSILON,
            "paths_tried mean should be 2.0, got {}", summary.paths_tried_per_request.mean);
    }

    #[test]
    fn test_retrieval_summary_empty() {
        let summary = build_retrieval_summary(&[]);

        assert_eq!(summary.total_attempts, 0);
        assert!((summary.success_rate - 0.0).abs() < f64::EPSILON);
        assert_eq!(summary.by_tier.instant.count, 0);
        assert_eq!(summary.by_tier.cached_endpoint.count, 0);
        assert_eq!(summary.by_tier.gossip.count, 0);
        assert_eq!(summary.by_tier.relay.count, 0);
        assert_eq!(summary.by_tier.failed.count, 0);
    }
}
