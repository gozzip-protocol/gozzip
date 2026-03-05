use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;

use rand::SeedableRng;
use serde::Serialize;

use crate::config::SimConfig;
use crate::graph;
use crate::sim::metrics::PactEventKind;
use crate::sim::orchestrator::{Orchestrator, SimResult};
use crate::types::{NodeId, SimTime};

// ── Params ──────────────────────────────────────────────────────────

pub struct EclipseParams {
    /// Number of sybil nodes targeting the victim.
    pub sybils: u32,
    /// Target node (0 = highest-degree node).
    pub target: u32,
    /// Percentage of legitimate nodes forced offline during the storm.
    pub churn_pct: f64,
    /// When the churn storm starts (fraction of sim duration, 0.0..1.0).
    pub churn_start_pct: f64,
    /// Duration of the churn storm in hours.
    pub churn_duration_hours: u32,
}

// ── Result ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct EclipseResult {
    /// The node targeted by the eclipse attack.
    pub target_node: NodeId,
    /// Number of sybil nodes that formed pacts (should be 0).
    pub sybil_pacts_captured: u32,
    /// Target's data availability during the churn storm.
    pub content_availability_during: f64,
    /// Target's data availability after the storm ends.
    pub content_availability_after: f64,
    /// Minimum pact count observed for target during the storm.
    pub min_pact_count_during_storm: u32,
    /// Percentage of target's pacts that survive the storm.
    pub pact_survival_pct: f64,
    /// Seconds until target recovers to pre-storm pact count.
    pub time_to_recovery_secs: f64,
    /// Number of standby-to-active promotions for the target.
    pub standby_promotions: u32,
    /// Total pacts formed network-wide.
    pub total_pacts_formed: u64,
    /// Total pacts dropped network-wide.
    pub total_pacts_dropped: u64,
}

// ── Run ─────────────────────────────────────────────────────────────

/// Run the eclipse attack + churn storm scenario.
///
/// This is "the scenario that matters most" per the protocol doc:
/// simultaneous sybil eclipse attempt + mass partner failures.
pub async fn run_eclipse(config: SimConfig, params: EclipseParams) -> EclipseResult {
    let start = Instant::now();

    // Build graph
    let mut graph_rng = rand_chacha::ChaCha8Rng::seed_from_u64(config.graph.seed);
    let mut g = graph::build_graph(&config, &mut graph_rng);

    // Pick target: 0 = highest-degree node
    let target_node = if params.target > 0 && params.target < config.graph.nodes {
        params.target
    } else {
        // Find highest-degree node
        (0..g.node_count)
            .max_by_key(|&id| g.degree(id))
            .unwrap_or(0)
    };

    // Inject eclipse sybils
    let sybil_ids = graph::inject_eclipse_sybil_nodes(&mut g, params.sybils, target_node);

    // Compute churn timing
    let total_secs = config.simulation.duration_days as f64 * 86_400.0;
    let storm_start = total_secs * params.churn_start_pct;
    let storm_end = storm_start + params.churn_duration_hours as f64 * 3600.0;

    // Build offline overrides
    let mut offline_overrides: HashMap<NodeId, Vec<(SimTime, SimTime)>> = HashMap::new();

    // Force a fraction of legitimate nodes offline during the storm
    let churn_count = (config.graph.nodes as f64 * params.churn_pct / 100.0) as u32;
    use rand::Rng;
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(config.graph.seed.wrapping_add(42));
    let mut churned: Vec<NodeId> = Vec::new();
    while churned.len() < churn_count as usize {
        let id = rng.gen_range(0..config.graph.nodes);
        if id != target_node && !churned.contains(&id) {
            churned.push(id);
        }
    }
    for &id in &churned {
        offline_overrides.entry(id).or_default().push((storm_start, storm_end));
    }

    // Force all sybil nodes offline simultaneously (they all fail at once)
    for &sid in &sybil_ids {
        offline_overrides.entry(sid).or_default().push((storm_start, storm_end));
    }

    // Run simulation
    let orchestrator = Orchestrator::with_graph(config.clone(), g)
        .with_offline_overrides(offline_overrides);
    let result = orchestrator.run().await;

    let elapsed = start.elapsed();

    // Analyse
    let eclipse_result = analyse_eclipse(
        &result, &config, target_node, &sybil_ids,
        storm_start, storm_end,
    );

    print_results(&eclipse_result, &params, &config, elapsed.as_secs_f64());
    write_json_report(&eclipse_result, &config);

    eclipse_result
}

// ── Analysis ────────────────────────────────────────────────────────

fn analyse_eclipse(
    result: &SimResult,
    config: &SimConfig,
    target: NodeId,
    sybil_ids: &[NodeId],
    storm_start: SimTime,
    storm_end: SimTime,
) -> EclipseResult {
    // Count sybil pacts with the TARGET (not sybil-to-sybil pacts).
    // A sybil→sybil pact is expected (they're in each other's WoT).
    // What matters is whether any sybil formed a pact with the target.
    let sybil_set: std::collections::HashSet<NodeId> = sybil_ids.iter().copied().collect();
    let sybil_pacts_captured = result.metrics.pact_events.iter()
        .filter(|e| e.kind == PactEventKind::Formed)
        .filter(|e| {
            (sybil_set.contains(&e.node) && e.partner == target)
                || (sybil_set.contains(&e.partner) && e.node == target)
        })
        .count() as u32;

    // Analyse target's pacts over time from pact events
    let target_pre_storm_pacts = count_target_pacts_at_time(
        &result.metrics.pact_events, target, storm_start,
    );
    let target_min_during = find_min_pact_count_during(
        &result.metrics.pact_events, target, storm_start, storm_end,
    );
    let _target_post_storm_pacts = count_target_pacts_at_time(
        &result.metrics.pact_events, target, storm_end,
    );

    // Pact survival percentage
    let pact_survival_pct = if target_pre_storm_pacts > 0 {
        (target_min_during as f64 / target_pre_storm_pacts as f64 * 100.0).clamp(0.0, 100.0)
    } else {
        100.0
    };

    // Content availability = 1 - P(all pacts offline)
    let online_frac = config.online_fraction();
    let content_availability_during = if target_min_during > 0 {
        1.0 - (1.0 - online_frac).powi(target_min_during as i32)
    } else {
        0.0
    };
    let final_pact_count = result.metrics.snapshots.get(&target)
        .map(|m| m.pact_count)
        .unwrap_or(0);
    let content_availability_after = if final_pact_count > 0 {
        1.0 - (1.0 - online_frac).powi(final_pact_count as i32)
    } else {
        0.0
    };

    // Time to recovery: when does target reach pre-storm pact count again?
    let time_to_recovery = compute_recovery_time(
        &result.metrics.pact_events, target, target_pre_storm_pacts, storm_end,
    );

    // Standby promotions for target
    let standby_promotions = count_target_standby_promotions(
        &result.metrics.pact_events, target, storm_start,
    );

    // Network-wide totals
    let total_pacts_formed = result.metrics.pact_events.iter()
        .filter(|e| e.kind == PactEventKind::Formed)
        .count() as u64;
    let total_pacts_dropped = result.metrics.pact_events.iter()
        .filter(|e| e.kind == PactEventKind::Dropped)
        .count() as u64;

    EclipseResult {
        target_node: target,
        sybil_pacts_captured,
        content_availability_during,
        content_availability_after,
        min_pact_count_during_storm: target_min_during,
        pact_survival_pct,
        time_to_recovery_secs: time_to_recovery,
        standby_promotions,
        total_pacts_formed,
        total_pacts_dropped,
    }
}

/// Count a node's pact count at a given time by replaying pact events.
fn count_target_pacts_at_time(
    events: &[crate::sim::metrics::PactEvent],
    target: NodeId,
    time: SimTime,
) -> u32 {
    let mut count: i32 = 0;
    for e in events {
        if e.time > time { break; }
        if e.node == target {
            match e.kind {
                PactEventKind::Formed => count += 1,
                PactEventKind::Dropped => count -= 1,
            }
        }
    }
    count.max(0) as u32
}

/// Find the minimum pact count for a node during a time window.
fn find_min_pact_count_during(
    events: &[crate::sim::metrics::PactEvent],
    target: NodeId,
    start: SimTime,
    end: SimTime,
) -> u32 {
    let mut count: i32 = 0;
    let mut min_during: i32 = i32::MAX;
    let mut entered_window = false;

    for e in events {
        if e.node == target {
            match e.kind {
                PactEventKind::Formed => count += 1,
                PactEventKind::Dropped => count -= 1,
            }
        }
        if e.time >= start && e.time < end {
            if !entered_window {
                entered_window = true;
                min_during = count;
            }
            if e.node == target {
                min_during = min_during.min(count);
            }
        }
        if e.time >= end { break; }
    }

    if !entered_window {
        // No events during the window — use the count at storm start
        return count.max(0) as u32;
    }

    min_during.max(0) as u32
}

/// Compute time until target recovers to pre-storm pact count after storm_end.
fn compute_recovery_time(
    events: &[crate::sim::metrics::PactEvent],
    target: NodeId,
    pre_storm_count: u32,
    storm_end: SimTime,
) -> f64 {
    let mut count: i32 = 0;
    for e in events {
        if e.node == target {
            match e.kind {
                PactEventKind::Formed => count += 1,
                PactEventKind::Dropped => count -= 1,
            }
        }
        if e.time >= storm_end && count >= pre_storm_count as i32 {
            return e.time - storm_end;
        }
    }
    // Never recovered
    f64::INFINITY
}

/// Count standby promotions for the target (formations after drops).
fn count_target_standby_promotions(
    events: &[crate::sim::metrics::PactEvent],
    target: NodeId,
    after_time: SimTime,
) -> u32 {
    let mut drops = 0u32;
    let mut promotions = 0u32;

    for e in events {
        if e.time < after_time { continue; }
        if e.node == target {
            match e.kind {
                PactEventKind::Dropped => drops += 1,
                PactEventKind::Formed => {
                    if drops > 0 {
                        promotions += 1;
                        drops -= 1;
                    }
                }
            }
        }
    }
    promotions
}

// ── Print ───────────────────────────────────────────────────────────

fn print_results(result: &EclipseResult, params: &EclipseParams, config: &SimConfig, elapsed: f64) {
    println!();
    println!(
        "\u{2501}\u{2501} Eclipse Attack + Churn Storm ({} nodes + {} sybils, seed={}) \u{2501}\u{2501}",
        config.graph.nodes, params.sybils, config.graph.seed
    );
    println!();
    println!("  Target node:              {} (highest-degree)", result.target_node);
    println!("  Sybil nodes:              {}", params.sybils);
    println!("  Churn storm:              {:.0}% of nodes offline for {} hours", params.churn_pct, params.churn_duration_hours);
    println!("  Storm start:              {:.0}% through simulation", params.churn_start_pct * 100.0);
    println!();
    println!("  Sybil pacts captured:     {} {}", result.sybil_pacts_captured,
        if result.sybil_pacts_captured == 0 { "(WoT blocks all)" } else { "WARNING" });
    println!("  Min pacts during storm:   {}", result.min_pact_count_during_storm);
    println!("  Pact survival:            {:.1}%", result.pact_survival_pct);
    println!("  Standby promotions:       {}", result.standby_promotions);
    println!();
    println!("  Availability during:      {:.4} ({:.2}%)", result.content_availability_during, result.content_availability_during * 100.0);
    println!("  Availability after:       {:.4} ({:.2}%)", result.content_availability_after, result.content_availability_after * 100.0);
    println!("  Time to recovery:         {:.0}s", result.time_to_recovery_secs);
    println!();
    println!("  Network pacts formed:     {}", result.total_pacts_formed);
    println!("  Network pacts dropped:    {}", result.total_pacts_dropped);
    println!();

    if result.sybil_pacts_captured == 0 && result.content_availability_after > 0.99 {
        println!("  Verdict: RESILIENT. WoT blocks eclipse, network recovers from churn.");
    } else if result.sybil_pacts_captured == 0 {
        println!("  Verdict: WoT holds, but availability degraded ({:.1}%).", result.content_availability_after * 100.0);
    } else {
        println!("  Verdict: VULNERABLE. {} sybils breached WoT.", result.sybil_pacts_captured);
    }

    println!();
    println!("Simulation completed in {:.2}s", elapsed);
    println!();
}

fn write_json_report(result: &EclipseResult, config: &SimConfig) {
    let dir = Path::new("results");
    if let Err(e) = std::fs::create_dir_all(dir) {
        eprintln!("Warning: could not create results directory: {}", e);
        return;
    }
    let filename = format!(
        "stress-eclipse-{}-seed{}.json",
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
            eprintln!("Warning: could not serialize eclipse result: {}", e);
        }
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_eclipse_result_json_serializable() {
        let result = EclipseResult {
            target_node: 0,
            sybil_pacts_captured: 0,
            content_availability_during: 0.95,
            content_availability_after: 0.999,
            min_pact_count_during_storm: 5,
            pact_survival_pct: 80.0,
            time_to_recovery_secs: 3600.0,
            standby_promotions: 3,
            total_pacts_formed: 1000,
            total_pacts_dropped: 200,
        };
        let json = serde_json::to_string_pretty(&result).expect("serialize");
        assert!(json.contains("sybil_pacts_captured"));
        assert!(json.contains("content_availability_during"));
    }

    #[tokio::test]
    async fn test_eclipse_smoke() {
        let mut config = SimConfig::default();
        config.graph.nodes = 50;
        config.graph.ba_edges_per_node = 3;
        config.simulation.duration_days = 1;
        config.simulation.tick_interval_secs = 3600;
        config.simulation.deterministic = true;

        let params = EclipseParams {
            sybils: 10,
            target: 0,
            churn_pct: 30.0,
            churn_start_pct: 0.5,
            churn_duration_hours: 6,
        };

        let result = run_eclipse(config, params).await;
        assert_eq!(result.sybil_pacts_captured, 0, "WoT should block all sybils");
    }
}
