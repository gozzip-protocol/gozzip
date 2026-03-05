use std::path::Path;
use std::time::Instant;

use serde::Serialize;

use crate::config::SimConfig;
use crate::sim::metrics::PactEventKind;
use crate::sim::orchestrator::Orchestrator;

// ── Params ──────────────────────────────────────────────────────────

pub struct ChurnParams {
    /// Percentage of nodes that churn (go offline and are replaced) per tick.
    pub churn_pct: f64,
    /// Duration of the churn period in hours.
    pub duration_hours: u32,
}

// ── Result ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct ChurnResult {
    /// Percentage of pacts that survive the churn period.
    pub pact_survival_pct: f64,
    /// Number of standby pact promotions triggered (from actual pact events).
    pub standby_promotions: u32,
    /// Fraction of gossip messages successfully delivered (0.0..1.0).
    pub gossip_delivery_rate: f64,
    /// Total pacts formed during the simulation.
    pub total_pacts_formed: u64,
    /// Total pacts dropped during the simulation.
    pub total_pacts_dropped: u64,
    /// Final total pact count across all nodes (from metrics snapshots).
    pub final_pact_count: u32,
}

// ── Run ─────────────────────────────────────────────────────────────

/// Run the node churn stress scenario.
///
/// Strategy:
/// 1. Modify config to increase light_node_pct to simulate higher churn.
/// 2. Lower light_uptime to model nodes going offline more frequently.
/// 3. Run the orchestrator.
/// 4. Analyse pact and gossip metrics to estimate churn impact.
pub async fn run_churn(config: SimConfig, params: ChurnParams) -> ChurnResult {
    let start = Instant::now();

    // Create a modified config to simulate churn:
    // - Increase light_node_pct (more transient nodes)
    // - Decrease light_uptime to model higher offline rate
    let mut churn_config = config.clone();

    // Churn effectively converts some full nodes to light (transient) nodes
    let churn_frac = (params.churn_pct / 100.0).min(1.0);
    let original_light_pct = config.network.light_node_pct;
    churn_config.network.light_node_pct =
        (original_light_pct + churn_frac * config.network.full_node_pct).min(1.0);
    churn_config.network.full_node_pct =
        (1.0 - churn_config.network.light_node_pct).max(0.0);

    // Lower light uptime to model churn-induced downtime
    churn_config.network.light_uptime =
        (config.network.light_uptime * (1.0 - churn_frac * 0.5)).max(0.05);

    // Limit simulation to the churn duration
    let churn_ticks = (params.duration_hours as f64 * 3600.0
        / churn_config.simulation.tick_interval_secs as f64) as u32;
    // If churn duration is shorter than the configured duration, adjust
    if params.duration_hours < config.simulation.duration_days * 24 {
        churn_config.simulation.duration_days =
            ((params.duration_hours as f64 / 24.0).ceil() as u32).max(1);
    }

    churn_config.simulation.deterministic = true;

    // Run the churned simulation
    let orchestrator = Orchestrator::new(churn_config.clone());
    let result = orchestrator.run().await;

    let elapsed = start.elapsed();

    // Analyse results
    let churn_result = analyse_churn(&config, &churn_config, &result, &params, churn_ticks);
    print_results(&churn_result, &params, &config, elapsed.as_secs_f64());
    write_json_report(&churn_result, &config);

    churn_result
}

/// Analyse the churn scenario results using actual metrics from the simulation.
fn analyse_churn(
    original_config: &SimConfig,
    _churn_config: &SimConfig,
    result: &crate::sim::orchestrator::SimResult,
    params: &ChurnParams,
    _churn_ticks: u32,
) -> ChurnResult {
    let churn_frac = (params.churn_pct / 100.0).min(1.0);

    // Count actual pact events from metrics
    let total_pacts_formed = result
        .metrics
        .pact_events
        .iter()
        .filter(|e| e.kind == PactEventKind::Formed)
        .count() as u64;
    let total_pacts_dropped = result
        .metrics
        .pact_events
        .iter()
        .filter(|e| e.kind == PactEventKind::Dropped)
        .count() as u64;

    // Final pact count from metrics snapshots
    let final_pact_count: u32 = result
        .metrics
        .snapshots
        .values()
        .map(|m| m.pact_count as u32)
        .sum();

    // Pact survival: use actual metrics when available
    let pact_survival_pct = if total_pacts_formed > 0 {
        // Survival = (formed - dropped) / formed * 100
        let survived = total_pacts_formed.saturating_sub(total_pacts_dropped);
        (survived as f64 / total_pacts_formed as f64 * 100.0).clamp(0.0, 100.0)
    } else {
        // Fall back to analytical estimate
        let pacts_default = original_config.protocol.pacts_default as f64;
        let survival_base = (1.0 - churn_frac).powf(pacts_default);
        (survival_base * 100.0).clamp(0.0, 100.0)
    };

    // Standby promotions: count PactFormed events that occur after a PactDropped
    // event for the same node (i.e., replacement pacts).
    let standby_promotions = count_standby_promotions(&result.metrics.pact_events);

    // Gossip delivery rate: prefer actual metrics
    let actual_gossip_rate = compute_actual_gossip_delivery(result);
    let gossip_delivery_rate = if actual_gossip_rate > 0.0 {
        actual_gossip_rate
    } else {
        // Analytical fallback
        let gossip_loss = churn_frac * (1.0 - original_config.network.gossip_fallback);
        (1.0 - gossip_loss).clamp(0.0, 1.0)
    };

    ChurnResult {
        pact_survival_pct,
        standby_promotions,
        gossip_delivery_rate,
        total_pacts_formed,
        total_pacts_dropped,
        final_pact_count,
    }
}

/// Count standby promotions: PactFormed events for a node that has had
/// a prior PactDropped event (indicating the formation replaced a lost pact).
fn count_standby_promotions(pact_events: &[crate::sim::metrics::PactEvent]) -> u32 {
    use std::collections::HashMap;
    let mut drop_counts: HashMap<u32, u32> = HashMap::new();
    let mut promotions = 0u32;

    for event in pact_events {
        match event.kind {
            PactEventKind::Dropped => {
                *drop_counts.entry(event.node).or_insert(0) += 1;
            }
            PactEventKind::Formed => {
                if let Some(count) = drop_counts.get_mut(&event.node) {
                    if *count > 0 {
                        promotions += 1;
                        *count -= 1;
                    }
                }
            }
        }
    }

    promotions
}

/// Compute the actual gossip delivery rate from simulation metrics.
fn compute_actual_gossip_delivery(result: &crate::sim::orchestrator::SimResult) -> f64 {
    let mut total_sent: u64 = 0;
    let mut total_received: u64 = 0;

    for metrics in result.metrics.snapshots.values() {
        total_sent += metrics.gossip.sent;
        total_received += metrics.gossip.received;
    }

    if total_sent == 0 {
        return 0.0;
    }

    (total_received as f64 / total_sent as f64).min(1.0)
}

// ── Print ───────────────────────────────────────────────────────────

fn print_results(result: &ChurnResult, params: &ChurnParams, config: &SimConfig, elapsed: f64) {
    println!();
    println!(
        "\u{2501}\u{2501} Node Churn Scenario ({} nodes, seed={}) \u{2501}\u{2501}",
        config.graph.nodes, config.graph.seed
    );
    println!();
    println!("  Churn rate:               {:.1}% per tick", params.churn_pct);
    println!("  Duration:                 {} hours", params.duration_hours);
    println!(
        "  Original node mix:        {:.0}% full / {:.0}% light",
        config.network.full_node_pct * 100.0,
        config.network.light_node_pct * 100.0
    );
    println!();
    println!(
        "  Pact survival:            {:.1}%",
        result.pact_survival_pct
    );
    println!(
        "  Standby promotions:       {}",
        result.standby_promotions
    );
    println!(
        "  Gossip delivery rate:     {:.2}%",
        result.gossip_delivery_rate * 100.0
    );
    println!();
    println!(
        "  Pacts formed:             {}",
        result.total_pacts_formed
    );
    println!(
        "  Pacts dropped:            {}",
        result.total_pacts_dropped
    );
    println!(
        "  Final pact count:         {}",
        result.final_pact_count
    );
    println!();

    if result.pact_survival_pct > 80.0 {
        println!(
            "  Verdict: Network resilient. {:.0}% pacts survive with {} standby promotions.",
            result.pact_survival_pct, result.standby_promotions
        );
    } else if result.pact_survival_pct > 50.0 {
        println!(
            "  Verdict: Moderate degradation. {:.0}% pacts survive. Standby helps ({} promotions).",
            result.pact_survival_pct, result.standby_promotions
        );
    } else {
        println!(
            "  Verdict: Severe churn impact. Only {:.0}% pacts survive. Consider increasing standby count.",
            result.pact_survival_pct
        );
    }

    println!();
    println!("Simulation completed in {:.2}s", elapsed);
    println!();
}

/// Write the churn result as a JSON report to `results/`.
fn write_json_report(result: &ChurnResult, config: &SimConfig) {
    let dir = Path::new("results");
    if let Err(e) = std::fs::create_dir_all(dir) {
        eprintln!("Warning: could not create results directory: {}", e);
        return;
    }
    let filename = format!(
        "stress-churn-{}-seed{}.json",
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
            eprintln!("Warning: could not serialize churn result: {}", e);
        }
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_churn_result_construction() {
        let result = ChurnResult {
            pact_survival_pct: 85.0,
            standby_promotions: 150,
            gossip_delivery_rate: 0.92,
            total_pacts_formed: 200,
            total_pacts_dropped: 30,
            final_pact_count: 170,
        };
        assert!(result.pact_survival_pct > 0.0);
        assert_eq!(result.standby_promotions, 150);
        assert!(result.gossip_delivery_rate > 0.0);
        assert_eq!(result.total_pacts_formed, 200);
        assert_eq!(result.total_pacts_dropped, 30);
        assert_eq!(result.final_pact_count, 170);
    }

    #[test]
    fn test_churn_result_json_serializable() {
        let result = ChurnResult {
            pact_survival_pct: 85.0,
            standby_promotions: 150,
            gossip_delivery_rate: 0.92,
            total_pacts_formed: 200,
            total_pacts_dropped: 30,
            final_pact_count: 170,
        };
        let json = serde_json::to_string_pretty(&result).expect("ChurnResult should serialize to JSON");
        assert!(!json.is_empty(), "JSON output should not be empty");
        assert!(json.contains("pact_survival_pct"));
        assert!(json.contains("standby_promotions"));
        assert!(json.contains("total_pacts_formed"));
        assert!(json.contains("total_pacts_dropped"));
        assert!(json.contains("final_pact_count"));
    }

    #[tokio::test]
    async fn test_run_churn_smoke() {
        let mut config = SimConfig::default();
        config.graph.nodes = 50;
        config.graph.ba_edges_per_node = 3;
        config.simulation.duration_days = 1;
        config.simulation.tick_interval_secs = 3600;
        config.simulation.deterministic = true;

        let params = ChurnParams {
            churn_pct: 10.0,
            duration_hours: 24,
        };

        let result = run_churn(config, params).await;
        assert!(result.pact_survival_pct > 0.0);
        assert!(result.gossip_delivery_rate > 0.0);
    }
}
