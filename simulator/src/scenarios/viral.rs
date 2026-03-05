use crate::config::SimConfig;

// ── Params ──────────────────────────────────────────────────────────

pub struct ViralParams {
    /// Number of concurrent viewers requesting the viral content.
    pub viewers: u32,
    /// Time window over which the viral burst occurs (minutes).
    pub window_minutes: u32,
}

// ── Result ──────────────────────────────────────────────────────────

pub struct ViralResult {
    /// Peak request rate per storage peer (req/s).
    pub peak_peer_load: f64,
    /// Seconds until read-cache dominates serving (> 50% from cache).
    pub cache_takeover_secs: f64,
    /// Peak bandwidth spike across all serving peers (Mbps).
    pub bandwidth_spike_mbps: f64,
    /// Percentage of requests served from read-cache (0..100).
    pub cache_serve_pct: f64,
}

// ── Run ─────────────────────────────────────────────────────────────

/// Run the viral content stress scenario.
///
/// This is a formula-based computation validating F-39 through F-42 from
/// the plausibility analysis. No full simulation is needed.
///
/// Formulas:
/// - F-39: peak_requests_per_sec = viewers / window_seconds
/// - F-40: peak_peer_load = peak_rps / (pacts_default * online_fraction)
/// - F-41: cache_takeover_secs = latency_ms_mean * ln(viewers) / 1000
/// - F-42: cache_serve_pct = 1 - 1/pacts_default (asymptotic)
/// - bandwidth = peak_rps * avg_event_size * 8 / 1_000_000 (Mbps)
pub async fn run_viral(config: SimConfig, params: ViralParams) -> ViralResult {
    let result = compute_viral(&config, &params);
    print_results(&result, &params, &config);
    result
}

/// Pure computation of the viral scenario metrics.
pub fn compute_viral(config: &SimConfig, params: &ViralParams) -> ViralResult {
    let window_secs = params.window_minutes as f64 * 60.0;
    let viewers = params.viewers as f64;

    // F-39: peak request rate (req/s)
    let peak_rps = viewers / window_secs;

    // F-40: load per storage peer
    // Each piece of content is stored by pacts_default peers.
    // Only online_fraction of them are available at any time.
    let serving_peers =
        config.protocol.pacts_default as f64 * config.online_fraction();
    let peak_peer_load = if serving_peers > 0.0 {
        peak_rps / serving_peers
    } else {
        peak_rps
    };

    // F-41: cache takeover time
    // After the first request hits origin, subsequent requests from the
    // same region hit cache. Modelled as latency * ln(viewers).
    let cache_takeover_secs =
        config.simulation.latency_ms_mean * (viewers.max(1.0).ln()) / 1000.0;

    // F-42: asymptotic cache serve percentage
    // With pacts_default replicas, after cache warm-up only 1/pacts_default
    // requests need origin. The rest are served from cache.
    let cache_serve_pct = if config.protocol.pacts_default > 0 {
        (1.0 - 1.0 / config.protocol.pacts_default as f64) * 100.0
    } else {
        0.0
    };

    // Bandwidth spike: peak_rps * avg_event_size bytes * 8 bits/byte / 1e6
    let bandwidth_spike_mbps = peak_rps * config.avg_event_size() * 8.0 / 1_000_000.0;

    ViralResult {
        peak_peer_load,
        cache_takeover_secs,
        bandwidth_spike_mbps,
        cache_serve_pct,
    }
}

// ── Print ───────────────────────────────────────────────────────────

fn print_results(result: &ViralResult, params: &ViralParams, config: &SimConfig) {
    println!();
    println!(
        "\u{2501}\u{2501} Viral Content Scenario ({} nodes, seed={}) \u{2501}\u{2501}",
        config.graph.nodes, config.graph.seed
    );
    println!();
    println!("  Viewers:                  {}", params.viewers);
    println!("  Window:                   {} minutes", params.window_minutes);
    println!(
        "  Peak request rate:        {:.2} req/s",
        params.viewers as f64 / (params.window_minutes as f64 * 60.0)
    );
    println!(
        "  Serving peers (online):   {:.1}",
        config.protocol.pacts_default as f64 * config.online_fraction()
    );
    println!();
    println!(
        "  Peak peer load:           {:.4} req/s per peer",
        result.peak_peer_load
    );
    println!(
        "  Cache takeover time:      {:.2} s",
        result.cache_takeover_secs
    );
    println!(
        "  Bandwidth spike:          {:.4} Mbps",
        result.bandwidth_spike_mbps
    );
    println!(
        "  Cache serve percentage:   {:.1}%",
        result.cache_serve_pct
    );
    println!();

    // Verdict
    if result.peak_peer_load < 10.0 {
        println!("  Verdict: Load is manageable ({:.2} req/s per peer).", result.peak_peer_load);
    } else if result.peak_peer_load < 100.0 {
        println!(
            "  Verdict: Moderate load ({:.2} req/s per peer). Cache helps after {:.1}s.",
            result.peak_peer_load, result.cache_takeover_secs
        );
    } else {
        println!(
            "  Verdict: HIGH load ({:.2} req/s per peer). Cache critical, takeover in {:.1}s.",
            result.peak_peer_load, result.cache_takeover_secs
        );
    }

    println!();
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_viral_result_construction() {
        let result = ViralResult {
            peak_peer_load: 1.5,
            cache_takeover_secs: 0.46,
            bandwidth_spike_mbps: 0.012,
            cache_serve_pct: 95.0,
        };
        assert!(result.peak_peer_load > 0.0);
        assert!(result.cache_takeover_secs > 0.0);
        assert!(result.bandwidth_spike_mbps > 0.0);
        assert!(result.cache_serve_pct > 0.0);
    }

    #[test]
    fn test_compute_viral_defaults() {
        let config = SimConfig::default();
        let params = ViralParams {
            viewers: 10_000,
            window_minutes: 60,
        };

        let result = compute_viral(&config, &params);

        // peak_rps = 10000 / 3600 ~ 2.78
        let expected_rps = 10_000.0 / 3600.0;
        assert!((result.peak_peer_load - expected_rps / (20.0 * 0.6875)).abs() < 0.01);

        // cache_serve_pct = (1 - 1/20) * 100 = 95%
        assert!((result.cache_serve_pct - 95.0).abs() < f64::EPSILON);

        // cache_takeover_secs = 50ms * ln(10000) / 1000
        let expected_takeover = 50.0 * (10_000.0_f64).ln() / 1000.0;
        assert!((result.cache_takeover_secs - expected_takeover).abs() < 0.001);

        // bandwidth > 0
        assert!(result.bandwidth_spike_mbps > 0.0);
    }

    #[tokio::test]
    async fn test_run_viral_smoke() {
        let config = SimConfig::default();
        let params = ViralParams {
            viewers: 1000,
            window_minutes: 10,
        };

        let result = run_viral(config, params).await;
        assert!(result.peak_peer_load > 0.0);
        assert!(result.cache_serve_pct > 0.0);
    }
}
