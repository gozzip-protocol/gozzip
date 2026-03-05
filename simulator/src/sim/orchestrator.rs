use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};

use rand::Rng;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use tokio::sync::mpsc;
use tokio::task::yield_now;

use crate::config::SimConfig;
use crate::graph::{self, Graph, WotTiers};
use crate::node::state::NodeState;
use crate::node::{self};
use crate::sim::clock::SimClock;
use crate::sim::metrics::{self, CollectedMetrics, MetricsCollector};
use crate::sim::router::{self, PartitionMap, Router};
use crate::types::*;

// ── SimResult ──────────────────────────────────────────────────────

pub struct SimResult {
    pub metrics: CollectedMetrics,
    pub graph: Graph,
    pub config: SimConfig,
    /// Per-node, per-tick online/offline decisions made by the orchestrator.
    /// Each entry is `true` if the node was set online that tick, `false` if offline.
    /// This is the authoritative source of truth for availability.
    pub availability_records: HashMap<NodeId, Vec<bool>>,
    /// Per-node activity weights used for author selection.
    /// With uniform distribution all weights are 1.0; with power_law they vary.
    pub activity_weights: Vec<f64>,
}

// ── PartitionSchedule ──────────────────────────────────────────────

/// Describes a network partition event: during `[start_time, end_time)`,
/// the router drops messages between nodes in different partition groups.
pub struct PartitionSchedule {
    /// Sim time (seconds) at which the partition begins.
    pub start_time: SimTime,
    /// Sim time (seconds) at which the partition ends (heals).
    pub end_time: SimTime,
    /// Maps each NodeId to a partition index.
    pub map: HashMap<NodeId, usize>,
}

// ── InteractionTracker ─────────────────────────────────────────────

/// Tracks interaction scores between nodes for feed referral mechanism.
struct InteractionTracker {
    /// Raw interaction events: (interactor, target) -> [(weight, time)]
    interactions: HashMap<(NodeId, NodeId), Vec<(f64, SimTime)>>,
    /// Decay half-life in days.
    decay_days: f64,
}

impl InteractionTracker {
    fn new(decay_days: f64) -> Self {
        Self {
            interactions: HashMap::new(),
            decay_days,
        }
    }

    fn record(&mut self, interactor: NodeId, target: NodeId, weight: f64, time: SimTime) {
        self.interactions
            .entry((interactor, target))
            .or_default()
            .push((weight, time));
    }

    fn score(&self, interactor: NodeId, target: NodeId, now: SimTime) -> f64 {
        self.interactions
            .get(&(interactor, target))
            .map(|entries| {
                entries.iter().map(|(w, t)| {
                    let age_days = (now - t) / 86400.0;
                    w * (-age_days / self.decay_days).exp()
                }).sum()
            })
            .unwrap_or(0.0)
    }

    /// Find authors that should enter a reader's Orbit via referral.
    /// Returns authors where >= min_contacts IC contacts have scores >= min_score.
    fn compute_referrals(
        &self,
        ic_contacts: &[NodeId],
        min_contacts: u32,
        min_score: f64,
        now: SimTime,
    ) -> HashSet<NodeId> {
        let mut author_endorsers: HashMap<NodeId, u32> = HashMap::new();
        for &contact in ic_contacts {
            for (&(interactor, target), _) in &self.interactions {
                if interactor == contact {
                    let s = self.score(interactor, target, now);
                    if s >= min_score {
                        *author_endorsers.entry(target).or_insert(0) += 1;
                    }
                }
            }
        }
        author_endorsers
            .into_iter()
            .filter(|(_, count)| *count >= min_contacts)
            .map(|(author, _)| author)
            .collect()
    }
}

// ── Orchestrator ───────────────────────────────────────────────────

pub struct Orchestrator {
    config: SimConfig,
    graph: Graph,
    clock: SimClock,
    rng: ChaCha8Rng,
    partition_schedule: Option<PartitionSchedule>,
    partition_map: PartitionMap,
    /// Per-node forced-offline windows. During each (start, end) interval,
    /// the node is forced offline regardless of its uptime roll.
    offline_overrides: HashMap<NodeId, Vec<(SimTime, SimTime)>>,
    /// Precomputed WoT tiers for tier-weighted read selection.
    wot_tiers: WotTiers,
    /// Tracks interactions for feed referral mechanism.
    interaction_tracker: InteractionTracker,
}

impl Orchestrator {
    /// Create a new orchestrator by building the social graph and
    /// initialising the virtual clock and RNG.
    pub fn new(config: SimConfig) -> Self {
        let mut graph_rng = ChaCha8Rng::seed_from_u64(config.graph.seed);
        let graph = graph::build_graph(&config, &mut graph_rng);
        let wot_tiers = graph.compute_wot_tiers();
        let clock = SimClock::new(config.simulation.deterministic);
        let rng = ChaCha8Rng::seed_from_u64(config.graph.seed.wrapping_add(1));

        let interaction_tracker = InteractionTracker::new(config.retrieval.interaction_decay_days);
        Self {
            config,
            graph,
            clock,
            rng,
            partition_schedule: None,
            partition_map: Arc::new(RwLock::new(None)),
            offline_overrides: HashMap::new(),
            wot_tiers,
            interaction_tracker,
        }
    }

    /// Create an orchestrator with an externally-built graph.
    ///
    /// This is used by scenarios that need to modify the graph before
    /// running the simulation (e.g. sybil injection).
    pub fn with_graph(config: SimConfig, graph: Graph) -> Self {
        let wot_tiers = graph.compute_wot_tiers();
        let clock = SimClock::new(config.simulation.deterministic);
        let rng = ChaCha8Rng::seed_from_u64(config.graph.seed.wrapping_add(1));
        let interaction_tracker = InteractionTracker::new(config.retrieval.interaction_decay_days);

        Self {
            config,
            graph,
            clock,
            rng,
            partition_schedule: None,
            partition_map: Arc::new(RwLock::new(None)),
            offline_overrides: HashMap::new(),
            wot_tiers,
            interaction_tracker,
        }
    }

    /// Set a partition schedule on the orchestrator. During the scheduled
    /// window, the router will drop messages between nodes in different
    /// partition groups.
    pub fn with_partition_schedule(mut self, schedule: PartitionSchedule) -> Self {
        self.partition_schedule = Some(schedule);
        self
    }

    /// Set offline overrides: during each (start, end) window for a node,
    /// it will be forced offline regardless of uptime rolls.
    pub fn with_offline_overrides(mut self, overrides: HashMap<NodeId, Vec<(SimTime, SimTime)>>) -> Self {
        self.offline_overrides = overrides;
        self
    }

    /// Run the full simulation and return the collected results.
    pub async fn run(mut self) -> SimResult {
        let node_count = self.graph.node_count as usize;

        // 1. Create router and metrics channels
        let (router_tx, router_rx) = router::create_router_channel(node_count * 100);
        let (metrics_tx, metrics_rx) = metrics::create_metrics_channel(node_count * 100);

        // 1b. Create NodeRegistry for Nostr event signing
        let nostr_enabled = self.config.simulation.nostr_events;
        let registry: Option<Arc<crate::nostr_bridge::NodeRegistry>> = if nostr_enabled {
            Some(Arc::new(crate::nostr_bridge::NodeRegistry::generate(self.graph.node_count)))
        } else {
            None
        };

        // 2. Per-node channels and spawn node tasks
        let mut node_senders: HashMap<NodeId, mpsc::Sender<Message>> = HashMap::new();

        for id in 0..self.graph.node_count {
            let (tx, rx) = mpsc::channel::<Message>(1024);
            node_senders.insert(id, tx);

            // Build NodeState from graph data
            let node_type = *self
                .graph
                .node_types
                .get(&id)
                .unwrap_or(&NodeType::Light);
            let mut state = NodeState::new(id, node_type, &self.config);

            // Copy follows and followers from graph
            if let Some(follows) = self.graph.follows.get(&id) {
                state.follows = follows.clone();
            }
            if let Some(followers) = self.graph.followers.get(&id) {
                state.followers = followers.clone();
            }

            // Set storage capacity with per-node variance
            let base = self.config.protocol.default_storage_capacity_mb as u64 * 1_048_576;
            let variance = self.config.protocol.storage_capacity_variance;
            let factor = 1.0 + (self.rng.gen::<f64>() * 2.0 - 1.0) * variance;
            state.storage_capacity = (base as f64 * factor) as u64;

            // Initialize activity rate to the base events/day (EMA will adjust)
            state.activity_rate = self.config.events.events_per_day;

            // Initialize karma balance
            if self.config.karma.enabled {
                state.karma = crate::node::karma::KarmaState::new(self.config.karma.initial_balance);
            }

            // Populate WoT peer trust scores from precomputed tiers
            if let Some(direct) = self.wot_tiers.direct_wot.get(&id) {
                for &peer in direct {
                    state.wot_peer_scores.insert(peer, 3);
                }
            }
            if let Some(one_hop) = self.wot_tiers.one_hop.get(&id) {
                for &peer in one_hop {
                    state.wot_peer_scores.entry(peer).or_insert(2);
                }
            }
            if let Some(two_hop) = self.wot_tiers.two_hop.get(&id) {
                for (&peer, &score) in two_hop {
                    state.wot_peer_scores.entry(peer).or_insert(score.min(2));
                }
            }

            // Set Nostr secret key if enabled
            if let Some(ref reg) = registry {
                state.nostr_secret_key = reg.get_secret_key_bytes(id);
            }

            tokio::spawn(node::run_node(
                state,
                rx,
                router_tx.clone(),
                metrics_tx.clone(),
                self.config.clone(),
            ));
        }

        // 3. Spawn router task (with partition awareness if scheduled)
        let router = Router::with_partitions(
            router_rx,
            node_senders.clone(),
            self.clock.clone(),
            self.config.clone(),
            self.partition_map.clone(),
        );
        tokio::spawn(router.run());

        // 4. Compute simulation parameters (needed for metrics collector)
        let tick_interval_secs = self.config.simulation.tick_interval_secs as f64;
        let total_seconds = self.config.simulation.duration_days as f64 * 86_400.0;
        let total_ticks = (total_seconds / tick_interval_secs) as u64;

        // 5. Spawn metrics collector task
        let collector = MetricsCollector::new(
            metrics_rx,
            total_ticks,
            &self.config.simulation.streaming,
            registry.clone(),
        );
        let metrics_handle = tokio::spawn(collector.run());
        let ticks_per_day = 86_400.0 / tick_interval_secs;

        // Events per tick: node_count * dau_pct * events_per_day / ticks_per_day
        let events_per_tick =
            node_count as f64 * self.config.network.dau_pct * self.config.events.events_per_day / ticks_per_day;

        let mut event_id_counter: u64 = 0;
        let mut read_request_id_counter: u64 = 0;
        let mut seq_counters: HashMap<NodeId, u64> = HashMap::new();
        let mut prev_hashes: HashMap<NodeId, u64> = HashMap::new();

        // Track per-node, per-tick online/offline decisions
        let mut availability_records: HashMap<NodeId, Vec<bool>> = HashMap::new();
        for id in 0..self.graph.node_count {
            availability_records.insert(id, Vec::with_capacity(total_ticks as usize));
        }

        // Progress bar (hidden when live tick output is enabled)
        let live_ticks = self.config.simulation.streaming.live_ticks;
        let pb = if live_ticks {
            indicatif::ProgressBar::hidden()
        } else {
            crate::output::cli::create_sim_progress(total_ticks)
        };

        // Pre-compute per-node activity weights for author selection
        let activity_weights: Vec<f64> = match self.config.events.activity_distribution.as_str() {
            "power_law" => {
                // Zipf distribution: weight_i = 1 / rank^s
                // Sort nodes by degree (hub nodes = higher activity) for correlation
                let mut ranked: Vec<(NodeId, usize)> = (0..self.graph.node_count)
                    .map(|id| {
                        let degree = self.graph.follows.get(&id).map_or(0, |f| f.len())
                            + self.graph.followers.get(&id).map_or(0, |f| f.len());
                        (id, degree)
                    })
                    .collect();
                ranked.sort_by(|a, b| b.1.cmp(&a.1)); // highest degree first

                let s = self.config.events.activity_skew;
                let mut weights = vec![0.0f64; node_count];
                for (rank, &(id, _)) in ranked.iter().enumerate() {
                    weights[id as usize] = 1.0 / ((rank + 1) as f64).powf(s);
                }
                // Normalize so weights sum to node_count (preserves total event rate)
                let sum: f64 = weights.iter().sum();
                let scale = node_count as f64 / sum;
                weights.iter().map(|w| w * scale).collect()
            }
            _ => {
                // Uniform: all weights = 1.0
                vec![1.0f64; node_count]
            }
        };

        // Build cumulative distribution for weighted sampling
        let total_weight: f64 = activity_weights.iter().sum();
        let cumulative_weights: Vec<f64> = activity_weights
            .iter()
            .scan(0.0, |acc, &w| {
                *acc += w / total_weight;
                Some(*acc)
            })
            .collect();

        // Track whether the partition is currently active
        let mut partition_active = false;

        // 6. Simulation loop
        for tick in 0..total_ticks {
            let time = tick as f64 * tick_interval_secs;
            self.clock.advance_to(time);

            // Apply or clear network partition based on schedule
            if let Some(ref schedule) = self.partition_schedule {
                if time >= schedule.start_time && time < schedule.end_time {
                    if !partition_active {
                        // Activate partition
                        let mut guard = self.partition_map.write().unwrap();
                        *guard = Some(schedule.map.clone());
                        partition_active = true;
                    }
                } else if partition_active {
                    // Deactivate partition (heal)
                    let mut guard = self.partition_map.write().unwrap();
                    *guard = None;
                    partition_active = false;
                }
            }

            // Set each node online/offline based on uptime
            for id in 0..self.graph.node_count {
                let node_type = *self
                    .graph
                    .node_types
                    .get(&id)
                    .unwrap_or(&NodeType::Light);
                let uptime = match node_type {
                    NodeType::Full => self.config.network.full_uptime,
                    NodeType::Light => self.config.network.light_uptime,
                };

                let roll: f64 = self.rng.gen();
                let mut is_online = roll < uptime;

                // Check offline overrides: force offline during scheduled windows
                if let Some(windows) = self.offline_overrides.get(&id) {
                    for &(start, end) in windows {
                        if time >= start && time < end {
                            is_online = false;
                            break;
                        }
                    }
                }

                let msg = if is_online {
                    Message::GoOnline
                } else {
                    Message::GoOffline
                };

                // Record orchestrator's authoritative availability decision
                availability_records.entry(id).or_default().push(is_online);

                if let Some(tx) = node_senders.get(&id) {
                    let _ = tx.send(msg).await;
                }
            }

            // Send Tick to every node
            for id in 0..self.graph.node_count {
                if let Some(tx) = node_senders.get(&id) {
                    let _ = tx.send(Message::Tick(time)).await;
                }
            }

            // Generate random publishes
            // Use Poisson-like approach: whole events + fractional probability
            let whole_events = events_per_tick as u64;
            let frac = events_per_tick - whole_events as f64;
            let extra = if self.rng.gen::<f64>() < frac { 1u64 } else { 0 };
            let n_events = whole_events + extra;

            for _ in 0..n_events {
                // Weighted author selection (uniform or power-law)
                let roll: f64 = self.rng.gen();
                let author = cumulative_weights
                    .binary_search_by(|w| w.partial_cmp(&roll).unwrap())
                    .unwrap_or_else(|i| i)
                    .min(node_count - 1) as NodeId;
                let kind = self.random_event_kind();
                let size = self.event_size(kind);

                let seq = seq_counters.entry(author).or_insert(0);
                *seq += 1;
                let prev_hash = *prev_hashes.get(&author).unwrap_or(&0);

                let nostr_json = if let Some(ref reg) = registry {
                    let kind_u16 = crate::nostr_bridge::sim_kind_to_nostr_kind(kind);
                    let content = format!("sim-event-{}", event_id_counter);
                    crate::nostr_bridge::create_signed_event(
                        reg,
                        author,
                        kind_u16,
                        &content,
                        vec![],
                        time as u64,
                    )
                } else {
                    None
                };
                // For Reaction/Repost, pick a random interaction target from follows
                // and record the interaction for referral scoring
                let interaction_target = match kind {
                    EventKind::Reaction | EventKind::Repost => {
                        let follows = self.graph.follows.get(&author);
                        follows.and_then(|f| {
                            if f.is_empty() {
                                None
                            } else {
                                let idx = self.rng.gen_range(0..f.len());
                                let target = f.iter().nth(idx).copied().unwrap();
                                let weight = match kind {
                                    EventKind::Repost => self.config.retrieval.interaction_weight_repost,
                                    _ => self.config.retrieval.interaction_weight_reaction,
                                };
                                self.interaction_tracker.record(author, target, weight, time);
                                Some(target)
                            }
                        })
                    }
                    _ => None,
                };

                let event = Event {
                    id: event_id_counter,
                    author,
                    kind,
                    size_bytes: size,
                    seq: *seq,
                    prev_hash,
                    created_at: time,
                    nostr_json,
                    interaction_target,
                };
                prev_hashes.insert(author, event_id_counter);
                event_id_counter += 1;

                if let Some(tx) = node_senders.get(&author) {
                    let _ = tx.send(Message::Publish(event)).await;
                }
            }

            // Generate random read requests
            let reads_per_tick = node_count as f64
                * self.config.network.dau_pct
                * self.config.retrieval.reads_per_day as f64
                / ticks_per_day;
            let whole_reads = reads_per_tick as u64;
            let frac_read = reads_per_tick - whole_reads as f64;
            let extra_read = if self.rng.gen::<f64>() < frac_read { 1u64 } else { 0 };
            let n_reads = whole_reads + extra_read;

            for _ in 0..n_reads {
                let reader = self.rng.gen_range(0..self.graph.node_count);

                // Get tier pools for this reader
                let direct = self.wot_tiers.direct_wot.get(&reader);
                let one_hop_set = self.wot_tiers.one_hop.get(&reader);
                let two_hop_map = self.wot_tiers.two_hop.get(&reader);

                let n_direct = direct.map_or(0, |s| s.len());
                let n_two_hop = two_hop_map.map_or(0, |m| m.len());

                // Expand Orbit with referral-promoted authors
                let ic_contacts: Vec<NodeId> = direct
                    .map(|s| s.iter().copied().collect())
                    .unwrap_or_default();
                let referrals = self.interaction_tracker.compute_referrals(
                    &ic_contacts,
                    self.config.retrieval.referral_min_contacts,
                    self.config.retrieval.referral_min_score,
                    time,
                );
                let mut orbit_pool: HashSet<NodeId> = one_hop_set
                    .cloned()
                    .unwrap_or_default();
                for author in &referrals {
                    if *author != reader
                        && !direct.map_or(false, |d| d.contains(author))
                    {
                        orbit_pool.insert(*author);
                    }
                }
                let n_orbit = orbit_pool.len();

                if n_direct + n_orbit + n_two_hop == 0 {
                    continue;
                }

                // Auto-redistribute weights: skip empty tiers, redistribute their weight
                let cfg = &self.config.retrieval;
                let raw = [
                    (cfg.feed_weight_inner_circle, n_direct),
                    (cfg.feed_weight_orbit, n_orbit),
                    (cfg.feed_weight_horizon, n_two_hop),
                ];
                let total_weight: f64 = raw.iter()
                    .filter(|(_, n)| *n > 0)
                    .map(|(w, _)| w)
                    .sum();
                if total_weight < f64::EPSILON {
                    continue;
                }

                // Pick tier via weighted random
                let roll = self.rng.gen::<f64>() * total_weight;
                let mut cumulative = 0.0;
                let mut chosen_tier = WotTier::Orbit;
                let mut target_author = None;

                for (i, (w, n)) in raw.iter().enumerate() {
                    if *n == 0 { continue; }
                    cumulative += w;
                    if roll < cumulative {
                        match i {
                            0 => {
                                chosen_tier = WotTier::InnerCircle;
                                let pool: Vec<NodeId> = direct.unwrap().iter().copied().collect();
                                target_author = Some(pool[self.rng.gen_range(0..pool.len())]);
                            }
                            1 => {
                                chosen_tier = WotTier::Orbit;
                                let pool: Vec<NodeId> = orbit_pool.iter().copied().collect();
                                target_author = Some(pool[self.rng.gen_range(0..pool.len())]);
                            }
                            2 => {
                                chosen_tier = WotTier::Horizon;
                                let map = two_hop_map.unwrap();
                                let total_score: u32 = map.values().sum();
                                let mut score_roll = self.rng.gen_range(0..total_score);
                                for (&author, &score) in map {
                                    if score_roll < score {
                                        target_author = Some(author);
                                        break;
                                    }
                                    score_roll -= score;
                                }
                                if target_author.is_none() {
                                    target_author = map.keys().next().copied();
                                }
                            }
                            _ => unreachable!(),
                        }
                        break;
                    }
                }

                let target_author = match target_author {
                    Some(a) => a,
                    None => continue,
                };

                if let Some(tx) = node_senders.get(&reader) {
                    let _ = tx
                        .send(Message::ReadRequest {
                            target_author,
                            request_id: read_request_id_counter,
                            wot_tier: chosen_tier,
                        })
                        .await;
                    read_request_id_counter += 1;
                }
            }

            // Yield to let tasks process
            yield_now().await;
            pb.inc(1);

            // Send TickComplete so metrics collector can summarise this tick
            let _ = metrics_tx
                .send(crate::node::MetricEvent::TickComplete { tick, time })
                .await;
        }

        crate::output::cli::finish_progress(&pb);

        // Clear any active partition before shutdown
        if partition_active {
            let mut guard = self.partition_map.write().unwrap();
            *guard = None;
        }

        // 7. Send Shutdown to all nodes
        for id in 0..self.graph.node_count {
            if let Some(tx) = node_senders.get(&id) {
                let _ = tx.send(Message::Shutdown).await;
            }
        }

        // 8. Drop senders so router and metrics collector can finish
        drop(node_senders);
        drop(router_tx);
        drop(metrics_tx);

        // 9. Await metrics collector
        let collected = metrics_handle
            .await
            .expect("metrics collector task panicked");

        // 10. Return result
        SimResult {
            metrics: collected,
            graph: self.graph,
            config: self.config,
            availability_records,
            activity_weights,
        }
    }

    // ── Helper methods ─────────────────────────────────────────────

    /// Sample a random event kind according to the configured event mix.
    fn random_event_kind(&mut self) -> EventKind {
        let mix = &self.config.events.mix;
        let roll: f64 = self.rng.gen();

        let mut cumulative = mix.note;
        if roll < cumulative {
            return EventKind::Note;
        }
        cumulative += mix.reaction;
        if roll < cumulative {
            return EventKind::Reaction;
        }
        cumulative += mix.repost;
        if roll < cumulative {
            return EventKind::Repost;
        }
        cumulative += mix.dm;
        if roll < cumulative {
            return EventKind::Dm;
        }
        EventKind::LongForm
    }

    /// Look up the configured byte size for a given event kind.
    fn event_size(&self, kind: EventKind) -> u32 {
        let e = &self.config.events;
        match kind {
            EventKind::Note => e.note_bytes,
            EventKind::Reaction => e.reaction_bytes,
            EventKind::Repost => e.repost_bytes,
            EventKind::Dm => e.dm_bytes,
            EventKind::LongForm => e.longform_bytes,
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_event_seq_increments_per_author() {
        let mut config = SimConfig::default();
        config.graph.nodes = 5;
        config.graph.ba_edges_per_node = 2;
        config.simulation.duration_days = 1;
        config.simulation.tick_interval_secs = 3600;
        config.simulation.deterministic = true;

        let mut seq_counters: HashMap<NodeId, u64> = HashMap::new();
        let mut prev_hashes: HashMap<NodeId, u64> = HashMap::new();
        let mut rng = ChaCha8Rng::seed_from_u64(config.graph.seed.wrapping_add(1));

        // Simulate event creation like orchestrator does
        let mut event_id_counter = 0u64;
        for _ in 0..10 {
            let author = rng.gen_range(0..5u32);
            let seq = seq_counters.entry(author).or_insert(0);
            *seq += 1;
            let prev_hash = *prev_hashes.get(&author).unwrap_or(&0);

            let event = Event {
                id: event_id_counter,
                author,
                kind: EventKind::Note,
                size_bytes: 100,
                seq: *seq,
                prev_hash,
                created_at: 0.0,
                nostr_json: None,
                interaction_target: None,
            };
            prev_hashes.insert(author, event_id_counter);
            event_id_counter += 1;

            // seq should be incrementing for each author
            assert!(event.seq > 0, "seq should be > 0");
        }

        // Check that at least one author has seq > 1
        let max_seq = seq_counters.values().max().copied().unwrap_or(0);
        assert!(max_seq > 1, "expected at least one author with seq > 1");
    }

    #[tokio::test]
    async fn test_orchestrator_sends_tick() {
        let mut config = SimConfig::default();
        config.graph.nodes = 5;
        config.graph.ba_edges_per_node = 2;
        config.simulation.duration_days = 1;
        config.simulation.tick_interval_secs = 86400; // 1 tick
        config.simulation.deterministic = true;

        let orchestrator = Orchestrator::new(config);
        let result = orchestrator.run().await;

        // Nodes should have received Tick and emitted snapshots
        assert!(
            !result.metrics.snapshots.is_empty(),
            "expected snapshots from Tick handling"
        );
    }

    #[tokio::test]
    async fn test_orchestrator_small_run() {
        let mut config = SimConfig::default();
        config.graph.nodes = 50;
        config.graph.ba_edges_per_node = 3;
        config.simulation.duration_days = 1;
        config.simulation.tick_interval_secs = 3600;
        config.simulation.deterministic = true;

        let orchestrator = Orchestrator::new(config);
        assert_eq!(orchestrator.graph.node_count, 50);

        let result = orchestrator.run().await;

        // Graph should still have 50 nodes
        assert_eq!(result.graph.node_count, 50);

        // Metrics should be non-empty: we generated events so at least
        // some nodes should have published or received something.
        let has_snapshots = !result.metrics.snapshots.is_empty();
        let has_deliveries = !result.metrics.event_deliveries.is_empty();
        assert!(
            has_snapshots || has_deliveries,
            "expected non-empty metrics from a 1-day / 50-node run"
        );
    }

    #[tokio::test]
    async fn test_availability_records_has_all_nodes() {
        let mut config = SimConfig::default();
        config.graph.nodes = 20;
        config.graph.ba_edges_per_node = 2;
        config.simulation.duration_days = 1;
        config.simulation.tick_interval_secs = 3600; // 24 ticks
        config.simulation.deterministic = true;

        let orchestrator = Orchestrator::new(config.clone());
        let result = orchestrator.run().await;

        // availability_records should have an entry for every node
        assert_eq!(
            result.availability_records.len(),
            config.graph.nodes as usize,
            "availability_records should have entries for all {} nodes, got {}",
            config.graph.nodes,
            result.availability_records.len()
        );

        // Each node should have exactly total_ticks samples
        let total_ticks = (config.simulation.duration_days as f64 * 86_400.0
            / config.simulation.tick_interval_secs as f64) as usize;
        for id in 0..config.graph.nodes {
            let samples = result
                .availability_records
                .get(&id)
                .expect("node should have availability records");
            assert_eq!(
                samples.len(),
                total_ticks,
                "node {} should have {} availability samples, got {}",
                id,
                total_ticks,
                samples.len()
            );
        }
    }

    #[tokio::test]
    async fn test_orchestrator_generates_read_requests() {
        let mut config = SimConfig::default();
        config.graph.nodes = 50;
        config.graph.ba_edges_per_node = 3;
        config.simulation.duration_days = 1;
        config.simulation.tick_interval_secs = 3600; // 24 ticks
        config.simulation.deterministic = true;

        let orchestrator = Orchestrator::new(config);
        let result = orchestrator.run().await;

        // With 50 nodes, dau_pct=0.50, reads_per_day=50, over 1 day,
        // we expect approximately 50 * 0.50 * 50 = 1250 read requests.
        // Some may be skipped (no follows), but read_results should be non-empty.
        assert!(
            !result.metrics.read_results.is_empty(),
            "expected non-empty read_results from a 50-node, 1-day sim"
        );
    }

    #[tokio::test]
    async fn test_availability_records_online_fraction_matches_expected() {
        let mut config = SimConfig::default();
        config.graph.nodes = 200;
        config.graph.ba_edges_per_node = 3;
        config.simulation.duration_days = 7;
        config.simulation.tick_interval_secs = 3600; // 168 ticks
        config.simulation.deterministic = true;

        let orchestrator = Orchestrator::new(config.clone());
        let result = orchestrator.run().await;

        // Compute observed online fraction from availability_records
        let mut total_samples: usize = 0;
        let mut online_samples: usize = 0;
        for samples in result.availability_records.values() {
            for &online in samples {
                total_samples += 1;
                if online {
                    online_samples += 1;
                }
            }
        }
        let observed_fraction = online_samples as f64 / total_samples as f64;

        // Expected: full_node_pct * full_uptime + light_node_pct * light_uptime
        let expected_fraction = config.online_fraction();

        // With 200 nodes * 168 ticks = 33,600 samples, the law of large
        // numbers gives us convergence. Allow 10% relative tolerance to
        // account for the non-uniform node type distribution in the graph.
        let deviation = (observed_fraction - expected_fraction).abs() / expected_fraction;
        assert!(
            deviation < 0.10,
            "online fraction {:.4} should be within 10% of expected {:.4} (deviation: {:.2}%)",
            observed_fraction,
            expected_fraction,
            deviation * 100.0
        );
    }

    #[tokio::test]
    async fn test_power_law_activity_distribution() {
        let mut config = SimConfig::default();
        config.graph.nodes = 100;
        config.graph.ba_edges_per_node = 5;
        config.simulation.duration_days = 7;
        config.simulation.tick_interval_secs = 3600; // 168 ticks
        config.simulation.deterministic = true;
        config.events.activity_distribution = "power_law".to_string();
        config.events.activity_skew = 1.2;

        let orchestrator = Orchestrator::new(config);
        let result = orchestrator.run().await;

        // Count events per author from event_deliveries
        let mut events_per_node: HashMap<NodeId, u64> = HashMap::new();
        for edm in result.metrics.event_deliveries.values() {
            *events_per_node.entry(edm.author).or_insert(0) += 1;
        }

        let mut counts: Vec<u64> = events_per_node.values().copied().collect();
        counts.sort();

        assert!(
            counts.len() >= 10,
            "expected at least 10 distinct authors, got {}",
            counts.len()
        );

        let top_10_pct = &counts[counts.len() * 9 / 10..];
        let bottom_10_pct = &counts[..counts.len() / 10];

        let top_avg: f64 = top_10_pct.iter().sum::<u64>() as f64 / top_10_pct.len() as f64;
        let bottom_avg: f64 = if bottom_10_pct.is_empty() {
            1.0
        } else {
            bottom_10_pct.iter().sum::<u64>().max(1) as f64 / bottom_10_pct.len() as f64
        };

        // Top 10% should publish at least 3x more than bottom 10%
        assert!(
            top_avg / bottom_avg > 3.0,
            "expected power-law skew: top_avg={:.1}, bottom_avg={:.1}, ratio={:.1}",
            top_avg,
            bottom_avg,
            top_avg / bottom_avg
        );
    }

    #[test]
    fn test_interaction_tracker_basic() {
        let mut tracker = InteractionTracker::new(30.0);
        tracker.record(0, 5, 1.0, 100.0);
        tracker.record(0, 5, 1.0, 100.0);
        let score = tracker.score(0, 5, 100.0);
        assert!(score > 1.9 && score < 2.1, "score={}", score);
    }

    #[test]
    fn test_interaction_tracker_decay() {
        let mut tracker = InteractionTracker::new(30.0);
        tracker.record(0, 5, 1.0, 0.0);
        let score_at_21d = tracker.score(0, 5, 21.0 * 86400.0);
        // exp(-21/30) ~ 0.497
        assert!(score_at_21d > 0.4 && score_at_21d < 0.6, "score={}", score_at_21d);
    }

    #[test]
    fn test_interaction_tracker_referrals() {
        let mut tracker = InteractionTracker::new(30.0);
        let ic_contacts = vec![1, 2, 3, 4];
        let time = 100.0;
        // 3 of 4 IC contacts interact with author 99
        tracker.record(1, 99, 3.0, time);
        tracker.record(2, 99, 2.0, time);
        tracker.record(3, 99, 1.0, time);
        let referrals = tracker.compute_referrals(&ic_contacts, 3, 0.5, time);
        assert!(referrals.contains(&99));
    }

    #[test]
    fn test_interaction_tracker_below_threshold() {
        let mut tracker = InteractionTracker::new(30.0);
        let ic_contacts = vec![1, 2, 3];
        let time = 100.0;
        // Only 2 contacts interact with author 88 — below min_contacts=3
        tracker.record(1, 88, 5.0, time);
        tracker.record(2, 88, 5.0, time);
        let referrals = tracker.compute_referrals(&ic_contacts, 3, 0.5, time);
        assert!(!referrals.contains(&88));
    }
}
