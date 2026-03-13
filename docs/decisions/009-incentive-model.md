# ADR 009: Incentive Model

**Date:** 2026-03-01
**Status:** Accepted

## Context

The network needs ecosystem-native incentives for storage peers and relays. External subscriptions or tokens create dependency on outside systems. The protocol already has reciprocal storage pacts, follow-as-commitment, guardian pacts (voluntary storage for newcomers), and zaps (kind 9734/9735), but no explicit mechanism connecting contribution to visibility. Relays need sustainable value within the ecosystem as delivery infrastructure with reduced data custody.

See the [full design document](../design/incentive-model.md) for detailed problem descriptions and rationale.

## Decision

Three-layer incentive model:

### 1. Pact-Aware Gossip Routing

When a node forwards gossip, it prioritizes content from pubkeys it has active storage pacts with. Priority ordering:

1. **Active pact partners** (highest) — nodes you have a live, verified storage pact with
2. **1-hop WoT** — pubkeys you directly follow
3. **2-hop WoT** — pubkeys followed by your follows
4. **Unknown** — never forwarded

A user with 20 reliable pact partners has 20 nodes eagerly forwarding their content. Dropped pact = lost forwarding advocate = reduced reach.

No public score — network topology IS the incentive.

### 2. Relay-as-Curator

Relays have their own root pubkey and publish curated event lists and recommendations. Users follow relay feeds like they follow people. A relay's follower count and WoT position determine how far its curations travel through gossip.

Three relay types:

- **Discovery** — curate by topic or quality
- **Infrastructure** — fast indexing, high availability
- **Community** — serve specific groups

Users benefit from publishing through well-connected relays, gaining a wider audience beyond their own WoT.

### 3. Lightning Premium Layer

Relays publish a service menu. Users zap to activate services:

- **Priority delivery** — faster indexing, wider push to subscribers
- **Extended retention** — events kept beyond the default window
- **Content boost** — post featured in curated feed, transparently marked
- **Relay-defined services** — analytics, custom filtering, etc.

Transparent, competitive (relays set their own prices), optional.

## Rejected Alternatives

### Blind Contribution Tokens

Pact partners issue blind tokens for passing storage challenges. Rejected: significant protocol complexity (blind signatures, anonymous credentials), gameable via colluding pact partners minting tokens for each other.

### Public Reputation Tiers

Coarse-grained public contribution level attested by threshold signature. Rejected: reveals storage participation metadata, requires threshold signature scheme, still gameable.

## Consequences

**Positive:**
- Self-sustaining incentive loop without tokens or external dependencies
- Privacy-preserving — no new metadata revealed
- Relay operators earn sustainable value through curation and Lightning services
- Storage reliability directly rewarded with content reach
- Graceful degradation — less contribution means less reach, not exclusion
- Pay-it-forward loop — today's *Seedling* becomes tomorrow's *Guardian*. Users who received guardian storage during bootstrap are encouraged to volunteer a guardian slot once they reach Sovereign phase, creating a self-reinforcing generosity cycle

**Negative:**
- Incentive is subtle — users may not directly perceive that reliable storage improves their reach
- Relay market may concentrate around a few popular discovery relays

**Neutral:**
- Lightning layer is fully optional
- Relay types are emergent (not protocol-enforced categories)
