# Gozzip Network Simulator — Design

**Date:** 2026-03-01
**Purpose:** Validate the 50 formulas from the plausibility analysis and stress-test the protocol under adversarial conditions.

---

## Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Language | Rust | Performance for large networks, type safety |
| Sim model | Hybrid | Discrete-event for messages, tick-based for background (pacts, checkpoints) |
| Architecture | Actor-based (Tokio) | Each node is an async task with real message passing — most realistic |
| Graph models | BA + WS (configurable) | BA for realistic topology, WS for controlled gossip tests |
| Output | CLI + JSON + HTML (plotly.js) | Live progress, machine-readable reports, visual charts |
| Determinism | Optional `--deterministic` flag | Seeded RNG + controlled message ordering for debugging |

---

## Architecture

```
┌─────────────────────────────────────────────────────┐
│                    Orchestrator                       │
│  (main task — spawns nodes, drives scenarios)         │
│                                                       │
│  ┌─────────┐  ┌─────────┐         ┌─────────┐       │
│  │ Node 0  │  │ Node 1  │   ...   │ Node N  │       │
│  │ (task)  │  │ (task)  │         │ (task)  │       │
│  └────┬────┘  └────┬────┘         └────┬────┘       │
│       │            │                    │            │
│       └────────────┼────────────────────┘            │
│                    │                                  │
│             Message Router                            │
│    (routes between nodes, simulates latency,          │
│     applies WoT rules, rate limits)                   │
│                                                       │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐           │
│  │ Metrics  │  │ Scenario │  │  Output   │           │
│  │Collector │  │  Driver  │  │(JSON+HTML)│           │
│  └──────────┘  └──────────┘  └──────────┘           │
└─────────────────────────────────────────────────────┘
```

- **Orchestrator** — spawns node tasks, injects scenario events, coordinates shutdown
- **Node tasks** — each node is a `tokio::spawn` task with its own `NodeState`, receiving messages via `mpsc::Receiver`
- **Message Router** — central task receiving all inter-node messages. Delivers with simulated latency (`tokio::time::sleep`). Enforces WoT forwarding rules and per-source rate limits
- **Metrics Collector** — receives measurement events from all nodes via a dedicated channel. Aggregates per-node and network-wide stats
- **Scenario Driver** — controls simulation flow: triggers publishes, takes nodes offline, injects sybils, creates partitions
- **Output** — renders JSON reports and HTML charts from collected metrics

### Deterministic Mode

When `--deterministic` is set:
- All RNG seeded from `--seed`
- Router delivers messages in deterministic order (sorted by simulated timestamp + node ID)
- `tokio::time::sleep` replaced with virtual clock advancement
- Same seed + same config = identical results

---

## Node Model

Each node task holds its full protocol state:

```rust
struct NodeState {
    // Identity
    id: NodeId,
    node_type: NodeType,  // Full(uptime=0.95) | Light(uptime=0.60)
    pubkey: PubKey,

    // WoT graph
    follows: HashSet<NodeId>,
    followers: HashSet<NodeId>,

    // Storage pacts
    active_pacts: Vec<Pact>,
    standby_pacts: Vec<Pact>,
    stored_events: HashMap<NodeId, Vec<Event>>,

    // Cache
    read_cache: LruCache<NodeId, Vec<Event>>,

    // Protocol state
    online: bool,
    reliability_scores: HashMap<NodeId, f64>,
    seq_counter: u64,
    checkpoint: Option<Checkpoint>,

    // Metrics
    bandwidth: BandwidthCounter,
    gossip_stats: GossipStats,
    challenge_stats: ChallengeStats,
}
```

Node task loop:
1. Receive message from router
2. Process according to protocol rules (gossip forward, challenge respond, pact negotiate, etc.)
3. Send response messages back through router
4. Report metrics to collector

---

## Protocol Simulation

### What we simulate

| Protocol layer | Event kinds modeled | Formulas covered |
|----------------|-------------------|-----------------|
| Storage pacts | 10053, 10055, 10056 | F-03..F-08, F-13..F-20 |
| Gossip | 10057, 10058 | F-21..F-33 |
| Challenge-response | 10054 | F-43..F-44 |
| Content delivery | 1, 6, 7, 14, 30023 | F-34..F-42, F-45..F-50 |
| Checkpoints | 10051 | F-09..F-12 |
| Cached endpoints | 10059 | Delivery path priority |

### What we don't simulate

- BLE mesh (separate system, modeled as a delivery probability)
- NIP-44 encryption (integrity verified by event signatures)
- Actual secp256k1 signatures (events are "signed" by author field)
- Lightning zaps (out of scope for network layer)
- DM content (modeled as event size = E_DM)

### Timing

- **Discrete events:** gossip propagation, challenge-response, pact negotiation, content requests
- **Tick-based (60s ticks):** pact health checks, checkpoint publishing, online/offline transitions, reliability score updates

---

## Graph Generation

### Barabási–Albert (scale-free)

Power-law degree distribution matching real social networks.

```toml
[graph]
model = "barabasi-albert"
nodes = 10000
ba_edges_per_node = 10     # new node connects to 10 existing
ba_full_node_pct = 0.25    # 25% full nodes
```

Properties:
- Hub nodes emerge (influencers with 100s-1000s of followers)
- Short average path length (~4-5 hops)
- Tests popular-account scaling naturally

### Watts–Strogatz (small-world)

High clustering + short paths for controlled gossip tests.

```toml
[graph]
model = "watts-strogatz"
nodes = 10000
ws_neighbors = 20
ws_rewire_prob = 0.1
ws_full_node_pct = 0.25
```

Properties:
- Uniform degree distribution (~20 ± 3)
- High clustering coefficient
- Good for isolating gossip propagation behavior

---

## Scenarios

### Formula Validation (`validate`)

Generate network, run normal activity for simulated 30 days, compare against all 50 formulas.

```
$ cargo run -- validate --nodes 10000 --seed 42

━━ Formula Validation (10,000 nodes, seed=42) ━━
F-01 avg_event_size:    expected=750B   actual=742B   ✔ (1.1%)
F-14 all_offline_prob:  expected=1e-8   actual=0      ✔
F-24 online_fraction:   expected=46.2%  actual=45.8%  ✔ (0.4%)
F-31 gossip_per_node:   expected=0.16/s actual=0.19/s ⚠ (18%)
...
Passed: 47/50  Warnings: 2  Failed: 1

Output: results/validate-10k-seed42.json
        results/validate-10k-seed42.html
```

Pass/warn/fail thresholds:
- ✔ Pass: within 15% of expected
- ⚠ Warning: 15-30% deviation
- ✗ Fail: >30% deviation

### Sybil Attack (`stress sybil`)

Inject N sybil nodes targeting a specific user. Sybils attempt to become storage peers via kind 10055/10056.

Measures:
- How many pact slots did sybils capture? (target: <3 per cluster diversity rule)
- Did WoT filtering reject offers from non-WoT sybils?
- Did identity age filtering reject offers from <30-day-old sybils?
- Data availability with sybil peers dropping

```
$ cargo run -- stress sybil --nodes 10000 --sybils 200 --target random
```

### Viral Event (`stress viral`)

One node publishes. N nodes request within M minutes.

Measures:
- Peak storage peer load (req/s per peer)
- Time to read-cache takeover (when cache sources > storage peer sources)
- Bandwidth spike duration and magnitude
- % of requests served from cache vs storage peers vs relay

```
$ cargo run -- stress viral --nodes 50000 --viewers 10000 --window-minutes 10
```

### Network Partition (`stress partition`)

Split network into K regions. No messages cross boundaries.

Measures:
- Data availability per partition (% of requests fulfilled)
- Pact health degradation rate
- Recovery time after partition heals (all pacts restored, data synced)

```
$ cargo run -- stress partition --nodes 10000 --partitions 2 --duration-hours 6
```

### Churn Storm (`stress churn`)

Rapidly cycle N% of nodes online/offline.

Measures:
- Pact stability (% of pacts surviving the storm)
- Standby promotion rate and latency
- Gossip delivery rate during churn
- Renegotiation storm prevention (jittered delay effectiveness)

```
$ cargo run -- stress churn --nodes 10000 --churn-pct 40 --duration-hours 2
```

---

## Per-User Metrics

The metrics collector tracks per individual node:

| Metric | Description | How measured |
|--------|-------------|-------------|
| Data integrity | % of events passing Merkle/hash-chain verification when retrieved from peers | Verify every retrieved event's hash chain against checkpoint |
| Data availability | % of time user's data is retrievable (≥1 peer responds) | Sample requests for user's data at random intervals |
| Content reach | For each published event: how many unique nodes received it, via which path | Track delivery notifications per event |
| Gossip latency | p50, p95, p99 time from publish to delivery at followers | Timestamp at publish vs timestamp at delivery |
| Pact health | Active pact count over time, standby promotions, renegotiations | Log pact state transitions |
| Bandwidth | Upload/download broken down by: gossip, pact storage, challenges, cache serving | Accumulate bytes per category per node |

Aggregates:
- Network-wide distributions (histograms)
- Worst-case outliers (bottom 5% of nodes)
- Per-node-type breakdown (full vs light)

---

## Output Format

### JSON Report

```json
{
  "config": { "nodes": 10000, "seed": 42, "graph": "barabasi-albert", ... },
  "formulas": {
    "F-01": { "expected": 750, "actual": 742, "deviation_pct": 1.1, "status": "pass" },
    ...
  },
  "per_node": {
    "data_availability": { "p50": 0.998, "p95": 0.991, "p99": 0.982, "min": 0.95 },
    "content_reach_pct": { "p50": 0.89, "p95": 0.72, "p99": 0.61 },
    "gossip_latency_ms": { "p50": 120, "p95": 450, "p99": 890 },
    "bandwidth_mb_day": { "full_node_p50": 6.4, "light_node_p50": 3.2 },
    ...
  },
  "scenarios": { ... }
}
```

### HTML Report (plotly.js)

Charts included:
- **Degree distribution** — log-log plot (should show power-law for BA)
- **Gossip propagation** — heatmap of hops × time showing message spread
- **Data availability histogram** — per-node availability distribution
- **Bandwidth by category** — stacked bar (gossip, pacts, challenges, cache)
- **Content reach CDF** — cumulative distribution of reach per published event
- **Pact health timeline** — active/standby/failed pacts over simulation time
- **Per-scenario dashboards** — specific charts per stress test

---

## Project Structure

```
simulator/
  Cargo.toml
  src/
    main.rs                  # CLI (clap): validate, stress sybil/viral/partition/churn
    config.rs                # TOML config parsing + defaults
    types.rs                 # NodeId, PubKey, Event, Pact, Checkpoint, Message, etc.
    graph/
      mod.rs                 # Graph trait + builder
      barabasi_albert.rs     # BA generator
      watts_strogatz.rs      # WS generator
    node/
      mod.rs                 # Node task spawn + message loop
      state.rs               # NodeState struct + methods
      gossip.rs              # Gossip send/receive/forward with WoT rules
      storage.rs             # Pact management, challenge-response, reliability scoring
      cache.rs               # LRU read-cache logic
    sim/
      mod.rs
      router.rs              # Central message router with latency + rate limiting
      orchestrator.rs        # Spawn nodes, coordinate scenarios, shutdown
      metrics.rs             # Metrics collector + aggregation
      clock.rs               # Virtual clock (deterministic mode) or real time
    scenarios/
      mod.rs                 # Scenario trait
      validate.rs            # Formula validation (50 formulas)
      sybil.rs               # Sybil attack scenario
      viral.rs               # Viral event scenario
      partition.rs           # Network partition scenario
      churn.rs               # Churn storm scenario
    output/
      mod.rs
      json.rs                # JSON report writer
      html.rs                # HTML + plotly.js chart generator
      cli.rs                 # Terminal progress bars + live stats
  config/
    default.toml             # Default config matching plausibility analysis constants
  templates/
    report.html              # HTML template with plotly.js CDN
```

### Dependencies

```toml
[dependencies]
tokio = { version = "1", features = ["full"] }
clap = { version = "4", features = ["derive"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"
rand = "0.8"
rand_chacha = "0.3"        # deterministic RNG
lru = "0.12"
sha2 = "0.10"              # Merkle root computation
indicatif = "0.17"         # progress bars
```

Estimated size: ~5,000–6,000 lines of Rust.

---

## Config (default.toml)

Maps 1:1 to the plausibility analysis input constants:

```toml
[protocol]
pacts_default = 20
pacts_standby = 3
pacts_popular = 40
volume_tolerance = 0.30
ttl = 3
checkpoint_window_days = 30
light_sync_depth = 50
dedup_cache_size = 10000
rate_limit_10055 = 10
rate_limit_10057 = 50
wot_forward_hops = 2
read_cache_max_mb = 100
challenge_freq_per_day = 1

[network]
full_node_pct = 0.25
light_node_pct = 0.75
full_uptime = 0.95
light_uptime = 0.60
dau_pct = 0.50
app_sessions = 10
gossip_fallback = 0.02
clustering = 0.25

[events]
note_bytes = 800
reaction_bytes = 500
repost_bytes = 600
dm_bytes = 900
longform_bytes = 5500
gossip_req_bytes = 300
challenge_bytes = 300
data_offer_bytes = 200

[events.mix]
note = 0.40
reaction = 0.30
repost = 0.15
dm = 0.10
longform = 0.05

[graph]
model = "barabasi-albert"
nodes = 10000
seed = 42
ba_edges_per_node = 10
ws_neighbors = 20
ws_rewire_prob = 0.1

[simulation]
duration_days = 30
tick_interval_secs = 60
deterministic = false
latency_ms_mean = 50
latency_ms_stddev = 20

[validation]
pass_threshold_pct = 15
warn_threshold_pct = 30
```

---

## Relationship to Plausibility Analysis

Every formula in the plausibility analysis (`plausibility-analysis.md`) maps to a measured metric in the simulator:

| Formula group | Plausibility section | Simulator measurement |
|---------------|---------------------|----------------------|
| F-01..F-08 | §1-2 Storage | Per-node storage usage |
| F-09..F-12 | §7 Merkle | Checkpoint verification success rate |
| F-13..F-20 | §2.5 Pact availability | Pact online count, full-node pact concentration |
| F-21..F-33 | §3-4 Gossip | Per-node gossip rate, discovery success, dedup effectiveness |
| F-34..F-42 | §5 Popular accounts | Peer load, read-cache takeover time, bandwidth spikes |
| F-43..F-44 | §6 Challenges | Challenge bandwidth and compute overhead |
| F-45..F-50 | §8 Bandwidth | Per-node daily bandwidth (full vs light) |
