use rand::Rng;
use rand_chacha::ChaCha8Rng;

use super::Graph;

/// Generate a graph using the Watts-Strogatz small-world model.
///
/// - `n`: total number of nodes
/// - `k`: each node connects to k nearest neighbors (k/2 on each side)
/// - `p`: rewiring probability (0 = regular ring, 1 = random)
/// - `rng`: deterministic RNG
///
/// The algorithm:
/// 1. Ring lattice: arrange nodes in a ring, each connecting to k/2
///    neighbors on each side.
/// 2. Rewire: for each edge, with probability p, replace the target
///    with a uniformly random node (avoiding self-loops and duplicates).
pub fn generate(n: u32, k: u32, p: f64, rng: &mut ChaCha8Rng) -> Graph {
    assert!(k >= 2, "WS model requires k >= 2");
    assert!(k % 2 == 0, "WS model requires even k");
    assert!(n > k, "WS model requires n > k");
    assert!((0.0..=1.0).contains(&p), "WS model requires 0 <= p <= 1");

    let mut graph = Graph::new(n);
    let half_k = k / 2;

    // Step 1: Ring lattice — each node connects to k/2 neighbors on each side
    for i in 0..n {
        for j in 1..=half_k {
            let neighbor = (i + j) % n;
            graph.add_edge(i, neighbor);
            graph.add_edge(neighbor, i);
        }
    }

    // Step 2: Rewire edges with probability p
    // Iterate over the original ring lattice edges (clockwise direction only)
    // and potentially rewire them.
    for i in 0..n {
        for j in 1..=half_k {
            let original_target = (i + j) % n;

            if rng.gen::<f64>() < p {
                // Pick a random new target that is not self and not already followed
                let mut new_target = original_target;
                let mut attempts = 0;
                while attempts < n {
                    let candidate = rng.gen_range(0..n);
                    if candidate != i
                        && !graph
                            .follows
                            .get(&i)
                            .map_or(false, |s| s.contains(&candidate))
                    {
                        new_target = candidate;
                        break;
                    }
                    attempts += 1;
                }

                if new_target != original_target {
                    // Remove the original edge (both directions)
                    if let Some(set) = graph.follows.get_mut(&i) {
                        set.remove(&original_target);
                    }
                    if let Some(set) = graph.followers.get_mut(&original_target) {
                        set.remove(&i);
                    }

                    // Add the new edge (both directions to maintain symmetry)
                    graph.add_edge(i, new_target);
                    graph.add_edge(new_target, i);
                }
            }
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
    fn test_ws_node_count() {
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let graph = generate(1000, 10, 0.1, &mut rng);
        assert_eq!(graph.node_count, 1000);
        assert_eq!(graph.follows.len(), 1000);
        assert_eq!(graph.followers.len(), 1000);
    }

    #[test]
    fn test_ws_uniform_degree() {
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let k = 10u32;
        let graph = generate(1000, k, 0.1, &mut rng);

        let avg = graph.avg_degree();
        let max_degree = (0..1000u32).map(|id| graph.degree(id)).max().unwrap();

        // Average degree should be approximately k (within 5)
        assert!(
            (avg - k as f64).abs() < 5.0,
            "Avg degree {:.1} should be within 5 of k={}",
            avg,
            k
        );

        // Max degree should be less than 2*k (small-world, not power-law)
        assert!(
            max_degree < (2 * k) as usize,
            "Max degree {} should be < 2*k={}",
            max_degree,
            2 * k
        );
    }

    #[test]
    fn test_ws_no_rewire_is_regular() {
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let k = 10u32;
        let graph = generate(1000, k, 0.0, &mut rng);

        // With p=0, every node should have exactly degree k
        for id in 0..1000u32 {
            assert_eq!(
                graph.degree(id),
                k as usize,
                "Node {} has degree {} instead of {}",
                id,
                graph.degree(id),
                k
            );
        }
    }
}
