# ADR 014: Presence-Aware Reliability Scoring

**Date:** 2026-07-29
**Status:** Accepted

## Context

Reliability scoring drove pact retention: a partner that failed data-availability challenges lost its score and was eventually replaced. The original design used an exponential moving average (α = 0.95) over **all** issued challenges, online or not.

The [game-theory review](../design/reviews/review-game-theory.md) (Issue 3) showed this quietly kills a persona. A **Witness** is a light node with ~30% (mobile) uptime by design — it is *supposed* to be offline most of the time. Scoring every challenge issued while the Witness is asleep as a failure makes an honest, well-behaved Witness algorithmically indistinguishable from a defector. The current spec effectively removes one of the five personas from the trust model.

## Decision

Adopt **presence-aware, window-based** reliability scoring:

- Only challenge a partner **while it is observed online** (iroh gives cheap presence signals; a partner that is unreachable is not challenged).
- Score = `passes ÷ challenges-issued-while-online`, over a rolling **14-day window**.
- Bands (canonical, used everywhere): **Healthy ≥90% · Degraded 70–90% · Unreliable 50–70% · Failed <50%**.
- Actions: Degraded → increase challenge frequency and begin seeking a replacement; Unreliable → actively replacing; **drop only when Failed is sustained ≥14 days**.

This is a **mandatory** part of a conforming client, not an optimization: without it, Witnesses cannot participate.

## Consequences

**Positive:**
- Rescues the Witness persona — a 30%-uptime light node that answers every challenge *it can* scores as Healthy.
- Distinguishes "offline" (fine) from "online but not serving" (the actual defection).
- The 14-day sustained-Failed rule prevents thrashing on transient outages.

**Negative:**
- Requires a presence signal; a partner that is genuinely unreachable for the whole window is indistinguishable from one that is offline-by-choice. Mitigated by the ~20-pact redundancy target.
- A partner could game presence by only appearing online briefly; the online-challenge sampling and window bound the benefit.

**Neutral:**
- Replaces the four-band tables that previously disagreed across five documents (see [ADR 019](019-documentation-simplification.md)); the [pact state machine](../protocol/pact-state-machine.md) is the normative home for the bands.
