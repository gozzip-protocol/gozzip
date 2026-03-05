use std::collections::{HashMap, HashSet};
use std::num::NonZeroUsize;

use lru::LruCache;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

use crate::config::SimConfig;
use crate::node::gossip::RateLimiter;
use crate::types::{
    BandwidthCounter, Bytes, CacheStats, ChallengeStats, Checkpoint, DeliveryPath, Event,
    GossipStats, NodeId, NodeType, Pact, SimTime, WotTier,
};

// ── DeliveryRecord ──────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct DeliveryRecord {
    pub event_id: u64,
    pub published_at: SimTime,
    pub delivered_to: Vec<(NodeId, SimTime, DeliveryPath)>,
}

// ── NodeState ───────────────────────────────────────────────────────

pub struct NodeState {
    pub id: NodeId,
    pub node_type: NodeType,
    pub follows: HashSet<NodeId>,
    pub followers: HashSet<NodeId>,
    pub active_pacts: Vec<Pact>,
    pub standby_pacts: Vec<Pact>,
    pub stored_events: HashMap<NodeId, Vec<Event>>,
    pub read_cache: LruCache<NodeId, (Vec<Event>, SimTime)>,
    pub online: bool,
    pub reliability_scores: HashMap<NodeId, f64>,
    pub seq_counter: u64,
    pub checkpoint: Option<Checkpoint>,
    pub own_events: Vec<Event>,
    pub seen_request_ids: LruCache<u64, ()>,
    pub bandwidth: BandwidthCounter,
    pub gossip_stats: GossipStats,
    pub challenge_stats: ChallengeStats,
    pub events_delivered: HashMap<u64, DeliveryRecord>,
    pub rng: ChaCha8Rng,
    pub pending_challenges: HashMap<NodeId, u64>,
    pub pending_reads: HashMap<NodeId, (u64, SimTime, bool, Option<NodeId>, WotTier)>,  // author → (request_id, request_time, relay_attempted, cached_endpoint_node, wot_tier)
    pub endpoint_cache: LruCache<NodeId, NodeId>,  // author → known storage peer
    pub cache_stats: CacheStats,
    pub rate_limiter: RateLimiter,
    pub last_checkpoint_time: SimTime,
    pub created_at: SimTime,
    /// Storage capacity in bytes (set by orchestrator with variance).
    pub storage_capacity: Bytes,
    /// Storage currently used in bytes.
    pub storage_used: Bytes,
    /// Time at which this node formed its first pact (None until first pact).
    pub first_pact_time: Option<SimTime>,
    /// Exponential moving average of this node's publishing rate (events/day).
    pub activity_rate: f64,
    /// Number of publish events since last activity check.
    pub events_since_last_check: u32,
    /// Last time activity rate was evaluated for renegotiation.
    pub last_activity_check: SimTime,
    /// Karma accounting state (only meaningful when karma.enabled = true).
    pub karma: super::karma::KarmaState,
    /// When nostr-events is active, holds this node's secret key bytes
    /// for reconstructing Nostr keys in the signing path.
    pub nostr_secret_key: Option<[u8; 32]>,
    /// Per-peer trust scores for gossip prioritization.
    pub wot_peer_scores: HashMap<NodeId, u32>,
}

impl NodeState {
    /// Create a new `NodeState` with default values, sizing LRU caches from config.
    pub fn new(id: NodeId, node_type: NodeType, config: &SimConfig) -> Self {
        // read_cache_max_mb -> approximate number of entries
        // We use a rough estimate: each cached author entry ~1 KB on average,
        // so max_mb * 1024 entries. Minimum 1 to satisfy NonZeroUsize.
        let read_cache_entries = (config.protocol.read_cache_max_mb as usize * 1024).max(1);
        let dedup_cache_size = config.protocol.dedup_cache_size.max(1);

        Self {
            id,
            node_type,
            follows: HashSet::new(),
            followers: HashSet::new(),
            active_pacts: Vec::new(),
            standby_pacts: Vec::new(),
            stored_events: HashMap::new(),
            read_cache: LruCache::new(NonZeroUsize::new(read_cache_entries).unwrap()),
            online: true,
            reliability_scores: HashMap::new(),
            seq_counter: 0,
            checkpoint: None,
            own_events: Vec::new(),
            seen_request_ids: LruCache::new(NonZeroUsize::new(dedup_cache_size).unwrap()),
            bandwidth: BandwidthCounter::default(),
            gossip_stats: GossipStats::default(),
            challenge_stats: ChallengeStats::default(),
            events_delivered: HashMap::new(),
            rng: ChaCha8Rng::seed_from_u64(id as u64),
            pending_challenges: HashMap::new(),
            pending_reads: HashMap::new(),
            endpoint_cache: LruCache::new(NonZeroUsize::new(256).unwrap()),
            cache_stats: CacheStats::default(),
            rate_limiter: RateLimiter::new(60.0),
            last_checkpoint_time: 0.0,
            created_at: 0.0,
            storage_capacity: 0,
            storage_used: 0,
            first_pact_time: None,
            activity_rate: 0.0,
            events_since_last_check: 0,
            last_activity_check: 0.0,
            karma: super::karma::KarmaState::new(0.0),
            nostr_secret_key: None,
            wot_peer_scores: HashMap::new(),
        }
    }

    /// Expected uptime fraction for this node type.
    pub fn uptime(&self, config: &SimConfig) -> f64 {
        match self.node_type {
            NodeType::Full => config.network.full_uptime,
            NodeType::Light => config.network.light_uptime,
        }
    }

    /// Number of active pacts.
    pub fn pact_count(&self) -> usize {
        self.active_pacts.len()
    }

    /// Number of standby pacts.
    pub fn standby_count(&self) -> usize {
        self.standby_pacts.len()
    }

    /// Total bytes stored across all stored_events entries.
    pub fn total_stored_bytes(&self) -> Bytes {
        self.stored_events
            .values()
            .flat_map(|events| events.iter())
            .map(|e| e.size_bytes as Bytes)
            .sum()
    }

    /// Bytes stored for a specific partner.
    pub fn stored_bytes_for(&self, partner: NodeId) -> Bytes {
        self.stored_events
            .get(&partner)
            .map(|events| events.iter().map(|e| e.size_bytes as Bytes).sum())
            .unwrap_or(0)
    }

    /// Available storage capacity in bytes.
    pub fn available_capacity(&self) -> Bytes {
        self.storage_capacity.saturating_sub(self.storage_used)
    }

    /// Storage capacity utilization as a fraction (0.0..1.0).
    pub fn capacity_utilization(&self) -> f64 {
        self.storage_used as f64 / self.storage_capacity.max(1) as f64
    }

    /// Activity tier based on events/day rate.
    /// Tiers: 1 = 0-10, 2 = 11-50, 3 = 51-200, 4 = 200+.
    pub fn activity_tier(&self) -> u8 {
        match self.activity_rate as u32 {
            0..=10 => 1,
            11..=50 => 2,
            51..=200 => 3,
            _ => 4,
        }
    }

    /// Returns true if `other` is in this node's follows or followers set (WoT peer).
    pub fn is_wot_peer(&self, other: NodeId) -> bool {
        self.follows.contains(&other) || self.followers.contains(&other)
    }

    /// Returns true if we have events for the given author in stored_events,
    /// read_cache, or own_events (if author == self.id).
    pub fn has_events_for(&self, author: NodeId) -> bool {
        if author == self.id && !self.own_events.is_empty() {
            return true;
        }
        if self
            .stored_events
            .get(&author)
            .map_or(false, |v| !v.is_empty())
        {
            return true;
        }
        self.read_cache.contains(&author)
    }

    /// Store events for a pact partner, tracking bandwidth.
    ///
    /// Light nodes prune events older than the checkpoint window;
    /// full nodes keep everything.
    pub fn store_events_for_pact(
        &mut self,
        partner: NodeId,
        events: Vec<Event>,
        current_time: SimTime,
        config: &SimConfig,
    ) {
        let total_bytes: Bytes = events.iter().map(|e| e.size_bytes as Bytes).sum();
        self.bandwidth.record_download(total_bytes);
        self.bandwidth.by_category.pact_down += total_bytes;

        // Track new storage
        self.storage_used = self.storage_used.saturating_add(total_bytes);

        let entry = self.stored_events.entry(partner).or_default();
        entry.extend(events);

        if self.node_type == NodeType::Light {
            let cutoff = current_time - config.protocol.checkpoint_window_secs();
            let before_bytes: Bytes = entry.iter().map(|e| e.size_bytes as Bytes).sum();
            entry.retain(|e| e.created_at >= cutoff);
            let after_bytes: Bytes = entry.iter().map(|e| e.size_bytes as Bytes).sum();
            self.storage_used = self.storage_used.saturating_sub(before_bytes - after_bytes);
        }
    }

    /// Cache events for an author in the read_cache LRU.
    pub fn cache_events(&mut self, author: NodeId, events: Vec<Event>, cached_at: SimTime) {
        let total_bytes: Bytes = events.iter().map(|e| e.size_bytes as Bytes).sum();
        self.bandwidth.record_download(total_bytes);
        self.bandwidth.by_category.cache_down += total_bytes;

        self.read_cache.put(author, (events, cached_at));
    }

    /// Evict cached entries older than the given TTL (in seconds).
    pub fn evict_expired_cache(&mut self, current_time: SimTime, ttl_secs: f64) {
        let mut expired = Vec::new();
        for (author, (_, cached_at)) in self.read_cache.iter() {
            if current_time - cached_at > ttl_secs {
                expired.push(*author);
            }
        }
        for author in expired {
            self.read_cache.pop(&author);
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::EventKind;

    fn make_event(id: u64, author: NodeId, size: u32) -> Event {
        make_event_at(id, author, size, 0.0)
    }

    #[test]
    fn test_node_state_new() {
        let config = SimConfig::default();
        let state = NodeState::new(42, NodeType::Full, &config);

        assert_eq!(state.id, 42);
        assert_eq!(state.node_type, NodeType::Full);
        assert!(state.follows.is_empty());
        assert!(state.followers.is_empty());
        assert!(state.active_pacts.is_empty());
        assert!(state.standby_pacts.is_empty());
        assert!(state.stored_events.is_empty());
        assert!(state.online);
        assert_eq!(state.seq_counter, 0);
        assert!(state.checkpoint.is_none());
        assert!(state.own_events.is_empty());
        assert_eq!(state.bandwidth.upload_bytes, 0);
        assert_eq!(state.bandwidth.download_bytes, 0);
        assert!((state.created_at - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_node_uptime() {
        let config = SimConfig::default();

        let full = NodeState::new(0, NodeType::Full, &config);
        assert!((full.uptime(&config) - 0.95).abs() < f64::EPSILON);

        let light = NodeState::new(1, NodeType::Light, &config);
        assert!((light.uptime(&config) - 0.60).abs() < f64::EPSILON);
    }

    #[test]
    fn test_store_events() {
        let config = SimConfig::default();
        let mut state = NodeState::new(1, NodeType::Full, &config);

        assert!(!state.has_events_for(99));

        let events = vec![
            make_event(1, 99, 256),
            make_event(2, 99, 512),
        ];
        state.store_events_for_pact(99, events, 0.0, &config);

        assert!(state.has_events_for(99));
        assert_eq!(state.total_stored_bytes(), 768);
        assert_eq!(state.bandwidth.download_bytes, 768);
    }

    #[test]
    fn test_new_state_has_empty_pending_challenges_and_rate_limiter() {
        let config = SimConfig::default();
        let state = NodeState::new(42, NodeType::Full, &config);

        assert!(state.pending_challenges.is_empty());
        assert!(state.rate_limiter.counters.is_empty());
        assert!((state.last_checkpoint_time - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_wot_peer() {
        let config = SimConfig::default();
        let mut state = NodeState::new(1, NodeType::Full, &config);

        state.follows.insert(10);
        state.followers.insert(20);

        assert!(state.is_wot_peer(10));
        assert!(state.is_wot_peer(20));
        assert!(!state.is_wot_peer(30));
    }

    fn make_event_at(id: u64, author: NodeId, size: u32, created_at: SimTime) -> Event {
        Event {
            id,
            author,
            kind: EventKind::Note,
            size_bytes: size,
            seq: 0,
            prev_hash: 0,
            created_at,
            nostr_json: None,
            interaction_target: None,
        }
    }

    #[test]
    fn test_light_node_prunes_old_events() {
        let config = SimConfig::default(); // checkpoint_window_days = 30
        let mut state = NodeState::new(1, NodeType::Light, &config);
        let window = config.protocol.checkpoint_window_days as f64 * 86_400.0;
        let now = window + 1000.0; // comfortably past the window

        let events = vec![
            make_event_at(1, 99, 256, 0.0),          // old — before cutoff
            make_event_at(2, 99, 512, now - 100.0),   // recent — within window
        ];
        state.store_events_for_pact(99, events, now, &config);

        // Only the recent event should survive
        assert!(state.has_events_for(99));
        let stored = state.stored_events.get(&99).unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].id, 2);
        assert_eq!(state.total_stored_bytes(), 512);
    }

    #[test]
    fn test_full_node_keeps_all_events() {
        let config = SimConfig::default();
        let mut state = NodeState::new(1, NodeType::Full, &config);
        let window = config.protocol.checkpoint_window_days as f64 * 86_400.0;
        let now = window + 1000.0;

        let events = vec![
            make_event_at(1, 99, 256, 0.0),          // old — before cutoff
            make_event_at(2, 99, 512, now - 100.0),   // recent — within window
        ];
        state.store_events_for_pact(99, events, now, &config);

        // Full node keeps everything
        assert!(state.has_events_for(99));
        let stored = state.stored_events.get(&99).unwrap();
        assert_eq!(stored.len(), 2);
        assert_eq!(state.total_stored_bytes(), 768);
    }
}
