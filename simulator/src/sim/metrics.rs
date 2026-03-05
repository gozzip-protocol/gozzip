use std::collections::HashMap;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::sync::Arc;

use serde::Serialize;
use tokio::sync::mpsc;

use crate::config::StreamingConfig;
use crate::node::MetricEvent;
use crate::nostr_bridge::NodeRegistry;
use crate::types::{
    BandwidthCounter, Bytes, CacheStats, ChallengeStats, DeliveryPath, FormulaResult, GossipStats,
    NodeId, ReadTier, SimTime, WotTier,
};

// ── NodeMetrics ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize)]
pub struct NodeMetrics {
    pub bandwidth: BandwidthCounter,
    pub gossip: GossipStats,
    pub challenges: ChallengeStats,
    pub cache_stats: CacheStats,
    pub pact_count: usize,
    pub stored_bytes: Bytes,
    pub storage_capacity: Bytes,
    pub storage_used: Bytes,
    pub first_pact_time: Option<SimTime>,
    pub events_published: u64,
    pub events_received: u64,
    pub availability_samples: Vec<bool>,
}

// ── EventDeliveryMetrics ────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize)]
pub struct EventDeliveryMetrics {
    pub author: NodeId,
    pub published_at: SimTime,
    pub deliveries: Vec<(NodeId, SimTime, DeliveryPath)>,
}

// ── AggregateMetrics ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct AggregateMetrics {
    pub data_availability: Percentiles,
    pub content_reach_pct: Percentiles,
    pub gossip_latency_ms: Percentiles,
    pub bandwidth_mb_day_full: Percentiles,
    pub bandwidth_mb_day_light: Percentiles,
    pub formula_results: Vec<FormulaResult>,
}

// ── Percentiles ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct Percentiles {
    pub p50: f64,
    pub p95: f64,
    pub p99: f64,
    pub min: f64,
    pub max: f64,
    pub mean: f64,
}

impl Percentiles {
    pub fn from_values(mut values: Vec<f64>) -> Self {
        if values.is_empty() {
            return Self {
                p50: 0.0,
                p95: 0.0,
                p99: 0.0,
                min: 0.0,
                max: 0.0,
                mean: 0.0,
            };
        }

        values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        let len = values.len();
        let mean = values.iter().sum::<f64>() / len as f64;
        let min = values[0];
        let max = values[len - 1];

        let p50 = percentile_at(&values, 0.50);
        let p95 = percentile_at(&values, 0.95);
        let p99 = percentile_at(&values, 0.99);

        Self {
            p50,
            p95,
            p99,
            min,
            max,
            mean,
        }
    }
}

/// Compute the value at the given percentile (0.0..1.0) using nearest-rank.
fn percentile_at(sorted: &[f64], pct: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = (pct * (sorted.len() - 1) as f64).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

// ── PactEvent ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct PactEvent {
    pub time: SimTime,
    pub kind: PactEventKind,
    pub node: NodeId,
    pub partner: NodeId,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub enum PactEventKind {
    Formed,
    Dropped,
}

// ── ReadResultRecord ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct ReadResultRecord {
    pub reader: NodeId,
    pub target_author: NodeId,
    pub request_id: u64,
    pub tier: ReadTier,
    pub wot_tier: WotTier,
    pub latency_secs: f64,
    pub paths_tried: u8,
    pub time: SimTime,
}

// ── JsonlEvent ─────────────────────────────────────────────────────

#[derive(Serialize)]
#[serde(tag = "type")]
enum JsonlEvent {
    EventPublished {
        tick: u64,
        time: f64,
        author: u32,
        event_id: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        nostr_event: Option<String>,
    },
    ReadResult {
        tick: u64,
        time: f64,
        reader: u32,
        target: u32,
        tier: String,
        wot_tier: String,
        latency_ms: f64,
    },
    PactFormed {
        tick: u64,
        time: f64,
        node: u32,
        partner: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        nostr_event: Option<String>,
    },
    PactDropped {
        tick: u64,
        time: f64,
        node: u32,
        partner: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        nostr_event: Option<String>,
    },
    TickSummary(TickSummary),
}

// ── TickSummary ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct TickSummary {
    pub tick: u64,
    pub time: SimTime,
    pub reads_ok: u64,
    pub reads_fail: u64,
    pub pacts_formed: u64,
    pub pacts_dropped: u64,
    pub gossip_sent: u64,
    pub events_published: u64,
    pub events_delivered: u64,
    pub cum_reads_ok: u64,
    pub cum_reads_fail: u64,
    pub cum_pacts: i64,
}

// ── CollectedMetrics ────────────────────────────────────────────────

pub struct CollectedMetrics {
    pub snapshots: HashMap<NodeId, NodeMetrics>,
    pub event_deliveries: HashMap<u64, EventDeliveryMetrics>,
    /// Per-delivery latencies in seconds (delivery_time - published_at).
    pub delivery_latencies: Vec<f64>,
    /// Chronological log of pact formation and dissolution events.
    pub pact_events: Vec<PactEvent>,
    /// Results of read-request resolution attempts.
    pub read_results: Vec<ReadResultRecord>,
    /// Sample real Nostr event JSONs (when nostr-events feature is active).
    pub sample_events: Vec<String>,
}

// ── MetricsCollector ────────────────────────────────────────────────

pub struct MetricsCollector {
    inbox: mpsc::Receiver<MetricEvent>,
    snapshots: HashMap<NodeId, NodeMetrics>,
    event_deliveries: HashMap<u64, EventDeliveryMetrics>,
    pact_events: Vec<PactEvent>,
    read_results: Vec<ReadResultRecord>,
    // Per-tick counters (reset each tick)
    tick_reads_ok: u64,
    tick_reads_fail: u64,
    tick_pacts_formed: u64,
    tick_pacts_dropped: u64,
    tick_gossip_sent: u64,
    tick_events_published: u64,
    tick_events_delivered: u64,
    // Cumulative counters
    total_reads_ok: u64,
    total_reads_fail: u64,
    total_pacts_formed: u64,
    total_pacts_dropped: u64,
    total_gossip_sent: u64,
    total_events_published: u64,
    total_events_delivered: u64,
    current_tick: u64,
    total_ticks: u64,
    live_ticks: bool,
    // JSONL streaming
    jsonl_writer: Option<BufWriter<File>>,
    jsonl_flush_interval: u32,
    ticks_since_flush: u32,
    // Nostr event signing
    registry: Option<Arc<NodeRegistry>>,
    // Sample real Nostr events for JSON report
    sample_events: Vec<String>,
}

impl MetricsCollector {
    pub fn new(
        inbox: mpsc::Receiver<MetricEvent>,
        total_ticks: u64,
        streaming: &StreamingConfig,
        registry: Option<Arc<NodeRegistry>>,
    ) -> Self {
        let jsonl_writer = if streaming.jsonl_path.is_empty() {
            None
        } else {
            match File::create(&streaming.jsonl_path) {
                Ok(f) => Some(BufWriter::new(f)),
                Err(e) => {
                    eprintln!("Warning: failed to open JSONL file {}: {}", streaming.jsonl_path, e);
                    None
                }
            }
        };
        Self {
            inbox,
            snapshots: HashMap::new(),
            event_deliveries: HashMap::new(),
            pact_events: Vec::new(),
            read_results: Vec::new(),
            tick_reads_ok: 0,
            tick_reads_fail: 0,
            tick_pacts_formed: 0,
            tick_pacts_dropped: 0,
            tick_gossip_sent: 0,
            tick_events_published: 0,
            tick_events_delivered: 0,
            total_reads_ok: 0,
            total_reads_fail: 0,
            total_pacts_formed: 0,
            total_pacts_dropped: 0,
            total_gossip_sent: 0,
            total_events_published: 0,
            total_events_delivered: 0,
            current_tick: 0,
            total_ticks,
            live_ticks: streaming.live_ticks,
            jsonl_writer,
            jsonl_flush_interval: streaming.jsonl_flush_interval,
            ticks_since_flush: 0,
            registry,
            sample_events: Vec::new(),
        }
    }

    pub async fn run(mut self) -> CollectedMetrics {
        while let Some(event) = self.inbox.recv().await {
            match event {
                MetricEvent::EventPublished {
                    author,
                    event_id,
                    time,
                    nostr_json,
                } => {
                    let node = self.snapshots.entry(author).or_default();
                    node.events_published += 1;
                    self.tick_events_published += 1;
                    self.total_events_published += 1;

                    // Collect up to 10 sample Nostr events for the JSON report
                    if let Some(ref json) = nostr_json {
                        if self.sample_events.len() < 10 {
                            self.sample_events.push(json.clone());
                        }
                    }

                    self.write_jsonl(&JsonlEvent::EventPublished {
                        tick: self.current_tick,
                        time,
                        author,
                        event_id,
                        nostr_event: nostr_json,
                    });

                    self.event_deliveries.insert(
                        event_id,
                        EventDeliveryMetrics {
                            author,
                            published_at: time,
                            deliveries: Vec::new(),
                        },
                    );
                }

                MetricEvent::EventDelivered {
                    event_id,
                    to,
                    time,
                    path,
                    ..
                } => {
                    if let Some(delivery) = self.event_deliveries.get_mut(&event_id) {
                        delivery.deliveries.push((to, time, path));
                    }

                    let node = self.snapshots.entry(to).or_default();
                    node.events_received += 1;
                    self.tick_events_delivered += 1;
                    self.total_events_delivered += 1;
                }

                MetricEvent::NodeSnapshot {
                    id,
                    online,
                    bandwidth,
                    gossip,
                    challenges,
                    cache_stats,
                    pact_count,
                    stored_bytes,
                    storage_capacity,
                    storage_used,
                    first_pact_time,
                    ..
                } => {
                    let node = self.snapshots.entry(id).or_default();
                    node.availability_samples.push(online);
                    node.bandwidth = bandwidth;
                    node.gossip = gossip;
                    node.challenges = challenges;
                    node.cache_stats = cache_stats;
                    node.pact_count = pact_count;
                    node.stored_bytes = stored_bytes;
                    node.storage_capacity = storage_capacity;
                    node.storage_used = storage_used;
                    node.first_pact_time = first_pact_time;
                }

                MetricEvent::PactFormed { node, partner, time } => {
                    let metrics = self.snapshots.entry(node).or_default();
                    metrics.pact_count += 1;
                    self.tick_pacts_formed += 1;
                    self.total_pacts_formed += 1;
                    #[cfg(feature = "nostr-events")]
                    let nostr_event = self.registry.as_ref().and_then(|reg| {
                        crate::nostr_bridge::create_storage_pact_event(
                            reg, node, partner, "active", 0, time as u64,
                        )
                    });
                    #[cfg(not(feature = "nostr-events"))]
                    let nostr_event: Option<String> = None;
                    self.write_jsonl(&JsonlEvent::PactFormed {
                        tick: self.current_tick,
                        time,
                        node,
                        partner,
                        nostr_event,
                    });
                    self.pact_events.push(PactEvent {
                        time,
                        kind: PactEventKind::Formed,
                        node,
                        partner,
                    });
                }

                MetricEvent::PactDropped { node, partner, time } => {
                    let metrics = self.snapshots.entry(node).or_default();
                    metrics.pact_count = metrics.pact_count.saturating_sub(1);
                    self.tick_pacts_dropped += 1;
                    self.total_pacts_dropped += 1;
                    #[cfg(feature = "nostr-events")]
                    let nostr_event = self.registry.as_ref().and_then(|reg| {
                        crate::nostr_bridge::create_storage_pact_event(
                            reg, node, partner, "dissolved", 0, time as u64,
                        )
                    });
                    #[cfg(not(feature = "nostr-events"))]
                    let nostr_event: Option<String> = None;
                    self.write_jsonl(&JsonlEvent::PactDropped {
                        tick: self.current_tick,
                        time,
                        node,
                        partner,
                        nostr_event,
                    });
                    self.pact_events.push(PactEvent {
                        time,
                        kind: PactEventKind::Dropped,
                        node,
                        partner,
                    });
                }

                MetricEvent::GossipSent { from, .. } => {
                    let node = self.snapshots.entry(from).or_default();
                    node.gossip.sent += 1;
                    self.tick_gossip_sent += 1;
                    self.total_gossip_sent += 1;
                }

                MetricEvent::ChallengeResult {
                    from, passed, ..
                } => {
                    let node = self.snapshots.entry(from).or_default();
                    if passed {
                        node.challenges.passed += 1;
                    } else {
                        node.challenges.failed += 1;
                    }
                }

                MetricEvent::ReadResult {
                    reader,
                    target_author,
                    request_id,
                    ref tier,
                    ref wot_tier,
                    latency_secs,
                    paths_tried,
                    time,
                } => {
                    if *tier == ReadTier::Failed {
                        self.tick_reads_fail += 1;
                        self.total_reads_fail += 1;
                    } else {
                        self.tick_reads_ok += 1;
                        self.total_reads_ok += 1;
                    }
                    self.write_jsonl(&JsonlEvent::ReadResult {
                        tick: self.current_tick,
                        time,
                        reader,
                        target: target_author,
                        tier: format!("{:?}", tier),
                        wot_tier: format!("{:?}", wot_tier),
                        latency_ms: latency_secs * 1000.0,
                    });
                    self.read_results.push(ReadResultRecord {
                        reader,
                        target_author,
                        request_id,
                        tier: tier.clone(),
                        wot_tier: wot_tier.clone(),
                        latency_secs,
                        paths_tried,
                        time,
                    });
                }

                MetricEvent::TickComplete { tick, time } => {
                    let summary = self.build_tick_summary(tick, time);
                    if self.live_ticks {
                        crate::output::cli::print_tick_summary(&summary, self.total_ticks);
                    }
                    self.write_jsonl(&JsonlEvent::TickSummary(summary));
                    // Periodic flush
                    self.ticks_since_flush += 1;
                    if self.ticks_since_flush >= self.jsonl_flush_interval {
                        if let Some(ref mut writer) = self.jsonl_writer {
                            let _ = writer.flush();
                        }
                        self.ticks_since_flush = 0;
                    }
                    // Reset per-tick counters
                    self.tick_reads_ok = 0;
                    self.tick_reads_fail = 0;
                    self.tick_pacts_formed = 0;
                    self.tick_pacts_dropped = 0;
                    self.tick_gossip_sent = 0;
                    self.tick_events_published = 0;
                    self.tick_events_delivered = 0;
                    self.current_tick = tick;
                }
            }
        }

        // Compute per-delivery latencies (seconds) from event deliveries.
        let delivery_latencies: Vec<f64> = self
            .event_deliveries
            .values()
            .flat_map(|edm| {
                edm.deliveries
                    .iter()
                    .map(move |(_to, delivery_time, _path)| delivery_time - edm.published_at)
            })
            .collect();

        CollectedMetrics {
            snapshots: self.snapshots,
            event_deliveries: self.event_deliveries,
            delivery_latencies,
            pact_events: self.pact_events,
            read_results: self.read_results,
            sample_events: self.sample_events,
        }
    }

    fn write_jsonl(&mut self, event: &JsonlEvent) {
        if let Some(ref mut writer) = self.jsonl_writer {
            if let Ok(line) = serde_json::to_string(event) {
                let _ = writeln!(writer, "{}", line);
            }
        }
    }

    fn build_tick_summary(&self, tick: u64, time: SimTime) -> TickSummary {
        TickSummary {
            tick,
            time,
            reads_ok: self.tick_reads_ok,
            reads_fail: self.tick_reads_fail,
            pacts_formed: self.tick_pacts_formed,
            pacts_dropped: self.tick_pacts_dropped,
            gossip_sent: self.tick_gossip_sent,
            events_published: self.tick_events_published,
            events_delivered: self.tick_events_delivered,
            cum_reads_ok: self.total_reads_ok,
            cum_reads_fail: self.total_reads_fail,
            cum_pacts: self.total_pacts_formed as i64 - self.total_pacts_dropped as i64,
        }
    }
}

// ── Helper ──────────────────────────────────────────────────────────

pub fn create_metrics_channel(
    buffer: usize,
) -> (mpsc::Sender<MetricEvent>, mpsc::Receiver<MetricEvent>) {
    mpsc::channel(buffer)
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_gossip_sent_updates_metrics() {
        let (tx, rx) = create_metrics_channel(16);
        let collector = MetricsCollector::new(rx, 0, &StreamingConfig::default(), None);
        let handle = tokio::spawn(collector.run());

        tx.send(MetricEvent::GossipSent {
            from: 1,
            time: 1.0,
        })
        .await
        .unwrap();
        tx.send(MetricEvent::GossipSent {
            from: 1,
            time: 2.0,
        })
        .await
        .unwrap();
        drop(tx);

        let collected = handle.await.unwrap();
        let node = collected.snapshots.get(&1).expect("node 1 should exist");
        assert_eq!(node.gossip.sent, 2);
    }

    #[tokio::test]
    async fn test_challenge_result_updates_metrics() {
        let (tx, rx) = create_metrics_channel(16);
        let collector = MetricsCollector::new(rx, 0, &StreamingConfig::default(), None);
        let handle = tokio::spawn(collector.run());

        tx.send(MetricEvent::ChallengeResult {
            from: 1,
            to: 2,
            passed: true,
            time: 1.0,
        })
        .await
        .unwrap();
        tx.send(MetricEvent::ChallengeResult {
            from: 1,
            to: 3,
            passed: false,
            time: 2.0,
        })
        .await
        .unwrap();
        drop(tx);

        let collected = handle.await.unwrap();
        let node = collected.snapshots.get(&1).expect("node 1 should exist");
        assert_eq!(node.challenges.passed, 1);
        assert_eq!(node.challenges.failed, 1);
    }

    #[tokio::test]
    async fn test_node_snapshot_includes_online_field() {
        let (tx, rx) = create_metrics_channel(16);
        let collector = MetricsCollector::new(rx, 0, &StreamingConfig::default(), None);
        let handle = tokio::spawn(collector.run());

        // Send a NodeSnapshot with online=true
        tx.send(MetricEvent::NodeSnapshot {
            id: 1,
            online: true,
            bandwidth: BandwidthCounter::default(),
            gossip: GossipStats::default(),
            challenges: ChallengeStats::default(),
            cache_stats: CacheStats::default(),
            pact_count: 0,
            stored_bytes: 0,
            storage_capacity: 0,
            storage_used: 0,
            first_pact_time: None,
            time: 1.0,
        })
        .await
        .unwrap();

        // Send a NodeSnapshot with online=false
        tx.send(MetricEvent::NodeSnapshot {
            id: 1,
            online: false,
            bandwidth: BandwidthCounter::default(),
            gossip: GossipStats::default(),
            challenges: ChallengeStats::default(),
            cache_stats: CacheStats::default(),
            pact_count: 0,
            stored_bytes: 0,
            storage_capacity: 0,
            storage_used: 0,
            first_pact_time: None,
            time: 2.0,
        })
        .await
        .unwrap();

        drop(tx);

        let collected = handle.await.unwrap();
        let node = collected.snapshots.get(&1).expect("node 1 should exist");
        assert_eq!(node.availability_samples.len(), 2);
        assert_eq!(node.availability_samples[0], true);
        assert_eq!(node.availability_samples[1], false);
    }

    #[tokio::test]
    async fn test_availability_samples_populated_after_ticks() {
        let (tx, rx) = create_metrics_channel(16);
        let collector = MetricsCollector::new(rx, 0, &StreamingConfig::default(), None);
        let handle = tokio::spawn(collector.run());

        // Simulate 5 ticks: 3 online, 2 offline
        for i in 0..5 {
            let online = i < 3; // first 3 online, last 2 offline
            tx.send(MetricEvent::NodeSnapshot {
                id: 42,
                online,
                bandwidth: BandwidthCounter::default(),
                gossip: GossipStats::default(),
                challenges: ChallengeStats::default(),
                cache_stats: CacheStats::default(),
                pact_count: 2,
                stored_bytes: 1024,
                storage_capacity: 0,
                storage_used: 0,
                first_pact_time: None,
                time: i as f64,
            })
            .await
            .unwrap();
        }

        drop(tx);

        let collected = handle.await.unwrap();
        let node = collected.snapshots.get(&42).expect("node 42 should exist");
        assert_eq!(node.availability_samples.len(), 5);
        assert_eq!(
            node.availability_samples.iter().filter(|&&s| s).count(),
            3,
            "expected 3 online samples"
        );
        assert_eq!(
            node.availability_samples.iter().filter(|&&s| !s).count(),
            2,
            "expected 2 offline samples"
        );
    }

    #[test]
    fn test_percentiles() {
        let values: Vec<f64> = (0..100).map(|v| v as f64).collect();
        let p = Percentiles::from_values(values);

        assert!((p.p50 - 50.0).abs() < f64::EPSILON);
        assert!((p.min - 0.0).abs() < f64::EPSILON);
        assert!((p.max - 99.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_percentiles_empty() {
        let p = Percentiles::from_values(vec![]);
        assert!((p.p50 - 0.0).abs() < f64::EPSILON);
        assert!((p.p95 - 0.0).abs() < f64::EPSILON);
        assert!((p.p99 - 0.0).abs() < f64::EPSILON);
        assert!((p.min - 0.0).abs() < f64::EPSILON);
        assert!((p.max - 0.0).abs() < f64::EPSILON);
        assert!((p.mean - 0.0).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn test_delivery_latency_is_positive_and_proportional() {
        let (tx, rx) = create_metrics_channel(64);
        let collector = MetricsCollector::new(rx, 0, &StreamingConfig::default(), None);
        let handle = tokio::spawn(collector.run());

        // Publish two events at different times
        tx.send(MetricEvent::EventPublished {
            author: 1,
            event_id: 100,
            time: 10.0,
            nostr_json: None,
        })
        .await
        .unwrap();
        tx.send(MetricEvent::EventPublished {
            author: 2,
            event_id: 101,
            time: 20.0,
            nostr_json: None,
        })
        .await
        .unwrap();

        // Deliver event 100 with a latency of 5.0s
        tx.send(MetricEvent::EventDelivered {
            author: 1,
            event_id: 100,
            to: 3,
            time: 15.0,
            path: DeliveryPath::CachedEndpoint,
        })
        .await
        .unwrap();

        // Deliver event 100 to another node with a latency of 8.0s
        tx.send(MetricEvent::EventDelivered {
            author: 1,
            event_id: 100,
            to: 4,
            time: 18.0,
            path: DeliveryPath::Gossip,
        })
        .await
        .unwrap();

        // Deliver event 101 with a latency of 3.0s
        tx.send(MetricEvent::EventDelivered {
            author: 2,
            event_id: 101,
            to: 5,
            time: 23.0,
            path: DeliveryPath::CachedEndpoint,
        })
        .await
        .unwrap();

        drop(tx);
        let collected = handle.await.unwrap();

        // Should have 3 delivery latencies
        assert_eq!(
            collected.delivery_latencies.len(),
            3,
            "expected 3 delivery latencies"
        );

        // All latencies must be positive
        for lat in &collected.delivery_latencies {
            assert!(
                *lat > 0.0,
                "delivery latency should be positive, got {}",
                lat
            );
        }

        // Sort latencies to verify expected values: 3.0, 5.0, 8.0
        let mut sorted = collected.delivery_latencies.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert!(
            (sorted[0] - 3.0).abs() < f64::EPSILON,
            "smallest latency should be 3.0, got {}",
            sorted[0]
        );
        assert!(
            (sorted[1] - 5.0).abs() < f64::EPSILON,
            "middle latency should be 5.0, got {}",
            sorted[1]
        );
        assert!(
            (sorted[2] - 8.0).abs() < f64::EPSILON,
            "largest latency should be 8.0, got {}",
            sorted[2]
        );
    }

    #[test]
    fn test_percentiles_computed_correctly_from_latency_data() {
        // Simulate known latency values and verify percentiles
        let latencies: Vec<f64> = vec![
            1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0,
        ];
        let p = Percentiles::from_values(latencies);

        // Mean = 5.5
        assert!(
            (p.mean - 5.5).abs() < f64::EPSILON,
            "mean should be 5.5, got {}",
            p.mean
        );
        assert!(
            (p.min - 1.0).abs() < f64::EPSILON,
            "min should be 1.0, got {}",
            p.min
        );
        assert!(
            (p.max - 10.0).abs() < f64::EPSILON,
            "max should be 10.0, got {}",
            p.max
        );
        // p50 of [1..10] with nearest-rank: index = round(0.50 * 9) = round(4.5) = 5 => value 6.0
        assert!(
            (p.p50 - 6.0).abs() < f64::EPSILON,
            "p50 should be 6.0, got {}",
            p.p50
        );
        // p95: index = round(0.95 * 9) = round(8.55) = 9 => value 10.0
        assert!(
            (p.p95 - 10.0).abs() < f64::EPSILON,
            "p95 should be 10.0, got {}",
            p.p95
        );
        // p99: index = round(0.99 * 9) = round(8.91) = 9 => value 10.0
        assert!(
            (p.p99 - 10.0).abs() < f64::EPSILON,
            "p99 should be 10.0, got {}",
            p.p99
        );
    }

    #[tokio::test]
    async fn test_pact_events_recorded_with_timestamps() {
        let (tx, rx) = create_metrics_channel(32);
        let collector = MetricsCollector::new(rx, 0, &StreamingConfig::default(), None);
        let handle = tokio::spawn(collector.run());

        // Send PactFormed events
        tx.send(MetricEvent::PactFormed {
            node: 1,
            partner: 2,
            time: 100.0,
        })
        .await
        .unwrap();
        tx.send(MetricEvent::PactFormed {
            node: 3,
            partner: 4,
            time: 200.0,
        })
        .await
        .unwrap();

        // Send PactDropped event
        tx.send(MetricEvent::PactDropped {
            node: 1,
            partner: 2,
            time: 500.0,
        })
        .await
        .unwrap();

        drop(tx);
        let collected = handle.await.unwrap();

        assert_eq!(
            collected.pact_events.len(),
            3,
            "expected 3 pact events (2 formed + 1 dropped)"
        );

        // Verify first event: PactFormed at time 100.0
        assert_eq!(collected.pact_events[0].kind, PactEventKind::Formed);
        assert_eq!(collected.pact_events[0].node, 1);
        assert_eq!(collected.pact_events[0].partner, 2);
        assert!(
            (collected.pact_events[0].time - 100.0).abs() < f64::EPSILON,
            "first event time should be 100.0"
        );

        // Verify second event: PactFormed at time 200.0
        assert_eq!(collected.pact_events[1].kind, PactEventKind::Formed);
        assert_eq!(collected.pact_events[1].node, 3);
        assert_eq!(collected.pact_events[1].partner, 4);
        assert!(
            (collected.pact_events[1].time - 200.0).abs() < f64::EPSILON,
            "second event time should be 200.0"
        );

        // Verify third event: PactDropped at time 500.0
        assert_eq!(collected.pact_events[2].kind, PactEventKind::Dropped);
        assert_eq!(collected.pact_events[2].node, 1);
        assert_eq!(collected.pact_events[2].partner, 2);
        assert!(
            (collected.pact_events[2].time - 500.0).abs() < f64::EPSILON,
            "third event time should be 500.0"
        );

        // Also verify pact_count is updated correctly
        let node1 = collected.snapshots.get(&1).expect("node 1 should exist");
        assert_eq!(
            node1.pact_count, 0,
            "node 1 formed then dropped, net pact_count should be 0"
        );
        let node3 = collected.snapshots.get(&3).expect("node 3 should exist");
        assert_eq!(
            node3.pact_count, 1,
            "node 3 formed once, pact_count should be 1"
        );
    }

    #[tokio::test]
    async fn test_pact_churn_rate_computed_correctly() {
        use crate::config::SimConfig;
        use crate::graph::Graph;
        use crate::output::json::build_per_node_summary;

        let (tx, rx) = create_metrics_channel(64);
        let collector = MetricsCollector::new(rx, 0, &StreamingConfig::default(), None);
        let handle = tokio::spawn(collector.run());

        // Create 2 nodes with snapshots so they appear in metrics.snapshots
        tx.send(MetricEvent::NodeSnapshot {
            id: 1,
            online: true,
            bandwidth: BandwidthCounter::default(),
            gossip: GossipStats::default(),
            challenges: ChallengeStats::default(),
            cache_stats: CacheStats::default(),
            pact_count: 0,
            stored_bytes: 0,
            storage_capacity: 0,
            storage_used: 0,
            first_pact_time: None,
            time: 1.0,
        })
        .await
        .unwrap();
        tx.send(MetricEvent::NodeSnapshot {
            id: 2,
            online: true,
            bandwidth: BandwidthCounter::default(),
            gossip: GossipStats::default(),
            challenges: ChallengeStats::default(),
            cache_stats: CacheStats::default(),
            pact_count: 0,
            stored_bytes: 0,
            storage_capacity: 0,
            storage_used: 0,
            first_pact_time: None,
            time: 1.0,
        })
        .await
        .unwrap();

        // 5 formed, 3 dropped
        for i in 0..5 {
            tx.send(MetricEvent::PactFormed {
                node: 1,
                partner: 2,
                time: (i * 100) as f64,
            })
            .await
            .unwrap();
        }
        for i in 0..3 {
            tx.send(MetricEvent::PactDropped {
                node: 1,
                partner: 2,
                time: (500 + i * 100) as f64,
            })
            .await
            .unwrap();
        }

        drop(tx);
        let collected = handle.await.unwrap();

        let mut config = SimConfig::default();
        config.simulation.duration_days = 10;
        let graph = Graph::new(2);

        let summary = build_per_node_summary(&collected, &config, &graph);

        assert_eq!(summary.pact_churn.total_formed, 5);
        assert_eq!(summary.pact_churn.total_dropped, 3);
        assert_eq!(summary.pact_churn.net_pacts, 2); // 5 - 3
        // churn_rate = total_dropped / node_count / duration_days = 3 / 2 / 10 = 0.15
        let expected_churn = 3.0 / 2.0 / 10.0;
        assert!(
            (summary.pact_churn.churn_rate - expected_churn).abs() < f64::EPSILON,
            "churn_rate should be {}, got {}",
            expected_churn,
            summary.pact_churn.churn_rate
        );
    }

    #[tokio::test]
    async fn test_read_result_collected() {
        use crate::types::{ReadTier, WotTier};

        let (tx, rx) = create_metrics_channel(32);
        let collector = MetricsCollector::new(rx, 0, &StreamingConfig::default(), None);
        let handle = tokio::spawn(collector.run());

        // Send ReadResult events with different tiers
        tx.send(MetricEvent::ReadResult {
            reader: 1,
            target_author: 2,
            request_id: 100,
            tier: ReadTier::CachedEndpoint,
            wot_tier: WotTier::Orbit,
            latency_secs: 0.5,
            paths_tried: 1,
            time: 10.0,
        })
        .await
        .unwrap();

        tx.send(MetricEvent::ReadResult {
            reader: 3,
            target_author: 4,
            request_id: 101,
            tier: ReadTier::Gossip,
            wot_tier: WotTier::Orbit,
            latency_secs: 2.0,
            paths_tried: 2,
            time: 20.0,
        })
        .await
        .unwrap();

        tx.send(MetricEvent::ReadResult {
            reader: 5,
            target_author: 6,
            request_id: 102,
            tier: ReadTier::Failed,
            wot_tier: WotTier::Orbit,
            latency_secs: 10.0,
            paths_tried: 3,
            time: 30.0,
        })
        .await
        .unwrap();

        drop(tx);
        let collected = handle.await.unwrap();

        assert_eq!(
            collected.read_results.len(),
            3,
            "expected 3 read results"
        );

        // Verify first result: CachedEndpoint tier
        let r0 = &collected.read_results[0];
        assert_eq!(r0.reader, 1);
        assert_eq!(r0.target_author, 2);
        assert_eq!(r0.request_id, 100);
        assert_eq!(r0.tier, ReadTier::CachedEndpoint);
        assert!((r0.latency_secs - 0.5).abs() < f64::EPSILON);
        assert_eq!(r0.paths_tried, 1);
        assert!((r0.time - 10.0).abs() < f64::EPSILON);

        // Verify second result: Gossip tier
        let r1 = &collected.read_results[1];
        assert_eq!(r1.reader, 3);
        assert_eq!(r1.target_author, 4);
        assert_eq!(r1.request_id, 101);
        assert_eq!(r1.tier, ReadTier::Gossip);
        assert!((r1.latency_secs - 2.0).abs() < f64::EPSILON);
        assert_eq!(r1.paths_tried, 2);
        assert!((r1.time - 20.0).abs() < f64::EPSILON);

        // Verify third result: Failed tier
        let r2 = &collected.read_results[2];
        assert_eq!(r2.reader, 5);
        assert_eq!(r2.target_author, 6);
        assert_eq!(r2.request_id, 102);
        assert_eq!(r2.tier, ReadTier::Failed);
        assert!((r2.latency_secs - 10.0).abs() < f64::EPSILON);
        assert_eq!(r2.paths_tried, 3);
        assert!((r2.time - 30.0).abs() < f64::EPSILON);
    }
}
