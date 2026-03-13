# Guardian Incentive Mechanisms

**Date:** 2026-03-14
**Status:** Draft
**Addresses:** Agent 03 (Game Theory) Issue 3, Agent 04 (Cold Start) Issue 2, Agent 08 (Red Team) Attack 5

## Problem Statement

Guardian pacts are one-sided: the Guardian stores a Seedling's data and receives nothing in return. Game-theoretic analysis shows the unique Nash equilibrium is "nobody volunteers" — a classic public goods problem where individual rationality produces collective failure.

The pay-it-forward framing relies on prosocial behavior, not rational self-interest. While prosocial behavior exists in practice (open-source contributors, Wikipedia editors, forum moderators), designing critical infrastructure around it is risky.

## Current State

- Guardian pacts use kind 10053 with `type: guardian` tag
- A Sovereign-phase user (15+ pacts) volunteers to store one Seedling's data
- Expiry: 90 days or Seedling reaches Hybrid phase (5+ reciprocal pacts)
- Each Guardian holds at most one active guardian pact
- No incentive mechanism exists beyond client UX encouragement

## Proposed Incentive Mechanisms

### Mechanism 1: Persistent WoT Edge (Recommended)

**Design:** Successful guardianship (Seedling reaches Hybrid phase before the 90-day expiry) creates a permanent, weighted WoT edge between Guardian and Seedling.

**How it works:**
1. Guardian accepts a Seedling via kind 10055 with `type: guardian`
2. When the Seedling reaches Hybrid phase (5+ reciprocal pacts), both clients publish a kind 10066 "guardianship completion" event
3. The completion event creates a persistent trust edge: the Seedling is now a first-class WoT contact of the Guardian, with a "guardian" attestation
4. This edge improves the Guardian's WoT graph density — more paths to more nodes, better gossip routing, larger potential pact partner pool

**Incentive analysis:**
- Tangible benefit: expanded WoT graph → more pact formation opportunities → better availability
- Scales with network growth: each successful guardianship adds a real social connection
- No token, no external value — entirely internal to the protocol
- Cost to Guardian: one user's data volume (~100-700 KB/month) for up to 90 days

**Limitation:** The benefit is small for Guardians with already-dense WoT graphs. May be insufficient for power users who don't need more connections.

### Mechanism 2: Gossip Forwarding Priority Boost

**Design:** Guardians receive a temporary boost in gossip forwarding priority. Their content is forwarded more eagerly by all nodes that can verify the Guardian's active guardian pact.

**How it works:**
1. The Guardian's kind 10055 advertisement with `type: guardian` is publicly visible
2. Nodes that forward gossip check whether the source has an active guardian pact
3. Active Guardians receive priority equivalent to "active pact partner" level for gossip forwarding from all nodes (not just their own pact partners)
4. Boost duration: while the guardian pact is active (up to 90 days)

**Incentive analysis:**
- Direct benefit: wider content reach during the guardianship period
- Verifiable: any node can check the guardian pact status
- Proportional: the boost lasts only while the Guardian is actively storing

**Limitation:** Benefits content creators only. Guardians who don't post much see little value. Requires gossip-layer changes to check guardian status.

### Mechanism 3: Guardian Reputation Visibility

**Design:** Publish guardianship history as a verifiable reputation signal. Clients display "Alice has helped N newcomers reach Hybrid phase."

**How it works:**
1. Kind 10066 completion events are public and signed by both parties
2. Clients aggregate completion counts per pubkey
3. Client UX displays guardian count as a trust signal (like follower count, but for community contribution)
4. Optional: discovery relays surface high-guardian-count users in recommendation feeds

**Incentive analysis:**
- Social capital: visible contribution to the network
- Trust signal: users may prefer to follow/pact with known community contributors
- Zero protocol complexity: just a client-side display decision

**Limitation:** Social capital is a weak incentive for most users. May attract gaming (creating fake Seedlings to boost guardian count).

### Mechanism 4: Multiple Guardian Pacts (Defense in Depth)

**Design:** Allow Seedlings to have 2-3 guardian pacts instead of the current maximum of 1.

**How it works:**
1. Increase `guardian_max` from 1 to 3 per Seedling
2. Each Guardian still holds at most 1 active guardian pact (distributes load)
3. Seedlings matched with multiple Guardians have redundancy against a malicious or unreliable single Guardian
4. Reduces the impact of Agent 08's Guardian Abuse attack (malicious Guardian silently censoring Seedling)

**Incentive analysis:**
- Not an incentive mechanism per se — a safety improvement
- Reduces the cost of individual Guardian failure
- Lowers the stakes for volunteering (if you're unreliable, the Seedling has backups)

## Recommended Approach

**Primary: Mechanism 1 (Persistent WoT Edge) + Mechanism 4 (Multiple Guardians)**

The combination provides:
- A concrete, measurable benefit for Guardians (expanded WoT)
- Defense in depth for Seedlings (2-3 Guardians instead of 1)
- No token or external incentive required
- Minimal protocol complexity (one new event kind: 10066)

**Secondary: Mechanism 3 (Reputation Visibility)**

Social reputation is a nice-to-have that requires only client-side changes. Implement when the first client ships.

**Deferred: Mechanism 2 (Gossip Priority Boost)**

The gossip-layer changes required are non-trivial and the benefit is limited to content creators. Evaluate after observing organic guardian behavior in production.

## Interaction with Genesis Bootstrap

During the first 6-12 months, organic guardian supply is zero (no Sovereign-phase users exist). Genesis Guardian nodes (see genesis-bootstrap.md) fill this gap. The incentive mechanisms activate once organic Sovereign-phase users emerge and can volunteer.

## Event Kind: Guardianship Completion (kind 10066)

```json
{
  "kind": 10066,
  "content": "",
  "tags": [
    ["p", "<guardian_pubkey>"],
    ["p", "<seedling_pubkey>"],
    ["type", "guardianship_completion"],
    ["started", "<unix_timestamp>"],
    ["completed", "<unix_timestamp>"],
    ["seedling_pact_count", "5"],
    ["protocol_version", "1"]
  ]
}
```

Published by both Guardian and Seedling (mutual attestation). The event is only valid when both versions exist and reference each other.

## Anti-Gaming

- **Fake Seedling farming:** Creating fake Seedlings to boost guardian count is detectable — fake Seedlings have no organic follows, no real content, and suspicious pact formation patterns. Client-side heuristics (see spam-resistance.md) catch this.
- **Guardian count inflation:** Require Seedlings to reach Hybrid phase (5+ organic reciprocal pacts) before the completion event is valid. This takes genuine social graph participation.
- **Collusion:** A Guardian and Seedling colluding to produce a fake completion is theoretically possible but provides negligible benefit (one WoT edge) for non-trivial effort (90 days of fake activity).
