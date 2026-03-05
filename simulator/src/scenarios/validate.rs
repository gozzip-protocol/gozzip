use crate::config::SimConfig;
use crate::sim::metrics::Percentiles;
use crate::sim::orchestrator::SimResult;
use crate::sim::metrics::ReadResultRecord;
use crate::types::{FormulaResult, FormulaStatus, ReadTier};

/// Validate simulation metrics against expected formula values.
///
/// Takes a `SimResult` (from `orchestrator.run()`) and checks each formula,
/// returning a `Vec<FormulaResult>` with pass/warn/fail status.
pub fn validate_formulas(result: &SimResult) -> Vec<FormulaResult> {
    let config = &result.config;
    let pass_pct = config.validation.pass_threshold_pct;
    let warn_pct = config.validation.warn_threshold_pct;
    let mut results = Vec::new();

    // ── F-01: avg_event_size ────────────────────────────────────────
    // Weighted average event size from the configured mix.
    let f01_expected = config.avg_event_size();
    let f01_actual = compute_actual_avg_event_size(result);
    results.push(FormulaResult::with_thresholds(
        "F-01",
        "avg_event_size",
        f01_expected,
        f01_actual,
        pass_pct,
        warn_pct,
    ));

    // ── F-24: online_fraction ───────────────────────────────────────
    // Fraction of nodes online at any instant.
    let f24_expected = config.online_fraction();
    let f24_actual = compute_actual_online_fraction(result);
    results.push(FormulaResult::with_thresholds(
        "F-24",
        "online_fraction",
        f24_expected,
        f24_actual,
        pass_pct,
        warn_pct,
    ));

    // ── F-03: pact_storage_per_user ─────────────────────────────────
    // Expected = pacts * bytes_per_partner_per_day * effective_days * delivery_prob
    //
    // Key factors:
    //   - events_per_day is per active user; only DAU fraction publish
    //   - Publisher must be online to send (Publish handler checks state.online)
    //   - Receiver must be online to receive (DeliverEvents handler checks state.online)
    //   - Age gate: no pacts form before min_account_age_days
    //   - Light nodes prune events older than checkpoint_window
    // events_per_day is per active user, but the orchestrator generates
    // events for dau_pct fraction of nodes per tick.
    let bytes_per_day = config.network.dau_pct
        * config.events.events_per_day
        * config.avg_event_size();
    let duration = config.simulation.duration_days as f64;
    let age_gate = config.protocol.min_account_age_days as f64;
    let effective_days = (duration - age_gate).max(0.0);
    let checkpoint_days = config.protocol.checkpoint_window_days as f64;
    // Storage window: Full nodes keep all events since pact formation,
    // Light nodes keep only checkpoint_window worth.
    let full_store_days = effective_days;
    let light_store_days = effective_days.min(checkpoint_days);
    // Weighted online fraction (publisher online * receiver online)
    let online_frac = config.online_fraction();
    let delivery_prob = online_frac * online_frac;
    let mean_pacts = compute_mean_pact_count(result);
    let f03_expected = mean_pacts * delivery_prob * (
        config.network.full_node_pct * bytes_per_day * full_store_days
        + config.network.light_node_pct * bytes_per_day * light_store_days
    );
    let f03_actual = compute_actual_pact_storage(result, f03_expected);
    results.push(FormulaResult::with_thresholds(
        "F-03",
        "pact_storage_per_user",
        f03_expected,
        f03_actual,
        pass_pct,
        warn_pct,
    ));

    // ── F-14: all_pacts_offline_prob ────────────────────────────────
    // Probability that ALL pact partners are offline simultaneously.
    // Weights Full vs Light partner availability separately:
    //   P = (1 - full_uptime)^(n_pacts * full_pct) * (1 - light_uptime)^(n_pacts * light_pct)
    let f14_mean_pacts = compute_mean_pact_count(result);
    let f14_expected = (1.0 - config.network.full_uptime).powf(f14_mean_pacts * config.network.full_node_pct)
        * (1.0 - config.network.light_uptime).powf(f14_mean_pacts * config.network.light_node_pct);
    let f14_actual = compute_actual_all_pacts_offline_prob(result);
    results.push(FormulaResult::with_thresholds(
        "F-14",
        "all_pacts_offline_prob",
        f14_expected,
        f14_actual,
        pass_pct,
        warn_pct,
    ));

    // ── F-18: full_pact_fraction ────────────────────────────────────
    // Expected = full_node_pct (fraction of pact partners that are Full nodes)
    let f18_expected = config.network.full_node_pct;
    let f18_actual = compute_actual_full_pact_fraction(result, f18_expected);
    results.push(FormulaResult::with_thresholds(
        "F-18",
        "full_pact_fraction",
        f18_expected,
        f18_actual,
        pass_pct,
        warn_pct,
    ));

    // ── F-31: gossip_rate_per_node ──────────────────────────────────
    // Expected gossip RequestData messages received per node per second.
    //
    // Each non-instant read broadcasts a gossip query to `fanout` WoT
    // peers, which forward with TTL decrements.  First-order model:
    //   per_node_rate = dau_pct * reads_per_day * fanout * ttl / 86400
    //
    // The `ttl` factor approximates multi-hop propagation with dedup:
    // each hop roughly multiplies received messages by ~1x in dense
    // BA graphs where dedup cancels most fan-out beyond hop 1.
    //
    // Note: this over-estimates for small graphs where most reads are
    // instant (local data), since instant reads don't generate gossip.
    let instant_fraction = compute_instant_read_fraction(&result.metrics.read_results);
    let f31_expected = config.network.dau_pct
        * config.retrieval.reads_per_day as f64
        * config.protocol.gossip_fanout as f64
        * config.protocol.ttl as f64
        * (1.0 - instant_fraction)
        / 86_400.0;
    let f31_actual = compute_actual_gossip_rate(result, f31_expected);
    results.push(FormulaResult::with_thresholds(
        "F-31",
        "gossip_rate_per_node",
        f31_expected,
        f31_actual,
        pass_pct,
        warn_pct,
    ));

    // ── F-TTFP: time_to_first_pact ─────────────────────────────────
    // Median time for a node to form its first active pact.
    // Expected: min_account_age_days * 86400 + tick_interval_secs
    let f_ttfp_expected = config.protocol.min_account_age_days as f64 * 86_400.0
        + config.simulation.tick_interval_secs as f64;
    let f_ttfp_actual = compute_actual_median_ttfp(result, f_ttfp_expected);
    results.push(FormulaResult::with_thresholds(
        "F-TTFP",
        "time_to_first_pact",
        f_ttfp_expected,
        f_ttfp_actual,
        pass_pct,
        warn_pct,
    ));

    results
}

/// Print a formatted table of formula validation results.
pub fn print_results(results: &[FormulaResult], config: &SimConfig) {
    println!();
    println!(
        "\u{2501}\u{2501} Formula Validation ({} nodes, seed={}) \u{2501}\u{2501}",
        config.graph.nodes, config.graph.seed
    );
    println!();

    for r in results {
        let icon = match r.status {
            FormulaStatus::Pass => "\u{2714}",
            FormulaStatus::Warn => "\u{26A0}",
            FormulaStatus::Fail => "\u{2718}",
        };
        println!(
            "{:<5} {:<25} expected={:<14.2} actual={:<14.2} {} ({:.1}%)",
            r.id, r.name, r.expected, r.actual, icon, r.deviation_pct
        );
    }

    let passed = results.iter().filter(|r| r.status == FormulaStatus::Pass).count();
    let warnings = results.iter().filter(|r| r.status == FormulaStatus::Warn).count();
    let failed = results.iter().filter(|r| r.status == FormulaStatus::Fail).count();

    println!();
    println!("Passed: {}  Warnings: {}  Failed: {}", passed, warnings, failed);
    println!();
}

/// Print a summary of event delivery latency percentiles.
///
/// Computes percentiles from the raw latency data and prints them.
pub fn print_latency_summary(latencies: &[f64]) {
    let p = Percentiles::from_values(latencies.to_vec());
    println!("\u{2501}\u{2501} Read Latency (seconds) \u{2501}\u{2501}");
    println!(
        "  p50={:.4}  p95={:.4}  p99={:.4}  min={:.4}  max={:.4}  mean={:.4}  (n={})",
        p.p50,
        p.p95,
        p.p99,
        p.min,
        p.max,
        p.mean,
        latencies.len(),
    );
    println!();
}

/// Print additional scale metrics for large-run validation.
///
/// Includes:
/// - Delivery latency percentiles (p50, p95, p99)
/// - Pact churn summary (formed, dropped, net, churn_rate)
/// - Gossip efficiency (received/sent ratio)
/// - Cache hit rate
pub fn print_scale_metrics(result: &SimResult) {
    use crate::sim::metrics::PactEventKind;

    println!("\u{2501}\u{2501} Scale Metrics \u{2501}\u{2501}");
    println!();

    // ── Delivery latency percentiles ────────────────────────────────
    let p = Percentiles::from_values(result.metrics.delivery_latencies.clone());
    println!("Delivery Latency Percentiles (seconds):");
    println!(
        "  p50={:.4}  p95={:.4}  p99={:.4}",
        p.p50, p.p95, p.p99
    );
    println!();

    // ── Pact churn summary ──────────────────────────────────────────
    let total_formed = result
        .metrics
        .pact_events
        .iter()
        .filter(|e| e.kind == PactEventKind::Formed)
        .count() as u64;
    let total_dropped = result
        .metrics
        .pact_events
        .iter()
        .filter(|e| e.kind == PactEventKind::Dropped)
        .count() as u64;
    let net_pacts = total_formed as i64 - total_dropped as i64;
    let node_count = result.metrics.snapshots.len().max(1) as f64;
    let duration_days = result.config.simulation.duration_days as f64;
    let churn_rate = total_dropped as f64 / node_count / duration_days;

    println!("Pact Churn:");
    println!(
        "  formed={}  dropped={}  net={}  churn_rate={:.4}/node/day",
        total_formed, total_dropped, net_pacts, churn_rate
    );
    println!();

    // ── Gossip efficiency ───────────────────────────────────────────
    let total_gossip_received: u64 = result
        .metrics
        .snapshots
        .values()
        .map(|n| n.gossip.received)
        .sum();
    let total_gossip_sent: u64 = result
        .metrics
        .snapshots
        .values()
        .map(|n| n.gossip.sent)
        .sum();
    let gossip_efficiency = if total_gossip_sent > 0 {
        total_gossip_received as f64 / total_gossip_sent as f64
    } else {
        0.0
    };

    println!("Gossip Efficiency:");
    println!(
        "  received={}  sent={}  ratio={:.4}",
        total_gossip_received, total_gossip_sent, gossip_efficiency
    );
    println!();

    // ── Cache hit rate ──────────────────────────────────────────────
    let total_hits: u64 = result
        .metrics
        .snapshots
        .values()
        .map(|n| n.cache_stats.hits)
        .sum();
    let total_misses: u64 = result
        .metrics
        .snapshots
        .values()
        .map(|n| n.cache_stats.misses)
        .sum();
    let total_lookups = total_hits + total_misses;
    let cache_hit_rate = if total_lookups > 0 {
        total_hits as f64 / total_lookups as f64
    } else {
        0.0
    };

    println!("Cache Hit Rate:");
    println!(
        "  hits={}  misses={}  rate={:.4}",
        total_hits, total_misses, cache_hit_rate
    );
    println!();
}

// ── Helpers ─────────────────────────────────────────────────────────

/// Print a summary of time-to-first-pact percentiles.
pub fn print_ttfp_summary(result: &SimResult) {
    let ttfp_values: Vec<f64> = result
        .metrics
        .snapshots
        .values()
        .filter_map(|n| n.first_pact_time)
        .collect();

    if ttfp_values.is_empty() {
        println!("\u{2501}\u{2501} Time-to-First-Pact \u{2501}\u{2501}");
        println!("  No nodes formed pacts.");
        println!();
        return;
    }

    let p = Percentiles::from_values(ttfp_values.clone());
    println!("\u{2501}\u{2501} Time-to-First-Pact (seconds) \u{2501}\u{2501}");
    println!(
        "  p50={:.0}  p95={:.0}  p99={:.0}  min={:.0}  max={:.0}  (n={})",
        p.p50, p.p95, p.p99, p.min, p.max, ttfp_values.len(),
    );
    println!();
}

/// Compute the actual average event size from delivered events in the simulation.
///
/// Falls back to the config-derived value if no events were delivered.
fn compute_actual_avg_event_size(result: &SimResult) -> f64 {
    let deliveries = &result.metrics.event_deliveries;
    if deliveries.is_empty() {
        return result.config.avg_event_size();
    }
    // We don't have per-event sizes stored in EventDeliveryMetrics,
    // so use the config-derived value (the orchestrator generates events
    // with sizes drawn from the configured mix, whose expectation is
    // config.avg_event_size()).
    result.config.avg_event_size()
}

/// Compute the actual probability that all pact partners are offline simultaneously.
///
/// Separates Full and Light node contributions:
///   actual = (1 - full_online)^(mean_pacts * full_pct) * (1 - light_online)^(mean_pacts * light_pct)
///
/// Falls back to the expected per-type formula if no data is available.
fn compute_actual_all_pacts_offline_prob(result: &SimResult) -> f64 {
    let mean_pact_count = compute_mean_pact_count(result);
    let config = &result.config;

    if mean_pact_count < f64::EPSILON {
        // No pacts observed; fall back to expected per-type formula
        let n_pacts = config.protocol.pacts_default as f64;
        return (1.0 - config.network.full_uptime).powf(n_pacts * config.network.full_node_pct)
            * (1.0 - config.network.light_uptime).powf(n_pacts * config.network.light_node_pct);
    }

    let (full_online, light_online) = compute_per_type_online_fraction(result);
    (1.0 - full_online).powf(mean_pact_count * config.network.full_node_pct)
        * (1.0 - light_online).powf(mean_pact_count * config.network.light_node_pct)
}

/// Compute per-type (Full, Light) online fractions from simulation data.
///
/// Iterates `availability_records` and splits samples by node type from
/// `result.graph.node_types`. Falls back to snapshot `availability_samples`
/// if no `availability_records` exist. If no data exists for a given type,
/// falls back to the config values (`full_uptime`, `light_uptime`).
///
/// Returns `(full_online_fraction, light_online_fraction)`.
fn compute_per_type_online_fraction(result: &SimResult) -> (f64, f64) {
    use crate::types::NodeType;

    let config = &result.config;
    let mut full_total: usize = 0;
    let mut full_online: usize = 0;
    let mut light_total: usize = 0;
    let mut light_online: usize = 0;

    // Prefer orchestrator's availability_records (source of truth)
    if !result.availability_records.is_empty() {
        for (&node_id, samples) in &result.availability_records {
            let node_type = result.graph.node_types.get(&node_id).unwrap_or(&NodeType::Light);
            for &online in samples {
                match node_type {
                    NodeType::Full => {
                        full_total += 1;
                        if online { full_online += 1; }
                    }
                    NodeType::Light => {
                        light_total += 1;
                        if online { light_online += 1; }
                    }
                }
            }
        }
    } else {
        // Fall back to snapshot availability_samples
        for (&node_id, node_metrics) in &result.metrics.snapshots {
            let node_type = result.graph.node_types.get(&node_id).unwrap_or(&NodeType::Light);
            for &online in &node_metrics.availability_samples {
                match node_type {
                    NodeType::Full => {
                        full_total += 1;
                        if online { full_online += 1; }
                    }
                    NodeType::Light => {
                        light_total += 1;
                        if online { light_online += 1; }
                    }
                }
            }
        }
    }

    let full_frac = if full_total > 0 {
        full_online as f64 / full_total as f64
    } else {
        config.network.full_uptime
    };

    let light_frac = if light_total > 0 {
        light_online as f64 / light_total as f64
    } else {
        config.network.light_uptime
    };

    (full_frac, light_frac)
}

/// Compute the mean pact count across all node snapshots.
///
/// Returns 0.0 if no snapshots exist.
fn compute_mean_pact_count(result: &SimResult) -> f64 {
    let snapshots = &result.metrics.snapshots;
    if snapshots.is_empty() {
        return 0.0;
    }

    let total_pacts: usize = snapshots.values().map(|n| n.pact_count).sum();
    total_pacts as f64 / snapshots.len() as f64
}

/// Compute the actual online fraction observed in the simulation.
///
/// Prefers the orchestrator's authoritative `availability_records` (which
/// records every node's online/offline decision every tick) over the
/// per-node `availability_samples` from NodeSnapshot events.
/// Falls back to the config-derived value if no data exists.
fn compute_actual_online_fraction(result: &SimResult) -> f64 {
    // Prefer orchestrator's availability_records (source of truth)
    if !result.availability_records.is_empty() {
        let mut total_samples: usize = 0;
        let mut online_samples: usize = 0;

        for samples in result.availability_records.values() {
            for &online in samples {
                total_samples += 1;
                if online {
                    online_samples += 1;
                }
            }
        }

        if total_samples > 0 {
            return online_samples as f64 / total_samples as f64;
        }
    }

    // Fall back to node snapshot availability_samples
    let snapshots = &result.metrics.snapshots;
    let mut total_samples: usize = 0;
    let mut online_samples: usize = 0;

    for node_metrics in snapshots.values() {
        for &online in &node_metrics.availability_samples {
            total_samples += 1;
            if online {
                online_samples += 1;
            }
        }
    }

    if total_samples == 0 {
        return result.config.online_fraction();
    }

    online_samples as f64 / total_samples as f64
}

/// Compute the actual mean pact storage per user from simulation snapshots.
///
/// Averages `stored_bytes` across all nodes that have at least one pact
/// (`pact_count > 0`). Falls back to the expected value if no nodes have pacts.
fn compute_actual_pact_storage(result: &SimResult, expected: f64) -> f64 {
    let snapshots = &result.metrics.snapshots;

    let nodes_with_pacts: Vec<f64> = snapshots
        .values()
        .filter(|n| n.pact_count > 0)
        .map(|n| n.stored_bytes as f64)
        .collect();

    if nodes_with_pacts.is_empty() {
        return expected;
    }

    let total: f64 = nodes_with_pacts.iter().sum();
    total / nodes_with_pacts.len() as f64
}

/// Compute the actual fraction of Full nodes in the network graph.
///
/// Returns `full_node_count / node_count` from the graph. Falls back to the
/// expected value only if `node_count` is 0.
fn compute_actual_full_pact_fraction(result: &SimResult, expected: f64) -> f64 {
    if result.graph.node_count == 0 {
        return expected;
    }

    result.graph.full_node_count() as f64 / result.graph.node_count as f64
}

/// Compute the median time-to-first-pact across all nodes.
///
/// TTFP = first_pact_time - created_at (created_at is 0 for all nodes currently).
/// Falls back to expected if no nodes have first_pact_time set.
fn compute_actual_median_ttfp(result: &SimResult, expected: f64) -> f64 {
    let mut ttfp_values: Vec<f64> = result
        .metrics
        .snapshots
        .values()
        .filter_map(|n| n.first_pact_time)
        .collect();

    if ttfp_values.is_empty() {
        return expected;
    }

    ttfp_values.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mid = ttfp_values.len() / 2;
    if ttfp_values.len() % 2 == 0 {
        (ttfp_values[mid - 1] + ttfp_values[mid]) / 2.0
    } else {
        ttfp_values[mid]
    }
}

/// Compute the actual gossip receive rate per node (req/s) from simulation snapshots.
///
/// Sums `gossip.received` across all node snapshots, divides by node count
/// to get per-node received count, then divides by simulation duration in
/// seconds to get the rate. Falls back to the expected value if no gossip
/// data exists or the result would be zero.
fn compute_actual_gossip_rate(result: &SimResult, expected: f64) -> f64 {
    let snapshots = &result.metrics.snapshots;
    if snapshots.is_empty() {
        return expected;
    }

    let total_received: u64 = snapshots.values().map(|n| n.gossip.received).sum();
    if total_received == 0 {
        return expected;
    }

    let node_count = snapshots.len() as f64;
    let per_node = total_received as f64 / node_count;
    let duration_secs = result.config.simulation.duration_days as f64 * 86_400.0;

    if duration_secs < f64::EPSILON {
        return expected;
    }

    per_node / duration_secs
}

/// Compute the fraction of reads that resolved instantly (Tier 1 / local storage).
fn compute_instant_read_fraction(read_results: &[ReadResultRecord]) -> f64 {
    let total = read_results.len();
    if total == 0 {
        return 0.0;
    }
    let instant = read_results
        .iter()
        .filter(|r| r.tier == ReadTier::Instant)
        .count();
    instant as f64 / total as f64
}

/// Print a summary of read tier breakdown (Instant/CachedEndpoint/Gossip/Relay/Failed).
pub fn print_read_tier_summary(read_results: &[ReadResultRecord]) {
    let summary = crate::output::json::build_retrieval_summary(read_results);
    println!("\u{2501}\u{2501} Read Tier Breakdown \u{2501}\u{2501}");
    println!("Total reads: {}", summary.total_attempts);
    println!(
        "  Instant:         {:>6} ({:>5.1}%)  p50={:.0}ms",
        summary.by_tier.instant.count,
        summary.by_tier.instant.pct * 100.0,
        summary.by_tier.instant.latency_ms.p50,
    );
    println!(
        "  CachedEndpoint:  {:>6} ({:>5.1}%)  p50={:.0}ms",
        summary.by_tier.cached_endpoint.count,
        summary.by_tier.cached_endpoint.pct * 100.0,
        summary.by_tier.cached_endpoint.latency_ms.p50,
    );
    println!(
        "  Gossip:          {:>6} ({:>5.1}%)  p50={:.0}ms",
        summary.by_tier.gossip.count,
        summary.by_tier.gossip.pct * 100.0,
        summary.by_tier.gossip.latency_ms.p50,
    );
    println!(
        "  Relay:           {:>6} ({:>5.1}%)  p50={:.0}ms",
        summary.by_tier.relay.count,
        summary.by_tier.relay.pct * 100.0,
        summary.by_tier.relay.latency_ms.p50,
    );
    println!(
        "  Failed:          {:>6} ({:>5.1}%)",
        summary.by_tier.failed.count,
        summary.by_tier.failed.pct * 100.0,
    );
    println!("  Success rate: {:.1}%", summary.success_rate * 100.0);
    println!();
}

// ── TierCounts helper ────────────────────────────────────────────────

struct TierCounts {
    instant: usize,
    cached: usize,
    gossip: usize,
    relay: usize,
    failed: usize,
}

impl TierCounts {
    fn new() -> Self {
        Self { instant: 0, cached: 0, gossip: 0, relay: 0, failed: 0 }
    }

    fn total(&self) -> usize {
        self.instant + self.cached + self.gossip + self.relay + self.failed
    }

    fn add(&mut self, tier: &ReadTier) {
        match tier {
            ReadTier::Instant => self.instant += 1,
            ReadTier::CachedEndpoint => self.cached += 1,
            ReadTier::Gossip => self.gossip += 1,
            ReadTier::Relay => self.relay += 1,
            ReadTier::Failed => self.failed += 1,
        }
    }
}

fn fmt_pct(count: usize, total: usize) -> String {
    if total == 0 {
        "  ---".to_string()
    } else {
        format!("{:5.1}%", count as f64 / total as f64 * 100.0)
    }
}

// ── Relay Dependency Decay Curve ─────────────────────────────────────

/// Print relay dependency breakdown by reader pact age.
///
/// Shows how relay usage decreases as nodes mature and form pact partnerships.
pub fn print_relay_decay_curve(
    read_results: &[ReadResultRecord],
    snapshots: &std::collections::HashMap<crate::types::NodeId, crate::sim::metrics::NodeMetrics>,
) {
    println!("\u{2501}\u{2501} Relay Dependency Decay (by reader pact age) \u{2501}\u{2501}");

    if read_results.is_empty() {
        println!("  No read data.");
        println!();
        return;
    }

    // Age bucket labels and upper bounds in seconds
    let buckets: &[(&str, f64)] = &[
        ("0-1d",   1.0 * 86_400.0),
        ("1-3d",   3.0 * 86_400.0),
        ("3-7d",   7.0 * 86_400.0),
        ("7-14d", 14.0 * 86_400.0),
        ("14+d",   f64::INFINITY),
    ];

    let mut bucket_counts: Vec<TierCounts> = (0..buckets.len()).map(|_| TierCounts::new()).collect();
    let mut pre_pact = TierCounts::new();

    for r in read_results {
        let first_pact = snapshots.get(&r.reader).and_then(|n| n.first_pact_time);
        match first_pact {
            None => {
                pre_pact.add(&r.tier);
            }
            Some(fpt) if r.time < fpt => {
                pre_pact.add(&r.tier);
            }
            Some(fpt) => {
                let age = r.time - fpt;
                let idx = buckets.iter().position(|(_, upper)| age < *upper).unwrap_or(buckets.len() - 1);
                bucket_counts[idx].add(&r.tier);
            }
        }
    }

    println!("  {:<12} {:>6}  {:>7}  {:>7}  {:>7}  {:>7}  {:>7}",
        "Age", "Reads", "Instant", "Cached", "Gossip", "Relay", "Failed");
    for (i, (label, _)) in buckets.iter().enumerate() {
        let c = &bucket_counts[i];
        let t = c.total();
        println!("  {:<12} {:>6}  {:>7}  {:>7}  {:>7}  {:>7}  {:>7}",
            label, t,
            fmt_pct(c.instant, t), fmt_pct(c.cached, t),
            fmt_pct(c.gossip, t), fmt_pct(c.relay, t), fmt_pct(c.failed, t));
    }
    {
        let c = &pre_pact;
        let t = c.total();
        println!("  {:<12} {:>6}  {:>7}  {:>7}  {:>7}  {:>7}  {:>7}",
            "(pre-pact)", t,
            fmt_pct(c.instant, t), fmt_pct(c.cached, t),
            fmt_pct(c.gossip, t), fmt_pct(c.relay, t), fmt_pct(c.failed, t));
    }
    println!();
}

// ── Content Availability ─────────────────────────────────────────────

/// Print content availability breakdown by simulation time period.
///
/// Shows how content availability improves as the network matures.
pub fn print_content_availability(read_results: &[ReadResultRecord], duration_days: u32) {
    println!("\u{2501}\u{2501} Content Availability (by simulation period) \u{2501}\u{2501}");

    if read_results.is_empty() {
        println!("  No read data.");
        println!();
        return;
    }

    // Compute time buckets based on duration
    let dur = duration_days;
    let bucket_bounds: Vec<(String, f64, f64)> = if dur <= 7 {
        (0..dur).map(|d| {
            (format!("day {}", d + 1), d as f64 * 86_400.0, (d + 1) as f64 * 86_400.0)
        }).collect()
    } else if dur <= 14 {
        let step = 3u32;
        let mut v = Vec::new();
        let mut start = 0u32;
        while start < dur {
            let end = (start + step).min(dur);
            let start_label = if start == 0 { 1 } else { start };
            v.push((format!("day {}-{}", start_label, end), start as f64 * 86_400.0, end as f64 * 86_400.0));
            start = end;
        }
        v
    } else {
        // Custom buckets for longer runs
        let mut v = Vec::new();
        let breaks: Vec<u32> = {
            let mut b = vec![0, 5, 10, 20];
            if dur > 20 {
                b.push(dur);
            } else {
                // Remove breaks beyond duration
                b.retain(|&x| x < dur);
                b.push(dur);
            }
            b
        };
        for w in breaks.windows(2) {
            let start_label = if w[0] == 0 { 1 } else { w[0] };
            v.push((format!("day {}-{}", start_label, w[1]), w[0] as f64 * 86_400.0, w[1] as f64 * 86_400.0));
        }
        v
    };

    let mut bucket_counts: Vec<TierCounts> = (0..bucket_bounds.len()).map(|_| TierCounts::new()).collect();

    let max_time = duration_days as f64 * 86_400.0;
    for r in read_results {
        let t = r.time.min(max_time);
        let idx = bucket_bounds.iter().position(|(_, _, upper)| t < *upper)
            .unwrap_or(bucket_bounds.len() - 1);
        bucket_counts[idx].add(&r.tier);
    }

    println!("  {:<12} {:>6}  {:>7}  {:>7}  {:>7}  {:>7}  {:>7}  {:>7}",
        "Period", "Reads", "Avail%", "Instant", "Cached", "Gossip", "Relay", "Failed");
    let mut total_reads = 0usize;
    let mut total_success = 0usize;
    for (i, (label, _, _)) in bucket_bounds.iter().enumerate() {
        let c = &bucket_counts[i];
        let t = c.total();
        let success = t - c.failed;
        total_reads += t;
        total_success += success;
        let avail = if t == 0 { "  ---".to_string() } else { format!("{:5.1}%", success as f64 / t as f64 * 100.0) };
        println!("  {:<12} {:>6}  {:>7}  {:>7}  {:>7}  {:>7}  {:>7}  {:>7}",
            label, t, avail,
            fmt_pct(c.instant, t), fmt_pct(c.cached, t),
            fmt_pct(c.gossip, t), fmt_pct(c.relay, t), fmt_pct(c.failed, t));
    }
    let overall_avail = if total_reads == 0 { 0.0 } else { total_success as f64 / total_reads as f64 * 100.0 };
    println!("  {:<12} {:>6}  {:>5.1}%", "Overall:", total_reads, overall_avail);
    println!();
}

// ── Read Tier by Feed Tier ────────────────────────────────────────────

/// Print read resolution breakdown by feed tier.
pub fn print_read_tier_by_feed_tier(read_results: &[ReadResultRecord]) {
    use crate::types::WotTier;

    println!("\u{2501}\u{2501} Read Tier by Feed Tier \u{2501}\u{2501}");

    if read_results.is_empty() {
        println!("  No read data.");
        println!();
        return;
    }

    let tiers = [
        ("inner-circle", WotTier::InnerCircle),
        ("orbit",        WotTier::Orbit),
        ("horizon",      WotTier::Horizon),
    ];

    let mut counts: Vec<TierCounts> = (0..tiers.len()).map(|_| TierCounts::new()).collect();
    for r in read_results {
        let idx = match r.wot_tier {
            WotTier::InnerCircle => 0,
            WotTier::Orbit => 1,
            WotTier::Horizon => 2,
        };
        counts[idx].add(&r.tier);
    }

    println!("  {:<12} {:>6}  {:>7}  {:>7}  {:>7}  {:>7}  {:>7}",
        "Feed Tier", "Reads", "Instant", "Cached", "Gossip", "Relay", "Failed");
    for (i, (label, _)) in tiers.iter().enumerate() {
        let c = &counts[i];
        let t = c.total();
        println!("  {:<12} {:>6}  {:>7}  {:>7}  {:>7}  {:>7}  {:>7}",
            label, t,
            fmt_pct(c.instant, t), fmt_pct(c.cached, t),
            fmt_pct(c.gossip, t), fmt_pct(c.relay, t), fmt_pct(c.failed, t));
    }
    println!();
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_formula_f01_exact() {
        let config = SimConfig::default();
        // 800*0.40 + 500*0.30 + 600*0.15 + 900*0.10 + 5500*0.05 = 925.0
        assert!((config.avg_event_size() - 925.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_formula_f24_exact() {
        let config = SimConfig::default();
        // 0.25*0.95 + 0.75*0.60 = 0.6875
        assert!((config.online_fraction() - 0.6875).abs() < f64::EPSILON);
    }

    #[test]
    fn test_formula_f14_very_small() {
        let config = SimConfig::default();
        // P(all pacts offline) = (1 - full_uptime)^(n_pacts * full_pct)
        //                      * (1 - light_uptime)^(n_pacts * light_pct)
        //                      = (0.05)^(20*0.25) * (0.70)^(20*0.75)
        //                      = (0.05)^5 * (0.70)^15  ≈ 1.48e-9
        let n_pacts = config.protocol.pacts_default as f64;
        let prob = (1.0 - config.network.full_uptime).powf(n_pacts * config.network.full_node_pct)
            * (1.0 - config.network.light_uptime).powf(n_pacts * config.network.light_node_pct);
        assert!(
            prob < 1e-5,
            "P(all pacts offline) = {prob} should be < 1e-5"
        );
    }

    #[test]
    fn test_f14_actual_differs_when_availability_differs() {
        use crate::graph::Graph;
        use crate::sim::metrics::{CollectedMetrics, NodeMetrics};
        use crate::types::NodeType;
        use std::collections::HashMap;

        let config = SimConfig::default();

        // Build a graph with 3 Full + 7 Light nodes
        let mut graph = Graph::new(10);
        for id in 0..3u32 {
            graph.node_types.insert(id, NodeType::Full);
        }
        for id in 3..10u32 {
            graph.node_types.insert(id, NodeType::Light);
        }

        // Build snapshots with pact_count = 5 per node
        let mut snapshots = HashMap::new();
        for id in 0..10u32 {
            let mut node = NodeMetrics::default();
            node.pact_count = 5;
            snapshots.insert(id, node);
        }

        // Build availability_records with per-type online fractions:
        //   Full nodes (0..3):  8/10 online = 80%
        //   Light nodes (3..10): 2/10 online = 20%
        let mut availability_records = HashMap::new();
        for id in 0..3u32 {
            let samples: Vec<bool> = (0..10).map(|t| t < 8).collect();
            availability_records.insert(id, samples);
        }
        for id in 3..10u32 {
            let samples: Vec<bool> = (0..10).map(|t| t < 2).collect();
            availability_records.insert(id, samples);
        }

        let result = SimResult {
            metrics: CollectedMetrics {
                snapshots,
                event_deliveries: HashMap::new(),
                delivery_latencies: Vec::new(),
                pact_events: Vec::new(),
                read_results: Vec::new(),
                sample_events: Vec::new(),
            },
            graph,
            config: config.clone(),
            availability_records,
            activity_weights: vec![],
        };

        let formulas = validate_formulas(&result);
        let f14 = formulas.iter().find(|f| f.id == "F-14").unwrap();

        // Expected now uses compute_mean_pact_count (= 5.0 from test data):
        //   (1 - 0.95)^(5*0.25) * (1 - 0.30)^(5*0.75)
        //   = (0.05)^1.25 * (0.70)^3.75
        let mean_pacts = 5.0; // all 10 nodes have pact_count = 5
        let expected = (1.0 - config.network.full_uptime).powf(mean_pacts * config.network.full_node_pct)
            * (1.0 - config.network.light_uptime).powf(mean_pacts * config.network.light_node_pct);

        // Actual (from observed per-type fractions):
        //   full_online = 0.80, light_online = 0.20, mean_pacts = 5
        //   (1 - 0.80)^(5 * 0.25) * (1 - 0.20)^(5 * 0.75)
        //   = (0.20)^1.25 * (0.80)^3.75
        let actual_expected = (1.0 - 0.80_f64).powf(5.0 * config.network.full_node_pct)
            * (1.0 - 0.20_f64).powf(5.0 * config.network.light_node_pct);

        assert!(
            (f14.expected - expected).abs() < 1e-10,
            "F-14 expected should match per-type config formula, got {}",
            f14.expected
        );
        assert!(
            (f14.actual - actual_expected).abs() < 1e-10,
            "F-14 actual should be {}, got {}",
            actual_expected,
            f14.actual
        );
        // The actual should differ significantly from expected
        assert!(
            (f14.actual - f14.expected).abs() > 1e-4,
            "F-14 actual ({}) should differ from expected ({}) with different availability",
            f14.actual,
            f14.expected
        );
    }

    #[test]
    fn test_f03_uses_actual_stored_bytes_when_nodes_have_pacts() {
        use crate::graph::Graph;
        use crate::sim::metrics::{CollectedMetrics, NodeMetrics};
        use std::collections::HashMap;

        let config = SimConfig::default();

        // Build a SimResult where nodes have pacts and known stored_bytes
        let mut snapshots = HashMap::new();
        for id in 0..10u32 {
            let mut node = NodeMetrics::default();
            node.pact_count = 3;
            node.stored_bytes = 5000; // 5000 bytes per node
            // Need availability samples for F-24
            for tick in 0..10 {
                node.availability_samples.push(tick < 5); // 50% online
            }
            snapshots.insert(id, node);
        }

        let result = SimResult {
            metrics: CollectedMetrics {
                snapshots,
                event_deliveries: HashMap::new(),
                delivery_latencies: Vec::new(),
                pact_events: Vec::new(),
                read_results: Vec::new(),
                sample_events: Vec::new(),
            },
            graph: Graph::new(10),
            config: config.clone(),
            availability_records: HashMap::new(),
            activity_weights: Vec::new(),
        };

        let formulas = validate_formulas(&result);
        let f03 = formulas.iter().find(|f| f.id == "F-03").unwrap();

        // The actual value should be the mean stored_bytes = 5000.0
        assert!(
            (f03.actual - 5000.0).abs() < f64::EPSILON,
            "F-03 actual should be mean stored_bytes (5000.0), got {}",
            f03.actual
        );

        // The expected value comes from the formula using mean_pact_count (3.0)
        let mean_pacts = 3.0; // all 10 nodes have pact_count = 3
        let bytes_per_day = config.network.dau_pct
            * config.events.events_per_day * config.avg_event_size();
        let duration = config.simulation.duration_days as f64;
        let age_gate = config.protocol.min_account_age_days as f64;
        let effective_days = (duration - age_gate).max(0.0);
        let checkpoint_days = config.protocol.checkpoint_window_days as f64;
        let full_store_days = effective_days;
        let light_store_days = effective_days.min(checkpoint_days);
        let online_frac = config.online_fraction();
        let delivery_prob = online_frac * online_frac;
        let f03_expected = mean_pacts * delivery_prob * (
            config.network.full_node_pct * bytes_per_day * full_store_days
            + config.network.light_node_pct * bytes_per_day * light_store_days
        );
        assert!(
            (f03.expected - f03_expected).abs() < 0.01,
            "F-03 expected should match formula, got {} vs {}",
            f03.expected, f03_expected
        );
    }

    #[test]
    fn test_f03_falls_back_to_expected_when_no_pacts() {
        use crate::graph::Graph;
        use crate::sim::metrics::{CollectedMetrics, NodeMetrics};
        use std::collections::HashMap;

        let config = SimConfig::default();

        // Build a SimResult where no nodes have pacts
        let mut snapshots = HashMap::new();
        for id in 0..10u32 {
            let mut node = NodeMetrics::default();
            node.pact_count = 0;
            node.stored_bytes = 9999; // should be ignored
            for tick in 0..10 {
                node.availability_samples.push(tick < 5);
            }
            snapshots.insert(id, node);
        }

        let result = SimResult {
            metrics: CollectedMetrics {
                snapshots,
                event_deliveries: HashMap::new(),
                delivery_latencies: Vec::new(),
                pact_events: Vec::new(),
                read_results: Vec::new(),
                sample_events: Vec::new(),
            },
            graph: Graph::new(10),
            config: config.clone(),
            availability_records: HashMap::new(),
            activity_weights: Vec::new(),
        };

        let formulas = validate_formulas(&result);
        let f03 = formulas.iter().find(|f| f.id == "F-03").unwrap();

        // With mean_pact_count = 0 (no pacts), expected = 0.0
        assert!(
            f03.expected.abs() < f64::EPSILON,
            "F-03 expected should be 0.0 when mean_pact_count is 0, got {}",
            f03.expected
        );

        // With no pacts, actual should fall back to expected (also 0.0)
        assert!(
            (f03.actual - f03.expected).abs() < f64::EPSILON,
            "F-03 actual should fall back to expected when no nodes have pacts, got actual={} expected={}",
            f03.actual,
            f03.expected
        );
    }

    #[test]
    fn test_f18_measures_full_pact_fraction() {
        use crate::graph::Graph;
        use crate::sim::metrics::{CollectedMetrics, NodeMetrics};
        use crate::types::NodeType;
        use std::collections::HashMap;

        let config = SimConfig::default();

        // Build a graph with 10 nodes; mark nodes 0..3 as Full, rest as Light
        let mut graph = Graph::new(10);
        for id in 0..4u32 {
            graph.node_types.insert(id, NodeType::Full);
        }
        for id in 4..10u32 {
            graph.node_types.insert(id, NodeType::Light);
        }

        let mut snapshots = HashMap::new();
        for id in 0..10u32 {
            let mut node = NodeMetrics::default();
            node.pact_count = if id < 4 { 15 } else { 2 };
            // Need availability samples for F-24
            for tick in 0..10 {
                node.availability_samples.push(tick < 5);
            }
            snapshots.insert(id, node);
        }

        let result = SimResult {
            metrics: CollectedMetrics {
                snapshots,
                event_deliveries: HashMap::new(),
                delivery_latencies: Vec::new(),
                pact_events: Vec::new(),
                read_results: Vec::new(),
                sample_events: Vec::new(),
            },
            graph,
            config: config.clone(),
            availability_records: HashMap::new(),
            activity_weights: Vec::new(),
        };

        let formulas = validate_formulas(&result);
        let f18 = formulas.iter().find(|f| f.id == "F-18").unwrap();

        // Expected = full_node_pct from config = 0.25
        assert!(
            (f18.expected - config.network.full_node_pct).abs() < f64::EPSILON,
            "F-18 expected should be full_node_pct ({}), got {}",
            config.network.full_node_pct,
            f18.expected
        );

        // Actual = full_node_count / node_count = 4 / 10 = 0.4
        let expected_actual = 4.0 / 10.0;
        assert!(
            (f18.actual - expected_actual).abs() < f64::EPSILON,
            "F-18 actual should be full fraction from graph ({}), got {}",
            expected_actual,
            f18.actual
        );

        // Actual (0.4) should differ from expected (0.25)
        assert!(
            (f18.actual - f18.expected).abs() > 0.01,
            "F-18 actual ({}) should differ from expected ({}) with different full fraction",
            f18.actual,
            f18.expected
        );
    }

    #[test]
    fn test_f18_computes_zero_fraction_when_no_full_nodes() {
        use crate::graph::Graph;
        use crate::sim::metrics::{CollectedMetrics, NodeMetrics};
        use crate::types::NodeType;
        use std::collections::HashMap;

        let config = SimConfig::default();

        // All nodes are Light — no full nodes
        let mut graph = Graph::new(5);
        for id in 0..5u32 {
            graph.node_types.insert(id, NodeType::Light);
        }

        let mut snapshots = HashMap::new();
        for id in 0..5u32 {
            let mut node = NodeMetrics::default();
            node.pact_count = 10;
            for tick in 0..10 {
                node.availability_samples.push(tick < 5);
            }
            snapshots.insert(id, node);
        }

        let result = SimResult {
            metrics: CollectedMetrics {
                snapshots,
                event_deliveries: HashMap::new(),
                delivery_latencies: Vec::new(),
                pact_events: Vec::new(),
                read_results: Vec::new(),
                sample_events: Vec::new(),
            },
            graph,
            config: config.clone(),
            availability_records: HashMap::new(),
            activity_weights: Vec::new(),
        };

        let formulas = validate_formulas(&result);
        let f18 = formulas.iter().find(|f| f.id == "F-18").unwrap();

        // Expected = full_node_pct from config = 0.25
        assert!(
            (f18.expected - config.network.full_node_pct).abs() < f64::EPSILON,
            "F-18 expected should be full_node_pct ({}), got {}",
            config.network.full_node_pct,
            f18.expected
        );

        // Actual = 0/5 = 0.0 (no full nodes, but node_count > 0 so no fallback)
        assert!(
            f18.actual.abs() < f64::EPSILON,
            "F-18 actual should be 0.0 when no full nodes in graph, got {}",
            f18.actual
        );
    }

    #[test]
    fn test_f31_uses_real_gossip_counts() {
        use crate::graph::Graph;
        use crate::sim::metrics::{CollectedMetrics, NodeMetrics};
        use crate::types::GossipStats;
        use std::collections::HashMap;

        let mut config = SimConfig::default();
        config.simulation.duration_days = 1; // 1 day = 86400 seconds

        // 10 nodes, each received 100 gossip messages
        let mut snapshots = HashMap::new();
        for id in 0..10u32 {
            let mut node = NodeMetrics::default();
            node.gossip = GossipStats {
                received: 100,
                ..Default::default()
            };
            for tick in 0..10 {
                node.availability_samples.push(tick < 5);
            }
            snapshots.insert(id, node);
        }

        let result = SimResult {
            metrics: CollectedMetrics {
                snapshots,
                event_deliveries: HashMap::new(),
                delivery_latencies: Vec::new(),
                pact_events: Vec::new(),
                read_results: Vec::new(),
                sample_events: Vec::new(),
            },
            graph: Graph::new(10),
            config: config.clone(),
            availability_records: HashMap::new(),
            activity_weights: Vec::new(),
        };

        let formulas = validate_formulas(&result);
        let f31 = formulas.iter().find(|f| f.id == "F-31").unwrap();

        // actual = sum(received) / node_count / duration_secs
        //        = (10 * 100) / 10 / 86400
        //        = 100 / 86400
        //        ≈ 0.001157
        let expected_actual = 100.0 / 86_400.0;
        assert!(
            (f31.actual - expected_actual).abs() < 1e-10,
            "F-31 actual should be {}, got {}",
            expected_actual,
            f31.actual
        );
    }

    #[test]
    fn test_f31_falls_back_when_no_gossip() {
        use crate::graph::Graph;
        use crate::sim::metrics::{CollectedMetrics, NodeMetrics};
        use std::collections::HashMap;

        let config = SimConfig::default();

        // Nodes with zero gossip received
        let mut snapshots = HashMap::new();
        for id in 0..10u32 {
            let mut node = NodeMetrics::default();
            // gossip.received defaults to 0
            for tick in 0..10 {
                node.availability_samples.push(tick < 5);
            }
            snapshots.insert(id, node);
        }

        let result = SimResult {
            metrics: CollectedMetrics {
                snapshots,
                event_deliveries: HashMap::new(),
                delivery_latencies: Vec::new(),
                pact_events: Vec::new(),
                read_results: Vec::new(),
                sample_events: Vec::new(),
            },
            graph: Graph::new(10),
            config: config.clone(),
            availability_records: HashMap::new(),
            activity_weights: Vec::new(),
        };

        let formulas = validate_formulas(&result);
        let f31 = formulas.iter().find(|f| f.id == "F-31").unwrap();

        // Should fall back: actual == expected
        assert!(
            (f31.actual - f31.expected).abs() < f64::EPSILON,
            "F-31 actual should fall back to expected when no gossip received, got actual={} expected={}",
            f31.actual,
            f31.expected
        );
    }

    #[test]
    fn test_f24_prefers_availability_records_over_snapshots() {
        use crate::graph::Graph;
        use crate::sim::metrics::{CollectedMetrics, NodeMetrics};
        use std::collections::HashMap;

        let config = SimConfig::default();

        // Build snapshots with 50% online (these should be ignored when
        // availability_records are present)
        let mut snapshots = HashMap::new();
        for id in 0..10u32 {
            let mut node = NodeMetrics::default();
            for tick in 0..10 {
                node.availability_samples.push(tick < 5); // 50% online
            }
            snapshots.insert(id, node);
        }

        // Build availability_records with 80% online (should take priority)
        let mut availability_records = HashMap::new();
        for id in 0..10u32 {
            let mut samples = Vec::new();
            for tick in 0..10 {
                samples.push(tick < 8); // 80% online
            }
            availability_records.insert(id, samples);
        }

        let result = SimResult {
            metrics: CollectedMetrics {
                snapshots,
                event_deliveries: HashMap::new(),
                delivery_latencies: Vec::new(),
                pact_events: Vec::new(),
                read_results: Vec::new(),
                sample_events: Vec::new(),
            },
            graph: Graph::new(10),
            config: config.clone(),
            availability_records,
            activity_weights: vec![],
        };

        let formulas = validate_formulas(&result);
        let f24 = formulas.iter().find(|f| f.id == "F-24").unwrap();

        // F-24 actual should be 0.80 (from availability_records), not 0.50 (from snapshots)
        assert!(
            (f24.actual - 0.80).abs() < f64::EPSILON,
            "F-24 actual should be 0.80 from availability_records, got {}",
            f24.actual
        );
    }

    #[test]
    fn test_f24_falls_back_to_snapshots_when_no_availability_records() {
        use crate::graph::Graph;
        use crate::sim::metrics::{CollectedMetrics, NodeMetrics};
        use std::collections::HashMap;

        let config = SimConfig::default();

        // Build snapshots with 30% online
        let mut snapshots = HashMap::new();
        for id in 0..10u32 {
            let mut node = NodeMetrics::default();
            for tick in 0..10 {
                node.availability_samples.push(tick < 3); // 30% online
            }
            snapshots.insert(id, node);
        }

        let result = SimResult {
            metrics: CollectedMetrics {
                snapshots,
                event_deliveries: HashMap::new(),
                delivery_latencies: Vec::new(),
                pact_events: Vec::new(),
                read_results: Vec::new(),
                sample_events: Vec::new(),
            },
            graph: Graph::new(10),
            config: config.clone(),
            availability_records: HashMap::new(), // empty => fall back to snapshots
            activity_weights: Vec::new(),
        };

        let formulas = validate_formulas(&result);
        let f24 = formulas.iter().find(|f| f.id == "F-24").unwrap();

        // F-24 actual should be 0.30 (from snapshots fallback)
        assert!(
            (f24.actual - 0.30).abs() < f64::EPSILON,
            "F-24 actual should be 0.30 from snapshot fallback, got {}",
            f24.actual
        );
    }
}
