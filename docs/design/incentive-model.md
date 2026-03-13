# Incentive Model — Design Document

**Date:** 2026-03-01
**Status:** Approved

## Principle

Value comes from within the ecosystem. Attention and reach are the primary currency — contributing more to the network makes your content more discoverable. Lightning (zaps) provides an optional premium layer on top.

No external subscriptions. No tokens. No mandatory payments.

## Pact-Aware Gossip Routing

The core mechanism. When a node forwards gossip or decides what content to propagate, it applies priority ordering:

1. **Active pact partners** — highest priority. You store their data, you forward their content eagerly. Self-interested: if their content is discoverable, your storage pact has value.
2. **WoT contacts** (1-hop follows) — standard priority.
3. **Extended WoT** (2-hop) — lower priority, forwarded if capacity allows.
4. **Unknown pubkeys** — served locally if available, never forwarded.

A user with 20 reliable pact partners has 20 nodes that eagerly forward their content. A user with 5 flaky pacts has fewer advocates. No score published — the network topology IS the incentive.

When a pact drops due to low reliability, the dropped peer loses a forwarding advocate. Their content distribution naturally shrinks.

## Relay Types and Relay-as-Curator

Relays aren't one thing. Different types provide different value.

**Discovery relays** — curate content by topic, quality, or community. Users follow a discovery relay's feed like they follow a person. The relay earns followers by being a good filter.

**Infrastructure relays** — store broadly, serve fast, high availability. Their value is performance and reliability. They remain useful as accelerators even in the sovereign phase (15+ pacts).

**Community relays** — serve specific groups (NIP-29 group chats, local communities, interest groups). Their value is exclusivity and curation within a community.

**How relays earn reach:** A relay has its own root pubkey. It publishes curated event lists, recommendations, or indexes. Users who find value subscribe (follow). The relay's follower count and WoT position determine how far its curations travel through gossip.

**How users benefit from relays:** Publishing through a well-connected relay makes your content visible to that relay's subscribers — an audience beyond your own WoT. The relay is a distribution channel.

## The Contribution-Reach Feedback Loop

**For a regular user:**
1. Join, follow people → bootstrap pacts form
2. Store reliably for pact partners → they forward your content eagerly
3. Posts reach more people through gossip → gain followers
4. More followers = more potential pact partners = more forwarding advocates
5. Content gets picked up by discovery relays → even wider reach

**For a relay operator:**
1. Run a relay, curate good content → users subscribe
2. More subscribers → curations travel further through gossip
3. Users want to publish through you → more content to curate
4. Relay becomes a hub for discovery beyond individual WoT graphs

**For a passive user (lurker):**
1. Minimal storage contribution → few pact partners → limited forwarding advocates
2. Content distribution is small but functional — WoT contacts still see posts
3. Can still discover content through relays and own follows
4. No punishment — just less amplification

**No cliff.** There's no minimum contribution to participate. Less contribution means less reach, not exclusion.

**Keeper ratio assumption:** The plausibility analysis models 25% of users running always-on full nodes (Keepers). This is an optimistic target. Comparable systems achieve 0.1-5% always-on participation (Bitcoin full nodes: ~0.01%, Mastodon instance operators: ~2%, Nostr relays: 0.2-1%). The protocol is designed to function at full-node ratios as low as 5%. At 5% Keepers, the all-light-node availability analysis applies (P(unavailable) ≈ 0.08%), which remains acceptable. The incentive loop above is designed to encourage Keeper operation organically, but the protocol must not depend on achieving 25%.

## Lightning as the Premium Layer

The base layer is free. Lightning adds a premium tier — relays publish a service menu, users zap to activate services.

| Service | What you get | How it works |
|---------|-------------|--------------|
| Priority delivery | Faster indexing, wider push to relay subscribers | Zap relay per-event or per-month |
| Extended retention | Events kept beyond default window | Zap relay for duration (e.g., 100 sats/month for 1-year retention) |
| Content boost | Specific post featured in relay's curated feed | Zap relay with tagged zap pointing to the event. Marked as boosted (transparent). |
| Relay-defined services | Whatever the operator invents | Analytics, custom filtering, API access, webhooks — relays compete on features |

**Properties:**
- **Transparent** — boosted content is visibly marked
- **Relay-competitive** — different relays offer different prices and services
- **Optional** — the free attention layer works without Lightning
- **Ecosystem-native** — sats flow between participants inside the network

## The Incentive Map

| Actor | Contributes | Earns (free layer) | Earns (Lightning layer) |
|-------|------------|-------------------|------------------------|
| User | Reliable storage for pact partners | Pact partners forward content → wider reach | Can pay relays for boost, priority, retention |
| Discovery relay | Curates quality content | Followers → influence → users publish through it | Earns sats from boosts, priority delivery |
| Infrastructure relay | Fast indexing, high availability | Users rely on it as accelerator → sticky base | Earns sats from extended retention, premium features |
| Community relay | Serves a group/topic | Community membership → exclusive content access | Earns sats from group-specific services |
| Passive user | Little/nothing | Basic WoT reach — network works, just smaller | Can pay to compensate for lack of organic reach |

## What Keeps Everyone Honest

- **Storage peers** — challenge-response + reliability scoring. Fail → lose pacts → lose forwarding advocates
- **Relays** — subscriber count. Bad curation or broken promises → users leave
- **Users** — reciprocity. Stop contributing → pact partners drop you → reach shrinks

No cliffs, no gatekeepers. Everything degrades gracefully.

## Rejected Alternatives

**Blind contribution tokens** — pact partners issue blind tokens for passing storage challenges. Accumulate tokens for priority. Rejected: significant protocol complexity (blind signatures, anonymous credentials), gameable via colluding pact partners.

**Public reputation tiers** — coarse-grained public contribution level attested by threshold signature. Rejected: reveals storage participation metadata, requires threshold signature scheme, still gameable.
