# Gozzip Network Simulator

A discrete-event network simulator for the Gozzip protocol — an open, censorship-resistant protocol for social media and messaging where users store each other's data through bilateral storage agreements (pacts) and discover content through Web-of-Trust (WoT) filtered gossip.

## What This Simulates

The simulator models a complete Gozzip network at scale: thousands of nodes following each other, publishing content, forming storage pacts, gossiping to discover data, and reading each other's posts. Every node runs as an independent async actor with its own state, communicating through a central message router that adds realistic network latency.

The purpose is to validate that the protocol actually works — that data is retrievable, bandwidth stays reasonable, gossip reaches who it needs to, and the network resists attacks.

### The Protocol In Brief

Gozzip is built on a simple idea: instead of relying on centralized servers, your social connections store your data for you.

1. **Storage Pacts** — Each user forms bilateral agreements with ~20 peers (drawn from their social graph) who agree to store their events. Peers hold each other accountable through periodic challenges.

2. **Gossip Discovery** — When a node needs data it doesn't have locally, it broadcasts a request through its WoT-filtered social connections. Peers who have the data (because they store it for the author via a pact) respond directly.

3. **Tiered Retrieval** — Reading someone's posts tries progressively slower paths:
   - **Tier 1 (Instant)**: Data already local — you have a pact with the author, or it's cached
   - **Tier 2 (Cached Endpoint)**: Direct query to a known storage peer (~60ms)
   - **Tier 3 (Gossip)**: WoT gossip chain, 1-3 hops (~80-240ms)
   - **Tier 4 (Relay)**: Centralized fallback (~200ms)
   - **Failed**: No path could serve the data

4. **Challenges** — Pact partners periodically challenge each other to prove they still hold the stored data, maintaining accountability.

## Architecture

```
┌──────────────────────────────────────────────────────┐
│                    Orchestrator                        │
│  (drives the simulation: time, events, reads)          │
│                                                        │
│  ┌─────────┐  ┌─────────┐         ┌─────────┐        │
│  │ Node 0  │  │ Node 1  │   ...   │ Node N  │        │
│  │ (actor) │  │ (actor) │         │ (actor) │        │
│  └────┬────┘  └────┬────┘         └────┬────┘        │
│       └────────────┼──────────────────┘               │
│                    │                                   │
│             Message Router                             │
│    (delivers messages with simulated latency,          │
│     enforces network partitions)                       │
│                                                        │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐            │
│  │ Metrics  │  │  Graph   │  │  Output   │            │
│  │Collector │  │Generator │  │(JSON+HTML)│            │
│  └──────────┘  └──────────┘  └──────────┘            │
└──────────────────────────────────────────────────────┘
```

Each node is a `tokio::spawn` async task with its own `NodeState`, processing messages sequentially from an `mpsc` channel. The router adds stochastic latency based on the delivery path (gossip hops, cached endpoint lookup, relay). A dedicated metrics collector aggregates events from all nodes.

### Why A Single Message Router

All node-to-node communication passes through a single `Router` task via an `mpsc` channel. This is intentional, not a bottleneck:

1. **Deterministic ordering** — The router uses a `BinaryHeap` ordered by `deliver_at` time, ensuring messages are delivered in the correct simulated chronological order regardless of tokio task scheduling. This is essential for reproducible runs with `--deterministic`.

2. **Latency simulation** — The router samples delivery latency from Normal distributions based on the message path (gossip hops, cached endpoint, relay), so every message experiences realistic network delay.

3. **Partition enforcement** — During network partition scenarios, the router drops messages between nodes in different partition groups. A single chokepoint makes this trivial to implement correctly.

At correct message volumes (with gossip fan-out limiting), the router handles 5k+ node simulations without being a bottleneck. The volume of messages — not the router architecture — determines performance.

### Simulation Loop (per tick, default 60s)

1. Advance the virtual clock
2. Toggle nodes online/offline based on uptime probabilities
3. Generate random content events (notes, reactions, reposts, DMs, longform)
4. Generate random read requests (followers reading who they follow)
5. Send `Tick` to all nodes for periodic maintenance (pact health, checkpoints, challenge timeouts)

## What We Test

### Formula Validation (`validate`)

The core scenario. Runs normal network activity for a simulated 30 days, then compares measured metrics against expected values from the protocol's mathematical model.

```bash
./gozzip-sim validate                           # default config
./gozzip-sim --nodes 5000 --ba-edges 50 validate  # 5k nodes, 50 follows/node
```

**Formulas validated include:**

| ID | What it checks | Example |
|----|----------------|---------|
| F-01 | Average event size matches weighted mix | 925 bytes expected |
| F-03 | Storage per user scales correctly with pact count | ~14 MB/user/month |
| F-14 | Probability all pact partners offline simultaneously | < 10^-8 |
| F-18 | Full nodes are well-distributed across pact sets | ~25% |
| F-24 | Online fraction matches uptime model | ~46.25% |
| F-31 | Gossip rate per node stays within limits | ~0.16/sec |

Each formula is graded: **PASS** (within 15%), **WARN** (15-30%), **FAIL** (>30% deviation).

**Retrieval metrics** (the key output for read simulation):

| Metric | What it measures |
|--------|-----------------|
| Success rate | % of read requests that return data |
| Tier distribution | What % resolve at each tier (instant/endpoint/gossip/relay/failed) |
| Per-tier latency | p50, p95, p99 response times per delivery path |
| Paths tried | How many retrieval methods were attempted per request |

### Sybil Attack (`stress sybil`)

Injects fake nodes targeting a victim. Tests whether WoT filtering and minimum account age prevent sybils from capturing storage pact slots.

```bash
./gozzip-sim stress sybil --sybils 200 --target 42
```

**Measures:** Pact slots captured by sybils, WoT rejections, age-based rejections, victim's data availability.

### Network Partition (`stress partition`)

Splits the network into isolated groups with no cross-partition communication.

```bash
./gozzip-sim stress partition --partitions 3 --duration-hours 6
```

**Measures:** Data availability per partition, delivery rates during/after partition, recovery speed.

### Churn Storm (`stress churn`)

Increases node turnover dramatically — nodes going offline at high rates.

```bash
./gozzip-sim stress churn --churn-pct 40
```

**Measures:** Pact survival rate, standby promotions, gossip delivery during churn.

### Viral Content (`stress viral`)

Models a content spike — many users requesting the same author's data simultaneously.

```bash
./gozzip-sim stress viral --viewers 10000 --window-minutes 10
```

**Measures:** Peak peer load, cache takeover time, bandwidth spikes.

## Social Graph Models

### Barabasi-Albert (default)

Preferential attachment — new nodes connect to popular nodes more often. Creates a scale-free power-law topology matching real social networks, where a few hub accounts have thousands of followers while most have dozens.

```toml
[graph]
model = "barabasi-albert"
nodes = 5000
ba_edges_per_node = 50   # each node follows ~50 others
```

`ba_edges_per_node` controls the follow count. With 50 follows and 20 pacts, roughly 60-80% of followed authors are NOT pact partners, forcing reads through gossip/relay — which is the realistic scenario we want to test.

### Watts-Strogatz

Ring lattice with random rewiring. Produces small-world properties (high clustering + short paths) with uniform degree. Useful for controlled gossip propagation tests.

```toml
[graph]
model = "watts-strogatz"
ws_neighbors = 20
ws_rewire_prob = 0.1
```

## Node Types

| Type | % of Network | Uptime | Storage | Description |
|------|-------------|--------|---------|-------------|
| **Full** | 25% | 95% | Keeps all events forever | Always-on desktop apps, servers |
| **Light** | 75% | 60% | Keeps events within checkpoint window (30 days) | Browser extensions, web apps (Phase 1: desktop/web/extension focus) |

## Simulated Behaviors

The simulator models three categories of user behavior: orchestrator-driven external events, autonomous node actions triggered each tick, and reactive responses to messages.

### Content Publishing

Each tick, the orchestrator generates events at a configurable rate (`events_per_day`, default 25). Authors are selected based on the `activity_distribution` setting:

| Distribution | Behavior | Use case |
|-------------|----------|----------|
| `uniform` (default) | All DAU nodes publish equally | Baseline validation |
| `power_law` | Few nodes publish heavily, most rarely (Zipf's law) | Realistic social network activity |

Event types are sampled from a weighted mix: notes (40%), reactions (30%), reposts (15%), DMs (10%), longform (5%). Each type has a different byte size, producing variable storage load.

### Content Reading

Active users generate read requests (`reads_per_day`, default 50) for random authors they follow. Each read attempts a 4-tier retrieval strategy:

1. **Instant** — Data already local (own events, pact partner data, or read cache). Latency: 0ms.
2. **Cached Endpoint** — Direct query to a previously-known storage peer. Latency: ~60ms (Normal distribution).
3. **Gossip** — WoT-filtered broadcast through social connections, up to TTL hops. Latency: ~160ms per 2 hops (Normal distribution).
4. **Relay** — Centralized fallback, attempted after `relay_stagger_secs` (2s). Latency: ~200ms. Success rate: 80%.
5. **Failed** — No path served data within `read_timeout_secs` (30s).

Cached endpoint and gossip queries fire in parallel; the first response with a matching `request_id` wins.

### Online/Offline Cycling

Each tick (60s), nodes go online or offline probabilistically:
- **Full nodes** (25% of network): 95% online probability
- **Light nodes** (75% of network): 60% online probability (browser extensions/web apps)

This produces a blended online fraction of ~68.75% at any instant (formula F-24).

### Pact Formation

Nodes autonomously seek storage pacts when below their target count (`pacts_default`, default 20):

1. **Candidate selection** — Only WoT peers (follows + followers) are eligible
2. **Volume balance** — Partner's stored volume must be within ±`volume_tolerance` (30%) of the requesting node's volume
3. **Age gating** — Both nodes must be older than `min_account_age_days` (7 days)
4. **Capacity check** — Receiver must have room for active or standby pacts

Standby pacts (`pacts_standby`, default 3) provide failover. When an active partner is dropped (failed challenge), a standby is promoted.

With **variable publishing rates**, high-volume publishers accumulate more stored data from their pact partners, creating volume imbalance. This forces the network to self-organize: prolific users form pacts with other prolific users, while low-volume users cluster together.

### Storage Challenges

Active pact partners challenge each other (`challenge_freq_per_day`, default 1/day) to verify data integrity:

1. Challenger sends a random nonce
2. Responder computes a hash over all stored events for the challenger + nonce
3. Challenger verifies against expected hash
4. Failed challenges decrease reliability score; repeated failures trigger pact drop

### Gossip Forwarding

When a node receives a `RequestData` it can't serve locally:

1. **WoT filter** — Only forward if sender is a WoT peer
2. **Deduplication** — Skip if request_id already seen (LRU cache)
3. **Rate limiting** — Per-sender sliding window (`rate_limit_10057`)
4. **Fan-out limit** — Forward to random `gossip_fanout` (8) peers, not all
5. **TTL decrement** — Stop forwarding when TTL reaches 1

### Light Node Pruning

Light nodes drop stored events older than `checkpoint_window_days` (30 days) each tick, keeping storage bounded. Full nodes retain everything.

### What Is NOT Simulated

- Users unfollowing/refollowing (social graph is static after initialization)
- New users joining mid-simulation
- Content popularity differences (reads target random followed authors uniformly)
- Geographic latency variation
- Bandwidth congestion / backpressure

## Configuration

All parameters are configurable via TOML. Two profiles are included:

- `config/default.toml` — Standard 5k-node config for production validation
- `config/realistic-5k.toml` — Tuned for realistic follow/pact ratios

```bash
./gozzip-sim --config config/realistic-5k.toml validate
```

### Key Parameters

```toml
[protocol]
pacts_default = 20          # target active pacts per user
pacts_standby = 3           # standby pacts for failover
ttl = 3                     # gossip forwarding hops
checkpoint_window_days = 30 # light nodes keep this much history
challenge_freq_per_day = 1  # proof-of-storage challenges
min_account_age_days = 7    # sybil resistance

[network]
full_node_pct = 0.25        # 25% full nodes
full_uptime = 0.95          # full nodes online 95% of time
light_uptime = 0.60         # light nodes online 60% of time (web/extension)
dau_pct = 0.50              # 50% daily active users

[retrieval]
reads_per_day = 50          # read requests per active user per day
read_timeout_secs = 30.0    # give up after 30s
relay_success_rate = 0.80   # relay fallback works 80% of the time
relay_stagger_secs = 2.0    # try relay after 2s instead of waiting for full timeout

[latency]
cached_endpoint_base_ms = 60.0   # direct peer query
gossip_per_hop_base_ms = 80.0    # per gossip hop
relay_base_ms = 200.0            # centralized relay
```

## Output

### JSON Report

Machine-readable output with full metrics:

```bash
./gozzip-sim validate
# Writes: results/validate-5000-seed42.json
#         results/validate-5000-seed42.html
```

Contains: config snapshot, formula results with pass/warn/fail, per-node percentiles (bandwidth, availability, pact count, latency), pact churn summary, and retrieval tier breakdown.

### HTML Report

Interactive charts powered by Plotly.js: degree distribution (log-log), bandwidth distribution, formula validation summary.

## Real-Time Monitoring

For long-running simulations, two real-time output modes are available:

### Live Tick Output (`--live`)

Prints per-tick summary lines to stderr instead of the progress bar:

```bash
./gozzip-sim --nodes 5000 --live validate
```

Output format:
```
[   1/43200 ] t=60s | pub=3 del=12 | reads 5/0 | pacts +2/-0 (net 2) | gossip 8
[   2/43200 ] t=120s | pub=2 del=8 | reads 3/1 | pacts +1/-1 (net 2) | gossip 5
```

### JSONL Streaming (`--jsonl`)

Writes machine-readable event log for external analysis:

```bash
./gozzip-sim --nodes 5000 --jsonl results/stream.jsonl validate
```

Each line is a JSON object with a `type` field: `ReadResult`, `PactFormed`, `PactDropped`, `EventPublished`, or `TickSummary`.

## Real Nostr Events (`--nostr-events`)

When built with the `nostr-events` feature, the simulator generates protocol-compliant NIP-01 Nostr events with secp256k1 signatures:

```bash
cargo build --release --features nostr-events
./gozzip-sim --nodes 100 --nostr-events validate
```

Each simulated node gets deterministic keys (derived from `SHA-256("gozzip-sim-node-" || id)`). Published events, pact operations (kinds 10053-10058), and challenges produce valid signed Nostr events stored in the `nostr_json` field.

## CLI Flags

```
gozzip-sim [OPTIONS] <COMMAND>

Options:
  --config <PATH>      TOML config file (default: built-in defaults)
  --seed <N>           RNG seed for reproducibility
  --nodes <N>          Override node count
  --ba-edges <N>       Override BA edges per node (follow count)
  --deterministic      Fully deterministic message ordering
  --live               Print per-tick summaries to stderr (replaces progress bar)
  --jsonl <PATH>       Write JSONL event stream to file
  --nostr-events       Generate real Nostr events (requires nostr-events feature)

Commands:
  validate             Run baseline simulation with formula validation
  stress sybil         Sybil attack scenario
  stress viral         Viral content spike
  stress partition     Network partition
  stress churn         High node turnover
```

## Running at Scale

The simulator is memory-bound. Rough estimates:

| Nodes | RAM Required | Duration (30 days simulated) |
|-------|-------------|------------------------------|
| 100   | < 1 GB      | ~9 seconds                   |
| 1,000 | ~1 GB       | ~2 minutes                   |
| 5,000 | ~4-8 GB     | ~23 minutes                  |
| 10,000| ~12-16 GB   | ~2-3 hours                   |
| 20,000| ~24+ GB     | ~8-12 hours                  |

Build and run:

```bash
cargo build --release
./target/release/gozzip-sim --nodes 5000 --ba-edges 50 --seed 42 validate
```

For long runs on a VPS:

```bash
nohup ./target/release/gozzip-sim --nodes 5000 --ba-edges 50 --seed 42 validate > results/run.log 2>&1 &
```

## Project Structure

```
simulator/
  src/
    main.rs              CLI entry point (clap)
    config.rs            TOML config structures & defaults
    types.rs             Core types (Event, Message, Pact, NodeType, etc.)
    graph/
      mod.rs             Graph structure, sybil injection
      barabasi_albert.rs Scale-free topology generator
      watts_strogatz.rs  Small-world topology generator
    node/
      mod.rs             Node actor loop (message handling)
      state.rs           Per-node mutable state
      gossip.rs          WoT-filtered forwarding, rate limiting
      storage.rs         Pact management, challenges, reliability scoring
    sim/
      orchestrator.rs    Main simulation coordinator
      router.rs          Message routing with latency simulation
      metrics.rs         Metrics collection & aggregation
      clock.rs           Virtual time management
    scenarios/
      validate.rs        Formula validation (baseline)
      sybil.rs           Sybil attack
      viral.rs           Viral content
      partition.rs       Network partition
      churn.rs           Node churn
    nostr_bridge.rs      Nostr event signing & key management
    output/
      json.rs            JSON report generation
      html.rs            HTML + Plotly.js charts
      cli.rs             Progress bar & live tick output
  config/
    default.toml         Default configuration
    realistic-5k.toml    5k-node realistic profile
```
