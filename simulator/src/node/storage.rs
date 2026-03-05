use rand::seq::SliceRandom;
use rand_chacha::ChaCha8Rng;

use crate::config::SimConfig;
use crate::types::{Bytes, NodeId};

use super::state::NodeState;

// ── ReliabilityAction ──────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReliabilityAction {
    Healthy,
    IncreaseChallenges,
    BeginReplacement,
    DropImmediately,
}

// ── Functions ──────────────────────────────────────────────────────

/// Check whether two volumes are within +-tolerance of each other.
pub fn is_balanced(own_volume: Bytes, partner_volume: Bytes, tolerance: f64) -> bool {
    let own = own_volume as f64;
    let partner = partner_volume as f64;
    let lower = own * (1.0 - tolerance);
    let upper = own * (1.0 + tolerance);
    partner >= lower && partner <= upper
}

/// Rolling average reliability update.
/// `current * 0.95 + (if passed 1.0 else 0.0) * 0.05`
pub fn update_reliability(current: f64, passed: bool) -> f64 {
    current * 0.95 + if passed { 1.0 } else { 0.0 } * 0.05
}

/// Map a reliability score to an action.
pub fn reliability_action(score: f64) -> ReliabilityAction {
    if score >= 0.90 {
        ReliabilityAction::Healthy
    } else if score >= 0.70 {
        ReliabilityAction::IncreaseChallenges
    } else if score >= 0.50 {
        ReliabilityAction::BeginReplacement
    } else {
        ReliabilityAction::DropImmediately
    }
}

/// Select pact partners from a list of candidates.
///
/// Filters candidates by:
/// 1. Volume tolerance (±config.protocol.volume_tolerance)
/// 2. WoT membership (must be a follows/follower)
/// 3. Not already a pact partner
///
/// Then shuffles the remaining candidates and truncates to the number
/// needed to reach `pacts_default`.
pub fn select_pact_partners(
    node: &NodeState,
    candidates: &[(NodeId, Bytes)],
    own_volume: Bytes,
    config: &SimConfig,
    rng: &mut ChaCha8Rng,
) -> Vec<NodeId> {
    let tolerance = config.protocol.volume_tolerance;
    let needed = (config.protocol.pacts_default as usize).saturating_sub(node.pact_count());

    if needed == 0 {
        return Vec::new();
    }

    // Collect existing partner IDs for quick lookup
    let existing_partners: std::collections::HashSet<NodeId> = node
        .active_pacts
        .iter()
        .map(|p| p.partner)
        .collect();

    let mut eligible: Vec<NodeId> = candidates
        .iter()
        .filter(|(id, vol)| {
            // Must be a WoT peer
            node.is_wot_peer(*id)
            // Must not already be a partner
            && !existing_partners.contains(id)
            // Must be volume-balanced
            && is_balanced(own_volume, *vol, tolerance)
        })
        .map(|(id, _)| *id)
        .collect();

    eligible.shuffle(rng);
    eligible.truncate(needed);
    eligible
}

/// Select standby pact partners from a list of candidates.
///
/// Filters candidates by:
/// 1. Volume tolerance (±config.protocol.volume_tolerance)
/// 2. WoT membership (must be a follows/follower)
/// 3. Not already an active or standby pact partner
///
/// Then shuffles the remaining candidates and truncates to the number
/// needed to reach `pacts_standby`.
pub fn select_standby_pact_partners(
    node: &NodeState,
    candidates: &[(NodeId, Bytes)],
    own_volume: Bytes,
    config: &SimConfig,
    rng: &mut ChaCha8Rng,
) -> Vec<NodeId> {
    let tolerance = config.protocol.volume_tolerance;
    let needed = (config.protocol.pacts_standby as usize).saturating_sub(node.standby_count());

    if needed == 0 {
        return Vec::new();
    }

    // Collect existing active AND standby partner IDs for quick lookup
    let existing_partners: std::collections::HashSet<NodeId> = node
        .active_pacts
        .iter()
        .chain(node.standby_pacts.iter())
        .map(|p| p.partner)
        .collect();

    let mut eligible: Vec<NodeId> = candidates
        .iter()
        .filter(|(id, vol)| {
            // Must be a WoT peer
            node.is_wot_peer(*id)
            // Must not already be an active or standby partner
            && !existing_partners.contains(id)
            // Must be volume-balanced
            && is_balanced(own_volume, *vol, tolerance)
        })
        .map(|(id, _)| *id)
        .collect();

    eligible.shuffle(rng);
    eligible.truncate(needed);
    eligible
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SimConfig;
    use crate::types::NodeType;
    use rand::SeedableRng;

    use super::super::state::NodeState;

    #[test]
    fn test_volume_balance() {
        let own = 1000u64;
        // +-30% of 1000 => [700, 1300]
        assert!(is_balanced(own, 700, 0.30));
        assert!(is_balanced(own, 1300, 0.30));
        assert!(is_balanced(own, 1000, 0.30));

        // +-40% out of the 30% tolerance range
        assert!(!is_balanced(own, 600, 0.30)); // 600 < 700
        assert!(!is_balanced(own, 1400, 0.30)); // 1400 > 1300
    }

    #[test]
    fn test_reliability_scoring() {
        assert_eq!(reliability_action(0.95), ReliabilityAction::Healthy);
        assert_eq!(reliability_action(0.80), ReliabilityAction::IncreaseChallenges);
        assert_eq!(reliability_action(0.60), ReliabilityAction::BeginReplacement);
        assert_eq!(reliability_action(0.40), ReliabilityAction::DropImmediately);
    }

    #[test]
    fn test_reliability_update() {
        // Stays near 1.0 on repeated passes
        let mut score = 1.0;
        for _ in 0..20 {
            score = update_reliability(score, true);
        }
        assert!(score > 0.99, "score should stay near 1.0, got {}", score);

        // Drops below 0.50 after repeated fails
        let mut score = 1.0;
        for _ in 0..20 {
            score = update_reliability(score, false);
        }
        assert!(score < 0.50, "score should drop below 0.50 after 20 fails, got {}", score);
    }

    #[test]
    fn test_select_pact_partners() {
        let config = SimConfig::default();
        let mut node = NodeState::new(1, NodeType::Full, &config);

        // Add WoT peers
        node.follows.insert(10);
        node.follows.insert(20);
        node.followers.insert(30);

        // Candidates: WoT peers (10, 20, 30) and non-WoT (99)
        let own_volume = 1000u64;
        let candidates: Vec<(NodeId, Bytes)> = vec![
            (10, 1000),
            (20, 1100),
            (30, 900),
            (99, 1000), // not in WoT
        ];

        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let selected = select_pact_partners(&node, &candidates, own_volume, &config, &mut rng);

        // All selected should be WoT peers
        for &id in &selected {
            assert!(node.is_wot_peer(id), "selected node {} should be a WoT peer", id);
        }
        // Non-WoT node 99 should not be selected
        assert!(!selected.contains(&99), "non-WoT node 99 should not be selected");
        // Should have selected the 3 WoT peers (all within tolerance)
        assert_eq!(selected.len(), 3);
    }

    #[test]
    fn test_select_standby_pact_partners_excludes_active_and_standby() {
        let mut config = SimConfig::default();
        config.protocol.pacts_standby = 2;

        let mut node = NodeState::new(1, NodeType::Full, &config);
        node.follows.insert(10);
        node.follows.insert(20);
        node.follows.insert(30);
        node.follows.insert(40);

        // 10 is an active partner, 20 is a standby partner
        node.active_pacts.push(crate::types::Pact {
            partner: 10,
            volume_bytes: 1000,
            formed_at: 0.0,
            is_standby: false,
        });
        node.standby_pacts.push(crate::types::Pact {
            partner: 20,
            volume_bytes: 1000,
            formed_at: 0.0,
            is_standby: true,
        });

        let own_volume = 1000u64;
        let candidates: Vec<(NodeId, Bytes)> = vec![
            (10, 1000),
            (20, 1000),
            (30, 1000),
            (40, 1000),
        ];

        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let selected =
            select_standby_pact_partners(&node, &candidates, own_volume, &config, &mut rng);

        // Should NOT include 10 (active) or 20 (standby)
        assert!(!selected.contains(&10), "active partner 10 should be excluded");
        assert!(!selected.contains(&20), "standby partner 20 should be excluded");
        // Should select at most 1 (need 2-1=1 to fill standby)
        assert_eq!(selected.len(), 1, "should select 1 standby partner (need 2 - 1 existing = 1)");
        // Selected should be 30 or 40
        assert!(
            selected.contains(&30) || selected.contains(&40),
            "selected should be one of the non-partner WoT peers"
        );
    }

    #[test]
    fn test_select_standby_pact_partners_returns_empty_when_full() {
        let mut config = SimConfig::default();
        config.protocol.pacts_standby = 1;

        let mut node = NodeState::new(1, NodeType::Full, &config);
        node.follows.insert(10);
        node.follows.insert(20);

        // Standby is already at capacity
        node.standby_pacts.push(crate::types::Pact {
            partner: 10,
            volume_bytes: 1000,
            formed_at: 0.0,
            is_standby: true,
        });

        let own_volume = 1000u64;
        let candidates: Vec<(NodeId, Bytes)> = vec![(20, 1000)];
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let selected =
            select_standby_pact_partners(&node, &candidates, own_volume, &config, &mut rng);

        assert!(
            selected.is_empty(),
            "should return empty when standby is at capacity"
        );
    }
}
