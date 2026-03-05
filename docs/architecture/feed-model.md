# Feed Model

How Gozzip clients construct a user's content feed. The feed is the user's primary interface to the network — a continuously-updated stream of content from their social graph, prioritized by social proximity and interaction patterns.

## Feed Tiers

Content arrives through three tiers, ordered by trust and availability:

| Tier | Name | Who | Sync Model | Cache TTL |
|------|------|-----|------------|-----------|
| 1 | **Inner Circle** | Mutual follows | Continuous — pact-stored | 30 days (pact window) |
| 2 | **Orbit** | High-interaction + socially-endorsed authors | Periodic polling | 14 days |
| 3 | **Horizon** | 2-hop graph + relay discoveries | On-demand | 3 days |

### Inner Circle

Mutual follows — people you follow who follow you back. Content is always available because storage pacts guarantee it. This is the foundation of the feed.

Within the Inner Circle, content is sorted chronologically with interaction-score boosting: authors you interact with more appear higher.

### Orbit

The referral layer. Contains two types of authors:

1. **High-interaction authors** — people you actively engage with (replies, reposts, reactions), even if the follow is unilateral
2. **Socially-endorsed authors** — people that 3+ of your Inner Circle contacts interact with heavily

Content is fetched via gossip or cached endpoints on a polling interval (default: 15 minutes). Cached for 14 days.

Orbit is where interaction becomes referral. When you reply to someone's posts, your followers' clients observe that interaction and may surface that author in their own Orbit.

### Horizon

Discovery. Two sources:

- **2-hop WoT** — authors connected through multiple paths from your social graph, weighted by edge count (an author followed by 8 of your mutuals ranks higher than one followed by 1)
- **Relay content** — content surfaced by relay subscriptions, weighted by relay reputation

Fetched on-demand when the user scrolls past Inner Circle and Orbit content. Cached for 3 days.

Horizon content that the user interacts with gets promoted: interaction raises the author's score, and if 3+ IC contacts also interact, the author enters Orbit.

## Interaction Score

The client tracks a rolling interaction score per author based on public events:

```
interaction_score(author) = sum(weight[event_kind] * recency_decay(age_days))
```

| Event Kind | Weight | Signal |
|-----------|--------|--------|
| Reply | 3 | Active conversation |
| Repost | 2 | Endorsement |
| Reaction | 1 | Lightweight engagement |

Recency decay: `exp(-age / 30)` — interactions half-life ~21 days.

### Referral mechanism

An author enters your Orbit when **3+ Inner Circle contacts** have interaction scores above the referral threshold with that author.

Default threshold: 5 weighted interactions in 30 days.

This is not an algorithmic recommendation. It's observable social proof: your close contacts' public interactions are visible, and the client surfaces authors that your social circle is actively engaging with.

### Referral is implicit

No new event kinds. No "Alice recommends Bob" message. Alice's followers' clients observe Alice's public interactions (reactions, replies, reposts) and compute interaction scores locally. The referral signal emerges from existing protocol events.

## Feed Sync

The client doesn't make one request per author. It syncs through pact partners in batches, covering most of the WoT in a few connections.

### Batch Sync Through Pact Partners

Each pact partner stores data for ~20 other authors (their own pact partners). Social clustering means those authors overlap heavily with your follow graph. The client exploits this:

```
Client syncs feed
  |
  +- Step 1: Compute sync plan (which partners to ask, in what order)
  |
  +- Step 2: Connect to pact partner Alice
  |   +- Send: "Give me updates for our shared WoT, excluding [authors I already have]"
  |   +- Alice computes intersection locally:
  |   |   authors_she_stores ∩ authors_you_follow - authors_you_exclude
  |   +- Alice sends batch response: events for all matching authors
  |   +- You now have Alice's data + data for N shared follows
  |
  +- Step 3: Connect to pact partner Bob
  |   +- Send exclude list (now includes everyone from step 2)
  |   +- Bob sends remaining shared follows you don't have yet
  |
  +- Step 4: Repeat for 3-5 more pact partners
  |   +- Each round, exclude list grows, responses shrink
  |   +- After ~5 partners, most Inner Circle + Orbit is covered
  |
  +- Step 5: Gossip/relay for uncovered follows
      +- Remaining authors not stored by any pact partner
      +- Use cached endpoints (kind 10059) or gossip (kind 10057)
      +- Relay fallback for cold discovery
```

### Coverage Example

You follow 150 people. You have 20 pact partners.

| Sync step | Connection | Authors covered | Cumulative |
|-----------|-----------|----------------|------------|
| Local | Own pact storage | 20 (your pact partners' data) | 20 |
| Partner 1 | Alice (stores 20 authors) | ~8 shared follows | 28 |
| Partner 2 | Bob (stores 20 authors) | ~6 new shared follows | 34 |
| Partner 3 | Carol | ~5 new | 39 |
| Partner 4 | Dave | ~4 new | 43 |
| Partner 5 | Eve | ~3 new | 46 |
| ... | 5 more partners | ~10 new total | ~56 |
| Gossip/relay | Remaining follows | ~94 uncovered | 150 |

With social clustering, pact partner overlap is higher than random. Realistic coverage after batch sync: 40-60% of follows from ~5-10 pact partner connections. The rest goes through gossip (most resolve in 1-2 hops since they're in-WoT) or relay.

### Sync Plan Precomputation

The client precomputes an optimal sync order to maximize coverage with minimum connections:

1. **Build a coverage map**: for each pact partner, compute how many of your follows they store
2. **Greedy ordering**: connect to the partner that covers the most uncovered follows first, then the next-best, etc.
3. **Stop when diminishing returns**: when the next partner would only cover 1-2 new authors, switch to per-author gossip for the remainder

The coverage map is recomputed when the WoT changes (see WoT Monitoring below).

## WoT Monitoring

The feed sync strategy depends on knowing your WoT graph. The client monitors changes and recomputes when needed.

### Change Events

| Event | What changed | Action |
|-------|-------------|--------|
| You follow/unfollow someone | Your follow list changed | Recompute IC/Orbit membership, update sync plan |
| Someone follows/unfollows you | Your follower set changed | Recompute IC (mutual follows may have changed) |
| Pact partner's follows change | Their storage set may have changed | Update coverage map for that partner |
| Pact formed/dropped | Your pact partner set changed | Recompute entire sync plan |
| Referral threshold crossed | New author enters/exits Orbit | Update Orbit membership, adjust polling |

### Monitoring Mechanism

- **Incoming events**: When you receive events from pact partners (continuous sync), check for kind 3 (follow list) updates. Any follow list change from an IC contact triggers a partial WoT recomputation.
- **Checkpoint reconciliation**: During checkpoint sync (kind 10051), detect WoT changes that happened while offline.
- **Daily resync**: Full WoT recomputation once per day regardless of detected changes. Catches anything the incremental monitoring missed. Rebuilds the sync plan from scratch.

### Daily Resync

Once per day (configurable), the client does a full reconciliation:

1. Fetch latest follow lists (kind 3) for all IC contacts
2. Recompute full WoT tiers (IC, Orbit, Horizon)
3. Recompute interaction scores and referral thresholds
4. Rebuild the sync plan (coverage map + greedy ordering)
5. Execute a full batch sync to fill any gaps

This is the safety net. The incremental monitoring handles 95% of changes in real-time; the daily resync catches edge cases and ensures consistency.

## Feed Construction

```
Client opens / background sync
  |
  +- Tier 1 (Inner Circle): batch-synced via pact partners
  |   +- Chronological, interaction-score boosted
  |   +- Covered by batch sync steps 1-4
  |
  +- Tier 2 (Orbit): batch-synced + gossip for gaps
  |   +- Partially covered by pact partner overlap
  |   +- Gossip/cached endpoints for uncovered Orbit authors
  |   +- Referral scan: check IC interaction patterns
  |   +- Sorted by interaction_score * recency
  |
  +- Tier 3 (Horizon): on-demand + relay
  |   +- 2-hop authors by edge count
  |   +- Relay subscriptions
  |   +- Sorted by trust_score * recency
  |
  +- Merge: interleave with weights
     60% Inner Circle, 25% Orbit, 15% Horizon
```

Weights auto-redistribute when a tier is empty or undersized. A new user with no Inner Circle sees 100% Horizon (relay content) until they build mutual follows.

## Tiered Caching

All fetched content is cached locally. Eviction follows tier-based TTLs:

| Tier | Default TTL | Config Key |
|------|------------|------------|
| Inner Circle | 30 days | (pact window) |
| Orbit | 14 days | `orbit_cache_ttl` |
| Horizon | 3 days | `horizon_cache_ttl` |
| Relay-only | 1 day | `relay_cache_ttl` |

- **Pact content** is not subject to cache eviction — managed by pact obligations
- **Orbit/Horizon content** evicted after TTL or when `read_cache_max_mb` (default: 100 MB) is reached, whichever comes first
- **All cached content** participates in cascading read-caches: respond to gossip requests for any cached content regardless of tier

## Content Lifecycle

```
Relay discovery (Horizon, 3-day cache)
  |
  +-- User interacts --> interaction score rises
  |
  +-- 3+ IC contacts also interact --> promoted to Orbit (14-day cache)
  |
  +-- Mutual follow formed --> promoted to Inner Circle (pact-stored)
```

Content flows from discovery to trust. Relays serve as the entry point; the mesh absorbs content that proves socially relevant. This creates a natural relay dependency decay: new users rely on relays for discovery, but as their social graph grows, the mesh handles an increasing share of their feed.

## Relation to Existing Protocol

No new event kinds are required. The feed model is **client-side logic** built on existing protocol primitives:

- **Pacts** (kind 10053) guarantee Inner Circle content availability
- **Reactions** (kind 7), **replies** (kind 1 with `e` tag), **reposts** (kind 6) provide interaction signals
- **Gossip** (kind 10057) and **cached endpoints** (kind 10059) handle Orbit fetching
- **Relay queries** handle Horizon content
- **Cascading read-caches** distribute cached content to the network

The three-phase adoption model maps naturally: Bootstrap phase users see mostly Horizon (relay), Hybrid users see Orbit growing, Sovereign users see mostly Inner Circle.
