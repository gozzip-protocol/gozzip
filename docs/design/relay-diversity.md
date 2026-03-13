# Relay Diversity Requirements

**Date:** 2026-03-14
**Status:** Draft
**Addresses:** Agent 08 (Red Team) Attack 2, Agent 05 (Privacy) Issue 5, Synthesis Verdict §1.2

## Problem Statement

The protocol relies on relays for NIP-46 pact communication (challenge-response, negotiation, event sync) and for event publishing/retrieval. If a user's pact communication concentrates on 1-2 relays, those relay operators can:

1. **Reconstruct pact topology** from NIP-46 timing analysis
2. **Sabotage challenge-response** by introducing 500ms-2s jitter, causing reliability score degradation
3. **Selectively censor** by dropping specific event kinds (10054, 10055, 10057)
4. **Reconstruct social graphs** by merging subscription and read patterns

A relay cartel (3-5 operators controlling 60-80% of traffic) can do all of the above at scale.

## Design Requirements

### Pact Communication Diversity

**MUST:** Distribute NIP-46 pact communication across at least 3 different relays per user.

**SHOULD:** No single relay handles more than 40% of a user's pact traffic.

**Implementation:**
- Each pact partner pair selects a relay from the intersection of their NIP-65 relay lists
- If the intersection contains fewer than 3 relays, each party adds 1-2 relays from a default list
- Relay assignment for each pact partner is stored in the kind 10053 pact event (encrypted)
- Relay rotation: every 30 days, the pact communication relay is rotated to a different relay from the available set

### Challenge-Response Relay Fallback

**MUST:** When a challenge times out via the primary relay, retry through an alternative relay before marking the challenge as failed.

This is already specified in the whitepaper (channel-aware challenge retry, Section 4.4). This document makes the requirement explicit:

1. Client sends challenge (kind 10054) via primary relay
2. If no response within 10 seconds, resend via alternative relay (selected from NIP-65 list)
3. If alternative relay succeeds, mark primary relay as unreliable for this pact pair
4. Future challenges for this pair use the working relay
5. After 7 days, retry the original relay (it may have been a transient failure)

### Direct Connection Fallback for Full-Full Pairs

**SHOULD:** For pact pairs where both partners are full nodes (95%+ uptime, static IP), offer direct challenge-response via NAT-hole-punched or direct TCP connections.

This removes the relay from the challenge path entirely for the most critical pact relationships:

1. Both partners exchange endpoint hints (kind 10059) via relay
2. Establish direct connection for challenge-response
3. Fall back to relay-mediated if direct connection fails
4. Direct connection is opt-in (exposes IP — see surveillance-surface.md)

### Event Publishing Diversity

**MUST:** Publish events to at least 3 relays (already standard via NIP-65 outbox model).

**SHOULD:** No single relay in the user's NIP-65 list should serve as outbox for more than 40% of their follows' reads.

**Client guidance:** When the client detects that >50% of its follows use the same relay, suggest adding alternative relays.

## Relay Health Monitoring

### Canary Probing

Clients periodically verify relay integrity:

1. Publish a test event (kind 1, tagged `["t", "canary"]`) to a relay
2. From a different network path (via a pact partner), attempt to fetch the event
3. If the event is not served, flag the relay as potentially dropping events
4. Repeat weekly per relay

### Kind-Specific Drop Detection

Monitor whether specific event kinds are being dropped:

1. Track success rate for kind 10054 (challenges), kind 10055 (pact requests), kind 10057 (data requests) per relay
2. If any kind has >20% failure rate while other kinds succeed normally, flag the relay as selectively dropping that kind
3. Switch affected event kinds to alternative relays

### Relay Latency Tracking

Track median latency per relay for NIP-46 operations:

1. Measure round-trip time for NIP-46 store/fetch pairs
2. If median latency exceeds 2 seconds for a relay while other relays are <500ms, flag as suspicious
3. Persistent high latency (>1 week) triggers relay replacement for affected pact pairs

## Relay Collusion Threat Model

### What colluding relays can see

With 3-5 popular relays colluding (serving ~80% of users):

| Observable | Method | Confidence |
|-----------|--------|------------|
| Social graph (~80%) | Merged subscription patterns | High |
| Pact pair candidates | Merged NIP-46 timing analysis | Medium-High |
| Request targets | Pre-compute H(pubkey \|\| date) for known pubkeys | High |
| DM communication graph | Sender pubkey + recipient p-tag | High |
| Content | Cannot (NIP-44 encrypted) | — |

### Mitigation hierarchy

1. **Relay diversity** (this document) — reduces any single relay's visibility
2. **NIP-59 gift wrapping for NIP-46** — hides recipient in pact communication (see surveillance-surface.md)
3. **Tor for relay connections** — hides IP from relays (implementation required, not yet designed)
4. **Cover traffic** — dummy NIP-46 operations to obscure real pact timing (bandwidth cost trade-off)

### What relay diversity cannot prevent

Even with perfect relay diversity:
- A colluding majority still reconstructs most of the graph from merged partial views
- Long-term timing analysis still identifies probable pact pairs
- IP correlation across relays defeats activity fragmentation

Relay diversity raises the cost and reduces the precision of surveillance. It does not eliminate it. Users facing state-level adversaries should use Tor for relay connections.

## Client Configuration

### Default Settings

```
relay_diversity:
  min_pact_relays: 3          # Minimum relays for pact communication
  max_relay_concentration: 0.40  # Max fraction of pact traffic per relay
  relay_rotation_days: 30      # Rotate pact relay assignment
  challenge_retry_timeout_ms: 10000  # Timeout before trying alternative relay
  canary_probe_interval_days: 7
  direct_challenge_fallback: false  # Opt-in for full-full pairs
```

### Privacy Mode (Opt-In)

```
relay_diversity_privacy:
  min_pact_relays: 5
  max_relay_concentration: 0.25
  relay_rotation_days: 14
  use_tor: true                # Route all relay connections through Tor
  cover_traffic: true          # Generate dummy NIP-46 operations
```
