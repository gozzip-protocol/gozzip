# ADR 016: Target-Based Pact Formation

**Date:** 2026-07-29
**Status:** Accepted

## Context

Pact formation was specified as an **equilibrium-seeking** process: a "comfort condition" evaluated as a Poisson-binomial convolution over 24 hourly availability windows, a `PACT_FLOOR`/`PACT_CEILING` band (12–40), a marginal-value calculation, and a six-state formation machine with hysteresis. The [complexity review](../design/reviews/review-complexity.md) (§Simplify-3) and the [game-theory review](../design/reviews/review-game-theory.md) (§6) both found this far heavier than the result justifies — simulations already showed 94–98% availability under much simpler formation rules — and the formation machine's states (Growing, Comfortable, Over-Provisioned…) shadowed the narrative's own Bootstrap→Hybrid→Sovereign arc with a second, differently-named lifecycle.

## Decision

Replace equilibrium-seeking with a **target-based** rule:

- Form pacts until **~20 active** (floor **12**); replace on failure.
- Prefer partners in **different timezones** and a **mix of Keepers and Witnesses** (Keeper ratio ≥15%) — stated as a **heuristic**, not an optimization to converge on.
- No comfort-condition math, no ceiling-seeking, no marginal-value computation.

**Dissolve the formation state machine into the three-phase adoption arc.** The arc *is* the formation lifecycle: Bootstrap (0–5 pacts, relay-primary) → Hybrid (5–15, mixed) → Sovereign (15+, peer-primary). Only two lifecycles remain protocol-wide: this arc and the four-state pact FSM ([ADR 015](015-renewal-by-default-pacts.md)).

20 is retained as the modeling constant so existing simulation evidence stays valid.

## Consequences

**Positive:**
- Removes the most mathematically elaborate, least-load-bearing component of the spec.
- The user-facing maturation story (the narrative spine) becomes the *only* lifecycle a reader must hold — no shadow machine.
- Keeps the one narrative-bearing insight from the old model — Keepers accept pacts beyond their own comfort so their excess capacity carries Witnesses — as prose in the [incentives](../design/incentives.md) doc.

**Negative:**
- A flat target is less adaptive than an equilibrium model in principle. In practice the simulator shows the adaptivity bought little, and diversity heuristics recover most of it.

**Neutral:**
- `PACT_FLOOR = 12` survives as the floor; `PACT_CEILING` and the comfort condition are removed.
