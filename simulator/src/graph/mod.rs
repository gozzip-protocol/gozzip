pub mod barabasi_albert;
pub mod lfr;
pub mod watts_strogatz;

use std::collections::{HashMap, HashSet, VecDeque};

use rand::Rng;
use rand_chacha::ChaCha8Rng;

use crate::config::SimConfig;
use crate::types::{NodeId, NodeType};

// ── Graph ────────────────────────────────────────────────────────────

pub struct Graph {
    pub node_count: u32,
    pub follows: HashMap<NodeId, HashSet<NodeId>>,
    pub followers: HashMap<NodeId, HashSet<NodeId>>,
    pub node_types: HashMap<NodeId, NodeType>,
}

impl Graph {
    /// Create a new empty graph with `node_count` nodes, all defaulting to Light.
    pub fn new(node_count: u32) -> Self {
        let mut follows = HashMap::with_capacity(node_count as usize);
        let mut followers = HashMap::with_capacity(node_count as usize);
        let mut node_types = HashMap::with_capacity(node_count as usize);

        for id in 0..node_count {
            follows.insert(id, HashSet::new());
            followers.insert(id, HashSet::new());
            node_types.insert(id, NodeType::Light);
        }

        Self {
            node_count,
            follows,
            followers,
            node_types,
        }
    }

    /// Add a directed follow edge from `from` to `to`.
    /// Skips self-loops. Updates both `follows` and `followers`.
    pub fn add_edge(&mut self, from: NodeId, to: NodeId) {
        if from == to {
            return;
        }
        self.follows.entry(from).or_default().insert(to);
        self.followers.entry(to).or_default().insert(from);
    }

    /// Randomly assign Full/Light node types based on `full_node_pct`.
    pub fn assign_node_types(&mut self, full_node_pct: f64, rng: &mut ChaCha8Rng) {
        for id in 0..self.node_count {
            let node_type = if rng.gen::<f64>() < full_node_pct {
                NodeType::Full
            } else {
                NodeType::Light
            };
            self.node_types.insert(id, node_type);
        }
    }

    /// Follower count for a node.
    pub fn degree(&self, node: NodeId) -> usize {
        self.followers
            .get(&node)
            .map_or(0, |set| set.len())
    }

    /// Average follower count across all nodes.
    pub fn avg_degree(&self) -> f64 {
        if self.node_count == 0 {
            return 0.0;
        }
        let total: usize = (0..self.node_count)
            .map(|id| self.degree(id))
            .sum();
        total as f64 / self.node_count as f64
    }

    /// Count of Full nodes.
    pub fn full_node_count(&self) -> u32 {
        self.node_types
            .values()
            .filter(|t| **t == NodeType::Full)
            .count() as u32
    }

    /// Count of Light nodes.
    pub fn light_node_count(&self) -> u32 {
        self.node_types
            .values()
            .filter(|t| **t == NodeType::Light)
            .count() as u32
    }

    /// BFS to find shortest WoT path from `from` to `to`, up to `max_hops`.
    /// Returns `None` if no path exists within the hop limit.
    pub fn wot_distance(&self, from: NodeId, to: NodeId, max_hops: u32) -> Option<u32> {
        if from == to {
            return Some(0);
        }

        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();

        visited.insert(from);
        queue.push_back((from, 0u32));

        while let Some((current, depth)) = queue.pop_front() {
            if depth >= max_hops {
                continue;
            }

            if let Some(neighbors) = self.follows.get(&current) {
                for &neighbor in neighbors {
                    if neighbor == to {
                        return Some(depth + 1);
                    }
                    if visited.insert(neighbor) {
                        queue.push_back((neighbor, depth + 1));
                    }
                }
            }
        }

        None
    }


    /// Compute WoT tiers for all nodes.
    pub fn compute_wot_tiers(&self) -> WotTiers {
        let mut direct_wot = HashMap::with_capacity(self.node_count as usize);
        let mut one_hop = HashMap::with_capacity(self.node_count as usize);
        let mut two_hop = HashMap::with_capacity(self.node_count as usize);

        for node in 0..self.node_count {
            let my_follows = self.follows.get(&node).cloned().unwrap_or_default();
            let my_followers = self.followers.get(&node).cloned().unwrap_or_default();

            let mutual: HashSet<NodeId> = my_follows.intersection(&my_followers).copied().collect();
            let non_mutual: HashSet<NodeId> = my_follows.difference(&mutual).copied().collect();

            let mut two_hop_map: HashMap<NodeId, u32> = HashMap::new();
            let excluded: HashSet<NodeId> = {
                let mut s = HashSet::new();
                s.insert(node);
                s.extend(&mutual);
                s.extend(&non_mutual);
                s
            };

            for &contact in &mutual {
                if let Some(contact_follows) = self.follows.get(&contact) {
                    for &author in contact_follows {
                        if !excluded.contains(&author) {
                            *two_hop_map.entry(author).or_insert(0) += 1;
                        }
                    }
                }
            }

            direct_wot.insert(node, mutual);
            one_hop.insert(node, non_mutual);
            two_hop.insert(node, two_hop_map);
        }

        WotTiers { direct_wot, one_hop, two_hop }
    }
}

// ── WotTiers ────────────────────────────────────────────────────────

/// Precomputed per-node WoT tier membership.
///
/// - `direct_wot`: Mutual follows (I follow them AND they follow me)
/// - `one_hop`: I follow them but they don't follow me back
/// - `two_hop`: Authors followed by my direct_wot contacts, with trust
///   score = number of direct_wot contacts that follow them
pub struct WotTiers {
    pub direct_wot: HashMap<NodeId, HashSet<NodeId>>,
    pub one_hop: HashMap<NodeId, HashSet<NodeId>>,
    pub two_hop: HashMap<NodeId, HashMap<NodeId, u32>>,
}

// ── inject_sybil_nodes ───────────────────────────────────────────────

/// Inject `count` sybil nodes into the graph targeting `target`.
///
/// Each sybil node is:
/// - `NodeType::Light`
/// - Has a one-way follow to `target` (sybil follows target)
/// - Target does NOT follow sybil back
///
/// Because `is_wot_peer` checks both `follows` and `followers`, adding a
/// sybil→target edge would place the sybil in target's `followers` set,
/// making `target.is_wot_peer(sybil)` return true.  To prevent this, we
/// add the edge **only** to the sybil's `follows` set (so the sybil node
/// actor will try to form pacts with the target) but do NOT update the
/// target's `followers` set.  This simulates the real-world scenario
/// where a sybil unilaterally "follows" someone but is not recognised by
/// the target's Web-of-Trust.
///
/// Returns the IDs of the injected sybil nodes.
pub fn inject_sybil_nodes(graph: &mut Graph, count: u32, target: NodeId) -> Vec<NodeId> {
    let start = graph.node_count;
    let mut sybil_ids = Vec::with_capacity(count as usize);

    for i in 0..count {
        let id = start + i;

        // Register the node
        graph.follows.insert(id, HashSet::new());
        graph.followers.insert(id, HashSet::new());
        graph.node_types.insert(id, NodeType::Light);

        // Sybil follows the target (one-way, NOT reciprocated).
        // Only update sybil's follows — do NOT touch target's followers.
        graph.follows.get_mut(&id).unwrap().insert(target);

        sybil_ids.push(id);
    }

    graph.node_count = start + count;
    sybil_ids
}

// ── inject_eclipse_sybil_nodes ───────────────────────────────────────

/// Inject `count` eclipse sybil nodes that form a mutual clique and
/// unilaterally follow `target`.
///
/// Unlike regular sybil injection, eclipse sybils also mutually follow
/// each other (forming a dense clique). This simulates attackers who
/// try to surround a target node. WoT should still block them because
/// the target does not follow any sybil back.
///
/// Returns the IDs of the injected sybil nodes.
pub fn inject_eclipse_sybil_nodes(graph: &mut Graph, count: u32, target: NodeId) -> Vec<NodeId> {
    let start = graph.node_count;
    let mut sybil_ids = Vec::with_capacity(count as usize);

    for i in 0..count {
        let id = start + i;
        graph.follows.insert(id, HashSet::new());
        graph.followers.insert(id, HashSet::new());
        graph.node_types.insert(id, NodeType::Light);

        // Sybil follows the target (one-way, NOT reciprocated)
        graph.follows.get_mut(&id).unwrap().insert(target);

        sybil_ids.push(id);
    }

    // Create mutual clique among sybil nodes
    for i in 0..sybil_ids.len() {
        for j in 0..sybil_ids.len() {
            if i != j {
                let from = sybil_ids[i];
                let to = sybil_ids[j];
                graph.follows.get_mut(&from).unwrap().insert(to);
                graph.followers.get_mut(&to).unwrap().insert(from);
            }
        }
    }

    graph.node_count = start + count;
    sybil_ids
}

// ── build_graph ──────────────────────────────────────────────────────

/// Build a graph based on the simulation config, dispatching to the
/// appropriate model (Barabasi-Albert or Watts-Strogatz), then assigning
/// node types.
pub fn build_graph(config: &SimConfig, rng: &mut ChaCha8Rng) -> Graph {
    let mut graph = match config.graph.model.as_str() {
        "barabasi-albert" => barabasi_albert::generate(
            config.graph.nodes,
            config.graph.ba_edges_per_node,
            rng,
        ),
        "watts-strogatz" => watts_strogatz::generate(
            config.graph.nodes,
            config.graph.ws_neighbors,
            config.graph.ws_rewire_prob,
            rng,
        ),
        "lfr" => lfr::generate(
            config.graph.nodes,
            config.graph.lfr_tau1,
            config.graph.lfr_tau2,
            config.graph.lfr_mu,
            config.graph.lfr_avg_degree,
            config.graph.lfr_min_community,
            config.graph.lfr_max_community,
            rng,
        ),
        other => panic!("Unknown graph model: {}", other),
    };

    graph.assign_node_types(config.network.full_node_pct, rng);
    graph
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inject_sybil_nodes_basic() {
        let mut graph = Graph::new(10);
        // Add some edges for the existing graph
        graph.add_edge(0, 1);
        graph.add_edge(1, 2);

        let target = 2;
        let sybil_ids = inject_sybil_nodes(&mut graph, 5, target);

        // Should have 15 nodes now
        assert_eq!(graph.node_count, 15);
        assert_eq!(sybil_ids.len(), 5);
        assert_eq!(sybil_ids, vec![10, 11, 12, 13, 14]);
    }

    #[test]
    fn test_inject_sybil_nodes_are_light() {
        let mut graph = Graph::new(5);
        let sybil_ids = inject_sybil_nodes(&mut graph, 3, 0);

        for &id in &sybil_ids {
            assert_eq!(
                *graph.node_types.get(&id).unwrap(),
                NodeType::Light,
                "sybil node {} should be Light",
                id
            );
        }
    }

    #[test]
    fn test_inject_sybil_nodes_follow_target_one_way() {
        let mut graph = Graph::new(5);
        let target = 2;
        let sybil_ids = inject_sybil_nodes(&mut graph, 3, target);

        for &sid in &sybil_ids {
            // Sybil's follows should include target
            assert!(
                graph.follows.get(&sid).unwrap().contains(&target),
                "sybil {} should follow target {}",
                sid,
                target
            );

            // Target's followers should NOT include sybil
            assert!(
                !graph.followers.get(&target).unwrap().contains(&sid),
                "target {} followers should NOT include sybil {}",
                target,
                sid
            );

            // Sybil should have no followers
            assert!(
                graph.followers.get(&sid).unwrap().is_empty(),
                "sybil {} should have no followers",
                sid
            );
        }
    }

    #[test]
    fn test_inject_sybil_nodes_zero_count() {
        let mut graph = Graph::new(5);
        let sybil_ids = inject_sybil_nodes(&mut graph, 0, 0);

        assert_eq!(graph.node_count, 5);
        assert!(sybil_ids.is_empty());
    }

    #[test]
    fn test_inject_sybil_nodes_does_not_affect_existing_edges() {
        let mut graph = Graph::new(5);
        graph.add_edge(0, 1);
        graph.add_edge(1, 2);
        graph.add_edge(2, 3);

        // Snapshot existing state
        let orig_follows_0 = graph.follows.get(&0).unwrap().clone();
        let orig_followers_1 = graph.followers.get(&1).unwrap().clone();

        inject_sybil_nodes(&mut graph, 10, 2);

        // Original edges should be unchanged
        assert_eq!(*graph.follows.get(&0).unwrap(), orig_follows_0);
        assert_eq!(*graph.followers.get(&1).unwrap(), orig_followers_1);
    }

    #[test]
    fn test_compute_wot_tiers_basic() {
        let mut graph = Graph::new(5);
        graph.add_edge(0, 1);
        graph.add_edge(1, 0);
        graph.add_edge(0, 2);
        graph.add_edge(1, 3);
        graph.add_edge(2, 3);

        let tiers = graph.compute_wot_tiers();

        let direct = tiers.direct_wot.get(&0).unwrap();
        assert!(direct.contains(&1), "0 <-> 1 should be direct WoT");
        assert!(!direct.contains(&2), "0 -> 2 is not mutual");

        let one_hop = tiers.one_hop.get(&0).unwrap();
        assert!(one_hop.contains(&2), "0 -> 2 is one-hop");
        assert!(!one_hop.contains(&1), "1 is direct, not one-hop");

        let two_hop_map = tiers.two_hop.get(&0).unwrap();
        assert!(two_hop_map.contains_key(&3), "3 is 2-hop from 0 via 1");
        assert_eq!(*two_hop_map.get(&3).unwrap(), 1);
        assert!(!two_hop_map.contains_key(&4));
    }

    #[test]
    fn test_wot_tiers_two_hop_trust_score() {
        let mut graph = Graph::new(6);
        for peer in [1, 2, 3] {
            graph.add_edge(0, peer);
            graph.add_edge(peer, 0);
        }
        graph.add_edge(1, 5);
        graph.add_edge(2, 5);
        graph.add_edge(3, 5);
        graph.add_edge(1, 4);

        let tiers = graph.compute_wot_tiers();
        let two_hop_map = tiers.two_hop.get(&0).unwrap();
        assert_eq!(*two_hop_map.get(&5).unwrap(), 3);
        assert_eq!(*two_hop_map.get(&4).unwrap(), 1);
    }

    #[test]
    fn test_wot_tiers_excludes_self_and_existing_tiers() {
        let mut graph = Graph::new(4);
        graph.add_edge(0, 1);
        graph.add_edge(1, 0);
        graph.add_edge(1, 2);

        let tiers = graph.compute_wot_tiers();
        let two_hop_map = tiers.two_hop.get(&0).unwrap();
        assert!(!two_hop_map.contains_key(&0), "self excluded");
        assert!(!two_hop_map.contains_key(&1), "direct-wot excluded");
        assert!(two_hop_map.contains_key(&2), "2 should be 2-hop");
    }
}
