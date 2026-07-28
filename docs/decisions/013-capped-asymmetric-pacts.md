# ADR 013: Capped Asymmetric Pacts (Replacing Volume Matching)

**Date:** 2026-07-29
**Status:** Accepted

## Context

The original storage-pact design required **volume matching**: two partners had to produce a comparable volume of events (within ±30% tolerance), with continuous monitoring and renegotiation when the ratio drifted. The [game-theory review](../design/reviews/review-game-theory.md) (Issues 1–2) and the [protocol-design review](../design/reviews/review-protocol-design.md) (Issue 4) identified two fatal problems:

1. **It metered friendship.** Real reciprocal relationships tolerate asymmetry ("I help you move, you feed my cat"). Volume matching dissolved pacts over ordinary changes in human posting rhythm, and it was the single largest driver of the net-negative pact churn observed in *every* simulated topology.
2. **It excluded the 60–80% of users who are lurkers.** A user who rarely posts had almost nothing to offer a volume-matched partner, so the majority of the network could not enter the pact economy at all — contradicting the very community the protocol's narrative is about.

## Decision

Replace volume matching with **capped asymmetric pacts**:

- Each partner agrees to store whatever the other produces, **up to a cap of ~10 MB/month per partner** (roughly a month of a heavy user's text events).
- No ±30% band, no volume monitoring, no drift-triggered renegotiation.
- A pact ends only on sustained unreliability ([ADR 014](014-presence-aware-reliability.md)) or explicit exit ([ADR 015](015-renewal-by-default-pacts.md)) — never on volume asymmetry.
- Lurkers store for their more-active friends and are stored in return; the cap bounds the worst case.

## Consequences

**Positive:**
- Removes the dominant churn driver; combined with [ADR 015](015-renewal-by-default-pacts.md) the pact set becomes a ratchet toward stability rather than a slow-motion collapse.
- Readmits the lurker majority to the pact economy — the audience is part of the community.
- Simpler client logic: no volume accounting, no renegotiation state.
- Strengthens the reciprocity-as-infrastructure narrative instead of contradicting it.

**Negative:**
- A heavy poster paired with a pure lurker gets less redundancy from that specific pact than a symmetric pact would have given. Mitigated by the ~20-pact target ([ADR 016](016-target-based-pact-formation.md)) and Keeper-ratio floor.
- The 10 MB/month cap is a parameter to tune with production data.

**Neutral:**
- Existing simulation evidence used the 20-pact model; the cap does not change the availability results, only the formation/dissolution dynamics.
