# ADR 019: Documentation Simplification (Cut / Defer / Merge)

**Date:** 2026-07-29
**Status:** Accepted

## Context

Five adversarial reviews ([complexity](../design/reviews/review-complexity.md), [protocol-design](../design/reviews/review-protocol-design.md), [game-theory](../design/reviews/review-game-theory.md), [cryptography](../design/reviews/review-cryptography.md), [deployment-reality](../design/reviews/review-deployment-reality.md)) plus a 2026-07 four-perspective pass converged on one diagnosis: the protocol's **core** (bilateral storage pacts inside a Web of Trust, verified by challenges, narrated as community formation) is strong, but the **periphery** (gossip machinery, equilibrium math, media pacts, versioning strategy, moderation pipelines, monitoring suites, interplanetary compatibility) was ~60–70% of the specification for a small fraction of the value. A protocol at zero users does not need this much surface.

This ADR records the structural simplification. The hard constraint throughout: **the community-evolution narrative is preserved and, where possible, strengthened** — the five personas (Keeper/Witness/Guardian/Seedling/Herald), the Community Parallel, the Bootstrap→Hybrid→Sovereign arc, and the Inner Circle/Orbit/Horizon feed tiers are untouched or promoted.

## Decision

### Cut (deleted)
- **Interplanetary compatibility** (`design/interplanetary.md`) — the document itself declared this premature.
- **Blocklist federation** (kind 10070-as-blocklist) — NIP-51 mute lists suffice at this scale.
- **Legal/DPIA analysis** embedded in the GDPR doc — belongs to counsel, not a protocol spec.

### Event kinds removed
- **10059** (storage endpoint hints) — iroh handles addressing.
- **10066** (guardianship completion) — the graduation *rite* survives as prose; the event does not.
- **10067** (notification relay advertisement) and **20062** (wake-up hint) — discovery infra for a market of 1–2 relays.
- **10070-as-blocklist** — 10070 is now unambiguously the iroh cross-key binding.

This also resolves two live **kind collisions**: 10065 is now solely Temporary Suspension (Media Manifest deferred), and 10070 is solely the iroh binding. The canonical registry lives in [`protocol/messages.md`](../protocol/messages.md).

### Deferred (moved to [`docs/future/`](../future/README.md), content frozen)
Media layer, protocol-versioning strategy, BitChat/BLE mesh, post-quantum roadmap, proof-of-storage alternatives survey, spam-resistance analysis, monitoring & diagnostics suite. The one adopted item from proof-of-storage (Merkle-proof challenges) moved into [`storage.md`](../architecture/storage.md).

### Merged
- Moderation + moderation-framework + GDPR deletion mechanics → one [`design/moderation.md`](../design/moderation.md).
- Incentive model + equilibrium-pact-formation + guardian-incentives → one [`design/incentives.md`](../design/incentives.md).
- Surveillance-surface + relay-diversity + gossip-privacy → one [`design/privacy-model.md`](../design/privacy-model.md).
- The three iroh docs → one [`architecture/iroh.md`](../architecture/iroh.md).
- The two dated `plans/` feed-model docs → archived under `docs/future/history/`; [`architecture/feed-model.md`](../architecture/feed-model.md) is the sole live telling.

### Consistency fixes
The 2026-07 pass also resolved 34 documentation-integrity findings (kind-range drift, reliability-band contradictions, retrieval-tier renumbering, transport-documentation inconsistencies, orphaned docs, a karma economy left in the test plan) and 24 soundness findings; the mechanism changes are recorded in [ADR 013](013-capped-asymmetric-pacts.md)–[ADR 018](018-honest-security-and-relay-framing.md).

## Consequences

**Positive:**
- Core reader-facing spec shrinks from ~121k words to ~55–60k; newcomer required reading drops from ~15 documents to ~6 (vision → glossary → system-overview → storage → messages → pact-state-machine).
- Event kinds 20 → 16; pact states 7 → 4; retrieval tiers 5 → 3; overlapping lifecycle machines 3 → 2.
- Every deferred idea survives under `docs/future/` — nothing good is lost, only relocated.

**Negative:**
- Contributors must learn the new layout; deferred docs may drift from the core over time (mitigated by the `docs/future/` banner marking them as history).

**Neutral:**
- The five adversarial reviews are retained verbatim as the record of *why*; this ADR is their adjudication — which recommendations were adopted, deferred, or declined.
