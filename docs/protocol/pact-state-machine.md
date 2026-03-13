# Pact State Machine — Formal Specification

**Date:** 2026-03-14
**Status:** Draft
**Addresses:** Agent 10 (Practical Deployment) §1, §2

## Overview

This document specifies the formal state machine governing pact lifecycle. Two independent implementations given this specification and the same inputs MUST produce the same state transitions.

## States

```
┌──────────┐    ┌──────────┐    ┌──────────┐    ┌──────────┐
│ Pending  │───→│ Offered  │───→│ Active   │───→│ Standby  │
└──────────┘    └──────────┘    └──────────┘    └──────────┘
                     │               │ │              │
                     │               │ │              │
                     ▼               ▼ │              │
                ┌──────────┐   ┌──────────┐          │
                │ Rejected │   │ Degraded │          │
                └──────────┘   └──────────┘          │
                                    │                │
                                    ▼                │
                               ┌──────────┐         │
                               │  Failed  │←────────┘
                               └──────────┘
                                    │
                                    ▼
                               ┌──────────┐
                               │ Dropped  │
                               └──────────┘
```

### State Definitions

| State | Description | Events stored? | Challenges sent? |
|-------|-------------|----------------|-----------------|
| **Pending** | Local intent to form pact; kind 10055 broadcast | No | No |
| **Offered** | Kind 10056 offer received from candidate | No | No |
| **Active** | Kind 10053 exchanged; mutual storage in effect | Yes | Yes |
| **Standby** | Receives events but not challenged; promotes on failure | Yes | No |
| **Degraded** | Active pact with reliability score 50-70%; replacement sought | Yes | Yes (3x frequency) |
| **Failed** | Reliability score <50% or unresponsive for >72 hours | No (stop storing) | No |
| **Dropped** | Pact terminated; cleanup complete | No | No |
| **Rejected** | Offer declined (volume mismatch, WoT distance, capacity) | No | No |

## Transitions

### Pending → Offered

**Trigger:** Receive kind 10056 offer from a WoT peer in response to our kind 10055 request.

**Guards:**
- Offerer is within 2-hop WoT distance
- Offerer's volume is within ±30% tolerance (or ±100% during bootstrap if network < 500 users)
- Offerer's account age ≥ 7 days (3 days during bootstrap)
- Mutual follow history ≥ 30 days (7 days during bootstrap; exempt for bootstrap pacts)
- We have capacity (active + standby < ceiling of 43)

**Action:** Store offer locally. Evaluate against other pending offers.

### Offered → Active

**Trigger:** Client selects this offer (best volume match, best uptime complementarity, or first qualifying offer).

**Guards:**
- All guards from Pending → Offered still hold
- Functional diversity: accepting this pact does not drop Keeper ratio below 15%

**Action:**
1. Publish kind 10053 (accept) encrypted to the offerer
2. Begin storing the partner's events
3. Initialize reliability score to 0.90 (benefit of the doubt)
4. Schedule first challenge in 1-24 hours (random jitter)

### Offered → Rejected

**Trigger:** Offer does not meet guards, or a better offer was selected, or timeout (48 hours without decision).

**Action:** No event published. Offer silently dropped. (No explicit rejection kind — silence means no.)

### Active → Degraded

**Trigger:** Reliability score drops below 70%.

**Guards:** Score computed from at least 5 challenges (prevent premature degradation from small sample).

**Action:**
1. Increase challenge frequency to 3× normal
2. Begin seeking replacement (publish kind 10055 if below comfort condition)
3. Continue storing and serving the partner's events

### Active → Standby

**Trigger:** Client has more Active pacts than needed for comfort condition AND this pact is the lowest-priority candidate for demotion.

**Guards:**
- Comfort condition is met without this pact
- Active count > PACT_FLOOR (12)

**Action:**
1. Stop sending challenges
2. Continue storing and receiving events
3. Partner is informed via NIP-46 message (courtesy, not required)

### Standby → Active

**Trigger:** An Active pact transitions to Failed or Dropped, and a standby is needed.

**Priority:** Promote the standby partner with the highest reliability score. If tied, prefer the partner whose uptime pattern best fills coverage gaps.

**Action:**
1. Resume challenge scheduling
2. First challenge within 1 hour

### Degraded → Active

**Trigger:** Reliability score recovers above 80% (hysteresis: degrade at 70%, recover at 80%).

**Action:**
1. Reduce challenge frequency to normal (1×)
2. Cancel replacement search if comfort condition is met

### Degraded → Failed

**Trigger:** Reliability score drops below 50%.

**Action:**
1. Stop storing the partner's events
2. Stop sending challenges
3. Promote a standby partner to Active

### Active → Failed (Direct)

**Trigger:** Partner unresponsive for >72 consecutive hours (no challenge responses, no events received, no NIP-46 messages).

**Exception:** If partition detection is active (≥3 partners in same WoT cluster failed within 1 hour), suspend scoring and do NOT transition to Failed. See partition handling below.

**Action:** Same as Degraded → Failed.

### Standby → Failed

**Trigger:** Standby partner unresponsive for >7 days (longer tolerance since not actively challenged).

**Action:** Remove from standby list. Seek replacement if standby count < 3.

### Failed → Dropped

**Trigger:** Always immediate. Failed is a transient state.

**Action:**
1. Publish kind 10053 with `status: dropped` (encrypted to former partner)
2. Delete stored events for the former partner (after 7-day grace period for their retrieval)
3. Remove from all tracking data structures

## Special Transitions

### Partition Handling

**Detection:** ≥3 pact partners in the same WoT cluster (>50% follow overlap) fail challenges within a 1-hour window while the client's own relay connectivity remains functional.

**During partition:**
- Reliability scoring SUSPENDED for partition-affected peers
- No transitions to Degraded or Failed
- Standby pacts provide availability
- Client operates with remaining reachable partners + relay fallback

**After partition heals:**
- Resume challenge-response with previously partitioned peers
- Reconcile events via checkpoint comparison
- Restore reliability scores to pre-partition levels after 3 consecutive successful challenges

### Renegotiation

**Trigger:** Volume drift >2× the original formation ratio (checked at each checkpoint, ~30 days).

**Action:**
1. NIP-46 message to partner proposing renegotiation
2. If partner agrees to new volume terms, pact continues (no state change)
3. If partner declines or no response within 48 hours, transition to Degraded (seek replacement)
4. Random jitter: 0-48 hours before broadcasting replacement request (prevents renegotiation storms)

### Guardian Pact Lifecycle

Guardian pacts use the same state machine with modifications:
- **Formation:** No volume matching required. No mutual follow requirement.
- **Expiry:** Automatic transition to Dropped after 90 days OR when Seedling reaches Hybrid phase (5+ reciprocal pacts)
- **Completion:** If Seedling reaches Hybrid phase, publish kind 10066 (guardianship completion) before dropping

## Timing Specifications

| Operation | Timing | Jitter |
|-----------|--------|--------|
| Challenge frequency (Active) | 1 per day per pact | ±6 hours uniform random |
| Challenge frequency (Degraded) | 3 per day per pact | ±2 hours uniform random |
| Challenge timeout | 10 seconds | — |
| Challenge retry (alt relay) | After timeout | Immediate |
| Offer evaluation window | 48 hours | — |
| Renegotiation delay | 0-48 hours | Uniform random |
| Partition detection window | 1 hour | — |
| Post-partition recovery | 3 successful challenges | — |
| Standby unresponsive timeout | 7 days | — |
| Active unresponsive timeout | 72 hours | — |
| Event cleanup after drop | 7-day grace period | — |
| Pact relay rotation | 30 days | ±7 days uniform random |

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

## Pact Request Timing

When does a client broadcast kind 10055?

1. **On app launch** — if active pact count < comfort condition
2. **On pact drop** — immediately (with 0-5 minute jitter)
3. **Periodically** — every 24 hours (±2 hour jitter) while below comfort condition
4. **Never** — when at comfort condition AND active ≥ PACT_FLOOR AND standby ≥ 3

## Volume Calculation

Volume for pact matching is calculated as:

```
volume = sum of byte sizes of all public events published in the last 30 days

Included kinds: 0 (metadata), 1 (note), 3 (contacts), 6 (repost), 7 (reaction), 30023 (long-form)
Excluded: kind 14 (DMs), kind 10050-10066 (protocol events), ephemeral events (kind 20000-29999)

The volume figure is the 30-day trailing sum, recalculated at each checkpoint.
```

## Salt Computation for Rotating Request Tokens

```
Salt date is always UTC: YYYY-MM-DD format
Token: SHA-256(target_root_pubkey_hex || date_string_utf8)
Matching: check today's AND yesterday's UTC date (handles ±24h clock skew)
```
