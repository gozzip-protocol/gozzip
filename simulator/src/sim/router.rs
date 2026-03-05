use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};
use std::sync::{Arc, RwLock};

use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use rand_distr::{Distribution, Normal};
use tokio::sync::mpsc;

use crate::config::SimConfig;
use crate::sim::clock::SimClock;
use crate::types::{DeliveryPath, Message, NodeId, SimTime};

/// Shared partition map: when `Some`, maps each NodeId to a partition index.
/// Messages between nodes in different partitions are dropped.
pub type PartitionMap = Arc<RwLock<Option<HashMap<NodeId, usize>>>>;

// ── Envelope ─────────────────────────────────────────────────────────

/// A routable message wrapper with source, destination and scheduled delivery time.
pub struct Envelope {
    pub from: NodeId,
    pub message: Message,
    pub to: NodeId,
    pub deliver_at: SimTime,
    pub path_hint: Option<DeliveryPath>,
}

// ── Scheduled envelope for BinaryHeap ────────────────────────────────

struct ScheduledEnvelope {
    from: NodeId,
    deliver_at: SimTime,
    to: NodeId,
    message: Message,
}

impl PartialEq for ScheduledEnvelope {
    fn eq(&self, other: &Self) -> bool {
        self.deliver_at == other.deliver_at
    }
}

impl Eq for ScheduledEnvelope {}

impl PartialOrd for ScheduledEnvelope {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ScheduledEnvelope {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reverse ordering for min-heap (earliest first)
        other
            .deliver_at
            .partial_cmp(&self.deliver_at)
            .unwrap_or(Ordering::Equal)
    }
}

// ── Router ───────────────────────────────────────────────────────────

/// Central message router that receives envelopes and delivers messages
/// to the appropriate node channels.
pub struct Router {
    inbox: mpsc::Receiver<Envelope>,
    node_senders: HashMap<NodeId, mpsc::Sender<Message>>,
    clock: SimClock,
    config: SimConfig,
    rng: ChaCha8Rng,
    partition_map: PartitionMap,
}

impl Router {
    /// Create a new router.
    ///
    /// The RNG is seeded from `config.graph.seed + 1000` to avoid
    /// colliding with graph-generation seeds.
    pub fn new(
        inbox: mpsc::Receiver<Envelope>,
        node_senders: HashMap<NodeId, mpsc::Sender<Message>>,
        clock: SimClock,
        config: SimConfig,
    ) -> Self {
        let rng = ChaCha8Rng::seed_from_u64(config.graph.seed.wrapping_add(1000));
        Self {
            inbox,
            node_senders,
            clock,
            config,
            rng,
            partition_map: Arc::new(RwLock::new(None)),
        }
    }

    /// Create a router with a shared partition map for network partition simulation.
    ///
    /// When the partition map is `Some`, messages between nodes in different
    /// partitions are silently dropped.
    pub fn with_partitions(
        inbox: mpsc::Receiver<Envelope>,
        node_senders: HashMap<NodeId, mpsc::Sender<Message>>,
        clock: SimClock,
        config: SimConfig,
        partition_map: PartitionMap,
    ) -> Self {
        let rng = ChaCha8Rng::seed_from_u64(config.graph.seed.wrapping_add(1000));
        Self {
            inbox,
            node_senders,
            clock,
            config,
            rng,
            partition_map,
        }
    }

    /// Check if a message from `from` to `to` should be dropped due to partitioning.
    fn is_partitioned(&self, from: NodeId, to: NodeId) -> bool {
        let guard = self.partition_map.read().unwrap();
        if let Some(map) = guard.as_ref() {
            let from_partition = map.get(&from).copied();
            let to_partition = map.get(&to).copied();
            match (from_partition, to_partition) {
                (Some(fp), Some(tp)) => fp != tp,
                // If a node is not in the partition map, allow the message
                _ => false,
            }
        } else {
            false
        }
    }

    /// Run the router loop: receive envelopes and deliver to target nodes.
    ///
    /// The loop exits when all senders to the inbox have been dropped
    /// (i.e., `recv()` returns `None`).
    pub async fn run(mut self) {
        let default_dist = Normal::new(
            self.config.simulation.latency_ms_mean,
            self.config.simulation.latency_ms_stddev,
        )
        .unwrap_or_else(|_| Normal::new(50.0, 20.0).unwrap());

        let cached_dist = Normal::new(
            self.config.latency.cached_endpoint_base_ms,
            self.config.latency.cached_endpoint_jitter_ms.max(0.001),
        )
        .unwrap_or_else(|_| Normal::new(60.0, 20.0).unwrap());

        let gossip_dist = Normal::new(
            self.config.latency.gossip_per_hop_base_ms,
            self.config.latency.gossip_per_hop_jitter_ms.max(0.001),
        )
        .unwrap_or_else(|_| Normal::new(80.0, 30.0).unwrap());

        let relay_dist = Normal::new(
            self.config.latency.relay_base_ms,
            self.config.latency.relay_jitter_ms.max(0.001),
        )
        .unwrap_or_else(|_| Normal::new(200.0, 50.0).unwrap());

        if self.config.simulation.deterministic {
            self.run_deterministic(default_dist, cached_dist, gossip_dist, relay_dist).await;
        } else {
            self.run_nondeterministic(default_dist, cached_dist, gossip_dist, relay_dist).await;
        }
    }

    fn sample_latency(
        path_hint: Option<DeliveryPath>,
        default_dist: &Normal<f64>,
        cached_dist: &Normal<f64>,
        gossip_dist: &Normal<f64>,
        relay_dist: &Normal<f64>,
        rng: &mut ChaCha8Rng,
    ) -> f64 {
        let dist = match path_hint {
            Some(DeliveryPath::CachedEndpoint) | Some(DeliveryPath::ReadCache) => cached_dist,
            Some(DeliveryPath::Gossip) => gossip_dist,
            Some(DeliveryPath::Relay) => relay_dist,
            None => default_dist,
        };
        dist.sample(rng).max(1.0)
    }

    async fn run_deterministic(
        &mut self,
        default_dist: Normal<f64>,
        cached_dist: Normal<f64>,
        gossip_dist: Normal<f64>,
        relay_dist: Normal<f64>,
    ) {
        let mut heap: BinaryHeap<ScheduledEnvelope> = BinaryHeap::new();

        while let Some(envelope) = self.inbox.recv().await {
            // Drop messages between different partitions
            if self.is_partitioned(envelope.from, envelope.to) {
                continue;
            }

            // Sample latency and compute delivery time
            let latency_ms = Self::sample_latency(
                envelope.path_hint,
                &default_dist, &cached_dist, &gossip_dist, &relay_dist,
                &mut self.rng,
            );
            let deliver_at = envelope.deliver_at + latency_ms / 1000.0;

            heap.push(ScheduledEnvelope {
                from: envelope.from,
                deliver_at,
                to: envelope.to,
                message: envelope.message,
            });

            // Drain all ready envelopes (those at or before current clock time)
            // We advance the clock to the next delivery time and deliver
            while let Some(scheduled) = heap.peek() {
                let sched_time = scheduled.deliver_at;
                if sched_time <= self.clock.now() + latency_ms / 1000.0 {
                    let scheduled = heap.pop().unwrap();
                    if sched_time > self.clock.now() {
                        self.clock.advance_to(sched_time);
                    }
                    // Re-check partition at delivery time (partition may have changed)
                    if !self.is_partitioned(scheduled.from, scheduled.to) {
                        if let Some(sender) = self.node_senders.get(&scheduled.to) {
                            let _ = sender.try_send(scheduled.message);
                        }
                    }
                } else {
                    break;
                }
            }
        }

        // Drain remaining envelopes
        while let Some(scheduled) = heap.pop() {
            if scheduled.deliver_at > self.clock.now() {
                self.clock.advance_to(scheduled.deliver_at);
            }
            if !self.is_partitioned(scheduled.from, scheduled.to) {
                if let Some(sender) = self.node_senders.get(&scheduled.to) {
                    let _ = sender.try_send(scheduled.message);
                }
            }
        }
    }

    async fn run_nondeterministic(
        &mut self,
        default_dist: Normal<f64>,
        cached_dist: Normal<f64>,
        gossip_dist: Normal<f64>,
        relay_dist: Normal<f64>,
    ) {
        while let Some(envelope) = self.inbox.recv().await {
            // Drop messages between different partitions
            if self.is_partitioned(envelope.from, envelope.to) {
                continue;
            }

            let latency_ms = Self::sample_latency(
                envelope.path_hint,
                &default_dist, &cached_dist, &gossip_dist, &relay_dist,
                &mut self.rng,
            );
            let deliver_at = envelope.deliver_at + latency_ms / 1000.0;

            if deliver_at > self.clock.now() {
                self.clock.advance_to(deliver_at);
            }

            if let Some(sender) = self.node_senders.get(&envelope.to) {
                let _ = sender.try_send(envelope.message);
            }
        }
    }
}

// ── Helper ───────────────────────────────────────────────────────────

/// Create a router channel pair with the given buffer size.
pub fn create_router_channel(buffer: usize) -> (mpsc::Sender<Envelope>, mpsc::Receiver<Envelope>) {
    mpsc::channel(buffer)
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_router_delivers_message() {
        // Create router channel
        let (router_tx, router_rx) = create_router_channel(16);

        // Create a node channel for node 0
        let (node_tx, mut node_rx) = mpsc::channel::<Message>(16);

        let mut node_senders = HashMap::new();
        node_senders.insert(0u32, node_tx);

        let clock = SimClock::new(true);
        let config = SimConfig::default();

        let router = Router::new(router_rx, node_senders, clock, config);

        // Spawn the router
        let handle = tokio::spawn(router.run());

        // Send a Shutdown envelope to node 0
        router_tx
            .send(Envelope {
                from: 0,
                message: Message::Shutdown,
                to: 0,
                deliver_at: 1.0,
                path_hint: None,
            })
            .await
            .expect("send envelope");

        // Drop the sender so the router loop can exit
        drop(router_tx);

        // Verify node 0 receives the message
        let received = node_rx.recv().await.expect("receive message");
        assert!(matches!(received, Message::Shutdown));

        // Wait for router to finish
        handle.await.expect("router task");
    }

    #[tokio::test]
    async fn test_deterministic_router_delivers_in_order() {
        let (router_tx, router_rx) = create_router_channel(16);
        let (node_tx, mut node_rx) = mpsc::channel::<Message>(16);

        let mut node_senders = HashMap::new();
        node_senders.insert(0u32, node_tx);

        let clock = SimClock::new(true);
        let mut config = SimConfig::default();
        config.simulation.deterministic = true;
        config.simulation.latency_ms_mean = 10.0;
        config.simulation.latency_ms_stddev = 0.001; // near-zero stddev for predictable ordering

        let router = Router::new(router_rx, node_senders, clock, config);
        let handle = tokio::spawn(router.run());

        // Send envelopes with increasing deliver_at
        for i in 0..3 {
            router_tx
                .send(Envelope {
                    from: 0,
                    message: Message::Tick(i as f64),
                    to: 0,
                    deliver_at: i as f64 * 1.0,
                    path_hint: None,
                })
                .await
                .unwrap();
        }

        drop(router_tx);

        // Verify all messages are delivered
        let mut received = Vec::new();
        while let Some(msg) = node_rx.recv().await {
            if let Message::Tick(t) = msg {
                received.push(t);
            }
        }

        assert_eq!(received.len(), 3, "expected 3 messages delivered");
        // Should be in order since deliver_at increases
        for i in 1..received.len() {
            assert!(
                received[i] >= received[i - 1],
                "messages should be in order"
            );
        }

        handle.await.expect("router task");
    }

    #[tokio::test]
    async fn test_latency_values_positive() {
        let (router_tx, router_rx) = create_router_channel(16);
        let (node_tx, mut node_rx) = mpsc::channel::<Message>(16);

        let mut node_senders = HashMap::new();
        node_senders.insert(0u32, node_tx);

        let clock = SimClock::new(true);
        let mut config = SimConfig::default();
        config.simulation.deterministic = true;

        let router = Router::new(router_rx, node_senders, clock.clone(), config);
        let handle = tokio::spawn(router.run());

        // Send an envelope at time 0
        router_tx
            .send(Envelope {
                from: 0,
                message: Message::Tick(0.0),
                to: 0,
                deliver_at: 0.0,
                path_hint: None,
            })
            .await
            .unwrap();

        drop(router_tx);

        // Verify message is delivered
        let msg = node_rx.recv().await.expect("should receive message");
        assert!(matches!(msg, Message::Tick(_)));

        // The clock should have advanced past 0 due to latency
        handle.await.expect("router task");
        assert!(
            clock.now() > 0.0,
            "clock should advance due to latency, got {}",
            clock.now()
        );
    }

    #[tokio::test]
    async fn test_partition_drops_cross_partition_messages() {
        // Node 0 is in partition 0, node 1 is in partition 1.
        // Messages from node 0 to node 1 should be dropped.
        let (router_tx, router_rx) = create_router_channel(16);

        let (node0_tx, mut _node0_rx) = mpsc::channel::<Message>(16);
        let (node1_tx, mut node1_rx) = mpsc::channel::<Message>(16);

        let mut node_senders = HashMap::new();
        node_senders.insert(0u32, node0_tx);
        node_senders.insert(1u32, node1_tx);

        let clock = SimClock::new(true);
        let config = SimConfig::default();

        // Set up partition: node 0 in partition 0, node 1 in partition 1
        let mut pmap = HashMap::new();
        pmap.insert(0u32, 0usize);
        pmap.insert(1u32, 1usize);
        let partition_map: PartitionMap = Arc::new(RwLock::new(Some(pmap)));

        let router = Router::with_partitions(
            router_rx,
            node_senders,
            clock,
            config,
            partition_map,
        );
        let handle = tokio::spawn(router.run());

        // Send a message from node 0 to node 1 (cross-partition — should be dropped)
        router_tx
            .send(Envelope {
                from: 0,
                message: Message::Tick(1.0),
                to: 1,
                deliver_at: 1.0,
                path_hint: None,
            })
            .await
            .unwrap();

        // Drop sender so router exits
        drop(router_tx);
        handle.await.expect("router task");

        // Node 1 should NOT have received the message
        let received = node1_rx.try_recv();
        assert!(
            received.is_err(),
            "cross-partition message should be dropped"
        );
    }

    #[tokio::test]
    async fn test_partition_delivers_same_partition_messages() {
        // Node 0 and node 1 are both in partition 0.
        // Messages between them should be delivered normally.
        let (router_tx, router_rx) = create_router_channel(16);

        let (node0_tx, mut _node0_rx) = mpsc::channel::<Message>(16);
        let (node1_tx, mut node1_rx) = mpsc::channel::<Message>(16);

        let mut node_senders = HashMap::new();
        node_senders.insert(0u32, node0_tx);
        node_senders.insert(1u32, node1_tx);

        let clock = SimClock::new(true);
        let config = SimConfig::default();

        // Set up partition: both nodes in partition 0
        let mut pmap = HashMap::new();
        pmap.insert(0u32, 0usize);
        pmap.insert(1u32, 0usize);
        let partition_map: PartitionMap = Arc::new(RwLock::new(Some(pmap)));

        let router = Router::with_partitions(
            router_rx,
            node_senders,
            clock,
            config,
            partition_map,
        );
        let handle = tokio::spawn(router.run());

        // Send a message from node 0 to node 1 (same partition — should be delivered)
        router_tx
            .send(Envelope {
                from: 0,
                message: Message::Tick(1.0),
                to: 1,
                deliver_at: 1.0,
                path_hint: None,
            })
            .await
            .unwrap();

        // Drop sender so router exits
        drop(router_tx);
        handle.await.expect("router task");

        // Node 1 should have received the message
        let received = node1_rx.try_recv();
        assert!(
            received.is_ok(),
            "same-partition message should be delivered"
        );
    }

    #[tokio::test]
    async fn test_partition_none_allows_all_messages() {
        // When partition_map is None, all messages should be delivered.
        let (router_tx, router_rx) = create_router_channel(16);

        let (node0_tx, mut _node0_rx) = mpsc::channel::<Message>(16);
        let (node1_tx, mut node1_rx) = mpsc::channel::<Message>(16);

        let mut node_senders = HashMap::new();
        node_senders.insert(0u32, node0_tx);
        node_senders.insert(1u32, node1_tx);

        let clock = SimClock::new(true);
        let config = SimConfig::default();

        // No partition (None)
        let partition_map: PartitionMap = Arc::new(RwLock::new(None));

        let router = Router::with_partitions(
            router_rx,
            node_senders,
            clock,
            config,
            partition_map,
        );
        let handle = tokio::spawn(router.run());

        // Send a message from node 0 to node 1
        router_tx
            .send(Envelope {
                from: 0,
                message: Message::Tick(1.0),
                to: 1,
                deliver_at: 1.0,
                path_hint: None,
            })
            .await
            .unwrap();

        drop(router_tx);
        handle.await.expect("router task");

        // Should be delivered when no partition is active
        let received = node1_rx.try_recv();
        assert!(
            received.is_ok(),
            "message should be delivered when partition_map is None"
        );
    }

    #[tokio::test]
    async fn test_path_hint_affects_latency() {
        // Verify that different path_hints produce different latencies.
        // With near-zero stddev, the sampled latency should be very close
        // to the configured base_ms for each path.
        // Default config: cached=60ms, gossip=80ms, relay=200ms.
        // We expect: cached < gossip < relay (on average).

        // Helper: run a batch of messages with a given path_hint and return
        // the average clock advancement (which reflects accumulated latency).
        async fn measure_avg_latency(path_hint: Option<DeliveryPath>) -> f64 {
            let (router_tx, router_rx) = create_router_channel(256);
            let (node_tx, mut node_rx) = mpsc::channel::<Message>(256);

            let mut node_senders = HashMap::new();
            node_senders.insert(0u32, node_tx);

            let clock = SimClock::new(true);
            let mut config = SimConfig::default();
            config.simulation.deterministic = true;
            // Near-zero stddev for predictable results
            config.simulation.latency_ms_stddev = 0.001;
            config.latency.cached_endpoint_jitter_ms = 0.001;
            config.latency.gossip_per_hop_jitter_ms = 0.001;
            config.latency.relay_jitter_ms = 0.001;

            let clock_clone = clock.clone();
            let router = Router::new(router_rx, node_senders, clock, config);
            let handle = tokio::spawn(router.run());

            for i in 0..20u32 {
                router_tx
                    .send(Envelope {
                        from: 0,
                        message: Message::Tick(i as f64),
                        to: 0,
                        deliver_at: (i as f64) * 10.0,
                        path_hint,
                    })
                    .await
                    .unwrap();
            }

            drop(router_tx);
            handle.await.expect("router task");

            // Drain received messages
            while node_rx.try_recv().is_ok() {}

            // Clock advanced = sum of deliver_at + latency offsets.
            // The final clock value reflects cumulative latency.
            clock_clone.now()
        }

        let cached_clock = measure_avg_latency(Some(DeliveryPath::CachedEndpoint)).await;
        let gossip_clock = measure_avg_latency(Some(DeliveryPath::Gossip)).await;
        let relay_clock = measure_avg_latency(Some(DeliveryPath::Relay)).await;

        // With near-zero jitter, the clock advances by sum(deliver_at_i + base_ms/1000).
        // The deliver_at values are the same for all three, so differences come from
        // the per-path base latency: cached(60ms) < gossip(80ms) < relay(200ms).
        assert!(
            cached_clock < gossip_clock,
            "cached ({}) should produce less total latency than gossip ({})",
            cached_clock, gossip_clock,
        );
        assert!(
            gossip_clock < relay_clock,
            "gossip ({}) should produce less total latency than relay ({})",
            gossip_clock, relay_clock,
        );
    }
}
