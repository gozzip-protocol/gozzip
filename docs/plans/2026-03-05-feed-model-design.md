# Feed Model Design

**Date:** 2026-03-05
**Status:** Approved

## Problem

The existing docs describe content retrieval as one-off fetches ("Bob wants Alice's events"). In practice, users need a continuous feed — the client actively syncs and surfaces content from their social graph as their primary information source. The protocol needs an explicit feed construction model that defines: what content to fetch, in what priority order, from which social tiers, and how long to cache it.

Additionally, interaction patterns should serve as natural referral signals. If Alice interacts heavily with Bob, Alice's followers should discover Bob through that activity — not just through follow graph topology.

## Feed Tiers

Three tiers replace the previous "WoT-Tiered Read Strategy" tier definitions:

| Tier | Name | Who | Feed Behavior | Cache TTL | Trust Signal |
|------|------|-----|---------------|-----------|--------------|
| **1** | **Inner Circle** | Mutual follows | Continuous sync — pact-stored, always available | 30 days (pact-covered) | Bidirectional follow |
| **2** | **Orbit** | High-interaction authors + socially-endorsed authors | Periodic polling — client fetches on interval | 14 days | Interaction frequency + shared WoT edges |
| **3** | **Horizon** | 2-hop authors weighted by path count + relay discoveries | On-demand with opportunistic caching | 3 days | Edge multiplicity + relay curation |

**Inner Circle** content is always available because storage pacts guarantee it. This is the user's trusted social core — people they know and talk to.

**Orbit** is the referral layer. It contains two types of authors:
1. Authors the user interacts with frequently (reactions, replies, reposts) even if the follow is unilateral
2. Authors that multiple Inner Circle contacts interact with heavily — socially endorsed through observed behavior

**Horizon** is discovery. It covers 2-hop graph reach (authors connected through multiple paths from the user's WoT) and relay-surfaced content. This is how users encounter new voices.

## Interaction-Based Referral

The protocol already makes interactions observable — reactions, replies, and reposts are public signed events. The client tracks a rolling **interaction score** per author:

```
interaction_score(author) = sum(weight[event_kind] * recency_decay(age_days))
```

Interaction weights:

| Event Kind | Weight | Rationale |
|-----------|--------|-----------|
| Reply (kind 1 with `e` tag) | 3 | Highest signal — active conversation |
| Repost (kind 6) | 2 | Endorsement — sharing with followers |
| Reaction (kind 7) | 1 | Lightweight engagement |

Recency decay: `recency_decay(age) = exp(-age / 30)` — interactions half-life is ~21 days.

### What the interaction score does

1. **Ranks content within the feed** — high-interaction authors surface first within their tier
2. **Creates referral signals** — when multiple Inner Circle contacts have high interaction scores with the same author, that author enters the user's Orbit tier

### Referral threshold

An author enters a user's Orbit when **3+ Inner Circle contacts** have interaction scores above a configurable minimum with that author.

Default minimum: 5 weighted interactions in 30 days (e.g., 5 reactions, or 1 reply + 1 repost, etc.).

This captures real social behavior: if several of your close contacts are actively engaging with someone, that person is worth your attention. No algorithmic recommendation — just observable social proof.

### Referral is bidirectional

When you interact heavily with an author, you're implicitly referring that author to your Inner Circle. This is not an explicit action — it's a natural consequence of your public interactions being visible to your followers. The protocol doesn't broadcast "Alice recommends Bob." Instead, Alice's followers' clients observe Alice's interactions and surface Bob when the threshold is met.

## Feed Construction Flow

```
Client opens / background sync runs
  |
  +- Tier 1 (Inner Circle): Already synced via pacts
  |   +- Sort by: created_at (chronological)
  |   +- Interaction score boosts position within tier
  |   +- Content always available -- pact partners store it
  |
  +- Tier 2 (Orbit): Poll for updates
  |   +- For each Orbit author: fetch via gossip/cached endpoints
  |   +- Cache received events (TTL: 14 days)
  |   +- Sort by: interaction_score * recency
  |   +- Referral scan: check Inner Circle interactions
  |   |   +- Author with 3+ IC contacts interacting -> add to Orbit
  |   +- Polling interval: configurable (default: every 15 minutes)
  |
  +- Tier 3 (Horizon): On-demand + opportunistic
  |   +- 2-hop authors weighted by shared edge count
  |   +- Relay-curated content (relay subscriptions)
  |   +- Cache received events (TTL: 3 days)
  |   +- Sort by: trust_score (edge count) * recency
  |   +- Fetched when user scrolls past Tier 1+2 content
  |
  +- Merged feed: interleave tiers with configurable weights
     Default: 60% Inner Circle, 25% Orbit, 15% Horizon
```

### Batch sync through pact partners

The client doesn't make one request per author. It syncs through pact partners in batches. Each pact partner stores ~20 authors' data, and social clustering means those overlap with your follow graph.

1. Connect to pact partner Alice
2. Send: "give me updates for our shared WoT, excluding [authors I already synced]"
3. Alice computes the intersection locally and sends a batch response
4. Repeat for ~5 more partners, growing the exclude list each time
5. After ~5-10 connections, most IC + Orbit is covered
6. Gossip/relay for the remaining uncovered follows

**Coverage**: 20 pact partners × ~20 stored authors = ~400 slots. With overlap and clustering, ~40-60% of follows are reachable through batch sync. The rest goes through gossip (in-WoT, resolves in 1-2 hops) or relay.

**Sync plan precomputation**: The client builds a coverage map (which partner stores which of your follows) and uses greedy ordering (connect to the partner covering the most uncovered authors first). Recomputed when the WoT changes.

### WoT monitoring

The sync strategy depends on the WoT graph. The client monitors changes:

- **Follow/unfollow events**: Recompute IC/Orbit membership and sync plan
- **Pact partner follow changes**: Update coverage map (their stored authors may have changed)
- **Pact formed/dropped**: Recompute entire sync plan
- **Daily resync**: Full WoT recomputation once per day as a safety net — recompute all tiers, rebuild sync plan, execute full batch sync to fill gaps

### Feed merge behavior

The merged feed interleaves content from all three tiers. Within each tier, content is sorted by the tier's ranking function. Across tiers, the weight determines how many items from each tier appear per screen of content.

A user with a small Inner Circle and large Orbit will see the weights auto-redistribute (same mechanism as the existing auto-redistribute in the simulation model). If a tier is empty, its budget shifts proportionally to remaining tiers.

## Tiered Caching

All fetched content is cached locally, regardless of source. Cache eviction follows tier-based TTLs:

| Tier | Default TTL | Config Key | Eviction Strategy |
|------|------------|------------|-------------------|
| Inner Circle | 30 days | (pact window) | Pact-managed, not cache |
| Orbit | 14 days | `orbit_cache_ttl` | LRU within TTL boundary |
| Horizon | 3 days | `horizon_cache_ttl` | LRU within TTL boundary |
| Relay content | 1 day | `relay_cache_ttl` | LRU within TTL boundary |

### Cache behavior

- **Pact-covered content** (Inner Circle) is not subject to cache eviction — it's stored as part of the pact obligation
- **Orbit and Horizon content** is cached on fetch and evicted after TTL expires or when cache storage limits are reached (whichever comes first)
- **Relay content** has the shortest TTL — relay-discovered content that isn't promoted to Orbit decays quickly
- **All cached content** participates in the cascading read-cache mechanism: if someone gossips for content you have cached, you respond regardless of tier

### Storage budget

Cache storage is bounded by the existing `read_cache_max_mb` setting (default: 100 MB). Within that budget, tiers are not hard-partitioned — LRU eviction naturally prioritizes frequently-accessed content. The TTL ensures stale content doesn't occupy space indefinitely.

## Relay Role in the Feed

Relays serve the Horizon tier. Their content appears alongside 2-hop graph discoveries but with a distinct trust signal:

- **2-hop WoT content**: trust weighted by edge count (how many of your contacts follow this author)
- **Relay content**: trust weighted by the relay's reputation (user-configurable relay trust level)

Relay content that the user interacts with gets promoted: if you react to a relay-discovered post, that author's interaction score increases. If the threshold is met (3+ IC contacts also interact), the author enters Orbit and is fetched via mesh instead of relay.

This creates a natural flow: relay -> horizon -> orbit. Relays serve as discovery engines; the mesh absorbs content that proves socially relevant.

## Simulation Model Impact

The existing WoT-Tiered Read Strategy in the simulation model should be updated to reflect these tiers:

- **Direct WoT tier** -> **Inner Circle** (same mechanics, pact-stored)
- **1-hop tier** -> split between **Inner Circle** (mutual follows) and **Orbit** (non-mutual + referrals)
- **2-hop tier** -> **Horizon**
- **New**: interaction scoring as a read-selection weight within each tier
- **New**: referral mechanism promoting 2-hop authors to Orbit based on IC interaction patterns

Default read weights update: 60% Inner Circle (was 50% direct WoT), 25% Orbit (was 30% 1-hop), 15% Horizon (was 20% 2-hop).
