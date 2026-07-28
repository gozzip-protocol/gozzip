# Spam Resistance Analysis

**Date:** 2026-03-12
**Status:** Draft

## Overview

This document maps how different types of spam and abuse are handled across device types, network positions, and protocol phases. The goal is to be honest about what the protocol prevents, what it makes expensive, and where residual risks remain.

## How spam resistance works in Gozzip

Gozzip does not have a moderation layer. It does not have a global reputation score. It does not have an admin who can ban accounts. Instead, spam resistance emerges from the network's structure — the same way that gossip in a village self-regulates: strangers can't walk in and shout at everyone, because nobody will repeat what a stranger says.

The protocol achieves this through six reinforcing layers:

1. **WoT-bounded gossip** — forwarding is restricted to 2-hop Web of Trust. Unknown sources are never forwarded.
2. **Per-source rate limiting** — hard caps on request rates per pubkey (10 req/s for pact requests, 50 req/s for data requests).
3. **Proof-of-storage challenges** — pact partners must prove they hold your data. Free-riders are detected and dropped.
4. **Volume-matched pacts** — both sides of a pact store roughly equal amounts (30% tolerance). No dumping.
5. **Account age requirements** — minimum 7 days before pact formation. Prevents instant Sybil pacts.
6. **Guardian pact scarcity** — newcomers get storage through one-at-a-time guardian relationships with established users.

No single layer is sufficient. Together they create a system where spam is not forbidden — it is structurally expensive to produce and structurally limited in reach.

## The epidemic threshold argument

On open networks, epidemic spreading has no threshold — any piece of content, no matter how uninteresting, can reach everyone if the network is connected (Pastor-Satorras and Vespignani's theorem: λ_c → 0 on scale-free networks).

By restricting gossip to a 2-hop WoT boundary, the protocol creates a finite epidemic threshold (λ_c ≥ 1/√150 ≈ 0.08). Content from unknown sources dies locally. It doesn't propagate. This is the foundational spam defense — everything else is reinforcement.

### What this means in practice

With a 46% effective online rate (25% Keepers at 95% uptime + 75% Witnesses at 30% uptime), gossip reach at each hop is limited:

- **Hop 1:** ~9.25 effective online peers can forward
- **Hop 2:** those peers forward to their online WoT peers
- **Hop 3 (TTL limit):** gossip terminates

Per-node forwarding load converges to ~0.158 req/s (~47 bytes/s) regardless of network size. The protocol scales without gossip load scaling.

### What this does not prevent

- **Spam within your WoT.** If someone you follow starts posting garbage, the protocol delivers it faithfully. The defense is social: unfollow them, and their content stops reaching you.
- **Spam from 2-hop contacts.** A friend-of-friend you've never heard of can reach you through the gossip layer. Their content is lower-priority (2-hop peers only forward if capacity permits), but it exists.

## Spam surface by device type

### Full node (Keeper) — desktop, home server, or VPS

**Profile:** ~95% uptime, 20 active pacts + 3 standby, serves gossip queries, participates in challenge-response.

**Spam exposure:**
- Receives gossip from active pact partners (highest priority) and 1-2 hop WoT peers
- Receives rotating-token data requests (kind 10057) at up to 50 req/s per source
- Receives pact requests (kind 10055) at up to 10 req/s per source
- Processes challenge-response for all 23 pact partners

**Defenses active:**
- WoT filtering: unknown sources never forwarded, never processed beyond rate-limit check
- Rate limiting: excess requests silently dropped (no feedback to attacker)
- Request deduplication: LRU cache of 10,000 request IDs prevents replay amplification
- Challenge-response: pact partners who don't store data are detected within 30-day window and replaced
- Volume matching: can't be forced into asymmetric storage obligation

**What a spammer can do:**
- Send up to 50 data requests/s to this node's relay (rate-limited, not forwarded beyond WoT)
- Send up to 10 pact requests/s (rate-limited, rejected without WoT relationship)
- If the spammer IS in the node's WoT (1-2 hops): gossip reaches the node, consuming bandwidth and processing

**What a spammer cannot do:**
- Get their content forwarded beyond the node's WoT boundary
- Form a pact without mutual WoT relationship and 7-day account age
- Overwhelm the node — rate limits cap per-source load regardless of attacker resources
- Avoid challenge-response detection if they form a pact but don't store data

**Residual risk:** A Keeper's always-on presence makes it a persistent target for rate-limited probing. The per-source rate limit means an attacker with many pubkeys can multiply their throughput — but each pubkey must independently be within the WoT to have its gossip forwarded, and creating WoT-embedded Sybil identities requires 7 days + genuine social relationships.

### Light node (Witness) — phone, laptop

**Profile:** ~30% uptime, fewer pact partners, syncs in bursts, rolling 30-day window only.

**Spam exposure:**
- Receives gossip only when online (burst sync windows)
- Shorter sessions mean smaller attack surface in time
- Fetches events rather than being pushed to — spam must be present on relays or WoT peers to reach this node

**Defenses active:**
- Same WoT filtering, rate limiting, and deduplication as Keepers
- Additionally: intermittent connectivity means spammers can't maintain sustained pressure
- Batch fetching on sync reduces timing correlation and limits gossip processing to discrete windows

**What a spammer can do:**
- Pollute relay storage with events that this node will fetch during sync (if the spammer is followed or in WoT)
- Time attacks to coincide with known sync windows (but sync timing is not predictable from outside)

**What a spammer cannot do:**
- Push content to an offline device
- Overwhelm during sync — rate limits apply per-source, and sync duration is device-controlled
- Persist on the device — 30-day rolling window means old spam is automatically purged

**Residual risk:** Light nodes depend more on relays for catch-up, so relay-side spam (events stored by a followed-but-compromised account) can pollute sync batches. The defense is social (unfollow) rather than protocol-level.

### BLE-only device (BitChat mesh)

**Profile:** No internet, communicates over Bluetooth Low Energy, ~100m range.

**Spam exposure:**
- Only receives from physically nearby devices
- Spam requires physical presence within BLE range

**Defenses active:**
- Noise Protocol encrypted sessions — only established sessions can send data
- Bloom filter gossip deduplication — prevents re-propagation of already-seen messages
- Mesh relay hop limit (7 devices max) — bounds propagation distance
- Physical proximity requirement — attacker must be within ~100m

**What a spammer can do:**
- Broadcast BLE advertisements (received by all nearby devices)
- If part of the mesh: send encrypted messages that nearby peers must process
- Flood the mesh with duplicate messages (detected and dropped by Bloom filter)

**What a spammer cannot do:**
- Reach devices more than 7 hops away
- Send messages without being physically nearby
- Avoid Bloom filter deduplication (resending the same message is detected)
- Decrypt messages intended for other recipients (Noise Protocol)

**Residual risk:** A BLE mesh is inherently local, so spam is a local problem. An attacker flooding a mesh in a physical area can consume bandwidth of nearby devices. The defense is that BLE bandwidth is low (~1 Mbps shared), so the damage ceiling is low, and leaving the area ends the exposure.

### Multi-device user (typical)

**Profile:** Phone (Witness) + desktop (Keeper), same root identity.

**Spam exposure:**
- The Keeper bears the persistent spam surface (always-on, processes gossip continuously)
- The Witness syncs periodically with the Keeper and relays, inheriting whatever the Keeper has already filtered
- An attacker who reaches the Keeper's WoT reaches both devices

**Defenses active:**
- Device delegation means compromising one device doesn't compromise the other's keys
- The Keeper acts as a first-pass filter — content that doesn't pass WoT filtering on the Keeper never reaches the Witness during sync
- Rate limits apply independently per device (an attacker can't exhaust both devices' limits through one connection)

**Residual risk:** The link between devices (same root pubkey in `root_identity` tags) is visible to relays. An attacker who identifies the Keeper can infer the Witness exists. But the Witness's intermittent connectivity and dynamic IP make it harder to target directly.

## Spam surface by attack type

### Gossip flooding

**Attack:** Send massive volumes of events to relays, hoping they propagate through the network.

**Defense:**
- Rate limiting at 50 req/s per source (data requests) and 10 req/s (pact requests)
- WoT-only forwarding: events from unknown sources are never forwarded
- Request deduplication: replayed request IDs are dropped (LRU cache of 10,000)

Additionally, gossip-forwarded events are limited to 64 KB. Events exceeding this limit are dropped by forwarding nodes, preventing bandwidth amplification via oversized events.

**Residual exposure:** The relay itself must process and rate-limit the flood. Gozzip doesn't control relay behavior — a relay that doesn't rate-limit is a relay problem, not a protocol problem. But relays that forward spam from unknown sources will be dropped by clients that enforce WoT filtering.

### Sybil pact formation

**Attack:** Create many fake identities to form pacts with a target, then withhold or corrupt stored data.

**Defense:**
- Pacts require mutual WoT relationship (follow or followed-by)
- 7-day account age minimum prevents instant Sybil creation
- Volume matching prevents Sybil nodes from dumping storage obligations
- Challenge-response detects Sybil nodes that don't actually store data
- 20 active pacts + 3 standby = attacker must compromise majority to affect availability

**Residual exposure:** An attacker who spends weeks building genuine WoT relationships could form pacts with a target. But they would need to compromise 11+ of the target's 20 pact slots — each requiring a separate WoT relationship built over 7+ days with volume-matched reciprocity and ongoing challenge-response compliance. This is expensive.

**What the protocol is honest about:** The 7-day age requirement is a speed bump, not a wall. A determined attacker with patience and social engineering skills can infiltrate a WoT. The defense is that the attack doesn't scale — each Sybil identity requires independent social effort.

### Newcomer spam (cold start abuse)

**Attack:** Create new accounts to spam the network before WoT relationships exist.

**Defense:**
- Guardian pact system: newcomers get storage through established users who volunteer
- One guardian pact per Guardian at a time (prevents bulk newcomer spam)
- Guardians must be Sovereign-phase (15+ pacts) — you can't guardian-spam with new accounts
- Guardian pacts expire after 90 days or when newcomer reaches Hybrid phase (5+ pacts)
- Bootstrap pacts require following a real user (creates a WoT edge — you can't bootstrap without engaging)

**Residual exposure:** A newcomer can publish to relays before having any pacts. This relay-stored content is visible to anyone querying those relays, regardless of WoT. The WoT filter only applies to gossip propagation — relay queries are unrestricted. A relay operator can filter spam, but the protocol doesn't mandate it.

**What the protocol is honest about:** The cold-start phase (0-5 pacts) has the weakest spam defenses. A brand-new account can publish to relays and be visible. The WoT gossip filter doesn't help until the account has WoT relationships. The guardian system limits the rate of newcomer integration, but doesn't prevent newcomer publishing.

### Free-riding (storage parasitism)

**Attack:** Form pacts but don't actually store the partner's data. Benefit from storage without providing it.

**Defense:**
- Hash challenges: challenger specifies event range + nonce, peer returns hash proof. Can't fake without the data.
- Serve challenges: request specific events with latency measurement. >500ms suggests proxying, triggers 3x challenge frequency.
- Reliability scoring: exponential moving average (α=0.05) over 30-day window.
  - ≥90%: healthy
  - 70-90%: degraded (increased challenge frequency)
  - 50-70%: unreliable (replacement negotiation begins)
  - <50%: failed (immediate drop, standby promoted)

**Residual exposure:** A free-rider who proxies storage (fetches on demand from elsewhere to pass challenges) can maintain a pact indefinitely — at the cost of higher latency and increased challenge frequency. The serve challenge's 500ms threshold is a heuristic, not a proof. A well-resourced proxy could stay under the threshold.

**Challenge range bias:** Challenge range selection should be biased toward older data: 50% of challenges target data >30 days old, 30% target 7-30 day range, 20% target last 7 days. This counters selective deletion of old, rarely-challenged data.

**What the protocol is honest about:** Challenge-response proves possession, not dedication. A node that stores data but is frequently offline (like a mobile device) looks similar to a lazy free-rider. The reliability scoring accounts for this with the 30-day window, but there's inherent ambiguity between "unreliable" and "cheating."

### Eclipse attack (isolation)

**Attack:** Surround a target with attacker-controlled nodes so the target can only reach the attacker.

**Defense:**
- 20 active pacts + 3 standby = 23 total connections. Attacker must control a majority.
- Pact partner selection prefers WoT cluster diversity — not all pacts from the same social cluster
- Standby pacts selected for path diversity provide immediate failover
- Menger's theorem guarantee: with 20 WoT-connected pact partners, minimum vertex cut required is substantial

**Residual exposure:** If an attacker controls a disproportionate share of a target's WoT (e.g., a small community where the attacker has influence), eclipse becomes more feasible. The protocol's diversity selection mitigates but cannot eliminate this — it depends on the target having a diverse enough WoT to draw from.

### Relay-level censorship (not spam, but related)

**Attack:** Relay operator silently drops or delays certain pubkeys' events.

**Defense:**
- Users publish to multiple relays (outbox model)
- Pact partners store events independently of relays
- A censoring relay causes degraded delivery through that relay, but not data loss (events survive on other relays and pact partners)

**Residual exposure:** During the Bootstrap phase (0-5 pacts), relay censorship can effectively silence a user — they have few or no pact partners to serve as backup. After Sovereign phase (15+ pacts), relays are optional for existence. The transition period is the vulnerable window.

## Spam surface by protocol phase

| Phase | Pact count | Primary storage | WoT gossip | Relay dependency | Spam vulnerability |
|-------|-----------|----------------|------------|-----------------|-------------------|
| Seedling | 0 | Relays only | None (no WoT peers) | Total | Highest — relay-dependent, no gossip filter, no pact partners to vouch |
| Bootstrap | 1-5 | Relays + guardian/bootstrap pacts | Minimal | High | High — small WoT means small gossip reach but also small defense perimeter |
| Hybrid | 5-15 | Relays + reciprocal pacts | Growing | Moderate | Moderate — WoT filtering active but not fully populated; some pacts still forming |
| Sovereign | 15+ | Pact partners primary | Full (2-hop boundary) | Low (discovery only) | Low — full WoT gossip filter, 20+ pact partners, relay is optional |

**The honest assessment:** The protocol's spam resistance improves as the user builds their WoT. A new user has almost no spam protection beyond relay-level filtering (which is not protocol-guaranteed). This is by design — spam resistance comes from social structure, and social structure takes time to build. The guardian pact system makes this transition survivable, but not painless.

## Summary table

| Attack type | Protocol defense | Residual exposure | Phase-dependent? |
|------------|-----------------|-------------------|-----------------|
| Gossip flooding | Rate limiting + WoT boundary + deduplication | Relay must absorb flood; WoT-embedded spam propagates | No (rate limits always active) |
| Sybil pact formation | WoT requirement + 7-day age + volume matching + challenge-response | Patient attacker with social engineering can infiltrate | Yes (more WoT = more pacts to compromise) |
| Newcomer spam | Guardian scarcity + auto-expiry + bootstrap-requires-follow | Relay publishing unrestricted; WoT filter doesn't apply pre-WoT | Yes (Seedling phase most exposed) |
| Free-riding | Hash + serve challenges + reliability scoring + standby failover | Sophisticated proxy can pass challenges; offline ≈ cheating ambiguity | No (challenges always active) |
| Eclipse attack | 20+3 pact redundancy + cluster diversity + standby promotion | Small/homogeneous WoT more vulnerable | Yes (Sovereign phase hardest to eclipse) |
| Relay censorship | Multi-relay publishing + pact partner backup | Bootstrap/Seedling phase has no backup | Yes (total relay dependency in Seedling) |
| WoT-embedded spam | None (protocol delivers faithfully) | Social defense only: unfollow | No (this is a feature, not a bug) |

## Key properties

1. **Spam resistance is proportional to social integration.** A well-connected user with 20 pact partners and a diverse WoT has strong spam defenses. A newcomer with no WoT has almost none. The protocol is honest about this gradient.

2. **The WoT boundary is the primary defense.** Everything else — rate limiting, challenges, deduplication — reinforces the fundamental property that unknown sources cannot propagate through the network. If the WoT boundary fails (e.g., attacker infiltrates WoT), the secondary defenses slow the attacker but don't stop them.

3. **Rate limiting bounds cost, not possibility.** An attacker can always send requests — but at 50 req/s per pubkey, the damage per identity is bounded. Creating more identities requires WoT infiltration for each one.

4. **Challenge-response proves possession, not intent.** A node can store your data and still be hostile (sharing it with adversaries, timing attacks). The protocol verifies storage, not loyalty.

5. **Relay-level spam is a relay problem.** The protocol doesn't mandate relay behavior. A relay that stores and serves spam is within its rights. The defense is that gossip propagation is WoT-filtered client-side — relay spam stays on the relay unless the spammer is in someone's WoT.

6. **The cold-start problem is real.** Guardian pacts make it survivable. They don't make it instant. A new user must build genuine social relationships to achieve full spam resistance. This mirrors reality — a newcomer in a village doesn't have the same social infrastructure as a 10-year resident.

7. **WoT-embedded spam is unsolvable by protocol.** If someone you follow starts spamming, the protocol delivers it. The solution is social: unfollow them. This is not a weakness — it's the correct behavior. Filtering within trust relationships is a human judgment, not a protocol decision.
