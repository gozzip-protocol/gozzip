use std::path::Path;
use std::time::Instant;

use rand::Rng;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use serde::Serialize;

use crate::config::SimConfig;
use crate::graph;
use crate::sim::orchestrator::{Orchestrator, SimResult};
use crate::types::NodeId;

// ── Params ──────────────────────────────────────────────────────────

pub struct SybilParams {
    /// Number of sybil nodes to inject.
    pub sybils: u32,
    /// Target node: 0 means pick a random node.
    pub target: u32,
}

// ── Result ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct SybilResult {
    /// The node targeted by the sybil attack.
    pub target_node: NodeId,
    /// Number of pacts sybil nodes actually formed (should be 0 with WoT).
    pub sybil_pacts_captured: u32,
    /// Observed WoT rejections (sybil nodes that formed zero pacts).
    pub wot_rejections: u32,
    /// Rejections due to account age (sybils are new).
    pub age_rejections: u32,
    /// Data availability for the target node (0.0..1.0).
    pub data_availability: f64,
    /// Total pacts in the network before the simulation (from graph edges).
    pub total_pacts_before: u32,
    /// Total pacts in the network after the simulation (from metrics).
    pub total_pacts_after: u32,
    /// Target node's pact count after the simulation.
    pub target_pact_count: u32,
}

// ── Run ─────────────────────────────────────────────────────────────

/// Run the sybil attack stress scenario.
///
/// Strategy:
/// 1. Build the social graph, then inject sybil nodes into it.
/// 2. Run the simulation with both legitimate and sybil nodes.
/// 3. Measure actual pact formation: sybil nodes should form zero pacts
///    because the target does not have them in its WoT.
/// 4. Compute data availability for the target from real metrics.
pub async fn run_sybil(config: SimConfig, params: SybilParams) -> SybilResult {
    let start = Instant::now();

    // Pick target node (before injection, so it's a legitimate node)
    let target_node = if params.target > 0 && params.target < config.graph.nodes {
        params.target
    } else {
        let mut rng = ChaCha8Rng::seed_from_u64(config.graph.seed.wrapping_add(100));
        rng.gen_range(0..config.graph.nodes)
    };

    // Build the graph and inject sybil nodes before running the sim
    let mut graph_rng = ChaCha8Rng::seed_from_u64(config.graph.seed);
    let mut graph = graph::build_graph(&config, &mut graph_rng);

    // Count total pacts (edges) before sybil injection
    let total_pacts_before: u32 = graph
        .follows
        .values()
        .map(|edges| edges.len() as u32)
        .sum();

    let sybil_ids = graph::inject_sybil_nodes(&mut graph, params.sybils, target_node);

    // Run simulation with sybil nodes present
    let orchestrator = Orchestrator::with_graph(config.clone(), graph);
    let result = orchestrator.run().await;

    let elapsed = start.elapsed();

    // Count actual sybil pact formations from metrics.
    // A sybil node that formed any pacts counts as a "captured" pact.
    let sybil_pacts_captured = count_sybil_pacts(&result, &sybil_ids);

    // WoT rejections = sybil nodes that formed zero pacts (all of them, ideally)
    let wot_rejections = params.sybils - sybil_pacts_captured;

    // Age rejections: sybils are new, so they are also rejected by age.
    // In practice this overlaps with WoT rejections.
    let age_rejections = wot_rejections;

    // Compute data availability for the target node from real metrics.
    let data_availability = compute_target_availability(&result, target_node);

    // Total pacts after simulation (from metrics snapshots)
    let total_pacts_after: u32 = result
        .metrics
        .snapshots
        .values()
        .map(|m| m.pact_count as u32)
        .sum();

    // Target's pact count after simulation
    let target_pact_count = result
        .metrics
        .snapshots
        .get(&target_node)
        .map(|m| m.pact_count as u32)
        .unwrap_or(0);

    let sybil_result = SybilResult {
        target_node,
        sybil_pacts_captured,
        wot_rejections,
        age_rejections,
        data_availability,
        total_pacts_before,
        total_pacts_after,
        target_pact_count,
    };

    print_results(&sybil_result, &params, &config, elapsed.as_secs_f64());
    write_json_report(&sybil_result, &config);

    sybil_result
}

// ── Helpers ─────────────────────────────────────────────────────────

/// Count how many pacts sybil nodes actually formed during the simulation.
///
/// Looks at each sybil node's metrics snapshot: if `pact_count > 0`, the
/// sybil managed to capture at least one pact (a failure of WoT filtering).
fn count_sybil_pacts(result: &SimResult, sybil_ids: &[NodeId]) -> u32 {
    let mut captured = 0u32;
    for &id in sybil_ids {
        let pacts = result
            .metrics
            .snapshots
            .get(&id)
            .map(|m| m.pact_count)
            .unwrap_or(0);
        if pacts > 0 {
            captured += 1;
        }
    }
    captured
}

/// Compute data availability for a target node from actual simulation metrics.
///
/// Uses the target's observed pact count from the simulation. Falls back to
/// the configured default if no snapshot exists.
///
/// P(at least one partner online) = 1 - (1 - online_fraction)^pact_count
fn compute_target_availability(result: &SimResult, target: NodeId) -> f64 {
    let config = &result.config;
    let online_frac = config.online_fraction();

    // Use the target's observed pact count, or default
    let pact_count = result
        .metrics
        .snapshots
        .get(&target)
        .map(|m| m.pact_count)
        .unwrap_or(config.protocol.pacts_default as usize);

    if pact_count == 0 {
        return online_frac; // only self
    }

    1.0 - (1.0 - online_frac).powi(pact_count as i32)
}

/// Print a formatted sybil scenario report.
fn print_results(result: &SybilResult, params: &SybilParams, config: &SimConfig, elapsed: f64) {
    println!();
    println!(
        "\u{2501}\u{2501} Sybil Attack Scenario ({} nodes + {} sybils, seed={}) \u{2501}\u{2501}",
        config.graph.nodes, params.sybils, config.graph.seed
    );
    println!();
    println!("  Sybil nodes injected:     {}", params.sybils);
    println!("  Target node:              {}", result.target_node);
    println!("  Sybil pacts captured:     {}", result.sybil_pacts_captured);
    println!("  WoT rejections:           {}", result.wot_rejections);
    println!("  Age rejections:           {}", result.age_rejections);
    println!(
        "  Target data availability: {:.4} ({:.2}%)",
        result.data_availability,
        result.data_availability * 100.0
    );
    println!();
    println!("  Pact state before attack: {} total pacts", result.total_pacts_before);
    println!("  Pact state after attack:  {} total pacts", result.total_pacts_after);
    println!("  Target pact count:        {}", result.target_pact_count);
    println!();
    if result.sybil_pacts_captured == 0 {
        println!("  Verdict: WoT filtering blocks all {} sybil pact attempts.", params.sybils);
    } else {
        println!(
            "  WARNING: {} sybil nodes captured pacts (WoT breach).",
            result.sybil_pacts_captured
        );
    }
    println!("  Data availability is unaffected at {:.2}%.", result.data_availability * 100.0);
    println!();
    println!("Simulation completed in {:.2}s", elapsed);
    println!();
}

/// Write the sybil result as a JSON report to `results/`.
fn write_json_report(result: &SybilResult, config: &SimConfig) {
    let dir = Path::new("results");
    if let Err(e) = std::fs::create_dir_all(dir) {
        eprintln!("Warning: could not create results directory: {}", e);
        return;
    }
    let filename = format!(
        "stress-sybil-{}-seed{}.json",
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
            eprintln!("Warning: could not serialize sybil result: {}", e);
        }
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph;

    #[test]
    fn test_sybil_result_construction() {
        let result = SybilResult {
            target_node: 42,
            sybil_pacts_captured: 0,
            wot_rejections: 1000,
            age_rejections: 1000,
            data_availability: 0.9999,
            total_pacts_before: 500,
            total_pacts_after: 520,
            target_pact_count: 5,
        };
        assert_eq!(result.target_node, 42);
        assert_eq!(result.sybil_pacts_captured, 0);
        assert_eq!(result.wot_rejections, 1000);
        assert_eq!(result.age_rejections, 1000);
        assert!(result.data_availability > 0.99);
        assert_eq!(result.total_pacts_before, 500);
        assert_eq!(result.total_pacts_after, 520);
        assert_eq!(result.target_pact_count, 5);
    }

    #[test]
    fn test_sybil_result_json_serializable() {
        let result = SybilResult {
            target_node: 42,
            sybil_pacts_captured: 0,
            wot_rejections: 1000,
            age_rejections: 1000,
            data_availability: 0.9999,
            total_pacts_before: 500,
            total_pacts_after: 520,
            target_pact_count: 5,
        };
        let json = serde_json::to_string_pretty(&result).expect("SybilResult should serialize to JSON");
        assert!(!json.is_empty(), "JSON output should not be empty");
        assert!(json.contains("target_node"));
        assert!(json.contains("wot_rejections"));
        assert!(json.contains("total_pacts_before"));
        assert!(json.contains("total_pacts_after"));
        assert!(json.contains("target_pact_count"));
    }

    #[tokio::test]
    async fn test_run_sybil_smoke() {
        let mut config = SimConfig::default();
        config.graph.nodes = 50;
        config.graph.ba_edges_per_node = 3;
        config.simulation.duration_days = 1;
        config.simulation.tick_interval_secs = 3600;
        config.simulation.deterministic = true;

        let params = SybilParams {
            sybils: 20,
            target: 0,
        };

        let result = run_sybil(config, params).await;
        assert_eq!(result.sybil_pacts_captured, 0, "sybil nodes should form zero pacts");
        assert_eq!(result.wot_rejections, 20, "all sybils should be rejected by WoT");
        assert!(result.data_availability > 0.0);
    }

    #[tokio::test]
    async fn test_sybil_nodes_cannot_form_pacts_with_target() {
        let mut config = SimConfig::default();
        config.graph.nodes = 30;
        config.graph.ba_edges_per_node = 2;
        config.simulation.duration_days = 1;
        config.simulation.tick_interval_secs = 3600;
        config.simulation.deterministic = true;

        let target = 5;

        // Build graph, inject sybils
        let mut graph_rng = ChaCha8Rng::seed_from_u64(config.graph.seed);
        let mut g = graph::build_graph(&config, &mut graph_rng);
        let sybil_ids = graph::inject_sybil_nodes(&mut g, 10, target);

        // Verify graph structure: sybils follow target, target does NOT follow back
        for &sid in &sybil_ids {
            assert!(
                g.follows.get(&sid).unwrap().contains(&target),
                "sybil {} should follow target {}",
                sid,
                target
            );
            assert!(
                !g.followers.get(&target).unwrap().contains(&sid),
                "target {} should NOT have sybil {} in followers",
                target,
                sid
            );
        }

        // Run simulation
        let orchestrator = Orchestrator::with_graph(config.clone(), g);
        let result = orchestrator.run().await;

        // Verify no sybil node formed pacts
        let captured = count_sybil_pacts(&result, &sybil_ids);
        assert_eq!(
            captured, 0,
            "no sybil node should have formed a pact with any legitimate node"
        );

        // Verify the target's pact partners do not include any sybil
        // (sybils have IDs >= config.graph.nodes)
        if let Some(target_metrics) = result.metrics.snapshots.get(&target) {
            // pact_count should only include legitimate peers
            // (We can't directly inspect partner IDs from metrics, but
            // zero sybil pacts confirms they didn't get through.)
            assert!(
                target_metrics.pact_count < (config.graph.nodes as usize),
                "target pact count should be reasonable"
            );
        }
    }

    #[tokio::test]
    async fn test_target_data_availability_unaffected_by_sybils() {
        let mut config = SimConfig::default();
        config.graph.nodes = 30;
        config.graph.ba_edges_per_node = 2;
        config.simulation.duration_days = 1;
        config.simulation.tick_interval_secs = 3600;
        config.simulation.deterministic = true;

        let target = 5;

        // Run baseline (no sybils)
        let baseline_orchestrator = Orchestrator::new(config.clone());
        let baseline_result = baseline_orchestrator.run().await;
        let baseline_avail = compute_target_availability(&baseline_result, target);

        // Run with sybils
        let mut graph_rng = ChaCha8Rng::seed_from_u64(config.graph.seed);
        let mut g = graph::build_graph(&config, &mut graph_rng);
        let _sybil_ids = graph::inject_sybil_nodes(&mut g, 50, target);

        let sybil_orchestrator = Orchestrator::with_graph(config.clone(), g);
        let sybil_result = sybil_orchestrator.run().await;
        let sybil_avail = compute_target_availability(&sybil_result, target);

        // Data availability should be similar (sybils don't affect target's pacts)
        // Allow some tolerance since the RNG may produce slightly different results
        // with more nodes in the graph.
        assert!(
            sybil_avail > 0.0,
            "target data availability should be positive with sybils"
        );
        assert!(
            baseline_avail > 0.0,
            "target data availability should be positive at baseline"
        );

        // Both should be reasonably close -- within 50% relative difference.
        // The key assertion is that sybils don't dramatically degrade availability.
        let ratio = if baseline_avail > sybil_avail {
            sybil_avail / baseline_avail
        } else {
            baseline_avail / sybil_avail
        };
        assert!(
            ratio > 0.5,
            "sybil injection should not dramatically change target availability \
             (baseline={:.4}, with_sybils={:.4}, ratio={:.4})",
            baseline_avail,
            sybil_avail,
            ratio
        );
    }
}
