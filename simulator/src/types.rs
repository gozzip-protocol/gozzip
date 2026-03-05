use serde::{Deserialize, Serialize};
use std::fmt;

// ── Type aliases ──────────────────────────────────────────────────────

pub type NodeId = u32;
pub type SimTime = f64; // seconds since sim start
pub type Bytes = u64;

// ── NodeType ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NodeType {
    Full,
    Light,
}

// ── EventKind ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EventKind {
    Note,
    Reaction,
    Repost,
    Dm,
    LongForm,
}

// ── DeliveryPath ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum DeliveryPath {
    CachedEndpoint,
    Gossip,
    Relay,
    ReadCache,
}

// ── ReadTier ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum ReadTier {
    Instant,
    CachedEndpoint,
    Gossip,
    Relay,
    Failed,
}

// ── WotTier ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WotTier {
    InnerCircle,   // Mutual follow — pact-stored, continuous sync
    Orbit,         // High-interaction + socially-endorsed authors
    Horizon,       // 2-hop graph + relay discoveries
}

// ── Event ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub id: u64,
    pub author: NodeId,
    pub kind: EventKind,
    pub size_bytes: u32,
    pub seq: u64,
    pub prev_hash: u64,
    pub created_at: SimTime,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nostr_json: Option<String>,
    /// For Reaction/Repost events, the author being interacted with.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interaction_target: Option<NodeId>,
}

// ── Message ───────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum Message {
    ReadRequest { target_author: NodeId, request_id: u64, wot_tier: WotTier },
    Publish(Event),
    RequestData { from: NodeId, request_id: u64, ttl: u8, filter: String },
    DataOffer { events: Vec<Event> },
    DeliverEvents { from: NodeId, events: Vec<Event>, path: DeliveryPath, request_id: Option<u64> },
    PactRequest { from: NodeId, volume_bytes: Bytes, as_standby: bool, created_at: SimTime, activity_tier: u8 },
    PactOffer { pact: Pact },
    PactAccept { pact: Pact },
    PactDrop { partner: NodeId },
    Challenge { from: NodeId, nonce: u64 },
    ChallengeResponse { from: NodeId, proof: u64 },
    Tick(SimTime),
    GoOnline,
    GoOffline,
    Shutdown,
}

// ── Pact ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pact {
    pub partner: NodeId,
    pub volume_bytes: Bytes,
    pub formed_at: SimTime,
    pub is_standby: bool,
}

// ── Checkpoint ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Checkpoint {
    pub merkle_root: u64,
    pub event_count: u64,
    pub created_at: SimTime,
    pub per_device_heads: Vec<(NodeId, u64)>,
}

// ── BandwidthByCategory ───────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize)]
pub struct BandwidthByCategory {
    pub gossip_up: Bytes,
    pub gossip_down: Bytes,
    pub pact_up: Bytes,
    pub pact_down: Bytes,
    pub challenge_up: Bytes,
    pub challenge_down: Bytes,
    pub cache_up: Bytes,
    pub cache_down: Bytes,
    pub publish_up: Bytes,
    pub fetch_down: Bytes,
}

// ── BandwidthCounter ──────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize)]
pub struct BandwidthCounter {
    pub upload_bytes: Bytes,
    pub download_bytes: Bytes,
    pub by_category: BandwidthByCategory,
}

impl BandwidthCounter {
    pub fn record_upload(&mut self, bytes: Bytes) {
        self.upload_bytes += bytes;
    }

    pub fn record_download(&mut self, bytes: Bytes) {
        self.download_bytes += bytes;
    }
}

// ── GossipStats ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize)]
pub struct GossipStats {
    pub sent: u64,
    pub received: u64,
    pub forwarded: u64,
    pub deduplicated: u64,
    pub rate_limited: u64,
    pub wot_filtered: u64,
}

// ── ChallengeStats ────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize)]
pub struct ChallengeStats {
    pub sent: u64,
    pub received: u64,
    pub passed: u64,
    pub failed: u64,
}

// ── CacheStats ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
}

// ── FormulaStatus ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum FormulaStatus {
    Pass,
    Warn,
    Fail,
}

impl fmt::Display for FormulaStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FormulaStatus::Pass => write!(f, "PASS"),
            FormulaStatus::Warn => write!(f, "WARN"),
            FormulaStatus::Fail => write!(f, "FAIL"),
        }
    }
}

// ── FormulaResult ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct FormulaResult {
    pub id: String,
    pub name: String,
    pub expected: f64,
    pub actual: f64,
    pub deviation_pct: f64,
    pub status: FormulaStatus,
}

impl FormulaResult {
    /// Create a new FormulaResult with default thresholds (15% pass, 30% warn).
    ///
    /// Status thresholds:
    /// - Pass: deviation <= 15%
    /// - Warn: deviation <= 30%
    /// - Fail: deviation > 30%
    ///
    /// When expected is zero, deviation is based on the absolute actual value.
    pub fn new(id: impl Into<String>, name: impl Into<String>, expected: f64, actual: f64) -> Self {
        Self::with_thresholds(id, name, expected, actual, 15.0, 30.0)
    }

    /// Create a new FormulaResult with custom pass/warn thresholds.
    pub fn with_thresholds(
        id: impl Into<String>,
        name: impl Into<String>,
        expected: f64,
        actual: f64,
        pass_pct: f64,
        warn_pct: f64,
    ) -> Self {
        let deviation_pct = if expected.abs() < f64::EPSILON {
            actual.abs() * 100.0
        } else if expected.abs() < 1e-3 && actual.abs() < 1e-3 {
            // Both values are negligibly small — relative comparison is
            // meaningless (e.g. P(all pacts offline) ≈ 1e-9 vs 1e-6).
            // The fact that both are essentially zero IS the validation.
            0.0
        } else {
            ((actual - expected) / expected).abs() * 100.0
        };

        let status = if deviation_pct <= pass_pct {
            FormulaStatus::Pass
        } else if deviation_pct <= warn_pct {
            FormulaStatus::Warn
        } else {
            FormulaStatus::Fail
        };

        Self {
            id: id.into(),
            name: name.into(),
            expected,
            actual,
            deviation_pct,
            status,
        }
    }
}

impl fmt::Display for FormulaResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[{}] {} — expected: {:.2}, actual: {:.2}, deviation: {:.1}% ({})",
            self.id, self.name, self.expected, self.actual, self.deviation_pct, self.status
        )
    }
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_formula_result_pass() {
        let result = FormulaResult::new("F1", "Latency", 100.0, 103.0);
        assert_eq!(result.status, FormulaStatus::Pass);
        assert!((result.deviation_pct - 3.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_formula_result_warn() {
        // 20% deviation is Warn with 15/30 defaults
        let result = FormulaResult::new("F2", "Throughput", 100.0, 120.0);
        assert_eq!(result.status, FormulaStatus::Warn);
        assert!((result.deviation_pct - 20.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_formula_result_fail() {
        // 35% deviation is Fail with 15/30 defaults
        let result = FormulaResult::new("F3", "Bandwidth", 100.0, 135.0);
        assert_eq!(result.status, FormulaStatus::Fail);
        assert!((result.deviation_pct - 35.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_formula_result_with_thresholds() {
        // 20% deviation is Warn with 15/30 thresholds
        let result = FormulaResult::with_thresholds("F5", "Test", 100.0, 120.0, 15.0, 30.0);
        assert_eq!(result.status, FormulaStatus::Warn);

        // Same 20% deviation is Fail with old 5/15 thresholds
        let result_strict = FormulaResult::with_thresholds("F5", "Test", 100.0, 120.0, 5.0, 15.0);
        assert_eq!(result_strict.status, FormulaStatus::Fail);
    }

    #[test]
    fn test_formula_result_zero_expected() {
        let result = FormulaResult::new("F4", "Error rate", 0.0, 0.03);
        // When expected is 0, deviation = actual.abs() * 100 = 3.0%
        assert_eq!(result.status, FormulaStatus::Pass);
        assert!((result.deviation_pct - 3.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_bandwidth_counter() {
        let mut counter = BandwidthCounter::default();
        counter.record_upload(100);
        counter.record_upload(200);
        counter.record_download(500);
        assert_eq!(counter.upload_bytes, 300);
        assert_eq!(counter.download_bytes, 500);
    }

    #[test]
    fn test_event_creation() {
        let event = Event {
            id: 1,
            author: 42,
            kind: EventKind::Note,
            size_bytes: 256,
            seq: 0,
            prev_hash: 0,
            created_at: 1.5,
            nostr_json: None,
            interaction_target: None,
        };
        assert_eq!(event.id, 1);
        assert_eq!(event.author, 42);
        assert_eq!(event.kind, EventKind::Note);
        assert_eq!(event.size_bytes, 256);
        assert_eq!(event.seq, 0);
        assert_eq!(event.prev_hash, 0);
        assert!((event.created_at - 1.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_wot_tier_variants() {
        // Verify the feed-model variant names exist and are distinct
        let tiers = [WotTier::InnerCircle, WotTier::Orbit, WotTier::Horizon];
        assert_eq!(tiers.len(), 3);
        assert_ne!(tiers[0], tiers[1]);
        assert_ne!(tiers[1], tiers[2]);
        assert_ne!(tiers[0], tiers[2]);

        // Debug formatting should reflect new names
        assert_eq!(format!("{:?}", WotTier::InnerCircle), "InnerCircle");
        assert_eq!(format!("{:?}", WotTier::Orbit), "Orbit");
        assert_eq!(format!("{:?}", WotTier::Horizon), "Horizon");
    }

    #[test]
    fn test_pact_creation() {
        let pact = Pact {
            partner: 7,
            volume_bytes: 1_000_000,
            formed_at: 10.0,
            is_standby: false,
        };
        assert_eq!(pact.partner, 7);
        assert_eq!(pact.volume_bytes, 1_000_000);
        assert!((pact.formed_at - 10.0).abs() < f64::EPSILON);
        assert!(!pact.is_standby);
    }
}
