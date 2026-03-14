# Monitoring and Diagnostics
**Date:** 2026-03-14
**Status:** Draft

## Problem Statement

The protocol specifies pact formation, challenge-response, gossip routing, and tiered retrieval — but provides no diagnostic tools. When a data request fails, a pact degrades, or gossip doesn't reach a user's content, there is no way to understand why. Users see the symptom (content not loading, partner dropped) but never the cause (relay dropping kind 10054, partner's disk full, gossip TTL exhausted before reaching any holder).

This is especially damaging during the Bootstrap and Hybrid phases (0–15 pacts), when failures are most likely and the user's trust in the protocol is most fragile. A user who loses a pact with no explanation will assume the protocol is broken, not that their partner went offline for 73 hours.

This document specifies the diagnostic surface that clients must expose.

## Design Principles

1. **Local-first.** All diagnostic data is computed and stored on the user's device. Diagnostic logs are never published to relays, never shared with pact partners, and never leave the device unless the user explicitly exports them.
2. **Privacy by default.** Diagnostic displays use pubkey nicknames (from kind 0 metadata), not raw pubkeys. Export formats strip identifying information unless the user opts in.
3. **Progressive disclosure.** Standard users see high-level health indicators. Developer mode exposes full protocol traces. The default view never mentions event kinds, hash computations, or relay WebSocket frames.
4. **No new event kinds.** Diagnostics are purely client-side. No new protocol messages, no relay requirements, no changes to the pact state machine.

---

## 1. Pact Health Dashboard

### Per-Pact Health Indicators

Each active, standby, and degraded pact is represented as a card in the client's pact management view. The card displays:

| Indicator | Source | Display |
|-----------|--------|---------|
| Partner online status | Last event received from partner (any kind) | "Online", "Last seen 2h ago", "Offline 3d" |
| Challenge pass rate | Last 10 challenge-response results (kind 10054) | "10/10 passed", "7/10 — degrading", "3/10 — at risk" |
| Volume balance | Ratio of partner's 30-day volume to own 30-day volume | "Balanced (1.1x)", "Drifting (1.4x)", "Over tolerance (1.6x)" |
| Storage used vs committed | Byte count of stored events vs pact-agreed capacity | "12 MB / 50 MB", "47 MB / 50 MB — near limit" |
| Pact age | Time since kind 10053 exchange | "Active for 4 months" |
| Time to next renewal | Days until next checkpoint-based renegotiation window | "Next check in 12 days" |
| Pact state | Current state machine state | "Active", "Degraded", "Standby" |

**Color coding:**
- Green: challenge rate >= 90%, volume within 30% tolerance, partner seen in last 24h
- Yellow: challenge rate 70–89%, volume drifting toward tolerance, partner offline 24–48h
- Red: challenge rate < 70%, volume over tolerance, partner offline > 48h

### Aggregate Network Health

A summary view at the top of the pact management screen:

| Metric | Computation | Example |
|--------|-------------|---------|
| Total active pacts | Count of pacts in Active state | "18 active pacts" |
| Total standby pacts | Count of pacts in Standby state | "3 standby" |
| Availability estimate | Probability that at least 1 pact partner is online, given observed uptime distribution | "99.7% estimated availability" |
| Sovereignty phase | Bootstrap (0–5), Hybrid (5–15), Sovereign (15+) | "Sovereign phase" |
| Pacts at risk | Count of Degraded pacts | "1 pact degrading" |
| Coverage gaps | Time windows where fewer than 3 partners are typically online (from historical uptime patterns) | "Low coverage 03:00–05:00 UTC" |

---

## 2. Gossip Propagation Debugging

### Retrieval Trace

When the client retrieves content (events for a followed user), it records which delivery tier succeeded and displays a trace in the event detail view (accessible via long-press or info button):

**Trace format:**

```
Retrieved from pact partner Alice (Tier 1, cached endpoint, 45ms)
```
```
Found via gossip through Bob → Carol (Tier 2, 2 hops, 180ms)
```
```
Fell back to relay wss://relay.example.com (Tier 4, 220ms)
```
```
Failed: content not found (all tiers exhausted, 4.2s total)
```

The trace records:
- Tier used (0 = BLE mesh, 1 = cached endpoints, 2 = gossip, 3 = storage peers via DVM, 4 = relay fallback)
- For gossip: the forwarding path (which peers relayed the request)
- Latency per tier attempt
- Why lower-numbered tiers failed (timeout, no holders found, relay error)

**Display in standard mode:** Single line summarizing the source ("From your storage network" or "From relay").

**Display in developer mode:** Full multi-line trace with tier numbers, hop paths, and timing.

### Gossip Reach Estimate

The client estimates how many nodes can serve the user's content by counting:
- Active pact partners holding the user's events
- Standby pact partners holding the user's events
- Cached copies (estimated from recent kind 10058 data offer responses to requests for the user's data)

Displayed as: "Your content is held by ~23 peers across the network."

This is a lower-bound estimate. Cascading read-caches make the true reach higher for popular content, but the client cannot observe caches it doesn't know about.

---

## 3. Relay Interaction Logging

### Relay Usage Breakdown

The client maintains a local log of which relays are used for which purpose:

| Purpose | Example |
|---------|---------|
| NIP-46 pact channels | "wss://relay-a.com — 3 pact partner channels" |
| Event publishing (outbox) | "wss://relay-b.com — primary outbox" |
| Event reading (inbox) | "wss://relay-c.com — reading 12 followed authors" |
| Gossip fallback (Tier 3/4) | "wss://relay-d.com — 7 data requests this week" |
| Discovery | "wss://relay-e.com — kind 10055 pact requests" |

### Relay Health Metrics

Per relay, tracked over a rolling 7-day window:

| Metric | Measurement | Alert threshold |
|--------|-------------|-----------------|
| Median latency | Round-trip time for NIP-46 store/fetch operations | > 2 seconds |
| Error rate | Fraction of requests returning error or timeout | > 10% |
| Last successful interaction | Timestamp of last successful event publish or fetch | > 24 hours ago |
| Uptime (observed) | Fraction of connection attempts that succeeded | < 90% |

### Relay Rejection Log

When a relay rejects an event or request, the client logs the reason:

| Rejection type | Detection method | Display |
|----------------|------------------|---------|
| Rate limited | Relay returns NOTICE or CLOSED with rate-limit message | "Rate limited by wss://relay.com — 3 events dropped" |
| Event too large | Relay returns OK with `false` and size reason | "Event rejected: exceeds relay's 64 KB limit" |
| Kind not supported | Relay returns OK with `false` and kind reason | "Kind 10054 not accepted by wss://relay.com" |
| Connection refused | WebSocket handshake fails | "Cannot connect to wss://relay.com" |
| Auth required | Relay returns AUTH challenge (NIP-42) | "wss://relay.com requires authentication" |

### Privacy Constraint

All relay logs are stored in the client's local database. They are never published to any relay, never included in any event, and never transmitted to any peer. The log is viewable only in the client's settings screen. Export is available as a local file for debugging, with relay URLs included but no event content.

---

## 4. Challenge-Response Diagnostics

### Per-Partner Challenge History

For each pact partner, the client maintains a rolling log of the last 30 challenges (sent and received):

| Field | Value |
|-------|-------|
| Timestamp | When the challenge was sent or received |
| Direction | Sent (we challenged them) or Received (they challenged us) |
| Result | Passed, Failed, Timeout, No Response |
| Latency | Round-trip time for the challenge-response |
| Relay used | Which relay carried the NIP-46 challenge |

### Failure Diagnosis

When a challenge fails, the client categorizes the failure:

| Failure type | Detection | Likely cause |
|--------------|-----------|--------------|
| Timeout | No response within 10 seconds on primary relay, no response on retry via alternative relay | Partner offline or unreachable |
| Wrong response | Hash mismatch between expected and received | Data corruption or loss on partner's device. Partner may have incomplete events. |
| No response | Challenge delivered (relay confirms receipt) but partner never responds | Partner's client not processing challenges (app suspended, outdated client, or partner dropped the pact unilaterally) |
| Relay failure | Challenge could not be delivered to any relay | All configured relays for this pact pair are down |

### Reliability Score Trend

The client displays a reliability score trend line for each partner:

- Current score (exponential moving average, alpha = 0.95)
- 30-day trend: improving, stable, or degrading
- Visual: sparkline or simple arrow indicator

**Alerts:**
- "Partner [name] reliability dropped to 75% — pact is now degraded. Seeking replacement." (transition to Degraded state)
- "Partner [name] has failed 3 consecutive challenges. If this continues, the pact will be dropped." (approaching Failed threshold)
- "Partner [name] recovered to 85% — pact restored to Active." (transition back from Degraded)

### Degradation Threshold Warning

The client alerts the user before a pact transitions to a terminal state:

| Score range | Alert |
|-------------|-------|
| 80–70% | "Pact with [name] is weakening. No action needed — the protocol is monitoring." |
| 70–50% | "Pact with [name] is degraded. A replacement partner is being sought automatically." |
| < 50% | "Pact with [name] has been dropped. A new partner has been promoted from standby." |

---

## 5. WoT Visualization

### Social Graph View

The client renders a network graph of the user's social relationships:

- **Center:** The user's identity
- **Ring 1 (Inner Circle):** Mutual follows — pact partners and close contacts
- **Ring 2 (Orbit):** Authors the user interacts with heavily, plus referrals from Inner Circle
- **Ring 3 (Horizon):** 2-hop contacts reachable through the WoT

Node size scales with interaction score. Edge thickness represents relationship strength (mutual follow, one-way follow, pact partner).

### Pact Overlay

Toggling the pact overlay highlights which WoT contacts have active pacts with the user:

| Visual | Meaning |
|--------|---------|
| Solid border | Active pact partner |
| Dashed border | Standby pact partner |
| Yellow border | Degraded pact partner |
| No border | WoT contact without a pact |
| Shield icon | Guardian pact (the user is a Guardian for this contact) |
| Seedling icon | Guardian pact (the user is a Seedling receiving from this contact) |

### Trust Score Distribution

A histogram showing the distribution of WoT distances for the user's pact partners:
- How many partners are mutual follows (distance 1)
- How many are 2-hop contacts (distance 2)
- Whether the pact network is concentrated in one social cluster or distributed

### Community Detection

Visual clustering of the user's WoT graph using a force-directed layout:
- Clusters of densely connected contacts are grouped together
- Pact partners that span clusters provide structural resilience (if one cluster goes offline, partners in other clusters remain)
- The client highlights if all pact partners are in a single cluster: "Your pact partners are concentrated in one social group. Consider forming pacts with contacts in other communities for better resilience."

---

## 6. Health Check Command

### User-Initiated Health Check

The client provides a "Check Network Health" action (accessible from the pact management screen or settings). This runs a series of tests and returns a structured report.

### Test Suite

| Test | Method | Pass criteria |
|------|--------|---------------|
| Ping pact partners | Send a lightweight NIP-46 ping to each active and standby partner via their assigned relay | Response within 10 seconds |
| Verify relay connectivity | Attempt WebSocket connection to each relay in the user's NIP-65 list and NIP-46 pact relay assignments | Successful handshake |
| Check device key validity | Verify that the user's kind 10050 is published and all listed device pubkeys are reachable | Kind 10050 exists on at least 2 relays, current device is listed |
| Verify checkpoint freshness | Check that the latest kind 10051 checkpoint is less than 48 hours old | Checkpoint age < 48h |
| Test relay acceptance of protocol kinds | Publish a test event (kind 10054) to each pact relay and verify acceptance | Relay returns OK with `true` |
| Check volume balance | Recalculate 30-day volume for self and each partner | All active pacts within 30% tolerance |

### Report Format

```
Network Health: Good (5/6 checks passed)

[PASS] Pact partners: 17/18 reachable (Carol offline 2h — within normal range)
[PASS] Relay connectivity: 4/4 relays connected
[PASS] Device keys: Valid, published to 3 relays
[PASS] Checkpoint: 6 hours old
[WARN] Relay compatibility: wss://relay-x.com does not accept kind 10054
[PASS] Volume balance: All pacts within tolerance

Suggested actions:
  - Consider replacing wss://relay-x.com for pact communication (does not support challenge events)
```

### Lightweight Auto-Check

On app launch, the client runs a lightweight version (relays + device key validity only — no partner pings). If issues are detected, the client shows a non-intrusive notification: "1 relay is unreachable. Tap to see details."

Full health check is never run automatically — it generates observable NIP-46 traffic to all partners, which could be a timing signal. Only the user initiates it.

---

## 7. Protocol-Level Tracing (Developer Mode)

### Activation

Developer mode is enabled via a hidden settings toggle (e.g., tap version number 7 times). It is never on by default. When enabled, the client logs full protocol-level traces to a local buffer (circular buffer, configurable size, default 10 MB).

### Event Lifecycle Trace

Trace the full lifecycle of a single event from publish to delivery:

```
[14:03:01.234] EVENT PUBLISH kind=1 id=abc123
[14:03:01.240] → Signed with device key dev_xyz
[14:03:01.241] → root_identity tag: root_abc
[14:03:01.245] → Published to wss://relay-a.com (OK, 4ms)
[14:03:01.248] → Published to wss://relay-b.com (OK, 7ms)
[14:03:01.301] → Published to wss://relay-c.com (OK, 60ms)
[14:03:01.302] → Forwarded to 18 pact partners via storage sync
[14:03:02.100] → Pact partner Bob confirmed receipt (798ms)
[14:03:02.340] → Pact partner Carol confirmed receipt (1038ms)
[14:03:05.000] → 16/18 partners confirmed within 4s
```

### Pact Negotiation Trace

Step-by-step pact formation:

```
[10:00:00.000] PACT REQUEST published kind=10055 volume=42MB
[10:00:12.400] OFFER received kind=10056 from npub1abc... (volume=38MB, WoT distance=1)
[10:00:12.401]   Volume match: 38/42 = 0.90 (within ±30%)
[10:00:12.402]   WoT check: mutual follow, distance=1 (PASS)
[10:00:12.403]   Account age: 14 months (PASS, min=7 days)
[10:00:12.404]   Capacity check: 19/43 active+standby (PASS)
[10:00:12.500] OFFER accepted → publishing kind 10053
[10:00:12.600] PACT ACTIVE with npub1abc... — initial reliability=0.90
[10:00:12.601] First challenge scheduled in 14h 22m (jittered)
```

### Challenge-Response Trace

Full round-trip with timing:

```
[08:00:00.000] CHALLENGE SENT kind=10054 to npub1abc...
[08:00:00.001]   nonce=a1b2c3... range=[47..53] (7 events)
[08:00:00.002]   via wss://relay-a.com (primary)
[08:00:00.003]   expected hash=d4e5f6...
[08:00:04.200] RESPONSE RECEIVED from npub1abc...
[08:00:04.201]   response hash=d4e5f6... (MATCH)
[08:00:04.202]   latency=4198ms
[08:00:04.203]   reliability score: 0.91 → 0.91 (no change, pass)
```

### Gossip Propagation Trace

Track a data request through the gossip network:

```
[12:00:00.000] DATA REQUEST kind=10057 for target hash=h(pubkey||date)
[12:00:00.001]   Tier 0 (BLE): no mesh peers available
[12:00:00.002]   Tier 1 (cached endpoints): 2 endpoints known
[12:00:00.050]     endpoint wss://peer1:8080 — timeout (50ms)
[12:00:00.100]     endpoint wss://peer2:9090 — connected, no data
[12:00:00.101]   Tier 2 (gossip): broadcasting to 20 peers, TTL=3
[12:00:00.180]     Bob forwarded → Carol responded (2 hops, 80ms)
[12:00:00.181]     DATA OFFER received kind=10058 from Carol
[12:00:00.250]     Fetching events from Carol... 14 events, 23 KB
[12:00:00.251]     Signature verification: 14/14 valid
[12:00:00.252]     Merkle root check against checkpoint: PASS
[12:00:00.253]   RETRIEVAL COMPLETE: Tier 2, 253ms total
```

### Trace Export

Developer mode traces can be exported as a JSON file for debugging. The export includes:
- All trace entries in the buffer
- Client version and platform
- Current pact state summary (partner count, states — no pubkeys unless the user opts in)
- Relay list and health metrics

Pubkeys are replaced with pseudonyms (Partner-1, Partner-2) by default. Full pubkeys are included only if the user explicitly enables "include identities in export."

---

## 8. Relay Compatibility Reporting

### Compatibility Test

The client can test a relay against Gozzip's requirements. This is useful when adding a new relay to the user's NIP-65 list or when debugging pact communication failures.

| Requirement | Test method | Severity |
|-------------|-------------|----------|
| NIP-46 support | Attempt NIP-46 store/fetch cycle | Critical — required for pact communication |
| Kind 10053 acceptance | Publish test kind 10053 event | Critical — required for pact formation |
| Kind 10054 acceptance | Publish test kind 10054 event | Critical — required for challenge-response |
| Kind 10055/10056 acceptance | Publish test kind 10055 event | Important — required for pact discovery |
| Kind 10057/10058 acceptance | Publish test kind 10057 event | Important — required for data requests |
| NIP-44 encryption support | Verify relay does not reject NIP-44 encrypted content | Critical — all pact communication is encrypted |
| Event size limit | Check relay's maximum event size | Informational — affects large checkpoints |
| Rate limits | Measure how many events can be published per minute before throttling | Informational — affects high-volume users |
| NIP-59 gift wrap support | Publish a NIP-59 wrapped event | Important — used for privacy-preserving pact communication |

### Compatibility Score

Each relay receives a score based on test results:

| Score | Meaning |
|-------|---------|
| Full compatibility | All critical and important tests pass |
| Partial compatibility | All critical tests pass, some important tests fail |
| Incompatible | One or more critical tests fail |

The client displays the score when the user views relay details or adds a new relay.

### Compatibility Warnings

The client warns the user when a relay in their active configuration fails compatibility:

- "wss://relay.com does not support kind 10054. Challenge-response for pacts using this relay may fail."
- "wss://relay.com rejects events larger than 16 KB. Large checkpoints may not be publishable here."
- "wss://relay.com does not support NIP-46. This relay cannot be used for pact communication."

### Community Compatibility List

The client can fetch and display a community-maintained relay compatibility list:

- Published as a replaceable event (kind 30078, `d` tag = "gozzip-relay-compat") by a well-known project pubkey
- Contains relay URLs and their last-tested compatibility scores
- Updated periodically by automated testing infrastructure
- Client merges community data with its own local test results (local results take precedence)
- The user can disable community list fetching (opt-out for privacy)

---

## Storage and Retention

All diagnostic data is stored in the client's local database:

| Data type | Retention | Storage estimate |
|-----------|-----------|-----------------|
| Pact health indicators | Indefinite (while pact is active) | ~1 KB per pact |
| Challenge history | 90 days rolling | ~500 bytes per challenge |
| Relay health metrics | 30 days rolling | ~2 KB per relay |
| Relay rejection log | 30 days rolling | ~200 bytes per rejection |
| Gossip retrieval traces | 7 days rolling | ~500 bytes per retrieval |
| Developer mode traces | Circular buffer, 10 MB max | 10 MB |
| WoT graph cache | Refreshed on demand | ~50 KB for 500-node graph |

Total estimated storage for a user with 20 pacts, 5 relays, and standard usage: approximately 2–5 MB. Developer mode adds up to 10 MB.

---

## Implementation Priority

| Priority | Component | Rationale |
|----------|-----------|-----------|
| 1 | Pact health dashboard | Users need visibility into their most critical infrastructure |
| 2 | Challenge-response diagnostics | Pact failures are the most common support question |
| 3 | Relay interaction logging | Relay issues are the second most common failure mode |
| 4 | Health check command | Proactive issue detection reduces support burden |
| 5 | Relay compatibility reporting | Prevents users from configuring incompatible relays |
| 6 | Gossip propagation debugging | Valuable but less frequently needed |
| 7 | WoT visualization | Informational, not diagnostic — nice to have |
| 8 | Protocol-level tracing | Developer-only, can ship later |

Priorities 1–4 should ship with the first client release. Priorities 5–6 should ship within 3 months. Priorities 7–8 can ship when developer demand justifies the effort.
