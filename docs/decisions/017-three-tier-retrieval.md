# ADR 017: Three-Tier Retrieval; Gossip as an Optional Extension

**Date:** 2026-07-29
**Status:** Accepted

## Context

The read path was a five-tier cascade: BLE mesh → cached endpoints (kind 10059) → gossip (rotating request tokens, kind 10057 broadcast) → storage peers via DVM → relay fallback. The [protocol-design review](../design/reviews/review-protocol-design.md) (Issue 1) measured the payoff: **gossip delivers 0.1–9% of reads** (0.0–0.1% in the checked-in 2K sweeps) while imposing the single largest share of client complexity — WoT graph computation per forwarded message, TTL/fanout/dedup, rate limiting, rotating-token math. The dominant path is pact-local reads (74–92%); relay fallback handles the rest. Two of the five tiers are also independently deferred: **BLE** (no iroh BLE transport, [ADR 011](011-iroh-transport-integration.md)) and **cached endpoints** (kind 10059 is redundant now that iroh handles peer addressing and hole-punching below the protocol).

Separately, the [cryptography review](../design/reviews/review-cryptography.md) (Issue 8) found the **rotating request token** (`H(target || YYYY-MM-DD)`) to be "worse than no token": trivially reversible by anyone who knows the target's pubkey, and adding a day-boundary correlation vector plus a false sense of privacy.

## Decision

**Three tiers:**

1. **Local** — the reader's own pact storage and read-cache.
2. **Partner query** — a direct, **NIP-44-encrypted** request (kind 10057) to the author's known pact partners, addressed over iroh. No broadcast, no token.
3. **Relay** — fallback query, as in Nostr.

**Gossip is demoted to an optional censorship-resistance extension**, not a required tier: a conforming client need not implement WoT-filtered forwarding. It remains the paper's conceptual frame (§2.5, "gossip as curated propagation") and is available where broadcast discovery is genuinely needed.

**Rotating request tokens are removed.** The kind-10057 request is encrypted directly to a chosen storage peer; addressing comes from iroh, not from a pseudonymous broadcast lookup. Reader privacy relative to vanilla Nostr comes from *where* metadata flows (WoT peers, not arbitrary relay operators), not from requester anonymity — the requester is always identified (requests are signed and the reply routes to their pubkey).

## Consequences

**Positive:**
- Removes the largest complexity-for-value mismatch in the protocol.
- Kills a privacy mechanism that was actively misleading; the honest story is simpler and truer.
- kind 10059 (endpoint hints) removed; BLE cleanly deferred.

**Negative:**
- Losing gossip-as-default removes a censorship-resistance path for the small fraction of reads it served. Retained as an opt-in extension for high-adversity deployments.
- "Partner query" depends on knowing the author's pact partners; this is discoverable from the author's own published pact events.

**Neutral:**
- The whitepaper's retrieval section is re-tiered; the "four/five-tier" language is corrected to "three-tier" everywhere.
