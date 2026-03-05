use rand::seq::SliceRandom;
use rand_chacha::ChaCha8Rng;

use super::Graph;

/// Generate a graph using the Barabasi-Albert preferential attachment model.
///
/// - `n`: total number of nodes
/// - `m`: number of edges each new node adds
/// - `rng`: deterministic RNG
///
/// The algorithm:
/// 1. Seed: fully connect the first m+1 nodes.
/// 2. Maintain a `degree_list` where each node appears once per edge endpoint
///    (preferential attachment via uniform sampling).
/// 3. Each new node picks m distinct targets from `degree_list`.
/// 4. Add edges and update `degree_list`.
pub fn generate(n: u32, m: u32, rng: &mut ChaCha8Rng) -> Graph {
    assert!(m >= 1, "BA model requires m >= 1");
    assert!(n > m, "BA model requires n > m");

    let mut graph = Graph::new(n);

    let seed_count = m + 1;

    // Step 1: Fully connect the first m+1 nodes (bidirectional follows)
    for i in 0..seed_count {
        for j in 0..seed_count {
            if i != j {
                graph.add_edge(i, j);
            }
        }
    }

    // Step 2: Build initial degree_list
    // Each node in the seed clique has (seed_count - 1) followers,
    // and also follows (seed_count - 1) others.
    // For preferential attachment based on follower count, each seed node
    // appears (seed_count - 1) times.
    let mut degree_list: Vec<u32> = Vec::with_capacity((2 * m as usize) * n as usize);
    for i in 0..seed_count {
        for _ in 0..graph.degree(i) {
            degree_list.push(i);
        }
    }

    // Step 3: Each new node picks m targets via preferential attachment
    for new_node in seed_count..n {
        let mut targets = Vec::with_capacity(m as usize);

        // Sample m distinct targets from degree_list
        while (targets.len() as u32) < m {
            if let Some(&target) = degree_list.choose(rng) {
                if target != new_node && !targets.contains(&target) {
                    targets.push(target);
                }
            }
        }

        // Add edges: new_node follows each target
        for &target in &targets {
            graph.add_edge(new_node, target);
            // Update degree_list: target gains a follower
            degree_list.push(target);
        }

        // New node now has `m` followers worth of "followedness" as a follow source,
        // but for preferential attachment we track follower count.
        // The new node gets 0 followers initially, so it doesn't go into degree_list
        // based on followers. However, BA typically counts total degree (in + out).
        // We add the new node m times to represent its outgoing connections
        // so it has some probability of being selected.
        for _ in 0..m {
            degree_list.push(new_node);
        }
    }

    graph
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    #[test]
    fn test_ba_node_count() {
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let graph = generate(1000, 5, &mut rng);
        assert_eq!(graph.node_count, 1000);
        assert_eq!(graph.follows.len(), 1000);
        assert_eq!(graph.followers.len(), 1000);
    }

    #[test]
    fn test_ba_all_nodes_connected() {
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let graph = generate(100, 5, &mut rng);

        // No isolated nodes: every node should either follow someone or be followed
        for id in 0..100u32 {
            let follows_count = graph.follows.get(&id).map_or(0, |s| s.len());
            let follower_count = graph.degree(id);
            assert!(
                follows_count > 0 || follower_count > 0,
                "Node {} is isolated",
                id
            );
        }
    }

    #[test]
    fn test_ba_power_law_hubs() {
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let graph = generate(1000, 5, &mut rng);

        let avg = graph.avg_degree();
        let max_degree = (0..1000u32).map(|id| graph.degree(id)).max().unwrap();

        // Power law property: hubs should have degree >> average
        assert!(
            max_degree as f64 > 3.0 * avg,
            "Max degree {} should be > 3x avg degree {:.1}",
            max_degree,
            avg
        );
    }

    #[test]
    fn test_ba_deterministic() {
        let mut rng1 = ChaCha8Rng::seed_from_u64(123);
        let graph1 = generate(200, 3, &mut rng1);

        let mut rng2 = ChaCha8Rng::seed_from_u64(123);
        let graph2 = generate(200, 3, &mut rng2);

        // Same seed should produce identical graphs
        for id in 0..200u32 {
            assert_eq!(
                graph1.follows.get(&id),
                graph2.follows.get(&id),
                "Follows differ for node {}",
                id
            );
            assert_eq!(
                graph1.followers.get(&id),
                graph2.followers.get(&id),
                "Followers differ for node {}",
                id
            );
        }
    }
}
