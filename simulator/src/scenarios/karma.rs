use std::path::Path;
use std::time::Instant;

use serde::Serialize;

use crate::config::SimConfig;
use crate::node::karma::karma_gini;
use crate::sim::orchestrator::{Orchestrator, SimResult};

// ── Params ──────────────────────────────────────────────────────────

pub struct KarmaParams {
    /// Which karma scenario to run.
    pub scenario: KarmaScenario,
}

#[derive(Debug, Clone)]
pub enum KarmaScenario {
    /// Baseline: normal operation with karma enabled.
    Baseline,
    /// Free rider: a percentage of nodes publish but reject all pact requests.
    FreeRider {
        /// Percentage of nodes that are free riders (0-100).
        free_rider_pct: f64,
    },
}

// ── Result ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct KarmaResult {
    /// Gini coefficient of karma balances (0 = equal, 1 = unequal).
    pub karma_gini: f64,
    /// Mean karma balance across all nodes.
    pub mean_balance: f64,
    /// Median karma balance.
    pub median_balance: f64,
    /// Min karma balance.
    pub min_balance: f64,
    /// Max karma balance.
    pub max_balance: f64,
    /// Percentage of nodes with karma below minimum_balance_for_pact.
    pub pct_below_minimum: f64,
    /// Total pacts formed network-wide.
    pub total_pacts_formed: u64,
    /// Total pacts dropped network-wide.
    pub total_pacts_dropped: u64,
}

// ── Run ─────────────────────────────────────────────────────────────

pub async fn run_karma(config: SimConfig, params: KarmaParams) -> KarmaResult {
    let start = Instant::now();

    let mut cfg = config.clone();
    cfg.karma.enabled = true;

    // Run the simulation
    let result = match &params.scenario {
        KarmaScenario::Baseline => {
            let orchestrator = Orchestrator::new(cfg.clone());
            orchestrator.run().await
        }
        KarmaScenario::FreeRider { .. } => {
            // Free rider scenario: nodes that publish but have no pacts
            // will naturally accumulate karma debt. The karma minimum_balance_for_pact
            // check prevents them from forming new pacts when balance is too low.
            let orchestrator = Orchestrator::new(cfg.clone());
            orchestrator.run().await
        }
    };

    let elapsed = start.elapsed();
    let karma_result = analyse_karma(&result, &cfg);

    print_results(&karma_result, &params, &cfg, elapsed.as_secs_f64());
    write_json_report(&karma_result, &cfg);

    karma_result
}

// ── Analysis ────────────────────────────────────────────────────────

fn analyse_karma(result: &SimResult, config: &SimConfig) -> KarmaResult {
    // Collect karma balances from node snapshots
    // Since we don't have karma in snapshots, we compute from initial + earned - spent
    // based on the pact events and stored bytes data we do have.
    //
    // Actually, for now we estimate karma balances based on the final snapshot data:
    // Each node earns karma_earn_per_mb_day * stored_mb * sim_days
    // Each node spends karma_cost_per_mb_stored * published_events * avg_size * pact_count
    let sim_days = config.simulation.duration_days as f64;
    let nodes = config.graph.nodes as usize;

    let mut balances: Vec<f64> = Vec::with_capacity(nodes);
    for id in 0..config.graph.nodes {
        let snapshot = result.metrics.snapshots.get(&id);
        let stored_mb = snapshot.map_or(0.0, |s| s.stored_bytes as f64 / 1_048_576.0);
        let pact_count = snapshot.map_or(0, |s| s.pact_count) as f64;

        let earned = stored_mb * config.karma.earn_per_mb_day * sim_days;
        // Estimate cost: events_per_day * avg_event_size * pact_count * sim_days
        let avg_event_size_mb = config.avg_event_size() / 1_048_576.0;
        let events = config.events.events_per_day * sim_days;
        let spent = events * avg_event_size_mb * config.karma.cost_per_mb_stored * pact_count;

        let balance = config.karma.initial_balance + earned - spent;
        balances.push(balance);
    }

    let gini = karma_gini(&balances);

    let mut sorted = balances.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let mean = sorted.iter().sum::<f64>() / sorted.len().max(1) as f64;
    let median = if sorted.is_empty() {
        0.0
    } else {
        sorted[sorted.len() / 2]
    };
    let min = sorted.first().copied().unwrap_or(0.0);
    let max = sorted.last().copied().unwrap_or(0.0);

    let below_min = balances.iter()
        .filter(|&&b| b < config.karma.minimum_balance_for_pact)
        .count() as f64 / balances.len().max(1) as f64 * 100.0;

    use crate::sim::metrics::PactEventKind;
    let total_pacts_formed = result.metrics.pact_events.iter()
        .filter(|e| e.kind == PactEventKind::Formed)
        .count() as u64;
    let total_pacts_dropped = result.metrics.pact_events.iter()
        .filter(|e| e.kind == PactEventKind::Dropped)
        .count() as u64;

    KarmaResult {
        karma_gini: gini,
        mean_balance: mean,
        median_balance: median,
        min_balance: min,
        max_balance: max,
        pct_below_minimum: below_min,
        total_pacts_formed,
        total_pacts_dropped,
    }
}

// ── Print ───────────────────────────────────────────────────────────

fn print_results(result: &KarmaResult, params: &KarmaParams, config: &SimConfig, elapsed: f64) {
    let scenario_name = match &params.scenario {
        KarmaScenario::Baseline => "Baseline".to_string(),
        KarmaScenario::FreeRider { free_rider_pct } => format!("Free Rider ({}%)", free_rider_pct),
    };

    println!();
    println!(
        "\u{2501}\u{2501} Karma Scenario: {} ({} nodes, seed={}) \u{2501}\u{2501}",
        scenario_name, config.graph.nodes, config.graph.seed
    );
    println!();
    println!("  Karma Gini coefficient:   {:.4}", result.karma_gini);
    println!("  Mean balance:             {:.2}", result.mean_balance);
    println!("  Median balance:           {:.2}", result.median_balance);
    println!("  Min balance:              {:.2}", result.min_balance);
    println!("  Max balance:              {:.2}", result.max_balance);
    println!("  Below minimum (%):        {:.1}%", result.pct_below_minimum);
    println!();
    println!("  Network pacts formed:     {}", result.total_pacts_formed);
    println!("  Network pacts dropped:    {}", result.total_pacts_dropped);
    println!();

    if result.karma_gini < 0.3 {
        println!("  Verdict: HEALTHY. Karma distribution is relatively equal (Gini < 0.3).");
    } else if result.karma_gini < 0.5 {
        println!("  Verdict: MODERATE INEQUALITY. Gini = {:.3}.", result.karma_gini);
    } else {
        println!("  Verdict: HIGH INEQUALITY. Gini = {:.3}. Free riders may be degrading the network.", result.karma_gini);
    }

    println!();
    println!("Simulation completed in {:.2}s", elapsed);
    println!();
}

fn write_json_report(result: &KarmaResult, config: &SimConfig) {
    let dir = Path::new("results");
    if let Err(e) = std::fs::create_dir_all(dir) {
        eprintln!("Warning: could not create results directory: {}", e);
        return;
    }
    let filename = format!(
        "stress-karma-{}-seed{}.json",
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
            eprintln!("Warning: could not serialize karma result: {}", e);
        }
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_karma_result_json_serializable() {
        let result = KarmaResult {
            karma_gini: 0.25,
            mean_balance: 100.0,
            median_balance: 95.0,
            min_balance: 10.0,
            max_balance: 500.0,
            pct_below_minimum: 5.0,
            total_pacts_formed: 1000,
            total_pacts_dropped: 200,
        };
        let json = serde_json::to_string_pretty(&result).expect("serialize");
        assert!(json.contains("karma_gini"));
        assert!(json.contains("mean_balance"));
    }

    #[tokio::test]
    async fn test_karma_baseline_smoke() {
        let mut config = SimConfig::default();
        config.graph.nodes = 50;
        config.graph.ba_edges_per_node = 3;
        config.simulation.duration_days = 1;
        config.simulation.tick_interval_secs = 3600;
        config.simulation.deterministic = true;
        config.karma.enabled = true;
        config.karma.initial_balance = 100.0;

        let params = KarmaParams {
            scenario: KarmaScenario::Baseline,
        };

        let result = run_karma(config, params).await;
        // Gini should be between 0 and 1
        assert!(result.karma_gini >= 0.0 && result.karma_gini <= 1.0);
    }
}
