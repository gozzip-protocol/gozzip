# Pact State Machine — Formal Specification

**Status:** Current
**Status:** Normative for client implementations
**Addresses:** Agent 10 (Practical Deployment) §1, §2

## Overview

This document specifies the formal state machine governing pact lifecycle, and is the **normative home for the reliability bands** used across the docs. It targets client implementations. Pacts renew by default: a healthy pact stays in effect indefinitely, and dissolution requires sustained failure or an explicit exit ([ADR 015](../decisions/015-renewal-by-default-pacts.md)).

**Determinism (scoped honestly):** given the same challenge history and the same presence observations, two client implementations following this specification SHOULD reach the same state. Exact byte-for-byte agreement is not guaranteed, because presence observation (whether a partner was online when a challenge was issued) is inherently local and timing-dependent. The reference simulator implements a reduced model (see [Conformance](#conformance-and-the-reference-simulator)), not this full FSM.

## States

The pact lifecycle has **four states**: Forming → Active → Failing → Ended ([ADR 015](../decisions/015-renewal-by-default-pacts.md)). Standby and archival pacts are deferred and are not part of protocol v1.

```
┌──────────┐    ┌──────────┐    ┌──────────┐    ┌──────────┐
│ Forming  │───→│  Active  │───→│ Failing  │───→│  Ended   │
└──────────┘    └──────────┘    └──────────┘    └──────────┘
                     ▲               │
                     └───────────────┘
                    (reliability recovers ≥ 80%)
```

### State Definitions

| State | Description | Events stored? | Challenges sent? |
|-------|-------------|----------------|-----------------|
| **Forming** | Both sides negotiating; kind 10053 being exchanged. The pact exists once both have published `status: active`. | No | No |
| **Active** | Mutual storage in effect. Covers the Healthy (≥90%) and Degraded (70–90%) reliability bands. In the Degraded band the client raises challenge frequency and begins seeking a replacement, but the pact stays Active. | Yes | Yes |
| **Failing** | Reliability has fallen below 70% (Unreliable 50–70% or Failed <50%). The client is actively replacing this partner but keeps storing and serving their data until the pact ends. | Yes | Yes (3× frequency) |
| **Ended** | Pact dissolved. Reached through sustained failure, non-renewal, or explicit exit. Cleanup follows. | No (after grace) | No |

## Reliability Bands (Presence-Aware)

These four bands are normative and are referenced by the glossary, storage docs, and whitepaper ([ADR 014](../decisions/014-presence-aware-reliability.md)).

| Band | Score | Lifecycle state | Action |
|------|-------|-----------------|--------|
| **Healthy** | ≥ 90% | Active | Normal challenge frequency |
| **Degraded** | 70–90% | Active | Increase challenge frequency; begin seeking a replacement |
| **Unreliable** | 50–70% | Failing | Actively replacing; keep serving until replaced or Ended |
| **Failed** | < 50% | Failing | Actively replacing; drop after sustained failure |

**Presence-aware score.** The reliability score is `passes ÷ challenges-issued-while-the-partner-was-observed-online`, computed over a **rolling 14-day window**. Challenges issued while the partner was observed offline (per their kind 10050 uptime signal or a failed connection attempt) are **not counted against them**. This is what makes an intermittent Witness (≈30% uptime) score as a reliable partner rather than a defector: it is graded only on the challenges it had a real chance to answer.

**Dropping a partner.** A partner is dropped (pact → Ended) only when it stays in the **Failed** band (<50%) for a sustained period of **≥ 14 consecutive days**. Transient dips or a single missed window do not end a pact.

> **Conformance note:** the reference simulator approximates this score with an exponential moving average (α = 0.95, an effective window of ~3 weeks / ~20 samples) rather than an exact 14-day pass ratio, and does not implement presence-gating identically. The FSM here is the normative target for client implementations.

## Transitions

### Forming → Active

**Trigger:** Both partners have published a kind 10053 with `status: active` referencing each other.

**Guards:**
- Partner is within 2-hop WoT distance
- Partner's account age and mutual-follow history satisfy the bootstrap-relaxation table in [genesis-bootstrap.md](../design/genesis-bootstrap.md) (account age ≥ 3 days; mutual-follow age relaxed to 72 hours during the first growth year, tightening as the network matures, normal at 3,000 users). There is no volume-matching guard — capped asymmetric storage replaces it ([ADR 013](../decisions/013-capped-asymmetric-pacts.md)).
- Functional diversity: accepting this pact keeps the Keeper ratio ≥ 15% of the user's pacts
- We are below the pact ceiling (see [Pact Formation](#pact-formation))

**Action:**
1. Begin storing the partner's events (up to the storage cap)
2. Initialize reliability score to 0.90 (benefit of the doubt)
3. Schedule the first challenge in 1–24 hours (random jitter)

### Active → Failing

**Trigger:** Reliability score drops below 70% (into the Unreliable or Failed band).

**Guards:** Score computed from at least 5 counted (online-window) challenges, to prevent premature transition from a small sample.

**Action:**
1. Increase challenge frequency to 3× normal
2. Begin seeking a replacement (form a new pact per [Pact Formation](#pact-formation))
3. Continue storing and serving the partner's events

### Failing → Active

**Trigger:** Reliability score recovers to ≥ 80% (hysteresis: leave Active at 70%, return at 80%).

**Action:**
1. Reduce challenge frequency to normal (1×)
2. Cancel the replacement search once the pact target is met

### Failing → Ended

**Trigger:** Reliability stays in the **Failed** band (<50%) for **≥ 14 consecutive days**, OR the partner is unresponsive for that sustained window, OR either side explicitly exits.

**Exception:** If partition detection is active (≥3 partners in the same WoT cluster fail within 1 hour while our own relay connectivity is fine), suspend scoring and do NOT transition to Ended. See partition handling below.

**Action:**
1. Publish kind 10053 with `status: ended` (encrypted to the former partner)
2. Delete stored events for the former partner after a 7-day grace period (lets them retrieve first)
3. Remove from all tracking data structures, and form a replacement if below the pact target

### Active → Ended (explicit exit or non-renewal)

**Trigger:** Either party explicitly exits, or a pact is not renewed at its 90-day checkpoint.

**Renewal by default ([ADR 015](../decisions/015-renewal-by-default-pacts.md)):** a healthy pact auto-renews at 90 days via a lightweight handshake — no user action. Only sustained failure or an explicit exit ends a pact. **Action:** same cleanup as Failing → Ended.

## Pact Formation

Pact formation is **target-based**, not equilibrium-seeking ([ADR 016](../decisions/016-target-based-pact-formation.md)). There is no Poisson-binomial "comfort condition" and no equilibrium band.

- **Target:** 20 active pacts. **Floor:** 12. Below the target, the client seeks new partners; the 20 figure is also the modeling constant used by the simulator.
- **Replacement is simple:** when a pact ends or the count falls below the target, broadcast a kind 10055 request and accept qualifying offers (kind 10056) that satisfy the Forming → Active guards.
- **Diversity heuristic:** keep the Keeper ratio ≥ 15%, and spread partners across social clusters and 3+ timezone bands so correlated failures do not take out the whole set.

When does a client broadcast kind 10055?

1. **On app launch** — if the active pact count is below the target
2. **On pact end** — immediately (0–5 minute jitter)
3. **Periodically** — every 24 hours (±2 hour jitter) while below the target
4. **Never** — when at or above the target of 20

## Storage Volume (Capped, Asymmetric)

Volume is **not** a matching criterion, and there is no drift-triggered renegotiation ([ADR 013](../decisions/013-capped-asymmetric-pacts.md)). Each partner simply stores whatever the other produces, up to a cap of ~10 MB/month per partner. This readmits the lurker majority (people who read far more than they post) instead of metering friendship by output.

For reference, a partner's monthly output is:

```
volume = sum of byte sizes of all public events published in the last 30 days

Included kinds: 0 (metadata), 1 (note), 3 (contacts), 6 (repost), 7 (reaction), 30023 (long-form)
Excluded: kind 14 (DMs), Gozzip protocol events (kinds 10050–10065 and 10070; see the registry
          in protocol/messages.md), ephemeral events (kind 20000–29999)
```

A partner whose output exceeds the cap has the overflow stored on a best-effort basis or via a separate archival arrangement (deferred); it does not trigger renegotiation.

## Special Transitions

### Partition Handling

**Detection:** ≥3 pact partners in the same WoT cluster (>50% follow overlap) fail challenges within a 1-hour window while the client's own relay connectivity remains functional.

**During partition:**
- Reliability scoring SUSPENDED for partition-affected peers
- No transition to Failing or Ended
- Client operates with remaining reachable partners + relay fallback

**After partition heals:**
- Resume challenge-response with previously partitioned peers
- Reconcile events via checkpoint comparison
- Restore reliability scores to pre-partition levels after 3 consecutive successful challenges

### Guardian Pact Lifecycle

A Guardian pact is a one-sided sponsorship: an established member (a Guardian) stores a new user's (a Seedling's) data before the Seedling has enough reciprocal pacts of their own. It forms with no volume matching and no mutual-follow requirement, and it auto-expires (→ Ended) after 90 days or as soon as the Seedling reaches the Hybrid phase (5+ reciprocal pacts) — whichever comes first. There is no separate completion event. By the time a Guardian pact expires, the Seedling has grown roots: it now carries its own reciprocal memory in the network, and the Guardian's temporary shelter is no longer needed. Today's Seedling becomes tomorrow's Guardian.

## Timing Specifications

| Operation | Timing | Jitter |
|-----------|--------|--------|
| Challenge frequency (Active, Healthy/Degraded) | 1 per day per pact | ±6 hours uniform random |
| Challenge frequency (Failing) | 3 per day per pact | ±2 hours uniform random |
| Connection-attempt timeout | 10 seconds | — |
| Challenge response window (async) | 1h / 4–8h / 24h by pair (see [messages.md](messages.md#kind-10054--data-availability-challenge-ephemeral)) | — |
| Challenge retry (alt path) | After connection-attempt timeout | Immediate |
| Offer evaluation window | 48 hours | — |
| Partition detection window | 1 hour | — |
| Post-partition recovery | 3 successful challenges | — |
| Sustained-Failed drop threshold | 14 consecutive days | — |
| Renewal checkpoint | 90 days | — |
| Event cleanup after Ended | 7-day grace period | — |
| Pact relay rotation | 30 days | ±7 days uniform random |

**Connection-attempt timeout vs. response window.** The 10-second timeout is the deadline for *establishing a connection* to a partner before falling back to an alternate path; it is **not** the deadline for answering a challenge. A `hash` challenge works asynchronously — the response can legitimately arrive hours later, within the pair-aware response window (1h/4–8h/24h) set by the weaker device in the pair. A slow-but-online partner is never scored as failed merely because it did not answer within 10 seconds.

## Challenge Hash Computation

For interoperability, the exact challenge hash computation is:

```
Input:
  events: ordered list of events in range [seq_start, seq_end]
  nonce: 32 random bytes (hex-encoded in the challenge event)

Ordering:
  Events sorted by (device_pubkey ASC, seq ASC)
  — same ordering as Merkle tree construction

Serialization:
  For each event, compute canonical_json:
    [0, pubkey, created_at, kind, tags, content]
  This is the standard Nostr event serialization used for event ID computation.

Hash:
  SHA-256(canonical_json(e_1) || canonical_json(e_2) || ... || canonical_json(e_n) || nonce_bytes)
  where nonce_bytes is the raw 32 bytes (not hex), and || is byte concatenation.

Output:
  32-byte SHA-256 hash, hex-encoded in the response event.
```

## Conformance and the Reference Simulator

The reference simulator (in `simulator/`) implements a **reduced** model, not the full four-state FSM specified here: it tracks partners with a reliability score and drops them below threshold, closer to an active/standby scoring loop than to the Forming/Active/Failing/Ended lifecycle. It approximates the presence-aware score with an EMA (α = 0.95). The full FSM — presence-gated 14-day scoring, renewal-by-default, the four reliability bands — is the specification for **client** implementations; it is not yet exercised end-to-end by any implementation. Where the simulator and this spec overlap (TTL=3 gossip, the reliability thresholds 90/70/50, the challenge hash computation above), they agree.
