# ADR 018: Honest Security and Relay Framing

**Date:** 2026-07-29
**Status:** Accepted

## Context

The [cryptography review](../design/reviews/review-cryptography.md), [protocol-design review](../design/reviews/review-protocol-design.md), and a 2026-07 soundness pass found several documentation claims that were contradicted by the protocol's own spec or its own simulator. These are not design flaws — the underlying mechanisms are fine for their threat model — but the *framing* overstated them in ways that would embarrass the project the moment a reader ran the simulator or read the spec. This ADR records the deliberate decision to describe the system honestly, and one genuine spec change that follows from it.

## Decision

### Documentation truth-fixes (adopted)

1. **"Forward secrecy" → "periodic DM key rotation."** DM keys rotate every 90 days, which bounds the blast radius of a *device or epoch-key* compromise — but it is **not** forward secrecy: epoch keys are `KDF(root, "dm-…" || epoch)`, so root-key compromise yields every past and future epoch key.
2. **"Excellent compartmentalization" → "one-directional compartmentalization."** Compromising a device subkey does not expose the root or other subkeys. The reverse does **not** hold: root derives everything.
3. **Availability.** The analytical `~1.48×10⁻⁹` ("one-in-a-billion") unavailability is an **upper bound for a stable pact set**, presented alongside the simulated **94.8–98.4%** retrieval success (1.6–5.2% failure) that the project's own 30-day runs measure. Lead with the simulated number.
4. **Relays are "reduced, not optional."** Relays remain permanent infrastructure with a *reduced control surface*: discovery beyond the WoT, bootstrap, mobile-to-mobile mailbox, push delivery, and 0.2–21% of reads depending on topology and pact maturity. Relay-as-Keeper is the intended Genesis deployment model, not a failure condition.
5. **"Proof of storage" → "data availability verification."** Challenges prove a partner can *produce* data on demand, not that it stores a local copy (a well-provisioned proxy passes both challenge types).
6. **"Validated by simulation" → "stress-tested in simulation,"** naming the two open problems the simulator surfaced: transient low-redundancy windows from pact churn, and (pre-[ADR 015](015-renewal-by-default-pacts.md)) net-negative pact formation.
7. **Read privacy** comes from *where* metadata flows (WoT peers, not arbitrary relays), not from requester anonymity (see [ADR 017](017-three-tier-retrieval.md)).

### Spec change (adopted)

**Independent DM seed.** The DM key is derived from its **own** seed, not from the identity root. This gives DMs a genuine third compartment: root compromise no longer yields all DM history. This mirrors the independent Ed25519 transport key already adopted in [ADR 011](011-iroh-transport-integration.md).

## Consequences

**Positive:**
- The docs survive contact with a skeptical reader and with the simulator.
- The independent DM seed closes the concrete "root compromise reads all DMs forever" failure mode.

**Negative:**
- The honest numbers are less flattering in isolation; the competitive story now rests on sovereignty-as-gradient and metadata locality, which is the true differentiator anyway.
- The independent DM seed adds one more key to back up in recovery flows.

**Neutral:**
- These wordings are propagated across the whitepaper, README, `identity.md`, `key-management-ux.md` (merged/kept), and the privacy model; see [ADR 019](019-documentation-simplification.md) for the file-level record.
