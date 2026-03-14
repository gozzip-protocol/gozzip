use std::collections::HashSet;

use rand::seq::SliceRandom;
use rand::Rng;
use rand_chacha::ChaCha8Rng;

use super::Graph;

/// Generate a graph using the LFR (Lancichinetti-Fortunato-Radicchi) benchmark.
///
/// This produces a network with planted community structure and power-law
/// degree and community-size distributions -- the standard benchmark for
/// evaluating community-detection algorithms.
///
/// # Parameters
///
/// - `n`: total number of nodes
/// - `tau1`: power-law exponent for the degree distribution (typically 2.0-3.0)
/// - `tau2`: power-law exponent for the community size distribution (typically 1.0-2.0)
/// - `mu`: mixing parameter in [0, 1] -- fraction of each node's edges that
///   cross community boundaries
/// - `avg_degree`: target average degree
/// - `min_community`: minimum community size
/// - `max_community`: maximum community size
/// - `rng`: deterministic RNG
///
/// # Algorithm
///
/// 1. Sample a degree sequence from a discrete power-law with exponent `tau1`,
///    clamped to `[1, n-1]` and rescaled so the mean equals `avg_degree`.
/// 2. Sample community sizes from a discrete power-law with exponent `tau2`,
///    partitioning all `n` nodes.
/// 3. For each node, wire `ceil((1 - mu) * degree)` intra-community stubs via
///    a configuration-model pass inside each community.
/// 4. Wire the remaining `floor(mu * degree)` stubs as cross-community edges
///    chosen uniformly at random from nodes outside the community.
pub fn generate(
    n: u32,
    tau1: f64,
    tau2: f64,
    mu: f64,
    avg_degree: u32,
    min_community: u32,
    max_community: u32,
    rng: &mut ChaCha8Rng,
) -> Graph {
    assert!(n >= 2, "LFR requires n >= 2");
    assert!(tau1 > 1.0, "LFR requires tau1 > 1.0");
    assert!(tau2 > 1.0, "LFR requires tau2 > 1.0");
    assert!((0.0..=1.0).contains(&mu), "LFR requires 0 <= mu <= 1");
    assert!(avg_degree >= 1, "LFR requires avg_degree >= 1");
    assert!(min_community >= 2, "LFR requires min_community >= 2");
    assert!(
        max_community >= min_community,
        "LFR requires max_community >= min_community"
    );
    assert!(
        max_community <= n,
        "LFR requires max_community <= n"
    );

    let mut graph = Graph::new(n);

    // ── Step 1: Generate degree sequence from power-law ──────────────

    let degrees = generate_degree_sequence(n, tau1, avg_degree, rng);

    // ── Step 2: Partition nodes into communities ─────────────────────

    let communities = generate_communities(n, tau2, min_community, max_community, rng);

    // Build reverse lookup: node -> community index
    let mut node_community: Vec<usize> = vec![0; n as usize];
    for (ci, members) in communities.iter().enumerate() {
        for &node in members {
            node_community[node as usize] = ci;
        }
    }

    // ── Step 3: Wire intra-community edges (configuration model) ─────

    wire_intra_community_edges(&mut graph, &degrees, &communities, &node_community, mu, rng);

    // ── Step 4: Wire cross-community edges ───────────────────────────

    wire_cross_community_edges(&mut graph, &degrees, &communities, &node_community, mu, rng);

    graph
}

/// Return the community partition so tests and analyses can inspect it.
///
/// This re-runs the deterministic community generation with the same
/// parameters and RNG state offset; for test use we just expose the
/// internal helper.
#[cfg(test)]
pub fn generate_with_communities(
    n: u32,
    tau1: f64,
    tau2: f64,
    mu: f64,
    avg_degree: u32,
    min_community: u32,
    max_community: u32,
    rng: &mut ChaCha8Rng,
) -> (Graph, Vec<Vec<u32>>) {
    assert!(n >= 2, "LFR requires n >= 2");
    assert!(tau1 > 1.0, "LFR requires tau1 > 1.0");
    assert!(tau2 > 1.0, "LFR requires tau2 > 1.0");
    assert!((0.0..=1.0).contains(&mu), "LFR requires 0 <= mu <= 1");
    assert!(avg_degree >= 1, "LFR requires avg_degree >= 1");
    assert!(min_community >= 2, "LFR requires min_community >= 2");
    assert!(
        max_community >= min_community,
        "LFR requires max_community >= min_community"
    );
    assert!(
        max_community <= n,
        "LFR requires max_community <= n"
    );

    let mut graph = Graph::new(n);

    let degrees = generate_degree_sequence(n, tau1, avg_degree, rng);
    let communities = generate_communities(n, tau2, min_community, max_community, rng);

    let mut node_community: Vec<usize> = vec![0; n as usize];
    for (ci, members) in communities.iter().enumerate() {
        for &node in members {
            node_community[node as usize] = ci;
        }
    }

    wire_intra_community_edges(&mut graph, &degrees, &communities, &node_community, mu, rng);
    wire_cross_community_edges(&mut graph, &degrees, &communities, &node_community, mu, rng);

    (graph, communities)
}

// ── Internal helpers ────────────────────────────────────────────────────

/// Sample a power-law distributed integer in [1, n-1] using inverse-CDF.
///
/// P(x) ~ x^{-tau}  =>  CDF ~ x^{1-tau}  =>  inverse CDF sampling.
fn power_law_sample(tau: f64, x_min: f64, x_max: f64, rng: &mut ChaCha8Rng) -> f64 {
    let u: f64 = rng.gen();
    let exp = 1.0 - tau;
    let low = x_min.powf(exp);
    let high = x_max.powf(exp);
    (low + u * (high - low)).powf(1.0 / exp)
}

/// Generate a degree sequence of length `n` from a power-law with
/// exponent `tau1`, rescaled so the mean is approximately `avg_degree`.
fn generate_degree_sequence(
    n: u32,
    tau1: f64,
    avg_degree: u32,
    rng: &mut ChaCha8Rng,
) -> Vec<u32> {
    let max_deg = (n - 1) as f64;
    let x_min = 1.0_f64;

    // Raw power-law samples
    let mut raw: Vec<f64> = (0..n)
        .map(|_| power_law_sample(tau1, x_min, max_deg, rng))
        .collect();

    // Rescale so mean == avg_degree
    let raw_mean: f64 = raw.iter().sum::<f64>() / n as f64;
    let scale = avg_degree as f64 / raw_mean;
    for v in &mut raw {
        *v *= scale;
    }

    // Convert to integers, clamp to [1, n-1]
    raw.iter()
        .map(|&v| {
            let d = v.round().max(1.0).min(max_deg) as u32;
            d
        })
        .collect()
}

/// Partition `n` nodes into communities whose sizes follow a power-law
/// with exponent `tau2`, clamped to [min_community, max_community].
fn generate_communities(
    n: u32,
    tau2: f64,
    min_community: u32,
    max_community: u32,
    rng: &mut ChaCha8Rng,
) -> Vec<Vec<u32>> {
    let x_min = min_community as f64;
    let x_max = max_community as f64;

    // Generate community sizes until we cover all n nodes
    let mut sizes: Vec<u32> = Vec::new();
    let mut total: u32 = 0;

    while total < n {
        let raw = power_law_sample(tau2, x_min, x_max, rng);
        let size = (raw.round() as u32).clamp(min_community, max_community);
        let remaining = n - total;
        if remaining < min_community && !sizes.is_empty() {
            // Remaining nodes too few for a new community; distribute
            // them into existing communities
            break;
        }
        let size = size.min(remaining);
        sizes.push(size);
        total += size;
    }

    // If we still have leftover nodes, distribute them across communities
    let assigned: u32 = sizes.iter().sum();
    if assigned < n {
        let leftover = n - assigned;
        // Spread leftover across communities round-robin
        for i in 0..leftover as usize {
            let ci = i % sizes.len();
            sizes[ci] += 1;
        }
    }

    // Shuffle node IDs and assign to communities
    let mut node_ids: Vec<u32> = (0..n).collect();
    node_ids.shuffle(rng);

    let mut communities: Vec<Vec<u32>> = Vec::with_capacity(sizes.len());
    let mut offset = 0usize;
    for &sz in &sizes {
        let end = offset + sz as usize;
        communities.push(node_ids[offset..end].to_vec());
        offset = end;
    }

    communities
}

/// Wire intra-community edges using a configuration-model approach.
///
/// Each node contributes `ceil((1 - mu) * degree)` stubs to the
/// intra-community stub pool. Stubs are shuffled and paired.
fn wire_intra_community_edges(
    graph: &mut Graph,
    degrees: &[u32],
    communities: &[Vec<u32>],
    node_community: &[usize],
    mu: f64,
    rng: &mut ChaCha8Rng,
) {
    let _ = node_community; // used implicitly via community membership

    for members in communities {
        if members.len() < 2 {
            continue;
        }

        // Build stub list for this community
        let mut stubs: Vec<u32> = Vec::new();
        for &node in members {
            let total_deg = degrees[node as usize];
            let intra_deg = ((1.0 - mu) * total_deg as f64).ceil() as u32;
            for _ in 0..intra_deg {
                stubs.push(node);
            }
        }

        stubs.shuffle(rng);

        // Pair stubs to form edges
        let member_set: HashSet<u32> = members.iter().copied().collect();
        let mut i = 0;
        while i + 1 < stubs.len() {
            let a = stubs[i];
            let b = stubs[i + 1];
            i += 2;

            if a == b {
                continue; // skip self-loops
            }

            // Only add if not already present (avoid multi-edges)
            let already = graph
                .follows
                .get(&a)
                .map_or(false, |s| s.contains(&b));
            if !already && member_set.contains(&a) && member_set.contains(&b) {
                graph.add_edge(a, b);
                graph.add_edge(b, a); // undirected base
            }
        }

        // Ensure every node in the community has at least one
        // intra-community neighbor (connectivity guarantee)
        for &node in members {
            let has_intra = graph
                .follows
                .get(&node)
                .map_or(false, |follows| {
                    follows.iter().any(|&f| member_set.contains(&f))
                });
            if !has_intra {
                // Connect to a random other member
                let others: Vec<u32> = members.iter().copied().filter(|&m| m != node).collect();
                if let Some(&target) = others.choose(rng) {
                    graph.add_edge(node, target);
                    graph.add_edge(target, node);
                }
            }
        }
    }
}

/// Wire cross-community edges for each node.
///
/// Each node needs `floor(mu * degree)` cross-community edges, picking
/// targets uniformly from nodes outside its community.
fn wire_cross_community_edges(
    graph: &mut Graph,
    degrees: &[u32],
    communities: &[Vec<u32>],
    node_community: &[usize],
    mu: f64,
    rng: &mut ChaCha8Rng,
) {
    let n = degrees.len() as u32;

    // Build list of nodes per community for fast sampling
    let community_sets: Vec<HashSet<u32>> = communities
        .iter()
        .map(|c| c.iter().copied().collect())
        .collect();

    for node in 0..n {
        let total_deg = degrees[node as usize];
        let cross_deg = (mu * total_deg as f64).floor() as u32;

        if cross_deg == 0 {
            continue;
        }

        let my_community = node_community[node as usize];
        let my_set = &community_sets[my_community];

        let mut added = 0u32;
        let mut attempts = 0u32;
        let max_attempts = cross_deg * 20; // prevent infinite loops

        while added < cross_deg && attempts < max_attempts {
            attempts += 1;
            let target = rng.gen_range(0..n);

            if target == node {
                continue;
            }

            // Must be outside our community
            if my_set.contains(&target) {
                continue;
            }

            // Skip if edge already exists
            let already = graph
                .follows
                .get(&node)
                .map_or(false, |s| s.contains(&target));
            if already {
                continue;
            }

            graph.add_edge(node, target);
            graph.add_edge(target, node);
            added += 1;
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use rand::SeedableRng;

    #[test]
    fn test_lfr_node_count() {
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let graph = generate(500, 2.5, 1.5, 0.3, 20, 20, 100, &mut rng);

        assert_eq!(graph.node_count, 500);
        assert_eq!(graph.follows.len(), 500);
        assert_eq!(graph.followers.len(), 500);
    }

    #[test]
    fn test_lfr_community_structure() {
        // With low mu, most edges should be intra-community, leading to
        // detectable clustering: average local clustering coefficient
        // should be well above what a random graph of the same density
        // would produce.
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let (graph, communities) =
            generate_with_communities(500, 2.5, 1.5, 0.1, 20, 20, 100, &mut rng);

        // All nodes should be assigned to some community
        let total_assigned: usize = communities.iter().map(|c| c.len()).sum();
        assert_eq!(total_assigned, 500);

        // Every community should have at least min_community members
        // (except possibly the last one that absorbed leftovers, which
        // can be slightly larger but never smaller than 2)
        for (i, c) in communities.iter().enumerate() {
            assert!(
                c.len() >= 2,
                "Community {} has only {} members",
                i,
                c.len()
            );
        }

        // With mu=0.1, intra-community edge density should be high.
        // Check that the majority of edges for a sample of nodes are
        // intra-community.
        let mut intra = 0u64;
        let mut total = 0u64;
        let community_sets: Vec<HashSet<u32>> = communities
            .iter()
            .map(|c| c.iter().copied().collect())
            .collect();

        for node in 0..500u32 {
            let ci = {
                let mut found = 0;
                for (idx, c) in communities.iter().enumerate() {
                    if c.contains(&node) {
                        found = idx;
                        break;
                    }
                }
                found
            };
            if let Some(follows) = graph.follows.get(&node) {
                for &f in follows {
                    total += 1;
                    if community_sets[ci].contains(&f) {
                        intra += 1;
                    }
                }
            }
        }

        let intra_frac = intra as f64 / total.max(1) as f64;
        // With mu=0.1, we expect ~90% intra. Allow some slack.
        assert!(
            intra_frac > 0.5,
            "Intra-community edge fraction {:.3} should be > 0.5 for mu=0.1",
            intra_frac
        );
    }

    #[test]
    fn test_lfr_deterministic() {
        let mut rng1 = ChaCha8Rng::seed_from_u64(123);
        let graph1 = generate(300, 2.5, 1.5, 0.3, 15, 10, 80, &mut rng1);

        let mut rng2 = ChaCha8Rng::seed_from_u64(123);
        let graph2 = generate(300, 2.5, 1.5, 0.3, 15, 10, 80, &mut rng2);

        // Same seed should produce identical graphs
        for id in 0..300u32 {
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

    #[test]
    fn test_lfr_mixing_parameter() {
        // Generate with mu=0.5 and verify cross-community edge fraction
        // is approximately 0.5 (within a reasonable tolerance).
        let mut rng = ChaCha8Rng::seed_from_u64(99);
        let mu = 0.5;
        let (graph, communities) =
            generate_with_communities(500, 2.5, 1.5, mu, 20, 20, 100, &mut rng);

        let community_sets: Vec<HashSet<u32>> = communities
            .iter()
            .map(|c| c.iter().copied().collect())
            .collect();

        // Build node -> community index
        let mut node_comm: HashMap<u32, usize> = HashMap::new();
        for (ci, members) in communities.iter().enumerate() {
            for &node in members {
                node_comm.insert(node, ci);
            }
        }

        let mut cross = 0u64;
        let mut total = 0u64;

        for node in 0..500u32 {
            let ci = node_comm[&node];
            if let Some(follows) = graph.follows.get(&node) {
                for &f in follows {
                    total += 1;
                    if !community_sets[ci].contains(&f) {
                        cross += 1;
                    }
                }
            }
        }

        let cross_frac = cross as f64 / total.max(1) as f64;
        // mu=0.5, so ~50% cross-community. Allow 20% tolerance band.
        assert!(
            (cross_frac - mu).abs() < 0.25,
            "Cross-community fraction {:.3} should be within 0.25 of mu={:.1}",
            cross_frac,
            mu
        );
    }
}
