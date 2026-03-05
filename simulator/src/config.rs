use serde::{Deserialize, Serialize};
use std::path::Path;

// ── SimConfig ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimConfig {
    pub protocol: ProtocolConfig,
    pub network: NetworkConfig,
    pub events: EventConfig,
    pub graph: GraphConfig,
    pub simulation: SimulationConfig,
    pub validation: ValidationConfig,
    pub latency: LatencyConfig,
    pub retrieval: RetrievalConfig,
    #[serde(default)]
    pub karma: KarmaConfig,
}

// ── ProtocolConfig ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolConfig {
    pub pacts_default: u32,
    pub pacts_standby: u32,
    pub pacts_popular: u32,
    pub volume_tolerance: f64,
    pub ttl: u8,
    pub checkpoint_window_days: u32,
    pub light_sync_depth: u32,
    pub dedup_cache_size: usize,
    pub rate_limit_10055: u32,
    pub rate_limit_10057: u32,
    pub wot_forward_hops: u32,
    pub read_cache_max_mb: u32,
    pub challenge_freq_per_day: u32,
    pub min_account_age_days: u32,
    pub gossip_fanout: u32,
    #[serde(default = "default_storage_capacity_mb")]
    pub default_storage_capacity_mb: u32,
    #[serde(default = "default_storage_capacity_variance")]
    pub storage_capacity_variance: f64,
    /// Activity delta % that triggers pact renegotiation (0.5 = 50%).
    #[serde(default = "default_activity_renegotiation_threshold")]
    pub activity_renegotiation_threshold: f64,
    /// How often to check for activity-based renegotiation (hours).
    #[serde(default = "default_activity_check_interval_hours")]
    pub activity_check_interval_hours: u32,
}

impl ProtocolConfig {
    /// Checkpoint window duration in seconds.
    pub fn checkpoint_window_secs(&self) -> f64 {
        self.checkpoint_window_days as f64 * 86_400.0
    }
}

// ── NetworkConfig ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub full_node_pct: f64,
    pub light_node_pct: f64,
    pub full_uptime: f64,
    pub light_uptime: f64,
    pub dau_pct: f64,
    pub app_sessions: u32,
    pub gossip_fallback: f64,
    pub clustering: f64,
}

// ── EventConfig ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventConfig {
    pub note_bytes: u32,
    pub reaction_bytes: u32,
    pub repost_bytes: u32,
    pub dm_bytes: u32,
    pub longform_bytes: u32,
    pub gossip_req_bytes: u32,
    pub challenge_bytes: u32,
    pub data_offer_bytes: u32,
    pub events_per_day: f64,
    pub mix: EventMixConfig,
    /// Author selection distribution: "uniform" or "power_law"
    #[serde(default = "default_activity_distribution")]
    pub activity_distribution: String,
    /// Zipf exponent for power_law distribution (higher = more skewed)
    #[serde(default = "default_activity_skew")]
    pub activity_skew: f64,
}

fn default_storage_capacity_mb() -> u32 {
    1024
}

fn default_storage_capacity_variance() -> f64 {
    0.3
}

fn default_activity_renegotiation_threshold() -> f64 {
    0.5
}

fn default_activity_check_interval_hours() -> u32 {
    24
}

fn default_activity_distribution() -> String {
    "uniform".to_string()
}

fn default_activity_skew() -> f64 {
    1.2
}

// ── EventMixConfig ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventMixConfig {
    pub note: f64,
    pub reaction: f64,
    pub repost: f64,
    pub dm: f64,
    pub longform: f64,
}

// ── GraphConfig ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphConfig {
    pub model: String,
    pub nodes: u32,
    pub seed: u64,
    pub ba_edges_per_node: u32,
    pub ws_neighbors: u32,
    pub ws_rewire_prob: f64,
}

// ── SimulationConfig ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationConfig {
    pub duration_days: u32,
    pub tick_interval_secs: u64,
    pub deterministic: bool,
    pub latency_ms_mean: f64,
    pub latency_ms_stddev: f64,
    #[serde(default)]
    pub streaming: StreamingConfig,
    #[serde(default)]
    pub nostr_events: bool,
}

// ── StreamingConfig ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamingConfig {
    #[serde(default)]
    pub live_ticks: bool,
    #[serde(default)]
    pub jsonl_path: String,
    #[serde(default = "default_flush_interval")]
    pub jsonl_flush_interval: u32,
}

fn default_flush_interval() -> u32 {
    10
}

impl Default for StreamingConfig {
    fn default() -> Self {
        Self {
            live_ticks: false,
            jsonl_path: String::new(),
            jsonl_flush_interval: default_flush_interval(),
        }
    }
}

// ── LatencyConfig ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyConfig {
    pub cached_endpoint_base_ms: f64,
    pub cached_endpoint_jitter_ms: f64,
    pub gossip_per_hop_base_ms: f64,
    pub gossip_per_hop_jitter_ms: f64,
    pub relay_base_ms: f64,
    pub relay_jitter_ms: f64,
}

// ── RetrievalConfig ───────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalConfig {
    pub reads_per_day: u32,
    pub read_timeout_secs: f64,
    pub relay_success_rate: f64,
    #[serde(default = "default_relay_stagger")]
    pub relay_stagger_secs: f64,
    #[serde(default = "default_feed_weight_inner_circle", alias = "wot_weight_direct")]
    pub feed_weight_inner_circle: f64,
    #[serde(default = "default_feed_weight_orbit", alias = "wot_weight_one_hop")]
    pub feed_weight_orbit: f64,
    #[serde(default = "default_feed_weight_horizon", alias = "wot_weight_two_hop")]
    pub feed_weight_horizon: f64,
    #[serde(default = "default_interaction_weight_reply")]
    pub interaction_weight_reply: f64,
    #[serde(default = "default_interaction_weight_repost")]
    pub interaction_weight_repost: f64,
    #[serde(default = "default_interaction_weight_reaction")]
    pub interaction_weight_reaction: f64,
    #[serde(default = "default_interaction_decay_days")]
    pub interaction_decay_days: f64,
    #[serde(default = "default_referral_min_contacts")]
    pub referral_min_contacts: u32,
    #[serde(default = "default_referral_min_score")]
    pub referral_min_score: f64,
    #[serde(default = "default_orbit_cache_ttl_days")]
    pub orbit_cache_ttl_days: f64,
    #[serde(default = "default_horizon_cache_ttl_days")]
    pub horizon_cache_ttl_days: f64,
    #[serde(default = "default_relay_cache_ttl_days")]
    pub relay_cache_ttl_days: f64,
}

fn default_relay_stagger() -> f64 {
    2.0
}

fn default_feed_weight_inner_circle() -> f64 { 0.60 }
fn default_feed_weight_orbit() -> f64 { 0.25 }
fn default_feed_weight_horizon() -> f64 { 0.15 }
fn default_interaction_weight_reply() -> f64 { 3.0 }
fn default_interaction_weight_repost() -> f64 { 2.0 }
fn default_interaction_weight_reaction() -> f64 { 1.0 }
fn default_interaction_decay_days() -> f64 { 30.0 }
fn default_referral_min_contacts() -> u32 { 3 }
fn default_referral_min_score() -> f64 { 5.0 }
fn default_orbit_cache_ttl_days() -> f64 { 14.0 }
fn default_horizon_cache_ttl_days() -> f64 { 3.0 }
fn default_relay_cache_ttl_days() -> f64 { 1.0 }

// ── ValidationConfig ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationConfig {
    pub pass_threshold_pct: f64,
    pub warn_threshold_pct: f64,
}

// ── KarmaConfig ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KarmaConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_earn_per_mb_day")]
    pub earn_per_mb_day: f64,
    #[serde(default = "default_cost_per_mb_stored")]
    pub cost_per_mb_stored: f64,
    #[serde(default = "default_initial_balance")]
    pub initial_balance: f64,
    #[serde(default = "default_minimum_balance_for_pact")]
    pub minimum_balance_for_pact: f64,
}

fn default_earn_per_mb_day() -> f64 { 1.0 }
fn default_cost_per_mb_stored() -> f64 { 0.5 }
fn default_initial_balance() -> f64 { 100.0 }
fn default_minimum_balance_for_pact() -> f64 { 10.0 }

impl Default for KarmaConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            earn_per_mb_day: default_earn_per_mb_day(),
            cost_per_mb_stored: default_cost_per_mb_stored(),
            initial_balance: default_initial_balance(),
            minimum_balance_for_pact: default_minimum_balance_for_pact(),
        }
    }
}

// ── Default impl ─────────────────────────────────────────────────────

impl Default for SimConfig {
    fn default() -> Self {
        Self {
            protocol: ProtocolConfig {
                pacts_default: 20,
                pacts_standby: 3,
                pacts_popular: 40,
                volume_tolerance: 0.30,
                ttl: 3,
                checkpoint_window_days: 30,
                light_sync_depth: 50,
                dedup_cache_size: 10_000,
                rate_limit_10055: 10,
                rate_limit_10057: 50,
                wot_forward_hops: 2,
                read_cache_max_mb: 100,
                challenge_freq_per_day: 1,
                min_account_age_days: 7,
                gossip_fanout: 8,
                default_storage_capacity_mb: 1024,
                storage_capacity_variance: 0.3,
                activity_renegotiation_threshold: 0.5,
                activity_check_interval_hours: 24,
            },
            network: NetworkConfig {
                full_node_pct: 0.25,
                light_node_pct: 0.75,
                full_uptime: 0.95,
                light_uptime: 0.60,
                dau_pct: 0.50,
                app_sessions: 10,
                gossip_fallback: 0.02,
                clustering: 0.25,
            },
            events: EventConfig {
                note_bytes: 800,
                reaction_bytes: 500,
                repost_bytes: 600,
                dm_bytes: 900,
                longform_bytes: 5500,
                gossip_req_bytes: 300,
                challenge_bytes: 300,
                data_offer_bytes: 200,
                events_per_day: 25.0,
                mix: EventMixConfig {
                    note: 0.40,
                    reaction: 0.30,
                    repost: 0.15,
                    dm: 0.10,
                    longform: 0.05,
                },
                activity_distribution: default_activity_distribution(),
                activity_skew: default_activity_skew(),
            },
            graph: GraphConfig {
                model: "barabasi-albert".to_string(),
                nodes: 10_000,
                seed: 42,
                ba_edges_per_node: 10,
                ws_neighbors: 20,
                ws_rewire_prob: 0.1,
            },
            simulation: SimulationConfig {
                duration_days: 30,
                tick_interval_secs: 60,
                deterministic: false,
                latency_ms_mean: 50.0,
                latency_ms_stddev: 20.0,
                streaming: StreamingConfig::default(),
                nostr_events: false,
            },
            validation: ValidationConfig {
                pass_threshold_pct: 15.0,
                warn_threshold_pct: 30.0,
            },
            latency: LatencyConfig {
                cached_endpoint_base_ms: 60.0,
                cached_endpoint_jitter_ms: 20.0,
                gossip_per_hop_base_ms: 80.0,
                gossip_per_hop_jitter_ms: 30.0,
                relay_base_ms: 200.0,
                relay_jitter_ms: 50.0,
            },
            retrieval: RetrievalConfig {
                reads_per_day: 50,
                read_timeout_secs: 30.0,
                relay_success_rate: 0.80,
                relay_stagger_secs: 2.0,
                feed_weight_inner_circle: 0.60,
                feed_weight_orbit: 0.25,
                feed_weight_horizon: 0.15,
                interaction_weight_reply: 3.0,
                interaction_weight_repost: 2.0,
                interaction_weight_reaction: 1.0,
                interaction_decay_days: 30.0,
                referral_min_contacts: 3,
                referral_min_score: 5.0,
                orbit_cache_ttl_days: 14.0,
                horizon_cache_ttl_days: 3.0,
                relay_cache_ttl_days: 1.0,
            },
            karma: KarmaConfig::default(),
        }
    }
}

// ── Methods ──────────────────────────────────────────────────────────

impl SimConfig {
    /// Load configuration from a TOML file at the given path.
    pub fn load(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let contents = std::fs::read_to_string(path)?;
        let config: SimConfig = toml::from_str(&contents)?;
        Ok(config)
    }

    /// Try to load from the given path; fall back to defaults with a warning.
    pub fn load_or_default(path: Option<&Path>) -> Self {
        match path {
            Some(p) => match Self::load(p) {
                Ok(cfg) => cfg,
                Err(e) => {
                    eprintln!("Warning: failed to load config from {}: {}", p.display(), e);
                    eprintln!("Using default configuration.");
                    Self::default()
                }
            },
            None => Self::default(),
        }
    }

    /// Formula F-01: weighted average event size in bytes.
    pub fn avg_event_size(&self) -> f64 {
        let e = &self.events;
        let m = &e.mix;
        (e.note_bytes as f64) * m.note
            + (e.reaction_bytes as f64) * m.reaction
            + (e.repost_bytes as f64) * m.repost
            + (e.dm_bytes as f64) * m.dm
            + (e.longform_bytes as f64) * m.longform
    }

    /// Formula F-24: fraction of nodes online at any instant.
    pub fn online_fraction(&self) -> f64 {
        let n = &self.network;
        n.full_node_pct * n.full_uptime + n.light_node_pct * n.light_uptime
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn test_default_config_values() {
        let cfg = SimConfig::default();

        // Protocol
        assert_eq!(cfg.protocol.pacts_default, 20);
        assert_eq!(cfg.protocol.pacts_standby, 3);
        assert_eq!(cfg.protocol.pacts_popular, 40);
        assert_relative_eq!(cfg.protocol.volume_tolerance, 0.30);
        assert_eq!(cfg.protocol.ttl, 3);
        assert_eq!(cfg.protocol.checkpoint_window_days, 30);
        assert_eq!(cfg.protocol.dedup_cache_size, 10_000);
        assert_eq!(cfg.protocol.gossip_fanout, 8);

        // Events
        assert_relative_eq!(cfg.events.events_per_day, 25.0);
        assert_eq!(cfg.events.activity_distribution, "uniform");
        assert_relative_eq!(cfg.events.activity_skew, 1.2);

        // Network
        assert_relative_eq!(cfg.network.full_node_pct, 0.25);
        assert_relative_eq!(cfg.network.light_node_pct, 0.75);
        assert_relative_eq!(cfg.network.full_uptime, 0.95);
        assert_relative_eq!(cfg.network.light_uptime, 0.60);

        // Graph
        assert_eq!(cfg.graph.model, "barabasi-albert");
        assert_eq!(cfg.graph.nodes, 10_000);
        assert_eq!(cfg.graph.seed, 42);

        // Simulation
        assert_eq!(cfg.simulation.duration_days, 30);
        assert_eq!(cfg.simulation.tick_interval_secs, 60);
        assert!(!cfg.simulation.deterministic);

        // Validation
        assert_relative_eq!(cfg.validation.pass_threshold_pct, 15.0);
        assert_relative_eq!(cfg.validation.warn_threshold_pct, 30.0);

        // Latency
        assert_relative_eq!(cfg.latency.cached_endpoint_base_ms, 60.0);
        assert_relative_eq!(cfg.latency.cached_endpoint_jitter_ms, 20.0);
        assert_relative_eq!(cfg.latency.gossip_per_hop_base_ms, 80.0);
        assert_relative_eq!(cfg.latency.gossip_per_hop_jitter_ms, 30.0);
        assert_relative_eq!(cfg.latency.relay_base_ms, 200.0);
        assert_relative_eq!(cfg.latency.relay_jitter_ms, 50.0);

        // Retrieval
        assert_eq!(cfg.retrieval.reads_per_day, 50);
        assert_relative_eq!(cfg.retrieval.read_timeout_secs, 30.0);
        assert_relative_eq!(cfg.retrieval.relay_success_rate, 0.80);
        assert_relative_eq!(cfg.retrieval.feed_weight_inner_circle, 0.60);
        assert_relative_eq!(cfg.retrieval.feed_weight_orbit, 0.25);
        assert_relative_eq!(cfg.retrieval.feed_weight_horizon, 0.15);
    }

    #[test]
    fn test_avg_event_size_f01() {
        let cfg = SimConfig::default();
        // 800*0.40 + 500*0.30 + 600*0.15 + 900*0.10 + 5500*0.05
        // = 320 + 150 + 90 + 90 + 275 = 925.0
        assert_relative_eq!(cfg.avg_event_size(), 925.0);
    }

    #[test]
    fn test_online_fraction_f24() {
        let cfg = SimConfig::default();
        // 0.25*0.95 + 0.75*0.60 = 0.2375 + 0.45 = 0.6875
        assert_relative_eq!(cfg.online_fraction(), 0.6875);
    }

    #[test]
    fn test_roundtrip_toml() {
        let original = SimConfig::default();
        let toml_str = toml::to_string(&original).expect("serialize to TOML");
        let parsed: SimConfig = toml::from_str(&toml_str).expect("parse from TOML");

        assert_eq!(parsed.protocol.pacts_default, original.protocol.pacts_default);
        assert_eq!(parsed.graph.model, original.graph.model);
        assert_eq!(parsed.graph.nodes, original.graph.nodes);
        assert_eq!(parsed.graph.seed, original.graph.seed);
        assert_relative_eq!(parsed.events.mix.note, original.events.mix.note);
        assert_relative_eq!(parsed.network.full_uptime, original.network.full_uptime);
        assert_eq!(parsed.simulation.duration_days, original.simulation.duration_days);
        assert_relative_eq!(parsed.validation.pass_threshold_pct, original.validation.pass_threshold_pct);
    }
}
