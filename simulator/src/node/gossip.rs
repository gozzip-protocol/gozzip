use std::collections::HashMap;

use crate::config::SimConfig;
use crate::types::{NodeId, SimTime};

use super::state::NodeState;

// ── ForwardDecision ────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForwardDecision {
    Forward,
    ServeLocallyOnly,
}

// ── RateLimiter ────────────────────────────────────────────────────

/// Per-source rate limiter with a sliding window.
///
/// Tracks (window_start, count) per source NodeId. When the window
/// expires the counter resets.
pub struct RateLimiter {
    pub counters: HashMap<NodeId, (SimTime, u32)>,
    pub window_secs: f64,
}

impl RateLimiter {
    pub fn new(window_secs: f64) -> Self {
        Self {
            counters: HashMap::new(),
            window_secs,
        }
    }

    /// Returns `true` if the request is within the rate limit.
    ///
    /// Resets the window when it has expired. Increments the counter
    /// and returns `false` if the limit has been exceeded.
    pub fn check(&mut self, source: NodeId, now: SimTime, limit: u32) -> bool {
        let entry = self.counters.entry(source).or_insert((now, 0));

        // Reset window if expired
        if now - entry.0 >= self.window_secs {
            *entry = (now, 0);
        }

        entry.1 += 1;
        entry.1 <= limit
    }
}

// ── Functions ──────────────────────────────────────────────────────

/// Decide whether to forward a gossip request or only serve it locally.
///
/// If the source is a WoT peer, forward the request to other peers.
/// Otherwise, only serve data we already have locally.
pub fn should_forward(
    node: &NodeState,
    from: NodeId,
    _request_id: u64,
    _config: &SimConfig,
) -> ForwardDecision {
    if node.is_wot_peer(from) {
        ForwardDecision::Forward
    } else if node.wot_peer_scores.contains_key(&from) {
        ForwardDecision::Forward
    } else {
        ForwardDecision::ServeLocallyOnly
    }
}

/// Calculate cumulative gossip reach per hop.
///
/// At each hop, the new nodes reached is:
///   `previous_frontier * degree * (1 - clustering)`
///
/// Returns a vec of cumulative reach at each hop (1..=ttl).
pub fn gossip_reach(degree: usize, ttl: u8, clustering: f64) -> Vec<usize> {
    let mut cumulative = Vec::with_capacity(ttl as usize);
    let mut total: f64 = 0.0;
    let mut frontier: f64 = degree as f64;

    for _ in 0..ttl {
        total += frontier;
        cumulative.push(total as usize);
        // Each node in the frontier fans out to `degree` peers,
        // but `clustering` fraction are duplicates (already reached).
        frontier *= (degree as f64) * (1.0 - clustering);
    }

    cumulative
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SimConfig;
    use crate::types::NodeType;

    use super::super::state::NodeState;

    #[test]
    fn test_rate_limiter_allows() {
        let mut limiter = RateLimiter::new(60.0);
        // Fresh requests should pass
        assert!(limiter.check(1, 0.0, 50));
        assert!(limiter.check(1, 0.0, 50));
        assert!(limiter.check(1, 0.0, 50));
    }

    #[test]
    fn test_rate_limiter_blocks() {
        let mut limiter = RateLimiter::new(60.0);
        // Allow 50 requests
        for i in 0..50 {
            assert!(
                limiter.check(1, 0.0, 50),
                "request {} should be allowed",
                i + 1
            );
        }
        // 51st request should be blocked
        assert!(!limiter.check(1, 0.0, 50), "51st request should be blocked");
    }

    #[test]
    fn test_rate_limiter_window_reset() {
        let mut limiter = RateLimiter::new(60.0);
        // Fill up the limit
        for _ in 0..50 {
            limiter.check(1, 0.0, 50);
        }
        // Should be blocked
        assert!(!limiter.check(1, 0.0, 50));

        // After window expires, should be allowed again
        assert!(limiter.check(1, 60.0, 50));
    }

    #[test]
    fn test_gossip_reach() {
        // 20 peers, TTL=3, 25% clustering
        let reach = gossip_reach(20, 3, 0.25);
        assert_eq!(reach.len(), 3);
        // By hop 3, should reach > 100 nodes
        assert!(
            reach[2] > 100,
            "expected >100 nodes by hop 3, got {}",
            reach[2]
        );
    }

    #[test]
    fn test_forward_decision_wot() {
        let config = SimConfig::default();
        let mut node = NodeState::new(1, NodeType::Full, &config);

        // Add a WoT peer
        node.follows.insert(10);

        // WoT peer -> Forward
        assert_eq!(
            should_forward(&node, 10, 1, &config),
            ForwardDecision::Forward
        );

        // Stranger -> ServeLocallyOnly
        assert_eq!(
            should_forward(&node, 99, 2, &config),
            ForwardDecision::ServeLocallyOnly
        );
    }

    #[test]
    fn test_forward_decision_2hop_trusted() {
        let config = SimConfig::default();
        let mut node = NodeState::new(1, NodeType::Full, &config);
        node.wot_peer_scores.insert(20, 2);

        assert_eq!(
            should_forward(&node, 20, 1, &config),
            ForwardDecision::Forward,
            "2-hop trusted peer should be forwarded"
        );
        assert_eq!(
            should_forward(&node, 99, 2, &config),
            ForwardDecision::ServeLocallyOnly,
        );
    }
}
