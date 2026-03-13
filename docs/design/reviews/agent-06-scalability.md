# Scalability Review: Can Gozzip Survive Beyond Toy Networks?

**Reviewer:** Distributed Systems Engineer (Agent 06)
**Date:** 2026-03-12
**Scope:** Protocol whitepaper v1.1, plausibility analysis, architecture docs, simulation model, ADRs 005/008
**Perspective:** 100M+ user scale systems engineering

---

## Executive Summary

Gozzip's architecture is intellectually honest about its tradeoffs and better-grounded in network theory than most decentralized protocol papers. The plausibility analysis is thorough and internally consistent. However, the analysis reveals several assumptions that hold at 5K nodes but fracture at scale, a simulation gap that leaves critical behaviors untested, and a media-handling omission that makes the protocol unusable for real social networking without additional design work.

**Bottom line:** The protocol is plausible at 100K users. It faces significant structural challenges at 1M. At 100M, several subsystems require fundamental redesign. The good news is that none of the issues are architectural dead-ends -- they are engineering problems with known solution patterns.

**Severity summary:**
- Critical: 1 (media/large events)
- Significant: 3 (gossip with clustering, content discovery, simulation gap)
- Minor: 4 (storage scaling, pact renegotiation, blinded request matching, feed tier bandwidth)
- Nitpick: 1 (multi-device sync)

---

## 1. Gossip Overhead at Scale

### Claim

Per-node forwarding load converges to ~0.158 req/s regardless of network size (plausibility analysis F-31). The proof of convergence shows that the per-node rate is `DAU_PCT * gossip_per_user * gossip_reach / (86400 * online_pct)`, which cancels out N.

### Analysis

The convergence proof is mathematically correct -- and this is a genuinely elegant result. The cancellation of N in the numerator and denominator means that as the network grows, each node sees a constant gossip load. This is the best possible scaling behavior for a gossip protocol.

However, the proof depends on assumptions that become increasingly fragile at scale:

**Assumption 1: Clustering coefficient stays at 0.25.**

The analysis uses a fixed clustering coefficient C=0.25 throughout all calculations. In real social networks, clustering is not uniform. Dunbar's own research shows that inner circles have C approaching 0.7-0.8 (your close friends all know each other), while weak ties have C near 0.05. The WoT graph in Gozzip is built from follows, which are inherently trust-weighted. The pact formation rules (WoT membership required, volume matching) further bias the graph toward high-clustering neighborhoods.

At 1K users: C=0.25 is reasonable. The network is small and sparse enough that clustering is moderate.

At 100K users: Community structure emerges. The graph develops high-modularity clusters (Q > 0.3 as the whitepaper acknowledges via Girvan-Newman). Within these clusters, C may exceed 0.5. This means gossip reach at hop 2 drops from 57 nodes (C=0.25) to 23 nodes (C=0.5). At hop 3: from 354 to 58.

At 1M+ users: The graph becomes highly modular. Cross-community gossip depends on weak ties, which are precisely the connections that have low clustering. But gossip is WoT-bounded to 2 hops, which keeps it trapped within high-clustering neighborhoods. The effective gossip reach could drop to 50-100 online nodes in tightly clustered communities.

**Impact on convergence:** The convergence result still holds (N cancels), but the *constant it converges to* depends on gossip_reach, which depends on C. With C=0.5 in realistic social clusters:

```
gossip_reach ≈ 1 + 9.25 + (9.25 * 8.25 * 0.5) + (38.2 * 8.25 * 0.5)
             ≈ 1 + 9.25 + 38.2 + 157.6
             ≈ 206 (vs 422 at C=0.25)

per_node_rate = 0.50 * 30 * 206 / (86400 * 0.4625)
              = 3090 / 39960
              = 0.077 req/s
```

This is *lower* load (good), but also *lower reach* (bad for discovery). The gossip becomes more efficient but less effective. This is a double-edged tradeoff that the analysis does not address.

**Assumption 2: GOSSIP_FALLBACK stays at 2%.**

The analysis assumes 90% of fetches succeed via cached endpoints, 8% via relay, and only 2% need gossip. This is plausible for Inner Circle content (pact-stored), but the feed model shows that Horizon content (2-hop WoT + relay discoveries) is fetched on-demand. For users who follow 500+ accounts (power Twitter users), the Orbit and Horizon tiers dominate. The gossip fallback rate for these users could be 10-20%, not 2%.

At 100K users with 5% gossip fallback:
```
per_node_rate = 0.50 * 75 * 422 / (86400 * 0.4625)
              = 15825 / 39960
              = 0.396 req/s (still fine)
```

At 1M users with 10% gossip fallback and C=0.5:
```
per_node_rate = 0.50 * 150 * 206 / (86400 * 0.4625)
              = 15450 / 39960
              = 0.387 req/s (still fine)
```

Even with pessimistic assumptions, the per-node rate stays under 0.4 req/s. The convergence property is robust. The concern is not load but effectiveness -- whether gossip actually *finds* the data it needs.

**Assumption 3: The shared-friends overlap problem.**

If Alice and Bob share 12 of their 20 pact partners (high clustering), then when Bob gossips for Alice's data, 12 of the first 20 nodes he reaches are Alice's pact partners. This is *good* for discovery but means those 12 nodes all receive the same request. The dedup cache handles this, but the initial blast is concentrated on a small set of nodes. This is fine at low request rates, but a popular author with many followers in the same cluster will see their pact partners receive many redundant gossip requests.

### Verdict

| Scale | Status | Notes |
|-------|--------|-------|
| 1K | Works well | Low clustering, full reach |
| 100K | Works | Higher clustering reduces reach but gossip finds WoT-adjacent peers efficiently |
| 1M | Works with caveats | Per-node load is fine; discovery effectiveness depends on graph structure |
| 100M | Needs monitoring | Highly modular graph may require increasing TTL or relay fallback percentage |

**Severity: Significant**

The convergence result is real and valuable. The concern is that at high clustering, gossip becomes a local phenomenon that cannot bridge communities. The relay fallback catches this, but it means the protocol's "relay as optional accelerator" narrative weakens as the network grows and becomes more modular.

**Suggested fix:** Make TTL adaptive based on observed gossip success rate. If a node's gossip requests fail more than 20% of the time, increase TTL from 3 to 4 for subsequent requests. Add a clustering-aware routing heuristic: when forwarding, prefer peers with low overlap with your own pact set (they bridge to different communities).

---

## 2. Storage Scaling

### Claim

Storage obligation is modest: an active user at 675 KB/month occupies ~15.2 MB across 20 pact partners. Total on-device storage for an active user is ~103 MB (plausibility analysis F-07). This is <0.5% of a budget phone's storage.

### Analysis

The per-user storage math is sound. Let me stress-test the aggregate:

**At 1M users (assuming 50% active, 50% casual):**
```
Active users: 500K * 675 KB/month = 330 GB/month aggregate new data
Casual users: 500K * 112 KB/month = 55 GB/month aggregate new data
Total: ~385 GB/month of new events across the network
```

Each event is stored by ~20 pact partners (plus standbys), so actual storage consumed:
```
385 GB * 23 replicas = 8.9 TB/month across all nodes
Per node (1M nodes): 8.9 TB / 1M = 8.9 MB/month per node average
```

This is trivial. The math checks out.

**At 100M users:**
```
Total new data: ~38.5 TB/month
With 23x replication: ~885 TB/month across all nodes
Per node (100M nodes): 8.9 MB/month per node average
```

Still trivial per node, because the replication factor is constant (20 pacts) regardless of network size. This is a key architectural strength: storage scales linearly with users but the per-node burden stays constant because each user only stores for ~20 others.

**The real concern is not aggregate storage but full-node concentration.**

With 25% full nodes and the effective 80 pacts per full node (F-18):
```
At 1M users: 250K full nodes, each storing 80 users' complete history
80 users * 675 KB/month * 12 months = 648 MB/year of new data per full node
After 5 years: 3.24 GB
```

After 5 years at 1M users, a full node stores 3.24 GB. Manageable.

**At 100M users with the same 80 pacts/full node:**
Same per-node burden: 648 MB/year, 3.24 GB after 5 years. The constant pact count means full nodes don't get harder to run as the network grows.

**But: the analysis only counts text events.** See Issue 9 (Media and Large Events) for why this is a critical omission.

**A subtlety the analysis misses: volume matching at scale.**

The +-30% volume tolerance means a casual user (112 KB/month) cannot form pacts with a power user (2.2 MB/month). This is by design, but it creates a segmentation problem. If 80% of users are casual and 20% are active/power, casual users can only form pacts with other casual users. The pact graph becomes stratified by activity level, not just social graph. At small scales this is fine; at 100M users, it means the pact matching pool within the WoT and within the volume tolerance may be sparse for users at the extremes (very low or very high activity).

### Verdict

| Scale | Status | Notes |
|-------|--------|-------|
| 1K | No issue | Trivial storage requirements |
| 100K | No issue | Per-node burden well within consumer hardware |
| 1M | No issue for text | But media changes everything (see Issue 9) |
| 100M | No issue for text | Volume-matching sparsity may emerge at extremes |

**Severity: Minor** (for text-only; upgrades to Critical when media is considered)

**Suggested fix:** The volume tolerance should have a floor. A casual user producing 112 KB/month paired with another casual user producing 80 KB/month -- the ratio is fine, but the absolute volume is so low that the pact provides minimal value. Consider a minimum activity threshold below which users remain in bootstrap/hybrid phase rather than forming sovereign pacts with other near-inactive users.

---

## 3. Pact Renegotiation Storms

### Claim

Random jitter of 0-48 hours before broadcasting replacement requests prevents thundering herd. Standby pacts provide immediate failover during the delay (ADR 008, Decision 11).

### Analysis

The jitter mechanism is a well-understood pattern (TCP's synchronized loss recovery, Jacobson 1988). Let me model the actual failure scenario.

**Scenario: Popular user with 10K followers goes offline permanently.**

This user has 40 active + standby pacts (popular user scaling table). When they go offline:

1. **Their 40+ pact partners** each lose one active pact. Each partner needs to replace it. With 0-48h jitter, replacement requests are spread over 2 days. At 40 requests over 48 hours, that is ~0.83 requests/hour -- completely negligible network load.

2. **More importantly: cascading failure.** The popular user's 40 pact partners each stored data for the popular user AND for ~19 other users. The popular user going offline does NOT cause those other pacts to fail. The partners just stop receiving new events from the offline user. Their storage obligations for their other pacts are unaffected.

3. **The thundering herd concern is about discovery, not about pact failure.** When 40 nodes simultaneously need new pact partners, they all broadcast kind 10055 requests within the same 48-hour window. The gossip network processes ~0.83 extra 10055 requests per hour -- far below the 10 req/s rate limit.

**Real risk: correlated failure.**

The thundering herd scenario that matters is not one popular user going offline, but a correlated event that takes many users offline simultaneously:

- ISP outage in a geographic region
- OS update that restarts all devices
- Coordinated attack

If 10% of nodes in a 1M network go offline simultaneously (100K nodes), that triggers up to 100K * 20 = 2M pact renegotiations, spread over 48 hours:

```
2M / (48 * 3600) = ~11.6 requests/second network-wide
Per online node (900K * 0.4625 = 416K online): 11.6 / 416K = 0.000028 req/s
```

Negligible. The 48-hour jitter is more than sufficient.

**Edge case: all 20 pact partners in the same timezone go offline overnight.**

The 48h jitter handles this. The standby promotion handles immediate availability. Geographic diversity in pact selection (minimum 3 timezone bands, max 50% in same band) further mitigates this.

### Verdict

| Scale | Status | Notes |
|-------|--------|-------|
| 1K | No issue | Trivial renegotiation volume |
| 100K | No issue | Jitter spreads load effectively |
| 1M | No issue | Even 10% correlated failure produces negligible per-node load |
| 100M | No issue | The per-node renegotiation rate stays constant |

**Severity: Minor**

The jitter mechanism is well-designed. The standby promotion provides immediate failover. The geographic diversity requirement prevents the most dangerous correlated failure modes. This is one of the protocol's better-engineered subsystems.

**One improvement:** The 0-48h jitter is uniform random. A better approach is exponential backoff with jitter, where the first few replacements happen quickly (within minutes) and later ones spread out. This gets the most critical replacements done fast while still preventing thundering herd for the tail.

---

## 4. Blinded Request Scalability

### Claim

Daily hash rotation (`bp = H(target_pubkey || YYYY-MM-DD)`) provides reader privacy. Storage peers match incoming requests against both today's and yesterday's hashes to handle clock skew (ADR 008, Decision 9).

### Analysis

**Matching cost per request:**

When a storage peer receives a kind 10057 request with a `bp` tag, it must compute `H(stored_pubkey || today)` and `H(stored_pubkey || yesterday)` for each pubkey it stores, then compare against the request's `bp`.

A node storing data for 20 pact partners computes 40 hashes (20 pubkeys * 2 days) per incoming request.

```
SHA-256 of ~40 bytes: ~100ns on modern hardware
40 hashes: 4 microseconds per request
```

At the convergent gossip rate of 0.158 req/s:
```
0.158 * 4 microseconds = 0.63 microseconds/second
```

Trivial. Even at 10x the gossip rate: 6.3 microseconds/second.

**But: what about a relay that serves as a gossip forwarding point?**

The whitepaper says relays don't need modifications, but an "optimized relay" could optionally do blinded matching. If a relay stores events for 100K users and receives 100 blinded requests per second:

```
100K pubkeys * 2 days * 100 req/s = 20M hash comparisons/second
At 100ns each: 2 seconds of CPU per second
```

This exceeds a single core. But this is an *optional* relay optimization, not a protocol requirement. The protocol design correctly keeps matching on client devices, where the 20-pubkey matching set is tiny.

**The real scaling concern: request volume on popular relays.**

Even without blinded matching, a relay that forwards gossip requests sees every kind 10057 event published to it. In a 1M-user network:

```
Network gossip rate (F-28): 174 req/s
Distributed across ~1000 relays: ~0.174 req/s per relay (trivial)
Concentrated on 10 popular relays: ~17.4 req/s per relay (still trivial)
```

At 100M users:
```
Network gossip rate: 17,400 req/s
On 100 popular relays: 174 req/s per relay
```

174 req/s is well within any production relay's capacity (a Nostr relay routinely handles thousands of events per second).

**Privacy concern at scale:**

Daily hash rotation means an observer who knows a target pubkey can precompute `H(target_pubkey || date)` and monitor relays for matching `bp` values. This reveals *that someone requested the target's data*, though not *who* requested it. At scale, the set of targets is large enough that precomputing all possible hashes is expensive but not infeasible for a motivated adversary targeting specific users.

### Verdict

| Scale | Status | Notes |
|-------|--------|-------|
| 1K | No issue | Trivial matching cost |
| 100K | No issue | Even relay-side matching is feasible |
| 1M | No issue | Request volume within relay capacity |
| 100M | No issue technically | Privacy model is the real concern, not compute |

**Severity: Minor**

The blinded matching itself scales perfectly. The daily rotation is a reasonable privacy/performance tradeoff. The dual-day window for clock skew is a thoughtful addition. At very large scales, the privacy guarantee degrades for targeted surveillance (precomputing known-target hashes), but this is inherent to any deterministic blinding scheme. The alternative (per-request randomized blinding) would require interactive protocols that break the gossip model.

---

## 5. Feed Model Tiers

### Claim

Three feed tiers (Inner Circle/Orbit/Horizon) with sync weights 60/25/15. Inner Circle achieves ~95%+ instant delivery via pact storage. Orbit polls every 15 minutes. Horizon is on-demand.

### Analysis

**Bandwidth per user for Inner Circle:**

A user with 20 mutual follows (Inner Circle), each producing 25 events/day:
```
IC bandwidth: 20 * 25 * 750 bytes = 375 KB/day inbound
```

**Bandwidth for Orbit:**

Orbit polling every 15 minutes. Assume 50 Orbit authors producing 10 events/day each:
```
Events per poll: 50 * 10 * (15/1440) ≈ 5.2 events per poll
96 polls/day * 5.2 * 750 bytes = 375 KB/day
```

Plus gossip overhead for uncached Orbit authors: ~20% need gossip or relay fetch, adding another ~75 KB/day.

**Bandwidth for Horizon:**

On-demand, fetched when user scrolls. Assume 20 Horizon views per session, 10 sessions/day:
```
200 views * 750 bytes = 150 KB/day
```

**Total feed bandwidth: ~975 KB/day.** This is consistent with the plausibility analysis's ~2.5 MB/day for a light node (which includes pact storage, challenges, and gossip forwarding in addition to feed consumption).

**What happens with 500+ follows?**

A power user following 500 accounts:
- Inner Circle: maybe 50 mutual follows. 50 * 25 * 750 = 937 KB/day
- Orbit: maybe 150 high-interaction + socially-endorsed. Polling bandwidth: 150 * 10 * 750 / 96 * 96 = 1.1 MB/day
- Horizon: 300 on-demand authors. 300 * 5 * 750 = 1.1 MB/day (if they read all of them)

Total: ~3.1 MB/day. Still manageable on mobile.

**But: the batch sync through pact partners is the real efficiency gain.**

The feed model document describes batch sync: connect to 5-10 pact partners, fetch events for multiple followed authors per connection. Coverage example shows 56 of 150 follows covered by 10 pact partner connections, with the rest via gossip/relay.

With 500 follows, the batch sync coverage drops because social clustering means your pact partners store a smaller fraction of your follow list. Estimated coverage: 100 of 500 from batch sync (20%), leaving 400 authors for individual gossip/relay fetches.

400 individual fetches * 10 sessions/day = 4,000 per-author requests per day. At 2% gossip fallback, that is 80 gossip requests per day -- still well within the 30/day estimate from the plausibility analysis.

**The tier model holds but becomes less efficient at high follow counts.**

The key insight is that the batch sync optimization depends on social clustering overlap between your follow list and your pact partners' storage. At 500+ follows, you inevitably follow many people outside your pact partners' WoT neighborhoods. These require individual fetches, degrading the efficiency advantage of the tiered model.

### Verdict

| Scale | Status | Notes |
|-------|--------|-------|
| 1K users, 50 follows | Works excellently | High batch sync coverage |
| 100K users, 150 follows | Works well | 40-60% batch sync coverage |
| 1M users, 300 follows | Works adequately | Batch sync covers 25-40%, rest via gossip/relay |
| 100M users, 500+ follows | Works but loses efficiency | Batch sync covers <20%, relay becomes primary for distant follows |

**Severity: Minor**

The tiered model is well-designed and degrades gracefully. At high follow counts, it naturally shifts more load to relays, which is acceptable since distant follows (Horizon tier) are relay-appropriate content anyway. The model does not "collapse" -- it gracefully transitions from mesh-primary to relay-assisted as the follow graph expands beyond the WoT neighborhood.

**Suggested fix:** For users with 500+ follows, add a "relay subscription" tier that batches relay queries for all Horizon authors into a single subscription, avoiding per-author gossip overhead. This is just an optimization of the existing relay fallback.

---

## 6. Multi-Device Sync

### Claim

Checkpoint reconciliation (kind 10051) enables multi-device sync without a central coordinator. Fork-and-reconcile model with deterministic merge.

### Analysis

**Sync overhead per checkpoint:**

A checkpoint contains: per-device heads (event ID + seq per device), Merkle root, profile/follow list references. Size estimate: ~500 bytes per device in the checkpoint.

For a user with 3 devices, a checkpoint is ~2 KB including signatures. Published daily or on reconnect.

**With many devices:**

| Devices | Checkpoint size | Sync queries per reconnect |
|---------|----------------|---------------------------|
| 2 | ~1.5 KB | 1 (fetch sibling events) |
| 3 | ~2 KB | 2 |
| 5 | ~3 KB | 4 |
| 10 | ~5.5 KB | 9 |

A user with 10 devices (phone, tablet, 2 laptops, desktop, browser extension, work computer, VPS proxy, etc.) publishes a 5.5 KB checkpoint and queries for events from 9 sibling devices. This is still trivial.

**The real concern: replaceable event fork storms.**

With N devices, the probability of concurrent modifications to kind 0 or kind 3 (replaceable events) increases. The merge algorithm is deterministic and handles two-way forks cleanly, but multi-way forks (3+ devices modifying kind 3 simultaneously while offline) require iterated pairwise merging. This is N-1 merge operations in the worst case.

At 3 devices: 2 merges. At 10 devices: 9 merges. Each merge publishes a new kind 3 event. In the worst case, 9 merge events are published in rapid succession, creating relay churn.

**Practical impact:** Minimal. Multi-way forks of the follow list are extremely rare. Most users have 2-3 devices, and the merge algorithm handles the common 2-device fork case cleanly.

### Verdict

| Scale | Status | Notes |
|-------|--------|-------|
| 2-3 devices | Works cleanly | Standard case, well-designed |
| 5 devices | Works | Slightly more sync queries, still trivial |
| 10+ devices | Works with noise | Rare multi-way forks produce merge event churn |

**Severity: Nitpick**

Multi-device sync is well-designed for the realistic case (2-3 devices). The edge case of 10+ devices is unlikely for social network users and produces manageable overhead even in the worst case. The per-event hash chain provides completeness verification between checkpoints. The `seq` counter recovery procedure (ADR 008) handles device reinstallation.

---

## 7. Content Discovery

### Claim

WoT-bounded gossip handles in-WoT discovery. Relays handle cold discovery for strangers. Combined delivery probability approaches 100%.

### Analysis

**The discovery problem is the hardest unsolved problem in the protocol.**

For content within your WoT (1-2 hops), discovery is elegant: gossip reaches the right neighborhood because your follows define the neighborhood. The plausibility analysis correctly shows ~95%+ success rate for in-WoT requests.

For content outside your WoT, the protocol offers: relay fallback. This is not discovery -- it is delegation to centralized search.

**What "relay fallback for discovery" actually means:**

A user wants to find new accounts to follow. They cannot:
1. Search by keyword (no global index exists)
2. Browse trending topics (no aggregation exists)
3. Discover content from communities they are not yet connected to (gossip is WoT-bounded)

They CAN:
1. Query a relay's stored events (relay-dependent)
2. Browse a relay's curated feed (relay-dependent)
3. Follow someone recommended by an existing follow (WoT-dependent)

This means **content discovery is either relay-controlled or socially mediated**. There is no protocol-level mechanism for discovering content outside your social graph without relay assistance.

**At scale, this creates a discoverability moat.**

New content creators with no WoT connections cannot be discovered through the gossip layer. They must be found through relays. If relays curate (which the whitepaper says is their natural value proposition), then relays become gatekeepers for discoverability -- exactly the problem the protocol claims to solve.

The Lightning boost mechanism (data-flow.md) makes this explicit: users pay relays to surface their content. This is advertising on centralized platforms by another name.

**Comparison to existing platforms:**

| Platform | Discovery mechanism | Decentralized? |
|----------|-------------------|---------------|
| Twitter/X | Algorithmic timeline | No |
| Mastodon | Federated timeline, hashtags | Partially (instance-level) |
| Nostr | Relay-curated feeds, NIP-50 search | Partially (relay-level) |
| Gozzip | WoT gossip + relay fallback | Partially (WoT for known, relay for unknown) |

Gozzip's discovery model is no worse than Nostr's (both depend on relays for cold discovery) but the WoT boundary makes it harder to organically discover content from distant communities.

**The Horizon tier does not solve this.**

Horizon content is 2-hop WoT authors weighted by edge count + relay discoveries. The 2-hop boundary means Horizon is still your extended social circle, not the broader network. For genuine discovery of unknown content, the protocol has no answer beyond "ask a relay."

### Verdict

| Scale | Status | Notes |
|-------|--------|-------|
| 1K | Works | Everyone is 2-3 hops from everyone; WoT covers the network |
| 100K | Works for WoT content | Discovery beyond WoT requires relay |
| 1M | Problematic | Highly modular graph means large parts of the network are unreachable via gossip |
| 100M | Relay-dependent for discovery | The protocol effectively reimplements centralized discovery via relays |

**Severity: Significant**

This is not a bug -- it is a fundamental tradeoff of WoT-bounded protocols. You cannot have both (a) bounded gossip propagation and (b) global content discovery without some form of indexing or aggregation. The protocol correctly identifies relays as the answer, but should be more explicit that discovery is a permanently relay-dependent function, not a bootstrap-phase artifact.

**Suggested fixes:**
1. **Hashtag-based gossip.** Allow gossip requests to include topic tags. Nodes forward topic-tagged gossip to peers interested in that topic (based on their public interaction patterns), creating topic-based gossip channels that cross WoT boundaries.
2. **Relay indexing as explicit protocol function.** Define a relay indexing NIP that standardizes how relays surface content for discovery. Make this a first-class protocol feature rather than an implicit relay capability.
3. **DHT for pubkey resolution.** While a full DHT for content is unnecessary, a lightweight DHT for "who exists and what do they post about" could provide decentralized discovery without relay dependence. This is a significant engineering effort but would close the discovery gap.

---

## 8. Media and Large Events

### Claim

The whitepaper discusses events with an average size of 925 bytes (F-01). The event mix is 40% notes, 30% reactions, 15% reposts, 10% DMs, 5% long-form. Maximum event size in the analysis is 5,500 bytes (long-form articles).

### Analysis

**This is the most critical gap in the protocol design.**

A social network without images is not a social network. Modern social media content includes:

| Content type | Typical size | Frequency |
|-------------|-------------|-----------|
| Text post | 500-2000 B | Very common |
| Image (compressed JPEG) | 200 KB - 2 MB | Common |
| Short video (15s) | 2-10 MB | Common |
| Long video (5 min) | 20-100 MB | Occasional |
| Voice message | 500 KB - 5 MB | Growing |
| Image gallery (5 photos) | 1-10 MB | Common |

If we assume 30% of posts include an image (conservative -- Instagram is 100%, Twitter is ~50%), the average event size changes dramatically:

```
New weighted average with media:
E_AVG = 0.70 * 925 + 0.25 * 500,000 + 0.05 * 5,000,000
      = 647 + 125,000 + 250,000
      = 375,647 bytes ≈ 367 KB
```

That is a **400x increase** over the text-only estimate of 925 bytes.

**Impact on storage:**

An active user producing 30 events/day with media:
```
Monthly volume: 30 * 30 * 367 KB = 330 MB/month (vs 675 KB text-only)
Pact storage (20 partners): 20 * 330 MB = 6.6 GB
Total on-device: ~7 GB (vs 103 MB text-only)
```

7 GB is still feasible on modern devices (2-3% of a 256 GB phone), but it changes the economics significantly:

- Light nodes with 30-day windows store 6.6 GB per user instead of 15 MB
- Full nodes storing 80 pacts hold 26 GB per month of new data instead of 53 MB
- After 1 year, a full node stores 312 GB -- that is a meaningful fraction of a consumer SSD

**Impact on bandwidth:**

The plausibility analysis shows light nodes at ~3.3 MB/day total. With media:
```
Inbound feed: 150 follows * 10 events * 367 KB = 550 MB/day
```

550 MB/day is 16.5 GB/month -- that would consume a typical mobile data plan in 3-4 days.

**The pact model does not work with large media.**

Volume matching within 30% tolerance makes sense for text (a user producing 675 KB/month pairs with someone producing 500-877 KB/month). For media, volume variance is much higher: a photographer posting 5 high-res images/day produces 50 MB/day, while a text-only user produces 22 KB/day. Volume matching cannot find pairs across this 2000x range.

**How existing protocols handle media:**

| Protocol | Media approach |
|----------|--------------|
| Nostr | Media hosted on external CDNs, events contain URLs |
| Mastodon | Instance stores media, with remote instance caching |
| Bluesky | Content-addressed media on PDS, CDN for delivery |
| IPFS/Filecoin | Content-addressed storage, separate from metadata |

All of these separate media storage from event metadata. Gozzip's event model treats everything as "an event with content" and does not distinguish between a 500-byte reaction and a 5 MB image.

### Verdict

| Scale | Status | Notes |
|-------|--------|-------|
| 1K | Unusable for real social networking | No media support means no product-market fit |
| Any scale | Critical gap | The protocol needs a media layer |

**Severity: Critical**

This is the most important finding in this review. Without a media strategy, the protocol cannot compete with any existing social network. The pact model, gossip layer, and bandwidth calculations all assume text-only events. Media changes every number by 100-1000x.

**Suggested fix:** Separate media from event metadata. Events should contain content-addressed references (hashes) to media blobs. Media can be:
1. Stored on dedicated media hosting (CDN, S3, IPFS -- user's choice)
2. Optionally replicated by pact partners with a separate media pact (higher storage requirement, lower partner count)
3. Served via a media-specific retrieval protocol (HTTP/CDN for media, gossip for metadata)

The event itself remains small (~1 KB with media hash references). Pact partners store event metadata. Media storage is a separate, opt-in obligation. This preserves the text-event scaling properties while enabling media-rich social networking.

---

## 9. Simulation vs Reality

### Claim

5,000-node simulation validates protocol behavior over 30 simulated days. Results show 98.8% overall content availability, relay dependency decay from 16.9% to 0.2%, and 98.3% Inner Circle instant delivery.

### Analysis

**What the simulation tests well:**

- Pact formation dynamics (nodes finding volume-matched WoT partners)
- Relay dependency decay curve (validates core architectural thesis)
- Feed-tiered read strategy (Inner Circle vs Orbit vs Horizon)
- Content availability improvement over time (94.5% day 1-5 to 99.9% day 20-30)

These are the right things to test, and the results are encouraging.

**What the simulation does not test:**

1. **Scale effects.** 5K nodes with BA preferential attachment (m=50) produces a mean degree of ~100. At 1M nodes, the degree distribution changes. The simulation does not test whether gossip reach, pact formation, or retrieval behave differently at 100K or 1M nodes.

2. **Geographic and temporal correlation.** The simulation treats online/offline as IID random variables. Real networks have strong temporal correlation (most users online during daytime in their timezone) and geographic correlation (users in the same city tend to follow each other). These correlations affect gossip reach, pact availability, and relay load in ways that random uptime models cannot capture.

3. **Network dynamics.** The simulation runs for 30 days with a fixed node set. Real networks have churn: users join, leave, change activity levels, change follow lists. The simulation does not test pact reformation under realistic churn rates (typically 5-15% monthly attrition in social networks).

4. **Adversarial behavior.** The simulation model document describes attack vectors (Sybil, eclipse, free-riding, churn storms) but the validation tables show only baseline healthy network results. The attack scenarios are designed but apparently not yet validated.

5. **The Horizon tier.** The simulation results show "Horizon: 0 reads" because "the BA graph model generates few mutual follow pairs, limiting 2-hop candidate discovery." This means the entire discovery and Horizon tier of the feed model is untested. This is a significant blind spot since Horizon is where new user discovery happens.

6. **Media.** The simulation uses text-only event sizes. See Issue 9.

7. **Multi-community structure.** The BA model produces a single connected graph with power-law degree distribution. Real social networks have strong community structure (multiple dense clusters connected by sparse bridges). The BA model has low modularity, which means gossip reach in the simulation is optimistic compared to real-world high-modularity graphs.

**Confidence levels:**

| Claim | 5K sim confidence | Extrapolation to 1M | Extrapolation to 100M |
|-------|-------------------|---------------------|-----------------------|
| Relay dependency decay | High | High (mechanism is per-user, not per-network) | High |
| Pact formation succeeds | High | Medium (depends on WoT density at scale) | Low (untested) |
| 98%+ availability | High | Medium (correlated failures untested) | Low (untested) |
| Gossip discovery works | Medium | Medium (clustering effects unclear) | Low (modularity effects unclear) |
| Feed tier distribution | Medium | Low (Horizon untested) | Very low |
| Attack resilience | Not tested | Unknown | Unknown |

### Verdict

| Scale gap | Status | Notes |
|-----------|--------|-------|
| 5K to 100K | Reasonable extrapolation | Most per-node properties are N-independent |
| 5K to 1M | Low confidence | Community structure, temporal correlation untested |
| 5K to 100M | Very low confidence | Fundamentally different graph topology at this scale |

**Severity: Significant**

The simulation validates the protocol's core mechanisms at small scale. It does not provide confidence at 1M+ users because the graph model (BA) does not capture real-world community structure, temporal correlation, or churn dynamics. The untested Horizon tier and absence of adversarial testing are notable gaps.

**Suggested fixes:**
1. **Use LFR benchmark graphs** (Lancichinetti-Fortunato-Radicchi) instead of BA for community-structured networks. LFR generates graphs with tunable community structure, mixing parameter, and degree distribution -- much closer to real social networks.
2. **Add temporal correlation.** Model timezone-based online patterns (users online during their local daytime). This tests whether geographic diversity in pact selection actually prevents correlated failures.
3. **Test at 50K-100K.** Even without reaching 1M, testing at 50K with LFR community structure would significantly increase confidence.
4. **Run attack scenarios.** The simulation model document defines excellent attack vectors. Run them.
5. **Test churn.** Add node join/leave dynamics at realistic rates. Test pact reformation under 10% monthly churn.

---

## Summary of Findings

| # | Issue | Severity | Breaking Point | Key Risk |
|---|-------|----------|---------------|----------|
| 1 | Gossip with high clustering | Significant | Effectiveness drops in modular graphs at 1M+ | Gossip becomes local, relay needed for cross-community |
| 2 | Storage scaling (text) | Minor | None for text | Volume-matching sparsity at extremes |
| 3 | Pact renegotiation storms | Minor | None identified | 48h jitter is well-designed |
| 4 | Blinded request matching | Minor | None identified | Privacy degrades for targeted surveillance at scale |
| 5 | Feed tier bandwidth | Minor | 500+ follows degrade batch sync efficiency | Graceful degradation to relay-assisted |
| 6 | Multi-device sync | Nitpick | None for realistic device counts | 10+ devices create merge churn |
| 7 | Content discovery | Significant | 1M+ users with modular graph | No decentralized discovery; relay reimplements centralized search |
| 8 | Media and large events | Critical | Immediate -- no media = no social network | 100-1000x increase in all storage/bandwidth numbers |
| 9 | Simulation gap | Significant | Confidence drops sharply beyond 100K extrapolation | BA graph, no temporal correlation, no adversarial testing |

---

## What Gozzip Gets Right

This review is critical by mandate, so it is worth stating what the protocol gets right:

1. **The convergence proof is real.** Per-node gossip load being O(1) regardless of network size is a genuine and valuable property. Most gossip protocols do not achieve this.

2. **The availability math is robust.** 20 pact partners with mixed full/light nodes providing ~100% data availability through simple redundancy is convincing. The math checks out even under pessimistic assumptions.

3. **The phased adoption model is pragmatic.** Starting relay-dependent and transitioning to sovereign is the right approach. The three-phase model (Bootstrap/Hybrid/Sovereign) provides a realistic migration path.

4. **The incentive model is elegant.** Pact-aware gossip priority (storing someone's data earns you forwarding priority) creates organic incentives without tokens or economic mechanisms.

5. **The honest self-assessment is commendable.** The whitepaper's "What We Don't Know Yet" section and the plausibility analysis's bottleneck identification are refreshingly honest for a protocol paper.

6. **Self-authenticating events are portable.** Events signed by the author's keys can be verified regardless of source. This is the right foundation for a decentralized protocol.

---

## Recommendations (Priority Order)

1. **Design the media layer.** Without it, nothing else matters. Separate media blobs from event metadata. Define media pacts as a separate storage obligation.

2. **Test with realistic graph models.** Replace BA graphs with LFR benchmark graphs in the simulator. Add temporal correlation. Run at 50K+ nodes.

3. **Add adaptive gossip.** Make TTL and forwarding heuristics responsive to observed gossip success rates and local clustering.

4. **Define a discovery protocol.** Whether through hashtag-based gossip, decentralized indexing, or standardized relay discovery APIs, the protocol needs a content discovery mechanism that does not reduce to "ask a centralized server."

5. **Run adversarial simulations.** The attack vectors in the simulation model are well-defined. Execute them and publish results.
