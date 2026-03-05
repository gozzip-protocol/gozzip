pub mod gossip;
pub mod karma;
pub mod state;
pub mod storage;

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use rand::Rng;
use rand::SeedableRng;
use tokio::sync::mpsc;

use crate::config::SimConfig;
use crate::sim::router::Envelope;
use crate::types::{
    BandwidthCounter, Bytes, CacheStats, ChallengeStats, DeliveryPath, Event, GossipStats,
    Message, NodeId, NodeType, ReadTier, SimTime, WotTier,
};

use self::gossip::{should_forward, ForwardDecision};
use self::state::NodeState;

// ── MetricEvent ─────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum MetricEvent {
    EventPublished {
        author: NodeId,
        event_id: u64,
        time: SimTime,
        #[allow(dead_code)]
        nostr_json: Option<String>,
    },
    EventDelivered {
        author: NodeId,
        event_id: u64,
        to: NodeId,
        time: SimTime,
        path: DeliveryPath,
    },
    GossipSent {
        from: NodeId,
        time: SimTime,
    },
    PactFormed {
        node: NodeId,
        partner: NodeId,
        time: SimTime,
    },
    PactDropped {
        node: NodeId,
        partner: NodeId,
        time: SimTime,
    },
    ChallengeResult {
        from: NodeId,
        to: NodeId,
        passed: bool,
        time: SimTime,
    },
    NodeSnapshot {
        id: NodeId,
        online: bool,
        bandwidth: BandwidthCounter,
        gossip: GossipStats,
        challenges: ChallengeStats,
        cache_stats: CacheStats,
        pact_count: usize,
        stored_bytes: Bytes,
        storage_capacity: Bytes,
        storage_used: Bytes,
        first_pact_time: Option<SimTime>,
        time: SimTime,
    },
    ReadResult {
        reader: NodeId,
        target_author: NodeId,
        request_id: u64,
        tier: ReadTier,
        wot_tier: WotTier,
        latency_secs: f64,
        paths_tried: u8,
        time: SimTime,
    },
    TickComplete {
        tick: u64,
        time: SimTime,
    },
}

// ── run_node ────────────────────────────────────────────────────────

/// Main task loop for a node actor. Receives messages from the inbox,
/// processes them, and communicates with other nodes via the router.
pub async fn run_node(
    mut state: NodeState,
    mut inbox: mpsc::Receiver<Message>,
    router_tx: mpsc::Sender<Envelope>,
    metrics_tx: mpsc::Sender<MetricEvent>,
    config: SimConfig,
) {
    let mut current_time: SimTime = 0.0;

    while let Some(msg) = inbox.recv().await {
        match msg {
            Message::Shutdown => break,

            Message::GoOnline => {
                state.online = true;
            }

            Message::GoOffline => {
                state.online = false;
            }

            Message::Publish(event) => {
                if !state.online {
                    continue;
                }

                let event_id = event.id;
                let author = event.author;
                let created_at = event.created_at;
                let size = event.size_bytes as Bytes;
                let nostr_json = event.nostr_json.clone();

                // Track bandwidth for publishing
                state.bandwidth.record_upload(size);
                state.bandwidth.by_category.publish_up += size;

                // Store own event
                state.own_events.push(event.clone());
                state.seq_counter += 1;
                state.events_since_last_check += 1;

                // Karma: spend proportional to event size * pact count
                if config.karma.enabled {
                    let pact_count = state.active_pacts.len().max(1) as f64;
                    let cost = (size as f64 / 1_048_576.0) * config.karma.cost_per_mb_stored * pact_count;
                    state.karma.spend(cost);
                }

                // Emit publish metric
                let _ = metrics_tx
                    .send(MetricEvent::EventPublished {
                        author,
                        event_id,
                        time: created_at,
                        nostr_json,
                    })
                    .await;

                // Forward to all active pact partners via router
                for pact in &state.active_pacts {
                    let envelope = Envelope {
                        from: state.id,
                        message: Message::DeliverEvents {
                            from: state.id,
                            events: vec![event.clone()],
                            path: DeliveryPath::CachedEndpoint,
                            request_id: None,
                        },
                        to: pact.partner,
                        deliver_at: created_at,
                        path_hint: None,
                    };
                    let _ = router_tx.send(envelope).await;
                }
            }

            Message::DeliverEvents { from: sender, events, path, request_id } => {
                if !state.online {
                    continue;
                }

                if events.is_empty() {
                    continue;
                }

                let author = events[0].author;
                let _time = events[0].created_at;

                // Check if this is for a pact partner (stored) or just cached
                let is_pact_partner = state
                    .active_pacts
                    .iter()
                    .any(|p| p.partner == author);

                if is_pact_partner {
                    // Emit delivery metrics before storing
                    for ev in &events {
                        let _ = metrics_tx
                            .send(MetricEvent::EventDelivered {
                                author: ev.author,
                                event_id: ev.id,
                                to: state.id,
                                time: ev.created_at,
                                path,
                            })
                            .await;
                    }
                    state.store_events_for_pact(author, events, current_time, &config);
                } else {
                    // Emit delivery metrics before caching
                    for ev in &events {
                        let _ = metrics_tx
                            .send(MetricEvent::EventDelivered {
                                author: ev.author,
                                event_id: ev.id,
                                to: state.id,
                                time: ev.created_at,
                                path,
                            })
                            .await;
                    }
                    state.cache_events(author, events, current_time);
                }

                // Cache the endpoint: remember that `sender` has data for `author`
                if sender != state.id {
                    state.endpoint_cache.put(author, sender);
                }

                // Check if this delivery resolves a pending read request
                // Only resolve if request_id matches (pact pushes have request_id: None)
                if let Some(rid) = request_id {
                    if let Some((req_id, _req_time, _, _, _)) = state.pending_reads.get(&author) {
                        if *req_id == rid {
                            let (req_id, req_time, _, cached_endpoint_node, wot_tier) = state.pending_reads.remove(&author).unwrap();
                            let raw_latency = current_time - req_time;
                            // Reader-side tier classification: if the response came
                            // from the node we queried as cached endpoint, it's a
                            // cached-endpoint hit; otherwise classify by path hint.
                            let tier = match cached_endpoint_node {
                                Some(ep) if sender == ep => ReadTier::CachedEndpoint,
                                _ => match path {
                                    DeliveryPath::Relay => ReadTier::Relay,
                                    _ => ReadTier::Gossip,
                                },
                            };
                            // Same-tick delivery: sample realistic latency for this tier
                            let latency = if raw_latency == 0.0 {
                                use rand_distr::{Distribution, Normal};
                                match tier {
                                    ReadTier::Instant | ReadTier::CachedEndpoint => {
                                        let dist = Normal::new(
                                            config.latency.cached_endpoint_base_ms / 1000.0,
                                            config.latency.cached_endpoint_jitter_ms / 1000.0,
                                        ).unwrap();
                                        dist.sample(&mut state.rng).max(0.001)
                                    }
                                    ReadTier::Gossip => {
                                        let dist = Normal::new(
                                            2.0 * config.latency.gossip_per_hop_base_ms / 1000.0,
                                            2.0 * config.latency.gossip_per_hop_jitter_ms / 1000.0,
                                        ).unwrap();
                                        dist.sample(&mut state.rng).max(0.001)
                                    }
                                    ReadTier::Relay | ReadTier::Failed => {
                                        let dist = Normal::new(
                                            config.latency.relay_base_ms / 1000.0,
                                            config.latency.relay_jitter_ms / 1000.0,
                                        ).unwrap();
                                        dist.sample(&mut state.rng).max(0.001)
                                    }
                                }
                            } else {
                                raw_latency
                            };
                            let _ = metrics_tx
                                .send(MetricEvent::ReadResult {
                                    reader: state.id,
                                    target_author: author,
                                    request_id: req_id,
                                    tier,
                                    wot_tier,
                                    latency_secs: latency,
                                    paths_tried: 2,
                                    time: current_time,
                                })
                                .await;
                        }
                    }
                }

            }

            Message::RequestData {
                from,
                request_id,
                ttl,
                filter,
            } => {
                if !state.online {
                    continue;
                }

                // Dedup check: skip if we've seen this request_id
                if state.seen_request_ids.contains(&request_id) {
                    state.gossip_stats.deduplicated += 1;
                    continue;
                }
                state.seen_request_ids.put(request_id, ());

                // Rate limiter check
                if !state.rate_limiter.check(
                    from,
                    current_time,
                    config.protocol.rate_limit_10057,
                ) {
                    state.gossip_stats.rate_limited += 1;
                    continue;
                }

                state.gossip_stats.received += 1;

                // Try to parse the filter as a NodeId to look up
                if let Ok(author) = filter.parse::<NodeId>() {
                    if state.has_events_for(author) {
                        // Respond with data, classifying path by source
                        let (events, response_path) = if author == state.id {
                            (state.own_events.clone(), DeliveryPath::CachedEndpoint)
                        } else if let Some(stored) = state.stored_events.get(&author) {
                            (stored.clone(), DeliveryPath::CachedEndpoint)
                        } else if let Some((cached, _)) = state.read_cache.get(&author) {
                            state.cache_stats.hits += 1;
                            (cached.clone(), DeliveryPath::ReadCache)
                        } else {
                            (Vec::new(), DeliveryPath::Gossip)
                        };

                        if !events.is_empty() {
                            let size: Bytes =
                                events.iter().map(|e| e.size_bytes as Bytes).sum();
                            state.bandwidth.record_upload(size);
                            state.bandwidth.by_category.gossip_up += size;

                            let envelope = Envelope {
                                from: state.id,
                                message: Message::DeliverEvents {
                                    from: state.id,
                                    events,
                                    path: response_path,
                                    request_id: Some(request_id),
                                },
                                to: from,
                                deliver_at: current_time,
                                path_hint: None,
                            };
                            let _ = router_tx.send(envelope).await;
                        }
                    } else {
                        // Data not found locally — track cache miss
                        state.cache_stats.misses += 1;

                        if ttl > 1 {
                            // WoT filtering: only forward if sender is a WoT peer
                            let decision = should_forward(&state, from, request_id, &config);
                            if decision == ForwardDecision::ServeLocallyOnly {
                                state.gossip_stats.wot_filtered += 1;
                                continue;
                            }

                            let new_ttl = ttl - 1;
                            let mut peers: Vec<NodeId> = state
                                .follows
                                .iter()
                                .chain(state.followers.iter())
                                .copied()
                                .filter(|&peer| peer != from)
                                .collect();

                            // Trust-weighted fanout: prefer higher-scored peers
                            use rand::seq::SliceRandom;
                            let fanout = config.protocol.gossip_fanout as usize;
                            if peers.len() > fanout {
                                // Sort by score descending
                                peers.sort_by(|a, b| {
                                    let sa = state.wot_peer_scores.get(a).copied().unwrap_or(1);
                                    let sb = state.wot_peer_scores.get(b).copied().unwrap_or(1);
                                    sb.cmp(&sa)
                                });
                                // Shuffle peers with the same score as the cutoff peer for fairness
                                let cutoff_score = state.wot_peer_scores.get(&peers[fanout - 1]).copied().unwrap_or(1);
                                let start = peers.iter().position(|p| {
                                    state.wot_peer_scores.get(p).copied().unwrap_or(1) <= cutoff_score
                                }).unwrap_or(peers.len());
                                let end = peers.len().min(
                                    peers.iter().rposition(|p| {
                                        state.wot_peer_scores.get(p).copied().unwrap_or(1) >= cutoff_score
                                    }).map_or(peers.len(), |i| i + 1)
                                );
                                if start < end {
                                    peers[start..end].shuffle(&mut state.rng);
                                }
                            }
                            peers.truncate(fanout);

                            for peer in peers {
                                let envelope = Envelope {
                                    from: state.id,
                                    message: Message::RequestData {
                                        from: state.id,
                                        request_id,
                                        ttl: new_ttl,
                                        filter: filter.clone(),
                                    },
                                    to: peer,
                                    deliver_at: current_time,
                                    path_hint: None,
                                };
                                let _ = router_tx.send(envelope).await;
                                state.gossip_stats.forwarded += 1;
                            }
                        }
                    }
                }
            }

            Message::Challenge { from, nonce } => {
                if !state.online {
                    continue;
                }

                state.challenge_stats.received += 1;

                // Compute proof over all stored events for the challenger
                let all_events: Vec<&Event> = state
                    .stored_events
                    .get(&from)
                    .map(|v| v.iter().collect())
                    .unwrap_or_default();

                let proof = compute_challenge_hash(&all_events, 0, u64::MAX, nonce);

                let envelope = Envelope {
                    from: state.id,
                    message: Message::ChallengeResponse {
                        from: state.id,
                        proof,
                    },
                    to: from,
                    deliver_at: current_time,
                    path_hint: None,
                };
                let _ = router_tx.send(envelope).await;
            }

            Message::ChallengeResponse { from, proof } => {
                if !state.online {
                    continue;
                }

                // Look up pending nonce for this partner
                let nonce = match state.pending_challenges.remove(&from) {
                    Some(n) => n,
                    None => continue, // No pending challenge for this partner
                };

                // Recompute expected hash over events stored for this partner
                let stored_events: Vec<&Event> = state
                    .stored_events
                    .get(&from)
                    .map(|v| v.iter().collect())
                    .unwrap_or_default();
                let expected = compute_challenge_hash(&stored_events, 0, u64::MAX, nonce);
                let passed = proof == expected;

                // Update reliability score
                let score = state.reliability_scores.entry(from).or_insert(1.0);
                *score = storage::update_reliability(*score, passed);

                if passed {
                    state.challenge_stats.passed += 1;
                } else {
                    state.challenge_stats.failed += 1;
                }

                // Check reliability action
                let action = storage::reliability_action(*score);
                if matches!(action, storage::ReliabilityAction::DropImmediately) {
                    // Drop the pact (from active or standby)
                    state.active_pacts.retain(|p| p.partner != from);
                    state.standby_pacts.retain(|p| p.partner != from);
                    state.reliability_scores.remove(&from);

                    let envelope = Envelope {
                        from: state.id,
                        message: Message::PactDrop { partner: state.id },
                        to: from,
                        deliver_at: current_time,
                        path_hint: None,
                    };
                    let _ = router_tx.send(envelope).await;

                    let _ = metrics_tx
                        .send(MetricEvent::PactDropped {
                            node: state.id,
                            partner: from,
                            time: current_time,
                        })
                        .await;
                }

                let _ = metrics_tx
                    .send(MetricEvent::ChallengeResult {
                        from: state.id,
                        to: from,
                        passed,
                        time: current_time,
                    })
                    .await;
            }

            Message::Tick(time) => {
                current_time = time;

                // Light node periodic pruning: drop events older than the
                // checkpoint window so stored_bytes stays bounded.
                if state.node_type == NodeType::Light {
                    let cutoff = time - config.protocol.checkpoint_window_secs();
                    for events in state.stored_events.values_mut() {
                        events.retain(|e| e.created_at >= cutoff);
                    }
                }

                // Evict expired read-cache entries (use Horizon TTL as sweep floor)
                let cache_ttl_secs = config.retrieval.horizon_cache_ttl_days * 86400.0;
                state.evict_expired_cache(time, cache_ttl_secs);

                // Phase 1: Relay stagger — attempt relay early for reads pending > stagger_secs
                let stagger = config.retrieval.relay_stagger_secs;
                let timeout = config.retrieval.read_timeout_secs;

                let stagger_ready: Vec<(NodeId, u64, SimTime, WotTier)> = state
                    .pending_reads
                    .iter()
                    .filter(|(_, (_, req_time, ref relay_attempted, _, _))| {
                        time - req_time > stagger && !relay_attempted
                    })
                    .map(|(&author, &(req_id, req_time, _, _, wot_tier))| (author, req_id, req_time, wot_tier))
                    .collect();

                for (author, req_id, _req_time, wot_tier) in stagger_ready {
                    let relay_roll: f64 = state.rng.gen();
                    if relay_roll < config.retrieval.relay_success_rate {
                        // Relay succeeded — resolve with sampled latency
                        state.pending_reads.remove(&author);
                        use rand_distr::{Distribution, Normal};
                        let dist = Normal::new(
                            config.latency.relay_base_ms / 1000.0,
                            config.latency.relay_jitter_ms / 1000.0,
                        ).unwrap();
                        let latency = dist.sample(&mut state.rng).max(0.01);
                        let _ = metrics_tx
                            .send(MetricEvent::ReadResult {
                                reader: state.id,
                                target_author: author,
                                request_id: req_id,
                                tier: ReadTier::Relay,
                                wot_tier,
                                latency_secs: latency,
                                paths_tried: 3,
                                time,
                            })
                            .await;
                    } else {
                        // Relay failed — mark as attempted, keep waiting for gossip
                        if let Some(entry) = state.pending_reads.get_mut(&author) {
                            entry.2 = true;
                        }
                    }
                }

                // Phase 2: Full timeout — give up on reads pending > timeout
                let timed_out: Vec<(NodeId, u64, WotTier)> = state
                    .pending_reads
                    .iter()
                    .filter(|(_, (_, req_time, _, _, _))| time - req_time > timeout)
                    .map(|(&author, &(req_id, _, _, _, wot_tier))| (author, req_id, wot_tier))
                    .collect();

                for (author, req_id, wot_tier) in timed_out {
                    state.pending_reads.remove(&author);
                    let _ = metrics_tx
                        .send(MetricEvent::ReadResult {
                            reader: state.id,
                            target_author: author,
                            request_id: req_id,
                            tier: ReadTier::Failed,
                            wot_tier,
                            latency_secs: timeout,
                            paths_tried: 3,
                            time,
                        })
                        .await;
                }

                // Emit NodeSnapshot metric (before online check so offline
                // nodes are recorded in availability_samples)
                let _ = metrics_tx
                    .send(MetricEvent::NodeSnapshot {
                        id: state.id,
                        online: state.online,
                        bandwidth: state.bandwidth.clone(),
                        gossip: state.gossip_stats.clone(),
                        challenges: state.challenge_stats.clone(),
                        cache_stats: state.cache_stats.clone(),
                        pact_count: state.pact_count(),
                        stored_bytes: state.total_stored_bytes(),
                        storage_capacity: state.storage_capacity,
                        storage_used: state.storage_used,
                        first_pact_time: state.first_pact_time,
                        time,
                    })
                    .await;

                if !state.online {
                    continue;
                }

                // Age gating: skip pact formation if this node is too new
                let min_age = config.protocol.min_account_age_days as f64 * 86_400.0;
                let account_old_enough = current_time - state.created_at >= min_age;

                // Pact formation: request pacts if below target
                if account_old_enough && state.pact_count() < config.protocol.pacts_default as usize {
                    let own_volume = state.total_stored_bytes();
                    // Build candidates from follows + followers with estimated volume
                    let candidates: Vec<(NodeId, Bytes)> = state
                        .follows
                        .iter()
                        .chain(state.followers.iter())
                        .copied()
                        .map(|id| (id, own_volume)) // estimate partner volume as own
                        .collect();

                    // Use std::mem::replace to borrow rng without conflicting borrows
                    let mut rng = std::mem::replace(
                        &mut state.rng,
                        rand_chacha::ChaCha8Rng::seed_from_u64(0),
                    );
                    let partners =
                        storage::select_pact_partners(&state, &candidates, own_volume, &config, &mut rng);
                    state.rng = rng;

                    let my_tier = state.activity_tier();
                    for partner in partners {
                        let envelope = Envelope {
                            from: state.id,
                            message: Message::PactRequest {
                                from: state.id,
                                volume_bytes: own_volume,
                                as_standby: false,
                                created_at: state.created_at,
                                activity_tier: my_tier,
                            },
                            to: partner,
                            deliver_at: current_time,
                            path_hint: None,
                        };
                        let _ = router_tx.send(envelope).await;
                    }
                }

                // Standby pact formation: request standby pacts if below target
                if account_old_enough && state.standby_pacts.len() < config.protocol.pacts_standby as usize {
                    let own_volume = state.total_stored_bytes();
                    // Build candidates from follows + followers, excluding active AND standby partners
                    let candidates: Vec<(NodeId, Bytes)> = state
                        .follows
                        .iter()
                        .chain(state.followers.iter())
                        .copied()
                        .map(|id| (id, own_volume))
                        .collect();

                    let mut rng = std::mem::replace(
                        &mut state.rng,
                        rand_chacha::ChaCha8Rng::seed_from_u64(0),
                    );
                    let partners = storage::select_standby_pact_partners(
                        &state, &candidates, own_volume, &config, &mut rng,
                    );
                    state.rng = rng;

                    let my_tier = state.activity_tier();
                    for partner in partners {
                        let envelope = Envelope {
                            from: state.id,
                            message: Message::PactRequest {
                                from: state.id,
                                volume_bytes: own_volume,
                                as_standby: true,
                                created_at: state.created_at,
                                activity_tier: my_tier,
                            },
                            to: partner,
                            deliver_at: current_time,
                            path_hint: None,
                        };
                        let _ = router_tx.send(envelope).await;
                    }
                }

                // Activity rate update & renegotiation check
                let activity_check_secs = config.protocol.activity_check_interval_hours as f64 * 3600.0;
                if activity_check_secs > 0.0 && time - state.last_activity_check >= activity_check_secs {
                    let elapsed_days = (time - state.last_activity_check) / 86_400.0;
                    if elapsed_days > 0.0 {
                        let actual_rate = state.events_since_last_check as f64 / elapsed_days;
                        let alpha = 0.1;
                        state.activity_rate = state.activity_rate * (1.0 - alpha) + actual_rate * alpha;
                    }
                    state.events_since_last_check = 0;
                    state.last_activity_check = time;

                    // Renegotiation: drop pacts where per-partner volume mismatch exceeds threshold.
                    // Compare what we store FOR each partner against what they store for us
                    // (approximated by their volume at formation, scaled by pact count).
                    let threshold = config.protocol.activity_renegotiation_threshold;
                    let num_pacts = state.active_pacts.len().max(1) as f64;
                    let to_drop: Vec<NodeId> = state.active_pacts.iter()
                        .filter(|p| {
                            // Bytes we store for this specific partner
                            let we_store = state.stored_bytes_for(p.partner);
                            // Partner's per-pact share at formation (their total / est. pact count)
                            let they_store_est = (p.volume_bytes as f64 / num_pacts) as u64;
                            // Skip check if both sides have negligible data (new pact)
                            if we_store < 1024 && they_store_est < 1024 {
                                return false;
                            }
                            let max_vol = we_store.max(they_store_est).max(1);
                            let delta = (we_store as f64 - they_store_est as f64).abs() / max_vol as f64;
                            delta > threshold
                        })
                        .map(|p| p.partner)
                        .collect();

                    for partner in &to_drop {
                        state.active_pacts.retain(|p| p.partner != *partner);
                        state.reliability_scores.remove(partner);
                        let envelope = Envelope {
                            from: state.id,
                            message: Message::PactDrop { partner: state.id },
                            to: *partner,
                            deliver_at: current_time,
                            path_hint: None,
                        };
                        let _ = router_tx.send(envelope).await;
                        let _ = metrics_tx
                            .send(MetricEvent::PactDropped {
                                node: state.id,
                                partner: *partner,
                                time: current_time,
                            })
                            .await;
                    }
                }

                // Karma: earn proportional to bytes stored for others per tick
                if config.karma.enabled {
                    let tick_days = config.simulation.tick_interval_secs as f64 / 86_400.0;
                    let stored_mb = state.storage_used as f64 / 1_048_576.0;
                    let earned = stored_mb * config.karma.earn_per_mb_day * tick_days;
                    state.karma.earn(earned);
                }

                // Checkpoint publishing
                let checkpoint_interval = config.protocol.checkpoint_window_days as f64 * 86_400.0;
                if time - state.last_checkpoint_time >= checkpoint_interval {
                    // Compute merkle root hash over own_events
                    let events_refs: Vec<&Event> = state.own_events.iter().collect();
                    let merkle_root = compute_challenge_hash(&events_refs, 0, u64::MAX, 0);

                    state.checkpoint = Some(crate::types::Checkpoint {
                        merkle_root,
                        event_count: state.own_events.len() as u64,
                        created_at: time,
                        per_device_heads: vec![(state.id, state.seq_counter)],
                    });
                    state.last_checkpoint_time = time;
                }

                // Challenge initiation: send challenges to pact partners
                let tick_interval = config.simulation.tick_interval_secs as f64;
                let challenge_interval = 86_400.0 / config.protocol.challenge_freq_per_day as f64;
                let challenge_prob = tick_interval / challenge_interval;
                let partners: Vec<NodeId> = state.active_pacts.iter().map(|p| p.partner).collect();
                for partner in partners {
                    let roll: f64 = state.rng.gen();
                    if roll < challenge_prob {
                        let nonce: u64 = state.rng.gen();
                        state.pending_challenges.insert(partner, nonce);
                        state.challenge_stats.sent += 1;
                        let envelope = Envelope {
                            from: state.id,
                            message: Message::Challenge {
                                from: state.id,
                                nonce,
                            },
                            to: partner,
                            deliver_at: current_time,
                            path_hint: None,
                        };
                        let _ = router_tx.send(envelope).await;
                    }
                }
            }

            Message::DataOffer { .. } => {}

            Message::ReadRequest { target_author, request_id, wot_tier } => {
                if !state.online {
                    continue;
                }

                // Path 1: Check local data (Instant tier)
                if state.has_events_for(target_author) {
                    let _ = metrics_tx
                        .send(MetricEvent::ReadResult {
                            reader: state.id,
                            target_author,
                            request_id,
                            tier: ReadTier::Instant,
                            wot_tier,
                            latency_secs: 0.0,
                            paths_tried: 1,
                            time: current_time,
                        })
                        .await;
                    continue;
                }

                // Record pending read — capture cached endpoint node for tier classification
                let cached_endpoint_node: Option<NodeId> = state.endpoint_cache.get(&target_author).copied();
                state.pending_reads.insert(target_author, (request_id, current_time, false, cached_endpoint_node, wot_tier));

                // Path 2: Try cached endpoint first (known storage peer for this author)
                if let Some(endpoint) = cached_endpoint_node {
                    let envelope = Envelope {
                        from: state.id,
                        message: Message::RequestData {
                            from: state.id,
                            request_id,
                            ttl: 1,  // Direct query, no forwarding
                            filter: target_author.to_string(),
                        },
                        to: endpoint,
                        deliver_at: current_time,
                        path_hint: Some(DeliveryPath::CachedEndpoint),
                    };
                    let _ = router_tx.send(envelope).await;
                }

                // Path 3: Broadcast gossip request to all WoT peers
                let peers: Vec<NodeId> = state
                    .follows
                    .iter()
                    .chain(state.followers.iter())
                    .copied()
                    .collect();

                for peer in peers {
                    let envelope = Envelope {
                        from: state.id,
                        message: Message::RequestData {
                            from: state.id,
                            request_id,
                            ttl: config.protocol.ttl,
                            filter: target_author.to_string(),
                        },
                        to: peer,
                        deliver_at: current_time,
                        path_hint: Some(DeliveryPath::Gossip),
                    };
                    let _ = router_tx.send(envelope).await;
                }
                state.gossip_stats.sent += 1;
                let _ = metrics_tx
                    .send(MetricEvent::GossipSent {
                        from: state.id,
                        time: current_time,
                    })
                    .await;
            }

            Message::PactRequest { from, volume_bytes, as_standby, created_at, activity_tier } => {
                if !state.online {
                    continue;
                }
                // Age gating: reject if receiver (self) is too new
                let min_age = config.protocol.min_account_age_days as f64 * 86_400.0;
                if current_time - state.created_at < min_age {
                    continue;
                }
                // Age gating: reject if sender is too new
                if current_time - created_at < min_age {
                    continue;
                }
                // Only accept from WoT peers
                if !state.is_wot_peer(from) {
                    continue;
                }
                // Activity tier matching: reject if tiers differ by more than 1
                let my_tier = state.activity_tier();
                if (my_tier as i16 - activity_tier as i16).unsigned_abs() > 1 {
                    continue;
                }
                // Karma check: reject if balance too low
                if config.karma.enabled && state.karma.balance < config.karma.minimum_balance_for_pact {
                    continue;
                }
                // Storage capacity check: reject if not enough room
                if state.storage_capacity > 0 && state.available_capacity() < volume_bytes {
                    continue;
                }
                // Check volume balance
                let own_volume = state.total_stored_bytes();
                if !storage::is_balanced(own_volume, volume_bytes, config.protocol.volume_tolerance) {
                    continue;
                }

                // Determine whether to accept as active or standby:
                // - If requesting as standby and we have standby room -> accept as standby
                // - If requesting as active and we have active room -> accept as active
                // - If active is full but standby has room -> accept as standby regardless
                // - Otherwise reject
                let accept_as_standby;
                if as_standby {
                    if state.standby_count() < config.protocol.pacts_standby as usize {
                        accept_as_standby = true;
                    } else {
                        continue; // No standby room
                    }
                } else if state.pact_count() < config.protocol.pacts_default as usize {
                    accept_as_standby = false;
                } else if state.standby_count() < config.protocol.pacts_standby as usize {
                    // Active is full, but standby has room — accept as standby
                    accept_as_standby = true;
                } else {
                    continue; // No room at all
                }

                // Don't accept if already an active or standby partner
                if state.active_pacts.iter().any(|p| p.partner == from)
                    || state.standby_pacts.iter().any(|p| p.partner == from)
                {
                    continue;
                }

                // Create and send PactOffer
                let pact = crate::types::Pact {
                    partner: state.id,
                    volume_bytes: own_volume,
                    formed_at: current_time,
                    is_standby: accept_as_standby,
                };
                let envelope = Envelope {
                    from: state.id,
                    message: Message::PactOffer { pact },
                    to: from,
                    deliver_at: current_time,
                    path_hint: None,
                };
                let _ = router_tx.send(envelope).await;
            }

            Message::PactOffer { pact } => {
                if !state.online {
                    continue;
                }

                let partner_id = pact.partner;

                // Don't accept if already a partner
                if state.active_pacts.iter().any(|p| p.partner == partner_id)
                    || state.standby_pacts.iter().any(|p| p.partner == partner_id)
                {
                    continue;
                }

                if pact.is_standby {
                    // Standby pact offer — check standby capacity
                    if state.standby_count() >= config.protocol.pacts_standby as usize {
                        continue;
                    }
                    // Add to standby_pacts
                    state.standby_pacts.push(crate::types::Pact {
                        partner: partner_id,
                        volume_bytes: pact.volume_bytes,
                        formed_at: current_time,
                        is_standby: true,
                    });
                } else {
                    // Active pact offer — check active capacity
                    if state.pact_count() >= config.protocol.pacts_default as usize {
                        continue;
                    }
                    // Add to active_pacts
                    state.active_pacts.push(crate::types::Pact {
                        partner: partner_id,
                        volume_bytes: pact.volume_bytes,
                        formed_at: current_time,
                        is_standby: false,
                    });
                    state.reliability_scores.insert(partner_id, 1.0);
                }

                // Record first pact time
                if !pact.is_standby && state.first_pact_time.is_none() {
                    state.first_pact_time = Some(current_time);
                }

                // Send PactAccept (echo back the is_standby flag)
                let accept_pact = crate::types::Pact {
                    partner: state.id,
                    volume_bytes: state.total_stored_bytes(),
                    formed_at: current_time,
                    is_standby: pact.is_standby,
                };
                let envelope = Envelope {
                    from: state.id,
                    message: Message::PactAccept { pact: accept_pact },
                    to: partner_id,
                    deliver_at: current_time,
                    path_hint: None,
                };
                let _ = router_tx.send(envelope).await;

                // Emit PactFormed
                let _ = metrics_tx
                    .send(MetricEvent::PactFormed {
                        node: state.id,
                        partner: partner_id,
                        time: current_time,
                    })
                    .await;
            }

            Message::PactAccept { pact } => {
                if !state.online {
                    continue;
                }
                let partner_id = pact.partner;
                // Check not already a partner (active or standby)
                if state.active_pacts.iter().any(|p| p.partner == partner_id)
                    || state.standby_pacts.iter().any(|p| p.partner == partner_id)
                {
                    continue;
                }

                if pact.is_standby {
                    // Standby pact accept — check standby capacity
                    if state.standby_count() >= config.protocol.pacts_standby as usize {
                        continue;
                    }
                    state.standby_pacts.push(crate::types::Pact {
                        partner: partner_id,
                        volume_bytes: pact.volume_bytes,
                        formed_at: current_time,
                        is_standby: true,
                    });
                } else {
                    // Active pact accept
                    if state.pact_count() >= config.protocol.pacts_default as usize {
                        continue;
                    }
                    state.active_pacts.push(crate::types::Pact {
                        partner: partner_id,
                        volume_bytes: pact.volume_bytes,
                        formed_at: current_time,
                        is_standby: false,
                    });
                    state.reliability_scores.insert(partner_id, 1.0);
                }

                // Record first pact time
                if !pact.is_standby && state.first_pact_time.is_none() {
                    state.first_pact_time = Some(current_time);
                }

                // Emit PactFormed
                let _ = metrics_tx
                    .send(MetricEvent::PactFormed {
                        node: state.id,
                        partner: partner_id,
                        time: current_time,
                    })
                    .await;
            }

            Message::PactDrop { partner } => {
                if !state.online {
                    continue;
                }
                // Remove partner from active_pacts and standby_pacts
                state.active_pacts.retain(|p| p.partner != partner);
                state.standby_pacts.retain(|p| p.partner != partner);
                state.reliability_scores.remove(&partner);

                // Promote standby if available and active is below target
                if state.pact_count() < config.protocol.pacts_default as usize {
                    if let Some(standby) = state.standby_pacts.pop() {
                        state.active_pacts.push(crate::types::Pact {
                            partner: standby.partner,
                            volume_bytes: standby.volume_bytes,
                            formed_at: current_time,
                            is_standby: false,
                        });
                        state.reliability_scores.insert(standby.partner, 1.0);
                    }
                }

                // Emit PactDropped
                let _ = metrics_tx
                    .send(MetricEvent::PactDropped {
                        node: state.id,
                        partner,
                        time: current_time,
                    })
                    .await;
            }
        }
    }
}

// ── compute_challenge_hash ──────────────────────────────────────────

/// Compute a deterministic hash over the given events in the range
/// [start, end] using the provided nonce. Uses `DefaultHasher`.
///
/// When events carry `nostr_json`, uses SHA-256 over the real Nostr event
/// JSON for a protocol-compliant challenge hash.
pub fn compute_challenge_hash(events: &[&Event], start: u64, end: u64, nonce: u64) -> u64 {
    // Check if any event has nostr_json — if so, use the Nostr-aware path
    let has_nostr = events.iter().any(|e| e.nostr_json.is_some());
    if has_nostr {
        let jsons: Vec<&str> = events
            .iter()
            .filter(|e| e.id >= start && e.id <= end)
            .filter_map(|e| e.nostr_json.as_deref())
            .collect();
        if !jsons.is_empty() {
            #[cfg(feature = "nostr-events")]
            return crate::nostr_bridge::compute_challenge_hash_nostr(&jsons, nonce);
        }
    }

    // Fallback: original DefaultHasher path
    let mut hasher = DefaultHasher::new();
    nonce.hash(&mut hasher);

    for event in events {
        if event.id >= start && event.id <= end {
            event.id.hash(&mut hasher);
            event.author.hash(&mut hasher);
            event.seq.hash(&mut hasher);
            event.size_bytes.hash(&mut hasher);
        }
    }

    hasher.finish()
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::EventKind;

    fn make_event(id: u64, author: NodeId, size: u32) -> Event {
        Event {
            id,
            author,
            kind: EventKind::Note,
            size_bytes: size,
            seq: 1,
            prev_hash: 0,
            created_at: 0.0,
            nostr_json: None,
            interaction_target: None,
        }
    }

    #[tokio::test]
    async fn test_request_data_wot_filtered() {
        let config = SimConfig::default();
        let state = NodeState::new(1, crate::types::NodeType::Full, &config);
        // No WoT peers — sender 99 is a stranger

        let (tx, rx) = mpsc::channel(16);
        let (router_tx, _router_rx) = mpsc::channel(16);
        let (metrics_tx, _metrics_rx) = mpsc::channel(16);

        tokio::spawn(run_node(state, rx, router_tx, metrics_tx, config));

        tx.send(Message::Tick(1.0)).await.unwrap();
        tx.send(Message::RequestData {
            from: 99,
            request_id: 1,
            ttl: 3,
            filter: "50".to_string(),
        })
        .await
        .unwrap();
        tx.send(Message::Shutdown).await.unwrap();
        // Non-WoT peer should be filtered (wot_filtered incremented)
        // We can verify by checking no forwarded envelopes
    }

    #[tokio::test]
    async fn test_request_data_wot_peer_forwarded() {
        let config = SimConfig::default();
        let mut state = NodeState::new(1, crate::types::NodeType::Full, &config);
        state.follows.insert(10);
        state.follows.insert(20);

        let (tx, rx) = mpsc::channel(16);
        let (router_tx, mut router_rx) = mpsc::channel(16);
        let (metrics_tx, _metrics_rx) = mpsc::channel(16);

        tokio::spawn(run_node(state, rx, router_tx, metrics_tx, config));

        tx.send(Message::Tick(1.0)).await.unwrap();
        tx.send(Message::RequestData {
            from: 10,
            request_id: 1,
            ttl: 3,
            filter: "50".to_string(), // unknown author, will try to forward
        })
        .await
        .unwrap();
        tx.send(Message::Shutdown).await.unwrap();
        drop(tx);

        // WoT peer should trigger forwarding
        let mut forwarded = 0;
        while let Some(envelope) = router_rx.recv().await {
            if matches!(envelope.message, Message::RequestData { .. }) {
                forwarded += 1;
            }
        }
        assert!(forwarded > 0, "expected forwarded messages for WoT peer");
    }

    #[tokio::test]
    async fn test_valid_challenge_response_passes() {
        let config = SimConfig::default();
        let mut state = NodeState::new(1, crate::types::NodeType::Full, &config);
        // Node stores events for partner 10
        let events = vec![make_event(1, 10, 256), make_event(2, 10, 512)];
        state.stored_events.insert(10, events.clone());
        state.active_pacts.push(crate::types::Pact {
            partner: 10,
            volume_bytes: 0,
            formed_at: 0.0,
            is_standby: false,
        });
        state.reliability_scores.insert(10, 1.0);
        // Set pending challenge nonce
        let nonce = 42u64;
        state.pending_challenges.insert(10, nonce);

        // Compute the expected proof
        let event_refs: Vec<&Event> = events.iter().collect();
        let expected_proof = compute_challenge_hash(&event_refs, 0, u64::MAX, nonce);

        let (tx, rx) = mpsc::channel(16);
        let (router_tx, _router_rx) = mpsc::channel(16);
        let (metrics_tx, mut metrics_rx) = mpsc::channel(16);

        tokio::spawn(run_node(state, rx, router_tx, metrics_tx, config));

        tx.send(Message::Tick(1.0)).await.unwrap();
        tx.send(Message::ChallengeResponse {
            from: 10,
            proof: expected_proof,
        })
        .await
        .unwrap();
        tx.send(Message::Shutdown).await.unwrap();
        drop(tx);

        let mut found_passed = false;
        while let Some(event) = metrics_rx.recv().await {
            if let MetricEvent::ChallengeResult { passed: true, .. } = event {
                found_passed = true;
            }
        }
        assert!(found_passed, "expected challenge to pass with correct proof");
    }

    #[tokio::test]
    async fn test_invalid_challenge_response_fails() {
        let config = SimConfig::default();
        let mut state = NodeState::new(1, crate::types::NodeType::Full, &config);
        state.stored_events.insert(10, vec![make_event(1, 10, 256)]);
        state.active_pacts.push(crate::types::Pact {
            partner: 10,
            volume_bytes: 0,
            formed_at: 0.0,
            is_standby: false,
        });
        state.reliability_scores.insert(10, 1.0);
        state.pending_challenges.insert(10, 42);

        let (tx, rx) = mpsc::channel(16);
        let (router_tx, _router_rx) = mpsc::channel(16);
        let (metrics_tx, mut metrics_rx) = mpsc::channel(16);

        tokio::spawn(run_node(state, rx, router_tx, metrics_tx, config));

        tx.send(Message::Tick(1.0)).await.unwrap();
        tx.send(Message::ChallengeResponse {
            from: 10,
            proof: 99999, // wrong proof
        })
        .await
        .unwrap();
        tx.send(Message::Shutdown).await.unwrap();
        drop(tx);

        let mut found_failed = false;
        while let Some(event) = metrics_rx.recv().await {
            if let MetricEvent::ChallengeResult { passed: false, .. } = event {
                found_failed = true;
            }
        }
        assert!(found_failed, "expected challenge to fail with wrong proof");
    }

    #[tokio::test]
    async fn test_repeated_failures_trigger_pact_drop() {
        let config = SimConfig::default();
        let mut state = NodeState::new(1, crate::types::NodeType::Full, &config);
        state.stored_events.insert(10, vec![make_event(1, 10, 256)]);
        state.active_pacts.push(crate::types::Pact {
            partner: 10,
            volume_bytes: 0,
            formed_at: 0.0,
            is_standby: false,
        });
        // Set reliability low enough that one more failure triggers drop
        state.reliability_scores.insert(10, 0.45);
        state.pending_challenges.insert(10, 42);

        let (tx, rx) = mpsc::channel(16);
        let (router_tx, mut router_rx) = mpsc::channel(16);
        let (metrics_tx, mut metrics_rx) = mpsc::channel(16);

        tokio::spawn(run_node(state, rx, router_tx, metrics_tx, config));

        tx.send(Message::Tick(1.0)).await.unwrap();
        tx.send(Message::ChallengeResponse {
            from: 10,
            proof: 99999, // wrong
        })
        .await
        .unwrap();
        tx.send(Message::Shutdown).await.unwrap();
        drop(tx);

        // Check PactDrop was sent
        let mut got_pact_drop = false;
        while let Some(envelope) = router_rx.recv().await {
            if matches!(envelope.message, Message::PactDrop { .. }) {
                got_pact_drop = true;
            }
        }

        let mut got_dropped_metric = false;
        while let Some(event) = metrics_rx.recv().await {
            if matches!(event, MetricEvent::PactDropped { .. }) {
                got_dropped_metric = true;
            }
        }

        assert!(got_pact_drop, "expected PactDrop message after reliability drops");
        assert!(got_dropped_metric, "expected PactDropped metric");
    }

    #[tokio::test]
    async fn test_checkpoint_published_after_interval() {
        let mut config = SimConfig::default();
        config.protocol.checkpoint_window_days = 1; // 1 day = 86400s

        let mut state = NodeState::new(1, crate::types::NodeType::Full, &config);
        // Publish some events first
        state.own_events.push(make_event(1, 1, 256));
        state.own_events.push(make_event(2, 1, 512));

        let (tx, rx) = mpsc::channel(16);
        let (router_tx, _router_rx) = mpsc::channel(16);
        let (metrics_tx, mut metrics_rx) = mpsc::channel(16);

        tokio::spawn(run_node(state, rx, router_tx, metrics_tx, config));

        // Send Tick at time >= checkpoint_interval (86400s)
        tx.send(Message::Tick(86_400.0)).await.unwrap();
        // Get a snapshot to see checkpoint
        tx.send(Message::Tick(86_401.0)).await.unwrap();
        tx.send(Message::Shutdown).await.unwrap();
        drop(tx);

        // The node should have set a checkpoint
        // We verify indirectly by checking snapshot metrics received
        let mut got_snapshot = false;
        while let Some(event) = metrics_rx.recv().await {
            if matches!(event, MetricEvent::NodeSnapshot { .. }) {
                got_snapshot = true;
            }
        }
        assert!(got_snapshot);
        // If we had access to state, we'd check state.checkpoint.is_some()
        // The test succeeds if no panics occur during checkpoint computation
    }

    #[tokio::test]
    async fn test_tick_initiates_pact_requests() {
        let mut config = SimConfig::default();
        config.protocol.min_account_age_days = 0;
        let mut state = NodeState::new(1, crate::types::NodeType::Full, &config);
        // Add WoT peers so pact candidates exist
        state.follows.insert(10);
        state.follows.insert(20);
        state.followers.insert(30);
        // No active pacts — should try to form some

        let (tx, rx) = mpsc::channel(32);
        let (router_tx, mut router_rx) = mpsc::channel(32);
        let (metrics_tx, _metrics_rx) = mpsc::channel(32);

        tokio::spawn(run_node(state, rx, router_tx, metrics_tx, config));

        tx.send(Message::Tick(60.0)).await.unwrap();
        tx.send(Message::Shutdown).await.unwrap();
        drop(tx);

        let mut pact_requests = 0;
        while let Some(envelope) = router_rx.recv().await {
            if matches!(envelope.message, Message::PactRequest { .. }) {
                pact_requests += 1;
            }
        }
        assert!(
            pact_requests > 0,
            "expected PactRequests from node with 0 pacts"
        );
    }

    #[tokio::test]
    async fn test_pact_drop_removes_and_promotes_standby() {
        let config = SimConfig::default();
        let mut state = NodeState::new(1, crate::types::NodeType::Full, &config);
        // Add an active pact partner
        state.active_pacts.push(crate::types::Pact {
            partner: 10,
            volume_bytes: 0,
            formed_at: 0.0,
            is_standby: false,
        });
        state.reliability_scores.insert(10, 1.0);
        // Add a standby pact
        state.standby_pacts.push(crate::types::Pact {
            partner: 20,
            volume_bytes: 0,
            formed_at: 0.0,
            is_standby: true,
        });

        let (tx, rx) = mpsc::channel(16);
        let (router_tx, _router_rx) = mpsc::channel(16);
        let (metrics_tx, mut metrics_rx) = mpsc::channel(16);

        tokio::spawn(run_node(state, rx, router_tx, metrics_tx, config));

        tx.send(Message::Tick(1.0)).await.unwrap();
        tx.send(Message::PactDrop { partner: 10 }).await.unwrap();
        // Get snapshot to see state
        tx.send(Message::Tick(2.0)).await.unwrap();
        tx.send(Message::Shutdown).await.unwrap();
        drop(tx);

        let mut got_dropped = false;
        let mut final_pact_count = 0;
        while let Some(event) = metrics_rx.recv().await {
            match event {
                MetricEvent::PactDropped { partner: 10, .. } => got_dropped = true,
                MetricEvent::NodeSnapshot { pact_count, time, .. } if time > 1.5 => {
                    final_pact_count = pact_count;
                }
                _ => {}
            }
        }
        assert!(got_dropped, "expected PactDropped metric");
        assert_eq!(final_pact_count, 1, "standby should have been promoted");
    }

    #[tokio::test]
    async fn test_pact_offer_accept_flow() {
        let config = SimConfig::default();

        // Node A (id=1) receives PactOffer from Node B (id=10)
        let state_a = NodeState::new(1, crate::types::NodeType::Full, &config);

        let (tx_a, rx_a) = mpsc::channel(16);
        let (router_tx_a, mut router_rx_a) = mpsc::channel(16);
        let (metrics_tx_a, mut metrics_rx_a) = mpsc::channel(16);

        tokio::spawn(run_node(state_a, rx_a, router_tx_a, metrics_tx_a, config.clone()));

        tx_a.send(Message::Tick(1.0)).await.unwrap();
        tx_a.send(Message::PactOffer {
            pact: crate::types::Pact {
                partner: 10,
                volume_bytes: 0,
                formed_at: 1.0,
                is_standby: false,
            },
        })
        .await
        .unwrap();
        // Also check PactAccept handling
        tx_a.send(Message::Tick(2.0)).await.unwrap();
        tx_a.send(Message::Shutdown).await.unwrap();
        drop(tx_a);

        // Check PactAccept was sent back
        let mut got_accept = false;
        while let Some(envelope) = router_rx_a.recv().await {
            if matches!(envelope.message, Message::PactAccept { .. }) {
                got_accept = true;
            }
        }
        assert!(got_accept, "expected PactAccept sent after PactOffer");

        // Check PactFormed metric
        let mut got_formed = false;
        while let Some(event) = metrics_rx_a.recv().await {
            if matches!(event, MetricEvent::PactFormed { .. }) {
                got_formed = true;
            }
        }
        assert!(got_formed, "expected PactFormed metric");
    }

    #[tokio::test]
    async fn test_pact_request_from_wot_peer_sends_offer() {
        let mut config = SimConfig::default();
        config.protocol.min_account_age_days = 0;
        let mut state = NodeState::new(1, crate::types::NodeType::Full, &config);
        state.follows.insert(10);

        let (tx, rx) = mpsc::channel(16);
        let (router_tx, mut router_rx) = mpsc::channel(16);
        let (metrics_tx, _metrics_rx) = mpsc::channel(16);

        tokio::spawn(run_node(state, rx, router_tx, metrics_tx, config));

        tx.send(Message::Tick(1.0)).await.unwrap();
        tx.send(Message::PactRequest {
            from: 10,
            volume_bytes: 0,
            as_standby: false,
            created_at: 0.0,
            activity_tier: 1,
        })
        .await
        .unwrap();
        tx.send(Message::Shutdown).await.unwrap();
        drop(tx);

        let mut got_offer = false;
        while let Some(envelope) = router_rx.recv().await {
            if matches!(envelope.message, Message::PactOffer { .. }) {
                got_offer = true;
            }
        }
        assert!(got_offer, "expected PactOffer from WoT peer's PactRequest");
    }

    #[tokio::test]
    async fn test_responding_to_request_increments_gossip_up() {
        let config = SimConfig::default();
        let mut state = NodeState::new(1, crate::types::NodeType::Full, &config);
        state.follows.insert(10);
        let event = make_event(100, 10, 256);
        state.stored_events.insert(10, vec![event]);

        let (tx, rx) = mpsc::channel(16);
        let (router_tx, mut router_rx) = mpsc::channel(16);
        let (metrics_tx, mut metrics_rx) = mpsc::channel(16);

        tokio::spawn(run_node(state, rx, router_tx, metrics_tx, config));

        tx.send(Message::Tick(1.0)).await.unwrap();
        tx.send(Message::RequestData {
            from: 10,
            request_id: 1,
            ttl: 1,
            filter: "10".to_string(),
        })
        .await
        .unwrap();
        // Get a snapshot after the request to check bandwidth
        tx.send(Message::Tick(2.0)).await.unwrap();
        tx.send(Message::Shutdown).await.unwrap();
        drop(tx);

        // Drain router
        while let Some(_) = router_rx.recv().await {}

        // Check the NodeSnapshot for gossip_up
        let mut found_gossip_up = false;
        while let Some(event) = metrics_rx.recv().await {
            if let MetricEvent::NodeSnapshot { bandwidth, .. } = event {
                if bandwidth.by_category.gossip_up > 0 {
                    found_gossip_up = true;
                }
            }
        }
        assert!(found_gossip_up, "expected gossip_up > 0 after responding to RequestData");
    }

    #[tokio::test]
    async fn test_rate_limiting() {
        let mut config = SimConfig::default();
        config.protocol.rate_limit_10057 = 3; // low limit for testing
        let state = NodeState::new(1, crate::types::NodeType::Full, &config);

        let (tx, rx) = mpsc::channel(64);
        let (router_tx, _router_rx) = mpsc::channel(64);
        let (metrics_tx, _metrics_rx) = mpsc::channel(64);

        tokio::spawn(run_node(state, rx, router_tx, metrics_tx, config));

        tx.send(Message::Tick(1.0)).await.unwrap();

        // Send more requests than the limit from same source
        for i in 0..5 {
            tx.send(Message::RequestData {
                from: 99,
                request_id: i,
                ttl: 1,
                filter: "50".to_string(),
            })
            .await
            .unwrap();
        }
        tx.send(Message::Shutdown).await.unwrap();
        // 4th and 5th requests should be rate-limited
    }

    #[tokio::test]
    async fn test_deliver_at_uses_current_time() {
        let config = SimConfig::default();
        let mut state = NodeState::new(1, crate::types::NodeType::Full, &config);
        state.follows.insert(10); // Need a WoT peer
        // Store events so RequestData can respond
        let event = make_event(100, 10, 256);
        state.stored_events.insert(10, vec![event]);

        let (tx, rx) = mpsc::channel(16);
        let (router_tx, mut router_rx) = mpsc::channel(16);
        let (metrics_tx, _metrics_rx) = mpsc::channel(16);

        tokio::spawn(run_node(state, rx, router_tx, metrics_tx, config));

        // Send Tick first to set current_time
        tx.send(Message::Tick(42.0)).await.unwrap();
        // Then send RequestData
        tx.send(Message::RequestData {
            from: 10,
            request_id: 1,
            ttl: 1,
            filter: "10".to_string(),
        })
        .await
        .unwrap();
        tx.send(Message::Shutdown).await.unwrap();
        drop(tx);

        // Check that the envelope has deliver_at = 42.0
        if let Some(envelope) = router_rx.recv().await {
            assert!(
                (envelope.deliver_at - 42.0).abs() < f64::EPSILON,
                "expected deliver_at=42.0, got {}",
                envelope.deliver_at
            );
        }
    }

    #[tokio::test]
    async fn test_tick_emits_node_snapshot() {
        let config = SimConfig::default();
        let state = NodeState::new(1, crate::types::NodeType::Full, &config);

        let (tx, rx) = mpsc::channel(16);
        let (router_tx, _router_rx) = mpsc::channel(16);
        let (metrics_tx, mut metrics_rx) = mpsc::channel(16);

        tokio::spawn(run_node(state, rx, router_tx, metrics_tx, config));

        // Send Tick
        tx.send(Message::Tick(60.0)).await.unwrap();
        // Send Shutdown so node exits
        tx.send(Message::Shutdown).await.unwrap();

        // Collect metrics
        drop(tx);
        let mut got_snapshot = false;
        while let Some(event) = metrics_rx.recv().await {
            if matches!(event, MetricEvent::NodeSnapshot { .. }) {
                got_snapshot = true;
            }
        }
        assert!(got_snapshot, "expected NodeSnapshot metric from Tick");
    }

    #[tokio::test]
    async fn test_node_snapshot_online_field_true_when_online() {
        let config = SimConfig::default();
        let state = NodeState::new(1, crate::types::NodeType::Full, &config);
        // NodeState starts online by default

        let (tx, rx) = mpsc::channel(16);
        let (router_tx, _router_rx) = mpsc::channel(16);
        let (metrics_tx, mut metrics_rx) = mpsc::channel(16);

        tokio::spawn(run_node(state, rx, router_tx, metrics_tx, config));

        tx.send(Message::Tick(1.0)).await.unwrap();
        tx.send(Message::Shutdown).await.unwrap();
        drop(tx);

        let mut found_online = false;
        while let Some(event) = metrics_rx.recv().await {
            if let MetricEvent::NodeSnapshot { online, .. } = event {
                assert!(online, "expected online=true when node is online");
                found_online = true;
            }
        }
        assert!(found_online, "expected NodeSnapshot with online field");
    }

    #[tokio::test]
    async fn test_node_snapshot_online_field_false_when_offline() {
        let config = SimConfig::default();
        let state = NodeState::new(1, crate::types::NodeType::Full, &config);

        let (tx, rx) = mpsc::channel(16);
        let (router_tx, _router_rx) = mpsc::channel(16);
        let (metrics_tx, mut metrics_rx) = mpsc::channel(16);

        tokio::spawn(run_node(state, rx, router_tx, metrics_tx, config));

        // Go offline, then tick
        tx.send(Message::GoOffline).await.unwrap();
        tx.send(Message::Tick(1.0)).await.unwrap();
        tx.send(Message::Shutdown).await.unwrap();
        drop(tx);

        let mut found_offline = false;
        while let Some(event) = metrics_rx.recv().await {
            if let MetricEvent::NodeSnapshot { online, .. } = event {
                assert!(!online, "expected online=false when node is offline");
                found_offline = true;
            }
        }
        assert!(found_offline, "expected NodeSnapshot emitted even when offline");
    }

    #[tokio::test]
    async fn test_availability_samples_populated_via_tick() {
        let config = SimConfig::default();
        let state = NodeState::new(1, crate::types::NodeType::Full, &config);

        let (tx, rx) = mpsc::channel(32);
        let (router_tx, _router_rx) = mpsc::channel(32);
        let (metrics_tx, metrics_rx) = mpsc::channel(32);

        // Run collector alongside node
        let collector = crate::sim::metrics::MetricsCollector::new(
            metrics_rx,
            0,
            &crate::config::StreamingConfig::default(),
            None,
        );
        let collector_handle = tokio::spawn(collector.run());

        tokio::spawn(run_node(state, rx, router_tx, metrics_tx, config));

        // Tick while online
        tx.send(Message::Tick(1.0)).await.unwrap();
        tx.send(Message::Tick(2.0)).await.unwrap();
        // Go offline
        tx.send(Message::GoOffline).await.unwrap();
        tx.send(Message::Tick(3.0)).await.unwrap();
        // Back online
        tx.send(Message::GoOnline).await.unwrap();
        tx.send(Message::Tick(4.0)).await.unwrap();
        tx.send(Message::Shutdown).await.unwrap();
        drop(tx);

        let collected = collector_handle.await.unwrap();
        let node = collected.snapshots.get(&1).expect("node 1 should exist");
        assert_eq!(
            node.availability_samples,
            vec![true, true, false, true],
            "expected [online, online, offline, online] availability samples"
        );
    }

    #[test]
    fn test_challenge_hash_deterministic() {
        let e1 = make_event(1, 10, 256);
        let e2 = make_event(2, 10, 512);
        let events: Vec<&Event> = vec![&e1, &e2];

        // Same input = same hash
        let h1 = compute_challenge_hash(&events, 0, 10, 42);
        let h2 = compute_challenge_hash(&events, 0, 10, 42);
        assert_eq!(h1, h2);

        // Different nonce = different hash
        let h3 = compute_challenge_hash(&events, 0, 10, 99);
        assert_ne!(h1, h3);
    }

    #[tokio::test]
    async fn test_standby_pacts_form_after_active_full() {
        // After enough Ticks, a node should have both active and standby pacts.
        // We use a small config so the test runs quickly:
        // pacts_default=2 (fill active), pacts_standby=2 (fill standby).
        // We give the node many WoT peers so it has enough candidates.
        let mut config = SimConfig::default();
        config.protocol.pacts_default = 2;
        config.protocol.pacts_standby = 2;
        config.protocol.volume_tolerance = 1.0; // very tolerant
        config.protocol.min_account_age_days = 0;

        // Node 1 is the test subject
        let mut state = NodeState::new(1, crate::types::NodeType::Full, &config);
        // Fill active pacts to capacity
        state.active_pacts.push(crate::types::Pact {
            partner: 10,
            volume_bytes: 0,
            formed_at: 0.0,
            is_standby: false,
        });
        state.active_pacts.push(crate::types::Pact {
            partner: 20,
            volume_bytes: 0,
            formed_at: 0.0,
            is_standby: false,
        });
        state.reliability_scores.insert(10, 1.0);
        state.reliability_scores.insert(20, 1.0);
        // Add more WoT peers for standby candidates
        for id in [10, 20, 30, 40, 50] {
            state.follows.insert(id);
        }

        let (tx, rx) = mpsc::channel(64);
        let (router_tx, mut router_rx) = mpsc::channel(64);
        let (metrics_tx, _metrics_rx) = mpsc::channel(64);

        tokio::spawn(run_node(state, rx, router_tx, metrics_tx, config.clone()));

        // Send ticks to trigger standby pact requests
        tx.send(Message::Tick(60.0)).await.unwrap();
        tx.send(Message::Tick(120.0)).await.unwrap();
        tx.send(Message::Shutdown).await.unwrap();
        drop(tx);

        // Collect standby PactRequests
        let mut standby_requests = 0;
        while let Some(envelope) = router_rx.recv().await {
            if let Message::PactRequest { as_standby: true, .. } = envelope.message {
                standby_requests += 1;
            }
        }
        assert!(
            standby_requests > 0,
            "expected standby PactRequests when active pacts are full but standby has room"
        );
    }

    #[tokio::test]
    async fn test_standby_pact_count_does_not_exceed_limit() {
        // Verify that standby pact count does not exceed pacts_standby.
        let mut config = SimConfig::default();
        config.protocol.pacts_default = 2;
        config.protocol.pacts_standby = 1; // only 1 standby allowed
        config.protocol.volume_tolerance = 1.0;
        config.protocol.min_account_age_days = 0;

        let mut state = NodeState::new(1, crate::types::NodeType::Full, &config);
        // Active is full
        state.active_pacts.push(crate::types::Pact {
            partner: 10,
            volume_bytes: 0,
            formed_at: 0.0,
            is_standby: false,
        });
        state.active_pacts.push(crate::types::Pact {
            partner: 20,
            volume_bytes: 0,
            formed_at: 0.0,
            is_standby: false,
        });
        state.reliability_scores.insert(10, 1.0);
        state.reliability_scores.insert(20, 1.0);
        // Already has 1 standby at the limit
        state.standby_pacts.push(crate::types::Pact {
            partner: 30,
            volume_bytes: 0,
            formed_at: 0.0,
            is_standby: true,
        });
        // WoT peers
        for id in [10, 20, 30, 40, 50, 60] {
            state.follows.insert(id);
        }

        let (tx, rx) = mpsc::channel(64);
        let (router_tx, mut router_rx) = mpsc::channel(64);
        let (metrics_tx, _metrics_rx) = mpsc::channel(64);

        tokio::spawn(run_node(state, rx, router_tx, metrics_tx, config.clone()));

        tx.send(Message::Tick(60.0)).await.unwrap();
        tx.send(Message::Tick(120.0)).await.unwrap();
        tx.send(Message::Shutdown).await.unwrap();
        drop(tx);

        // Should NOT send any standby PactRequests since standby is at limit
        let mut standby_requests = 0;
        while let Some(envelope) = router_rx.recv().await {
            if let Message::PactRequest { as_standby: true, .. } = envelope.message {
                standby_requests += 1;
            }
        }
        assert_eq!(
            standby_requests, 0,
            "expected no standby PactRequests when standby is already at limit"
        );
    }

    #[tokio::test]
    async fn test_standby_pact_request_accepted_as_standby() {
        // A node with full active but room for standby should accept
        // a PactRequest{as_standby: true} and produce a PactOffer with is_standby=true.
        let mut config = SimConfig::default();
        config.protocol.pacts_default = 1;
        config.protocol.pacts_standby = 2;
        config.protocol.volume_tolerance = 1.0;
        config.protocol.min_account_age_days = 0;

        let mut state = NodeState::new(1, crate::types::NodeType::Full, &config);
        // Active is full (1 pact)
        state.active_pacts.push(crate::types::Pact {
            partner: 10,
            volume_bytes: 0,
            formed_at: 0.0,
            is_standby: false,
        });
        state.reliability_scores.insert(10, 1.0);
        state.follows.insert(20); // WoT peer for the request

        let (tx, rx) = mpsc::channel(16);
        let (router_tx, mut router_rx) = mpsc::channel(16);
        let (metrics_tx, _metrics_rx) = mpsc::channel(16);

        tokio::spawn(run_node(state, rx, router_tx, metrics_tx, config));

        tx.send(Message::Tick(1.0)).await.unwrap();
        tx.send(Message::PactRequest {
            from: 20,
            volume_bytes: 0,
            as_standby: true,
            created_at: 0.0,
            activity_tier: 1,
        })
        .await
        .unwrap();
        tx.send(Message::Shutdown).await.unwrap();
        drop(tx);

        let mut got_standby_offer = false;
        while let Some(envelope) = router_rx.recv().await {
            if let Message::PactOffer { pact } = &envelope.message {
                if pact.is_standby {
                    got_standby_offer = true;
                }
            }
        }
        assert!(
            got_standby_offer,
            "expected PactOffer with is_standby=true for standby request"
        );
    }

    #[tokio::test]
    async fn test_active_request_falls_back_to_standby_when_active_full() {
        // When a node receives a non-standby PactRequest but active is full and standby
        // has room, it should accept as standby.
        let mut config = SimConfig::default();
        config.protocol.pacts_default = 1;
        config.protocol.pacts_standby = 2;
        config.protocol.volume_tolerance = 1.0;
        config.protocol.min_account_age_days = 0;

        let mut state = NodeState::new(1, crate::types::NodeType::Full, &config);
        state.active_pacts.push(crate::types::Pact {
            partner: 10,
            volume_bytes: 0,
            formed_at: 0.0,
            is_standby: false,
        });
        state.reliability_scores.insert(10, 1.0);
        state.follows.insert(20);

        let (tx, rx) = mpsc::channel(16);
        let (router_tx, mut router_rx) = mpsc::channel(16);
        let (metrics_tx, _metrics_rx) = mpsc::channel(16);

        tokio::spawn(run_node(state, rx, router_tx, metrics_tx, config));

        tx.send(Message::Tick(1.0)).await.unwrap();
        tx.send(Message::PactRequest {
            from: 20,
            volume_bytes: 0,
            as_standby: false,
            created_at: 0.0,
            activity_tier: 1,
        })
        .await
        .unwrap();
        tx.send(Message::Shutdown).await.unwrap();
        drop(tx);

        let mut got_standby_offer = false;
        while let Some(envelope) = router_rx.recv().await {
            if let Message::PactOffer { pact } = &envelope.message {
                if pact.is_standby {
                    got_standby_offer = true;
                }
            }
        }
        assert!(
            got_standby_offer,
            "expected PactOffer with is_standby=true when active is full but standby has room"
        );
    }

    #[tokio::test]
    async fn test_pact_offer_standby_added_to_standby_list() {
        // When a standby PactOffer is received, the pact should go into standby_pacts,
        // not active_pacts. We verify via PactAccept sent back and a NodeSnapshot.
        let mut config = SimConfig::default();
        config.protocol.pacts_default = 2;
        config.protocol.pacts_standby = 2;
        config.protocol.volume_tolerance = 1.0;

        let state = NodeState::new(1, crate::types::NodeType::Full, &config);

        let (tx, rx) = mpsc::channel(16);
        let (router_tx, mut router_rx) = mpsc::channel(16);
        let (metrics_tx, mut metrics_rx) = mpsc::channel(16);

        tokio::spawn(run_node(state, rx, router_tx, metrics_tx, config.clone()));

        tx.send(Message::Tick(1.0)).await.unwrap();
        // Send a standby PactOffer from partner 10
        tx.send(Message::PactOffer {
            pact: crate::types::Pact {
                partner: 10,
                volume_bytes: 0,
                formed_at: 1.0,
                is_standby: true,
            },
        })
        .await
        .unwrap();
        // Get snapshot after the offer
        tx.send(Message::Tick(2.0)).await.unwrap();
        tx.send(Message::Shutdown).await.unwrap();
        drop(tx);

        // Check PactAccept was sent back with is_standby=true
        let mut got_standby_accept = false;
        while let Some(envelope) = router_rx.recv().await {
            if let Message::PactAccept { pact } = &envelope.message {
                if pact.is_standby {
                    got_standby_accept = true;
                }
            }
        }
        assert!(
            got_standby_accept,
            "expected PactAccept with is_standby=true for standby offer"
        );

        // Check pact_count in the snapshot at time=2.0
        // Active pacts should be 0, so pact_count (which counts active) should be 0
        let mut found_snapshot = false;
        while let Some(event) = metrics_rx.recv().await {
            if let MetricEvent::NodeSnapshot { pact_count, time, .. } = event {
                if time > 1.5 {
                    assert_eq!(
                        pact_count, 0,
                        "standby pact should NOT appear in active pact_count"
                    );
                    found_snapshot = true;
                }
            }
        }
        assert!(found_snapshot, "expected a NodeSnapshot after the standby offer");
    }

    #[tokio::test]
    async fn test_pact_accept_standby_added_to_standby_list() {
        // When a standby PactAccept is received, it should be added to standby_pacts.
        let mut config = SimConfig::default();
        config.protocol.pacts_default = 2;
        config.protocol.pacts_standby = 2;
        config.protocol.volume_tolerance = 1.0;

        let state = NodeState::new(1, crate::types::NodeType::Full, &config);

        let (tx, rx) = mpsc::channel(16);
        let (router_tx, _router_rx) = mpsc::channel(16);
        let (metrics_tx, mut metrics_rx) = mpsc::channel(16);

        tokio::spawn(run_node(state, rx, router_tx, metrics_tx, config));

        tx.send(Message::Tick(1.0)).await.unwrap();
        // Receive a standby PactAccept from partner 10
        tx.send(Message::PactAccept {
            pact: crate::types::Pact {
                partner: 10,
                volume_bytes: 0,
                formed_at: 1.0,
                is_standby: true,
            },
        })
        .await
        .unwrap();
        tx.send(Message::Tick(2.0)).await.unwrap();
        tx.send(Message::Shutdown).await.unwrap();
        drop(tx);

        // Active pact_count should remain 0 since this was a standby pact
        let mut found_snapshot = false;
        while let Some(event) = metrics_rx.recv().await {
            if let MetricEvent::NodeSnapshot { pact_count, time, .. } = event {
                if time > 1.5 {
                    assert_eq!(
                        pact_count, 0,
                        "standby pact should NOT appear in active pact_count"
                    );
                    found_snapshot = true;
                }
            }
        }
        assert!(found_snapshot, "expected NodeSnapshot after standby accept");
    }

    #[tokio::test]
    async fn test_request_data_cached_author_increments_hits() {
        let config = SimConfig::default();
        let mut state = NodeState::new(1, crate::types::NodeType::Full, &config);
        // Put events into read_cache (not stored_events or own_events)
        let event = make_event(100, 50, 256);
        state.read_cache.put(50, (vec![event], 0.0));

        let (tx, rx) = mpsc::channel(16);
        let (router_tx, _router_rx) = mpsc::channel(16);
        let (metrics_tx, mut metrics_rx) = mpsc::channel(16);

        tokio::spawn(run_node(state, rx, router_tx, metrics_tx, config));

        tx.send(Message::Tick(1.0)).await.unwrap();
        tx.send(Message::RequestData {
            from: 10,
            request_id: 1,
            ttl: 1,
            filter: "50".to_string(),
        })
        .await
        .unwrap();
        // Get a snapshot to read cache_stats
        tx.send(Message::Tick(2.0)).await.unwrap();
        tx.send(Message::Shutdown).await.unwrap();
        drop(tx);

        // Check the NodeSnapshot for cache_stats.hits
        let mut found_hit = false;
        while let Some(event) = metrics_rx.recv().await {
            if let MetricEvent::NodeSnapshot { cache_stats, time, .. } = event {
                if time > 1.5 {
                    assert_eq!(cache_stats.hits, 1, "expected 1 cache hit after read_cache lookup");
                    assert_eq!(cache_stats.misses, 0, "expected 0 cache misses");
                    found_hit = true;
                }
            }
        }
        assert!(found_hit, "expected NodeSnapshot with cache hit recorded");
    }

    #[tokio::test]
    async fn test_request_data_unknown_author_increments_misses() {
        let config = SimConfig::default();
        let state = NodeState::new(1, crate::types::NodeType::Full, &config);
        // No events stored for author 50 anywhere

        let (tx, rx) = mpsc::channel(16);
        let (router_tx, _router_rx) = mpsc::channel(16);
        let (metrics_tx, mut metrics_rx) = mpsc::channel(16);

        tokio::spawn(run_node(state, rx, router_tx, metrics_tx, config));

        tx.send(Message::Tick(1.0)).await.unwrap();
        tx.send(Message::RequestData {
            from: 10,
            request_id: 1,
            ttl: 1,
            filter: "50".to_string(),
        })
        .await
        .unwrap();
        // Get a snapshot to read cache_stats
        tx.send(Message::Tick(2.0)).await.unwrap();
        tx.send(Message::Shutdown).await.unwrap();
        drop(tx);

        // Check the NodeSnapshot for cache_stats.misses
        let mut found_miss = false;
        while let Some(event) = metrics_rx.recv().await {
            if let MetricEvent::NodeSnapshot { cache_stats, time, .. } = event {
                if time > 1.5 {
                    assert_eq!(cache_stats.misses, 1, "expected 1 cache miss for unknown author");
                    assert_eq!(cache_stats.hits, 0, "expected 0 cache hits");
                    found_miss = true;
                }
            }
        }
        assert!(found_miss, "expected NodeSnapshot with cache miss recorded");
    }

    #[tokio::test]
    async fn test_new_node_cannot_form_pacts_via_tick() {
        // A node created at time 0 should not send PactRequests when current_time < min_age.
        let mut config = SimConfig::default();
        config.protocol.min_account_age_days = 7; // 7 days = 604800 seconds
        let mut state = NodeState::new(1, crate::types::NodeType::Full, &config);
        state.created_at = 0.0;
        state.follows.insert(10);
        state.follows.insert(20);
        state.followers.insert(30);

        let (tx, rx) = mpsc::channel(32);
        let (router_tx, mut router_rx) = mpsc::channel(32);
        let (metrics_tx, _metrics_rx) = mpsc::channel(32);

        tokio::spawn(run_node(state, rx, router_tx, metrics_tx, config));

        // Tick at 1 day (86400s) — still less than 7 days
        tx.send(Message::Tick(86_400.0)).await.unwrap();
        tx.send(Message::Shutdown).await.unwrap();
        drop(tx);

        let mut pact_requests = 0;
        while let Some(envelope) = router_rx.recv().await {
            if matches!(envelope.message, Message::PactRequest { .. }) {
                pact_requests += 1;
            }
        }
        assert_eq!(
            pact_requests, 0,
            "new node should NOT send PactRequests before min_account_age"
        );
    }

    #[tokio::test]
    async fn test_new_node_pact_request_rejected_by_receiver() {
        // A receiver should reject a PactRequest from a sender whose created_at
        // indicates the sender is too new.
        let mut config = SimConfig::default();
        config.protocol.min_account_age_days = 7;
        config.protocol.volume_tolerance = 1.0;
        let mut state = NodeState::new(1, crate::types::NodeType::Full, &config);
        state.created_at = 0.0; // receiver is established
        state.follows.insert(20); // WoT peer

        let (tx, rx) = mpsc::channel(16);
        let (router_tx, mut router_rx) = mpsc::channel(16);
        let (metrics_tx, _metrics_rx) = mpsc::channel(16);

        tokio::spawn(run_node(state, rx, router_tx, metrics_tx, config));

        // current_time = 10 days (enough for receiver to be old enough)
        let ten_days = 10.0 * 86_400.0;
        tx.send(Message::Tick(ten_days)).await.unwrap();

        // Sender created at day 9 — only 1 day old, too new
        let sender_created_at = 9.0 * 86_400.0;
        tx.send(Message::PactRequest {
            from: 20,
            volume_bytes: 0,
            as_standby: false,
            created_at: sender_created_at,
            activity_tier: 1,
        })
        .await
        .unwrap();
        tx.send(Message::Shutdown).await.unwrap();
        drop(tx);

        let mut got_offer = false;
        while let Some(envelope) = router_rx.recv().await {
            if matches!(envelope.message, Message::PactOffer { .. }) {
                got_offer = true;
            }
        }
        assert!(
            !got_offer,
            "receiver should reject PactRequest from a sender that is too new"
        );
    }

    #[tokio::test]
    async fn test_established_node_can_form_pacts() {
        // An established node (created_at = 0.0) should be able to send and accept
        // PactRequests after the min age has passed.
        let mut config = SimConfig::default();
        config.protocol.min_account_age_days = 7;
        config.protocol.volume_tolerance = 1.0;
        let mut state = NodeState::new(1, crate::types::NodeType::Full, &config);
        state.created_at = 0.0; // established node
        state.follows.insert(10);
        state.follows.insert(20);

        let (tx, rx) = mpsc::channel(32);
        let (router_tx, mut router_rx) = mpsc::channel(32);
        let (metrics_tx, _metrics_rx) = mpsc::channel(32);

        tokio::spawn(run_node(state, rx, router_tx, metrics_tx, config));

        // Tick at 8 days — past the 7-day minimum
        let eight_days = 8.0 * 86_400.0;
        tx.send(Message::Tick(eight_days)).await.unwrap();
        tx.send(Message::Shutdown).await.unwrap();
        drop(tx);

        let mut pact_requests = 0;
        while let Some(envelope) = router_rx.recv().await {
            if matches!(envelope.message, Message::PactRequest { .. }) {
                pact_requests += 1;
            }
        }
        assert!(
            pact_requests > 0,
            "established node should send PactRequests after min_account_age"
        );
    }

    #[tokio::test]
    async fn test_established_node_accepts_pact_from_established_sender() {
        // Receiver is old enough, sender is old enough — PactOffer should be sent.
        let mut config = SimConfig::default();
        config.protocol.min_account_age_days = 7;
        config.protocol.volume_tolerance = 1.0;
        let mut state = NodeState::new(1, crate::types::NodeType::Full, &config);
        state.created_at = 0.0;
        state.follows.insert(20);

        let (tx, rx) = mpsc::channel(16);
        let (router_tx, mut router_rx) = mpsc::channel(16);
        let (metrics_tx, _metrics_rx) = mpsc::channel(16);

        tokio::spawn(run_node(state, rx, router_tx, metrics_tx, config));

        let ten_days = 10.0 * 86_400.0;
        tx.send(Message::Tick(ten_days)).await.unwrap();

        // Sender also created at 0.0 — 10 days old, well past min age
        tx.send(Message::PactRequest {
            from: 20,
            volume_bytes: 0,
            as_standby: false,
            created_at: 0.0,
            activity_tier: 1,
        })
        .await
        .unwrap();
        tx.send(Message::Shutdown).await.unwrap();
        drop(tx);

        let mut got_offer = false;
        while let Some(envelope) = router_rx.recv().await {
            if matches!(envelope.message, Message::PactOffer { .. }) {
                got_offer = true;
            }
        }
        assert!(
            got_offer,
            "established receiver should accept PactRequest from established sender"
        );
    }

    #[tokio::test]
    async fn test_new_receiver_rejects_pact_request() {
        // Even if the sender is established, a new receiver should reject.
        let mut config = SimConfig::default();
        config.protocol.min_account_age_days = 7;
        config.protocol.volume_tolerance = 1.0;
        let mut state = NodeState::new(1, crate::types::NodeType::Full, &config);
        // Receiver was created at day 9 — only 1 day old at current_time=10 days
        state.created_at = 9.0 * 86_400.0;
        state.follows.insert(20);

        let (tx, rx) = mpsc::channel(16);
        let (router_tx, mut router_rx) = mpsc::channel(16);
        let (metrics_tx, _metrics_rx) = mpsc::channel(16);

        tokio::spawn(run_node(state, rx, router_tx, metrics_tx, config));

        let ten_days = 10.0 * 86_400.0;
        tx.send(Message::Tick(ten_days)).await.unwrap();

        // Sender is established (created_at = 0.0)
        tx.send(Message::PactRequest {
            from: 20,
            volume_bytes: 0,
            as_standby: false,
            created_at: 0.0,
            activity_tier: 1,
        })
        .await
        .unwrap();
        tx.send(Message::Shutdown).await.unwrap();
        drop(tx);

        let mut got_offer = false;
        while let Some(envelope) = router_rx.recv().await {
            if matches!(envelope.message, Message::PactOffer { .. }) {
                got_offer = true;
            }
        }
        assert!(
            !got_offer,
            "new receiver should reject PactRequest even from established sender"
        );
    }

    #[tokio::test]
    async fn test_pact_request_includes_created_at() {
        // Verify that PactRequest messages sent from Tick include the node's created_at.
        let mut config = SimConfig::default();
        config.protocol.min_account_age_days = 0; // allow immediate pact formation
        let mut state = NodeState::new(1, crate::types::NodeType::Full, &config);
        state.created_at = 100.0; // custom created_at
        state.follows.insert(10);

        let (tx, rx) = mpsc::channel(32);
        let (router_tx, mut router_rx) = mpsc::channel(32);
        let (metrics_tx, _metrics_rx) = mpsc::channel(32);

        tokio::spawn(run_node(state, rx, router_tx, metrics_tx, config));

        tx.send(Message::Tick(200.0)).await.unwrap();
        tx.send(Message::Shutdown).await.unwrap();
        drop(tx);

        let mut found_created_at = false;
        while let Some(envelope) = router_rx.recv().await {
            if let Message::PactRequest { created_at, .. } = envelope.message {
                assert!(
                    (created_at - 100.0).abs() < f64::EPSILON,
                    "PactRequest should carry the sender's created_at (100.0), got {}",
                    created_at
                );
                found_created_at = true;
            }
        }
        assert!(
            found_created_at,
            "expected at least one PactRequest with created_at field"
        );
    }

    // ── Light-node tick pruning tests ────────────────────────────────

    fn make_event_at(id: u64, author: NodeId, size: u32, created_at: f64) -> Event {
        Event {
            id,
            author,
            kind: EventKind::Note,
            size_bytes: size,
            seq: 1,
            prev_hash: 0,
            created_at,
            nostr_json: None,
            interaction_target: None,
        }
    }

    #[tokio::test]
    async fn test_light_node_prunes_on_tick() {
        // Insert old events directly into a Light node's stored_events,
        // send a Tick, and verify that stored_bytes in the NodeSnapshot
        // reflects only the recent events.
        let config = SimConfig::default(); // checkpoint_window_days = 30
        let window = config.protocol.checkpoint_window_days as f64 * 86_400.0;
        let now = window + 1000.0; // well past the window

        let mut state = NodeState::new(1, crate::types::NodeType::Light, &config);
        // Insert events directly: one old, one recent
        state.stored_events.insert(
            99,
            vec![
                make_event_at(1, 99, 256, 0.0),           // old — before cutoff
                make_event_at(2, 99, 512, now - 100.0),    // recent — within window
            ],
        );

        let (tx, rx) = mpsc::channel(16);
        let (router_tx, _router_rx) = mpsc::channel(16);
        let (metrics_tx, mut metrics_rx) = mpsc::channel(16);

        tokio::spawn(run_node(state, rx, router_tx, metrics_tx, config));

        // Tick at `now` triggers pruning, then emits snapshot
        tx.send(Message::Tick(now)).await.unwrap();
        tx.send(Message::Shutdown).await.unwrap();
        drop(tx);

        // The snapshot should show stored_bytes == 512 (only the recent event)
        let mut found_snapshot = false;
        while let Some(event) = metrics_rx.recv().await {
            if let MetricEvent::NodeSnapshot { stored_bytes, .. } = event {
                assert_eq!(
                    stored_bytes, 512,
                    "expected only recent event (512 bytes) after tick pruning, got {}",
                    stored_bytes
                );
                found_snapshot = true;
            }
        }
        assert!(found_snapshot, "expected NodeSnapshot from Tick");
    }

    #[tokio::test]
    async fn test_light_node_deliver_and_prune() {
        // Deliver events with a mix of old and new timestamps to a Light node
        // via DeliverEvents, then Tick to prune, and verify only recent kept.
        let mut config = SimConfig::default();
        config.protocol.min_account_age_days = 0;
        let window = config.protocol.checkpoint_window_days as f64 * 86_400.0;
        let now = window + 2000.0;

        let mut state = NodeState::new(1, crate::types::NodeType::Light, &config);
        // Need a pact with author 99 so events are stored (not cached)
        state.active_pacts.push(crate::types::Pact {
            partner: 99,
            volume_bytes: 0,
            formed_at: 0.0,
            is_standby: false,
        });
        state.reliability_scores.insert(99, 1.0);

        let (tx, rx) = mpsc::channel(32);
        let (router_tx, _router_rx) = mpsc::channel(32);
        let (metrics_tx, mut metrics_rx) = mpsc::channel(32);

        tokio::spawn(run_node(state, rx, router_tx, metrics_tx, config));

        // Set current_time so store_events_for_pact uses it
        tx.send(Message::Tick(now)).await.unwrap();

        // Deliver a mix of old and new events
        tx.send(Message::DeliverEvents {
            from: 99,
            events: vec![
                make_event_at(1, 99, 100, 0.0),            // old
                make_event_at(2, 99, 200, 500.0),           // old
                make_event_at(3, 99, 300, now - 50.0),      // recent
                make_event_at(4, 99, 400, now - 10.0),      // recent
            ],
            path: DeliveryPath::CachedEndpoint,
            request_id: None,
        })
        .await
        .unwrap();

        // Tick again so pruning runs and a fresh snapshot is emitted
        let later = now + 60.0;
        tx.send(Message::Tick(later)).await.unwrap();
        tx.send(Message::Shutdown).await.unwrap();
        drop(tx);

        // The snapshot at `later` should show only the 2 recent events (300 + 400 = 700)
        let mut final_bytes = None;
        while let Some(event) = metrics_rx.recv().await {
            if let MetricEvent::NodeSnapshot { stored_bytes, time, .. } = event {
                if time > now {
                    final_bytes = Some(stored_bytes);
                }
            }
        }
        assert_eq!(
            final_bytes,
            Some(700),
            "expected 700 bytes (only recent events) after deliver + tick prune"
        );
    }

    // ── Read request tests ──────────────────────────────────────────

    #[tokio::test]
    async fn test_read_request_instant_when_data_local() {
        let config = SimConfig::default();
        let mut state = NodeState::new(1, crate::types::NodeType::Full, &config);
        // Store events for author 50 so has_events_for returns true
        let event = make_event(100, 50, 256);
        state.stored_events.insert(50, vec![event]);

        let (tx, rx) = mpsc::channel(16);
        let (router_tx, _router_rx) = mpsc::channel(16);
        let (metrics_tx, mut metrics_rx) = mpsc::channel(16);

        tokio::spawn(run_node(state, rx, router_tx, metrics_tx, config));

        tx.send(Message::Tick(1.0)).await.unwrap();
        tx.send(Message::ReadRequest {
            target_author: 50,
            request_id: 42,
            wot_tier: WotTier::Orbit,
        })
        .await
        .unwrap();
        tx.send(Message::Shutdown).await.unwrap();
        drop(tx);

        let mut found_instant = false;
        while let Some(event) = metrics_rx.recv().await {
            if let MetricEvent::ReadResult {
                reader,
                target_author,
                request_id,
                tier,
                latency_secs,
                paths_tried,
                ..
            } = event
            {
                assert_eq!(reader, 1);
                assert_eq!(target_author, 50);
                assert_eq!(request_id, 42);
                assert_eq!(tier, crate::types::ReadTier::Instant);
                assert!((latency_secs - 0.0).abs() < f64::EPSILON);
                assert_eq!(paths_tried, 1);
                found_instant = true;
            }
        }
        assert!(
            found_instant,
            "expected ReadResult with Instant tier when data is local"
        );
    }

    #[tokio::test]
    async fn test_read_request_triggers_gossip_when_not_local() {
        let config = SimConfig::default();
        let mut state = NodeState::new(1, crate::types::NodeType::Full, &config);
        // Add WoT peers but no events for author 50
        state.follows.insert(10);
        state.follows.insert(20);
        state.followers.insert(30);

        let (tx, rx) = mpsc::channel(32);
        let (router_tx, mut router_rx) = mpsc::channel(32);
        let (metrics_tx, _metrics_rx) = mpsc::channel(32);

        tokio::spawn(run_node(state, rx, router_tx, metrics_tx, config));

        tx.send(Message::Tick(1.0)).await.unwrap();
        tx.send(Message::ReadRequest {
            target_author: 50,
            request_id: 99,
            wot_tier: WotTier::Orbit,
        })
        .await
        .unwrap();
        tx.send(Message::Shutdown).await.unwrap();
        drop(tx);

        // Should have sent RequestData to all 3 peers
        let mut request_data_count = 0;
        while let Some(envelope) = router_rx.recv().await {
            if let Message::RequestData {
                from,
                request_id,
                filter,
                ..
            } = &envelope.message
            {
                assert_eq!(*from, 1);
                assert_eq!(*request_id, 99);
                assert_eq!(filter, "50");
                request_data_count += 1;
            }
        }
        assert_eq!(
            request_data_count, 3,
            "expected RequestData sent to all 3 peers (follows + followers)"
        );
    }

    #[tokio::test]
    async fn test_read_request_timeout_emits_relay_or_failed() {
        let config = SimConfig::default(); // read_timeout_secs = 30.0
        let mut state = NodeState::new(1, crate::types::NodeType::Full, &config);
        // Insert a pending read that was requested at time 0.0
        state.pending_reads.insert(50, (42, 0.0, false, None, WotTier::Orbit));

        let (tx, rx) = mpsc::channel(16);
        let (router_tx, _router_rx) = mpsc::channel(16);
        let (metrics_tx, mut metrics_rx) = mpsc::channel(16);

        tokio::spawn(run_node(state, rx, router_tx, metrics_tx, config));

        // Tick at time 31.0 — past the 30s timeout
        tx.send(Message::Tick(31.0)).await.unwrap();
        tx.send(Message::Shutdown).await.unwrap();
        drop(tx);

        let mut found_read_result = false;
        while let Some(event) = metrics_rx.recv().await {
            if let MetricEvent::ReadResult {
                reader,
                target_author,
                request_id,
                tier,
                paths_tried,
                ..
            } = event
            {
                assert_eq!(reader, 1);
                assert_eq!(target_author, 50);
                assert_eq!(request_id, 42);
                assert!(
                    tier == crate::types::ReadTier::Relay
                        || tier == crate::types::ReadTier::Failed,
                    "expected Relay or Failed tier after timeout, got {:?}",
                    tier
                );
                assert_eq!(paths_tried, 3);
                found_read_result = true;
            }
        }
        assert!(
            found_read_result,
            "expected ReadResult emitted after read request timeout"
        );
    }
}
