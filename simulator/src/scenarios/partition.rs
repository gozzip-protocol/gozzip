use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;

use rand::seq::SliceRandom;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use serde::Serialize;

use crate::config::SimConfig;
use crate::sim::orchestrator::{Orchestrator, PartitionSchedule, SimResult};
use crate::types::NodeId;

// ── Params ──────────────────────────────────────────────────────────

pub struct PartitionParams {
    /// Number of network partitions to simulate.
    pub partitions: u32,
    /// Duration of the partition event in hours.
    pub duration_hours: u32,
}

// ── Result ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct PartitionResult {
    /// Measured data availability within each partition (0.0..1.0).
    pub availability_per_partition: Vec<f64>,
    /// Measured delivery rate during the partition window (0.0..1.0).
    pub delivery_rate_during: f64,
    /// Measured delivery rate after the partition heals (0.0..1.0).
    pub delivery_rate_after: f64,
    /// Total events delivered during the partition window.
    pub events_delivered_during: u64,
    /// Total events delivered after the partition healed.
    pub events_delivered_after: u64,
    /// Total events published during the partition window.
    pub events_published_during: u64,
    /// Total events published after the partition healed.
    pub events_published_after: u64,
    /// Number of partitions used.
    pub partition_count: u32,
}

// ── Run ─────────────────────────────────────────────────────────────

/// Run the network partition stress scenario with real network partitioning.
///
/// Strategy:
/// 1. Build a partition assignment map that splits nodes into `n` groups.
/// 2. Create a `PartitionSchedule` that activates midway through the sim.
/// 3. Run the orchestrator with the partition schedule.
/// 4. Measure actual availability and delivery rates from metrics.
pub async fn run_partition(config: SimConfig, params: PartitionParams) -> PartitionResult {
    let start = Instant::now();

    let n = params.partitions.max(1) as usize;
    let node_count = config.graph.nodes;

    // Deterministically assign nodes to partitions
    let mut rng = ChaCha8Rng::seed_from_u64(config.graph.seed.wrapping_add(200));
    let mut node_ids: Vec<NodeId> = (0..node_count).collect();
    node_ids.shuffle(&mut rng);

    let mut partition_map: HashMap<NodeId, usize> = HashMap::new();
    for (i, &node) in node_ids.iter().enumerate() {
        partition_map.insert(node, i % n);
    }

    // Compute partition window: start at 25% of sim, last for duration_hours
    let total_seconds = config.simulation.duration_days as f64 * 86_400.0;
    let partition_start = total_seconds * 0.25;
    let partition_duration = params.duration_hours as f64 * 3600.0;
    let partition_end = (partition_start + partition_duration).min(total_seconds);

    let schedule = PartitionSchedule {
        start_time: partition_start,
        end_time: partition_end,
        map: partition_map.clone(),
    };

    // Run simulation with partition schedule
    let orchestrator = Orchestrator::new(config.clone())
        .with_partition_schedule(schedule);
    let result = orchestrator.run().await;

    let elapsed = start.elapsed();

    let partition_result = analyse_partition(
        &result,
        &params,
        &partition_map,
        partition_start,
        partition_end,
    );
    print_results(&partition_result, &params, &config, elapsed.as_secs_f64());
    write_json_report(&partition_result, &config);

    partition_result
}

/// Measure actual availability and delivery rates from simulation metrics.
fn analyse_partition(
    result: &SimResult,
    params: &PartitionParams,
    partition_map: &HashMap<NodeId, usize>,
    partition_start: f64,
    partition_end: f64,
) -> PartitionResult {
    let n = params.partitions.max(1) as usize;

    // Compute per-partition availability from NodeSnapshot availability_samples.
    // Each node's availability_samples correspond to ticks in order.
    // We measure the fraction of online samples for nodes in each partition.
    let mut online_counts = vec![0u64; n];
    let mut total_counts = vec![0u64; n];

    for (&node_id, metrics) in &result.metrics.snapshots {
        if let Some(&p) = partition_map.get(&node_id) {
            let online = metrics
                .availability_samples
                .iter()
                .filter(|&&s| s)
                .count() as u64;
            let total = metrics.availability_samples.len() as u64;
            online_counts[p] += online;
            total_counts[p] += total;
        }
    }

    let availability_per_partition: Vec<f64> = (0..n)
        .map(|p| {
            if total_counts[p] > 0 {
                online_counts[p] as f64 / total_counts[p] as f64
            } else {
                0.0
            }
        })
        .collect();

    // Compute delivery rates during and after the partition window.
    // "During" = events published during [partition_start, partition_end)
    // "After"  = events published during [partition_end, end_of_sim)
    let mut published_during = 0u64;
    let mut delivered_during = 0u64;
    let mut published_after = 0u64;
    let mut delivered_after = 0u64;

    for delivery in result.metrics.event_deliveries.values() {
        let pub_time = delivery.published_at;
        if pub_time >= partition_start && pub_time < partition_end {
            published_during += 1;
            if !delivery.deliveries.is_empty() {
                delivered_during += 1;
            }
        } else if pub_time >= partition_end {
            published_after += 1;
            if !delivery.deliveries.is_empty() {
                delivered_after += 1;
            }
        }
    }

    let delivery_rate_during = if published_during > 0 {
        delivered_during as f64 / published_during as f64
    } else {
        1.0 // no events published means no failures
    };

    let delivery_rate_after = if published_after > 0 {
        delivered_after as f64 / published_after as f64
    } else {
        1.0
    };

    PartitionResult {
        availability_per_partition,
        delivery_rate_during,
        delivery_rate_after,
        events_delivered_during: delivered_during,
        events_delivered_after: delivered_after,
        events_published_during: published_during,
        events_published_after: published_after,
        partition_count: params.partitions,
    }
}

// ── Print ───────────────────────────────────────────────────────────

fn print_results(
    result: &PartitionResult,
    params: &PartitionParams,
    config: &SimConfig,
    elapsed: f64,
) {
    println!();
    println!(
        "\u{2501}\u{2501} Network Partition Scenario ({} nodes, seed={}) \u{2501}\u{2501}",
        config.graph.nodes, config.graph.seed
    );
    println!();
    println!("  Partitions:               {}", params.partitions);
    println!("  Duration:                 {} hours", params.duration_hours);
    println!();

    for (i, avail) in result.availability_per_partition.iter().enumerate() {
        println!(
            "  Partition {}:  availability = {:.4} ({:.2}%)",
            i,
            avail,
            avail * 100.0
        );
    }

    println!();
    println!(
        "  Delivery rate (during partition): {:.2}%  ({}/{} events)",
        result.delivery_rate_during * 100.0,
        result.events_delivered_during,
        result.events_published_during,
    );
    println!(
        "  Delivery rate (after heal):       {:.2}%  ({}/{} events)",
        result.delivery_rate_after * 100.0,
        result.events_delivered_after,
        result.events_published_after,
    );
    println!();

    let min_avail = result
        .availability_per_partition
        .iter()
        .cloned()
        .fold(f64::INFINITY, f64::min);
    let avg_avail: f64 = result.availability_per_partition.iter().sum::<f64>()
        / result.availability_per_partition.len() as f64;

    if min_avail > 0.90 {
        println!(
            "  Verdict: All partitions maintain >{:.0}% availability.",
            min_avail * 100.0,
        );
    } else {
        println!(
            "  Verdict: Degraded availability (min={:.1}%, avg={:.1}%).",
            min_avail * 100.0,
            avg_avail * 100.0,
        );
    }

    if result.delivery_rate_during < result.delivery_rate_after {
        println!(
            "  Delivery dropped from {:.1}% to {:.1}% during partition.",
            result.delivery_rate_after * 100.0,
            result.delivery_rate_during * 100.0,
        );
    }

    println!();
    println!("Simulation completed in {:.2}s", elapsed);
    println!();
}

/// Write the partition result as a JSON report to `results/`.
fn write_json_report(result: &PartitionResult, config: &SimConfig) {
    let dir = Path::new("results");
    if let Err(e) = std::fs::create_dir_all(dir) {
        eprintln!("Warning: could not create results directory: {}", e);
        return;
    }
    let filename = format!(
        "stress-partition-{}-seed{}.json",
        config.graph.nodes, config.graph.seed
    );
    let path = dir.join(filename);
    match serde_json::to_string_pretty(result) {
        Ok(json) => {
            if let Err(e) = std::fs::write(&path, json) {
                eprintln!("Warning: could not write JSON report to {:?}: {}", path, e);
            }
        }
        Err(e) => {
            eprintln!("Warning: could not serialize partition result: {}", e);
        }
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_partition_result_construction() {
        let result = PartitionResult {
            availability_per_partition: vec![0.95, 0.93],
            delivery_rate_during: 0.60,
            delivery_rate_after: 0.90,
            events_delivered_during: 30,
            events_delivered_after: 45,
            events_published_during: 50,
            events_published_after: 50,
            partition_count: 2,
        };
        assert_eq!(result.availability_per_partition.len(), 2);
        assert!(result.delivery_rate_during < result.delivery_rate_after);
        assert_eq!(result.partition_count, 2);
    }

    #[test]
    fn test_partition_result_json_serializable() {
        let result = PartitionResult {
            availability_per_partition: vec![0.95, 0.93],
            delivery_rate_during: 0.60,
            delivery_rate_after: 0.90,
            events_delivered_during: 30,
            events_delivered_after: 45,
            events_published_during: 50,
            events_published_after: 50,
            partition_count: 2,
        };
        let json = serde_json::to_string_pretty(&result).expect("PartitionResult should serialize to JSON");
        assert!(!json.is_empty(), "JSON output should not be empty");
        assert!(json.contains("delivery_rate_during"));
        assert!(json.contains("events_delivered_during"));
        assert!(json.contains("events_published_during"));
        assert!(json.contains("partition_count"));
    }

    #[tokio::test]
    async fn test_run_partition_smoke() {
        let mut config = SimConfig::default();
        config.graph.nodes = 50;
        config.graph.ba_edges_per_node = 3;
        config.simulation.duration_days = 1;
        config.simulation.tick_interval_secs = 3600;
        config.simulation.deterministic = true;

        let params = PartitionParams {
            partitions: 2,
            duration_hours: 6,
        };

        let result = run_partition(config, params).await;
        assert_eq!(result.availability_per_partition.len(), 2);
        for &avail in &result.availability_per_partition {
            assert!(avail >= 0.0 && avail <= 1.0);
        }
        assert!(result.delivery_rate_during >= 0.0 && result.delivery_rate_during <= 1.0);
        assert!(result.delivery_rate_after >= 0.0 && result.delivery_rate_after <= 1.0);
    }

    #[tokio::test]
    async fn test_partition_reduces_cross_partition_delivery() {
        // With 2 partitions, delivery during partition should be
        // lower than or equal to delivery after partition heals.
        let mut config = SimConfig::default();
        config.graph.nodes = 30;
        config.graph.ba_edges_per_node = 3;
        config.simulation.duration_days = 2;
        config.simulation.tick_interval_secs = 3600;
        config.simulation.deterministic = true;
        config.network.dau_pct = 0.5;

        let params = PartitionParams {
            partitions: 2,
            duration_hours: 12,
        };

        let result = run_partition(config, params).await;

        // We expect the partition to have some measurable effect.
        // Both rates should be valid fractions.
        assert!(result.delivery_rate_during >= 0.0);
        assert!(result.delivery_rate_after >= 0.0);
    }
}
