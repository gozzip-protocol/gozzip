# WoT Mesh Protocol — Simulation Model

This is the test plan for the simulator: the node and pact state to model, the attack scenarios to run, and the metrics to collect. It is the blueprint for the rebuilt simulator; it carries no measured results.

> **Karma is a simulator-only experimental module** (`simulator/src/node/karma.rs`), disabled by default. The adopted incentive model ([ADR 009](../decisions/009-incentive-model.md)) is reach-based with no score or token; the `karma_*` state, metrics, and attack scenarios below test that variant, not the shipping design.

## Node State Variables

### Per Node
| Variable | Description |
|---|---|
| `pubkey` | Identity anchor |
| `node_type` | light / full |
| `activity_tier` | Events/day bucket (1–10, 11–50, 51–200, 200+) |
| `storage_capacity` | Available bytes |
| `storage_used` | Bytes currently committed to pacts |
| `karma_balance` | Multilateral contribution score (experimental module, disabled by default) |
| `wot_graph` | Mutual follow adjacency list with hop distances |
| `pact_list` | Active pacts with partner pubkeys |
| `online_ratio` | Uptime % over rolling 30 days |
| `join_date` | Network age |
| `device_count` | Number of devices (affects churn resilience) |

### Per Pact
| Variable | Description |
|---|---|
| `partner_pubkey` | Pact counterparty |
| `created_at` | Pact formation timestamp |
| `window_days` | Rolling storage window |
| `activity_match_delta` | Closeness of activity tiers |
| `challenge_success_rate` | Rolling challenge success % |
| `last_challenge_at` | Last data-availability challenge timestamp |
| `renegotiation_trigger_threshold` | Activity delta % that triggers renegotiation |
| `breach_count` | Cumulative challenge failures |

---

## Success Scenarios

### Baseline Healthy Network
- Mixed light/full ratio at 75/25
- Random activity distribution across tiers
- High online ratios (>80%)
- WoT graphs with natural social clustering
- Pacts form within 1–2 hops reliably
- Content retrieval latency stays low
- availability challenges pass consistently
- Renegotiation triggers resolve cleanly without data loss

### Power User as Hub
- Full node with high WoT degree
- Serves as bootstrap anchor for new users
- Handles above-average pact load without degrading
- **Verify:** does karma accumulate proportionally to contribution?

### New User Onboarding
- Zero WoT, zero pacts
- Reads relay-curated content
- Builds first 5 mutual follows
- First pact forms — measure time to first pact
- Gradually mesh-native — measure at what WoT size relay dependency drops

### Graceful Exit
- Node announces departure
- Handoff period: pact partners find replacements
- **Verify:** no data loss during transition window
- **Measure:** how long does full handoff take at different WoT sizes?

### Multi-Device User
- Same keypair on mobile + desktop
- Desktop goes offline for 2 weeks
- Mobile maintains pacts independently
- Desktop rejoins — measure resync time and data gap

### Rolling Window Expiry
- Events older than 15 days evicted cleanly
- No orphaned storage
- **Verify:** event retrieval fails gracefully for expired content, doesn't crash

### Activity Tier Upgrade
- User goes from 10 events/day to 80 events/day
- Protocol detects delta exceeds renegotiation threshold
- Finds new compatible pact partner within WoT
- Old pact terminates cleanly
- **Verify:** no storage gap during transition

---

## Attack Vectors

### Identity / Sybil Attacks

**Basic Sybil**
- Attacker creates N fake keypairs
- Attempts to mutual-follow real users to enter WoT
- Measure: how many real follows needed before Sybil enters 1-hop WoT of target?
- Defense variable: WoT score threshold for pact eligibility

**Sybil Pact Farm**
- Attacker controls 1000 low-activity nodes
- All mutual-follow each other
- Form pacts exclusively within Sybil cluster
- Never actually store data
- Pass availability challenges by caching temporarily at challenge time
- Measure: does this degrade real network or stay isolated?

**Identity Grinding**
- Attacker generates keypairs until one has high WoT proximity to target
- Probabilistic — measure expected keypairs needed
- Defense: WoT is built on organic follows, not key proximity

---

### Storage Attacks

**Lazy Storage**
- Node accepts pact, stores data initially
- Gradually evicts partner data to free space
- Only reconstructs data temporarily when challenge arrives
- Key question: how frequently must challenges occur to make this economically unviable?
- Simulate challenge frequency vs reconstruction cost tradeoff

**Storage Inflation**
- Node claims high storage capacity to attract pacts
- Actually has low capacity
- Accepts more pacts than it can honor
- Measure: how many pacts does it take before failure cascades?

**Selective Storage Breach**
- Node stores data for high-karma partners
- Silently drops data for low-karma partners
- Detect via: cross-referencing challenge results across pact network

**Replay Attack on Availability Challenges**
- Node caches a valid challenge response
- Deletes actual data
- Replays cached response to future challenges
- Defense: challenge must include timestamp + nonce, response must be fresh

**Eclipse Attack on Storage**
- Attacker surrounds target node with Sybil pact partners
- All Sybil nodes fail simultaneously
- Target loses all redundant copies
- Measure: minimum pact count N to survive M simultaneous failures
- This defines your minimum pact redundancy requirement

---

### Network Topology Attacks

**WoT Poisoning**
- Attacker builds legitimate reputation over time (months)
- Becomes high-WoT node
- Suddenly goes malicious — drops all stored data, rejects challenges
- Long-game attack — measure damage radius at different WoT centrality levels
- Defense: karma decay, pact redundancy, breach propagation speed

**Relay Capture**
- Attacker controls majority of relays
- Can't affect WoT mesh directly
- Can corrupt discovery layer — surface malicious nodes to new users
- Measure: at what relay capture % does onboarding become compromised?
- Defense: first pacts always within existing WoT, relay only for discovery

**Partition Attack**
- Network splits into two disconnected WoT clusters
- Relay bridge goes down simultaneously
- Measure: recovery time when bridge relay comes back
- Measure: what % of content is inaccessible during partition?

**Graph Fragmentation**
- High-degree WoT nodes go offline simultaneously
- Simulate cascading pact failures
- Measure: what's the critical hub removal % before network fragments?
- Directly tests your 75/25 light/full ratio assumption

---

### Karma Attacks

**Karma Laundering**
- Two colluding nodes exchange fake high-volume pacts
- Inflate each other's karma scores
- Spend inflated karma on real network resources
- Defense: witness nodes must corroborate pact activity
- Measure: collusion size needed to fool witness pool

**Karma Sinkhole**
- High-karma node accepts pacts, provides good service
- Accumulates massive karma
- Suddenly consumes all karma by requesting maximal resources
- Leaves network without contributing back
- Measure: damage radius, recovery time

**Free Rider at Scale**
- 20% of network are pure light nodes that never become full nodes
- Never contribute storage
- Only consume via relay and WoT reads
- Measure: at what free rider % does full node load become unsustainable?
- Directly tests whether 75/25 ratio is stable or drifts

---

### Timing Attacks

**Challenge Timing Manipulation**
- Malicious node monitors challenge intervals
- Reconstructs data just before expected challenge window
- Discards immediately after
- Defense: randomize challenge timing, unpredictable intervals

**Renegotiation Flooding**
- Attacker triggers mass renegotiation events simultaneously
- Floods pact negotiation layer with proposals
- Legitimate nodes can't find partners during the storm
- Measure: recovery time, % of nodes left without pacts

**Churn Storm**
- 30% of nodes go offline simultaneously (simulates mobile users sleeping)
- Measure: how many pacts break?
- Measure: how long to reform stable pact graph?
- Measure: what % of content is temporarily unavailable?

---

## Feed-Tiered Read Strategy

The simulator models content retrieval across three feed tiers, reflecting how real social network users discover and consume content. See [Feed Model](../architecture/feed-model.md) for the full design.

### Feed Tier Definitions

| Tier | Name | Definition | Trust Signal |
|------|------|-----------|-------------|
| **1** | **Inner Circle** | Mutual follows — pact-stored | Bidirectional follow |
| **2** | **Orbit** | High-interaction authors + socially-endorsed (3+ IC contacts interacting) | Interaction frequency + shared WoT edges |
| **3** | **Horizon** | 2-hop authors weighted by path count + relay discoveries | Edge multiplicity + relay curation |

Orbit includes referral: when 3+ of a reader's Inner Circle contacts have high interaction scores with an author, that author enters the reader's Orbit. Interaction scores are computed from public events (replies weight 3, reposts weight 2, reactions weight 1) with exponential recency decay.

### Read Selection

Each read selects a target author from a tier based on configurable weights (defaults: 60% Inner Circle, 25% Orbit, 15% Horizon). Per-reader **auto-redistribute** adapts these weights when a tier is empty or undersized — the budget shifts proportionally to remaining tiers.

Within Orbit, authors are selected weighted by their interaction score. Within Horizon, authors are weighted by trust score (number of shared WoT connections).

### Trust-Weighted Gossip Routing

Gossip fanout prioritizes high-trust peers. When forwarding a `RequestData` message, peers are scored by relevance to the target author (direct WoT contact of author -> score 3, 2-hop connection -> shared count, unknown -> 1). Top-scored peers are selected first.

The `should_forward()` decision also extends to 2-hop trusted peers — messages from senders with known trust scores are forwarded, not just direct WoT contacts.

### Expected Behavior

| Feed Tier | Instant% | Gossip/Relay% | Why |
|-----------|----------|---------------|-----|
| Inner Circle | High (~95%+) | Low | Pact partners store data locally |
| Orbit | Medium (~60-70%) | Medium | Some pact overlap + gossip fetch |
| Horizon | Low (~10-20%) | High | No pacts, relies on gossip/relay/cached endpoints |

These are the expected ranges the simulator should test against the core thesis: relay dependency drops as social proximity increases.

### Tiered Caching

Non-pact content is cached with tier-based TTLs: Inner Circle 30 days (pact-managed), Orbit 14 days, Horizon 3 days, relay-only 1 day. All cached content participates in cascading read-caches. See [Storage > Cascading Read-Caches](../architecture/storage.md#cascading-read-caches).

### Validation Tables to Collect

The rebuilt simulator should report the feed-tiered read outcomes as three breakdowns, each splitting reads into instant / cached / gossip / relay / failed shares:

- **Read tier by feed tier** — Inner Circle vs Orbit vs Horizon, testing whether pact partners reliably store each other's data and whether incidental pact overlap carries Orbit reads.
- **Relay dependency decay by reader pact age** — cohorts from pre-pact through 14+ day pact age, testing the core thesis that relay dependency decays as pacts form and mature.
- **Content availability by simulation period** — early days vs later days of a 30-day run, testing the network's self-healing capacity as pacts stabilize.

---

## Emergent Behaviors to Watch

**Tier Stratification** — does the network naturally stratify into stable activity tiers or do tiers blur? Blurring means activity-matching becomes unreliable.

**Pact Graph Topology** — does the pact graph mirror the WoT graph or diverge? Divergence means activity-matching is overriding social trust — could be good or bad depending on degree.

**Relay Dependency Decay Curve** — as users age in the network, does relay usage actually drop? Or do users remain relay-dependent indefinitely? This validates or invalidates the core architectural claim.

**Karma Distribution** — does karma concentrate in power users (Pareto) or distribute evenly? Heavy concentration means the system has a de facto aristocracy of nodes.

**Cold Start Equilibrium** — what's the minimum viable network size for the mesh to function without relay dependency? Below that threshold the protocol isn't viable standalone.

---

## Simulation Parameters to Sweep

| Parameter | Values |
|---|---|
| Network size | 100 / 1k / 10k / 100k nodes |
| Light/full ratio | 60/40, 75/25, 90/10 |
| Average WoT degree | 10, 50, 150, 500 |
| Online ratio distribution | High (80%+), mixed, low (40%) |
| Activity tier distribution | Uniform vs power law (realistic) |
| Pact redundancy N | 3, 5, 10 |
| Challenge frequency | Hourly, daily, weekly |
| Rolling window | 7, 15, 30 days |
| Renegotiation threshold | 20%, 50%, 100% activity delta |

---

## Success Metrics

| Metric | What it tests |
|---|---|
| Mean time to first pact | Onboarding speed |
| Content availability % at 1/5/30 days | Storage reliability |
| Relay dependency % by user age cohort | Mesh-native adoption curve |
| Pact reformation time after churn | Resilience |
| Sybil penetration depth into real WoT | Identity security |
| Karma gini coefficient | Distribution fairness |
| Network partition recovery time | Topology resilience |
| Storage overhead ratio | Efficiency (committed vs used) |

---

## Priority Simulation

The scenario that matters most: **eclipse attack combined with churn storm** — simultaneous pact partner failures during a high-churn event. This is the realistic worst case for mobile-heavy networks. If the protocol survives that with acceptable content availability, the architecture is sound.
