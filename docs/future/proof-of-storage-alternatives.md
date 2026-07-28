# Data Availability Verification: Design Analysis

**Date:** 2026-03-12
**Status:** Draft

## Overview

This document evaluates the current proof-of-storage mechanism and its alternatives. The goal is to understand the design space, be honest about what the current approach does and doesn't guarantee, and assess whether stronger primitives are justified for Gozzip's threat model.

## Design decision

We evaluated six alternative proof-of-storage mechanisms and rejected all of them for the current protocol version. The current design — hash challenges + serve challenges with latency measurement + exponential moving average reliability scoring — is retained as the best fit for Gozzip's specific threat model and constraints.

**Why:** Gozzip's pact partners are WoT peers on consumer devices storing KB-MB of data, not anonymous miners on data centers storing TB. The threats are lazy friends and occasional bad actors, not adversarial miners optimizing against financial incentives. Every alternative we evaluated either solves a harder problem than we have (PoR, erasure coding, compact proofs), adds complexity without proportional benefit (Merkle trees), or weakens the guarantee unacceptably (reputation-only).

Two alternatives are noted as future considerations:
- **Merkle tree challenges** could incrementally improve diagnostics (identify *which* event is missing, not just which range) without a redesign
- **Periodic full sync** as a monthly audit complement is practical at current storage volumes

The detailed analysis of each alternative, including what it would buy, what it would cost, and why it was rejected, follows below. This trade-off analysis is intentionally preserved as a design record.

## Current design

Gozzip uses two complementary challenge types to verify that pact partners store data.

**Note:** This mechanism verifies that a peer can produce the data on demand ("proof of accessibility"), not that they store it locally. A well-provisioned proxy could pass all challenges by fetching data from another source. The term "proof of storage" is used loosely in this document; the formal guarantee is data accessibility within the challenge response window.

The two challenge types are:

### Hash challenge

The challenger specifies an event sequence range and a nonce. The peer returns `H(events[i..j] || nonce)`. This proves the peer possesses the events in that range without transferring them.

**What it proves:** The peer has the exact bytes of events i through j at the time of the challenge.

**What it does not prove:**
- That the peer stored them *before* the challenge (they could fetch on demand and hash)
- That the peer will store them *after* the challenge (they could delete immediately)
- That the peer stores the *complete* dataset (unchallenged ranges might be missing)

### Serve challenge

The challenger requests a specific event and measures response latency. Responses over 500ms suggest the peer is proxying — fetching from elsewhere rather than serving from local storage.

**What it proves:** The peer can produce the event within a latency bound.

**What it does not prove:**
- That the data is stored locally (a well-provisioned proxy with low-latency upstream could beat 500ms)
- That the event was stored before the request (same on-demand fetch concern)
- That the 500ms threshold is correct (varies by network conditions, device performance, geographic distance)

### Reliability scoring

Results feed into an exponential moving average (α=0.05) over a 30-day effective window:

| Score | Status | Action |
|-------|--------|--------|
| ≥90% | Healthy | No action |
| 70-90% | Degraded | Increase challenge frequency |
| 50-70% | Unreliable | Begin replacement negotiation |
| <50% | Failed | Drop immediately, promote standby |

### Honest assessment of current design

**Strengths:**
- Simple to implement and reason about
- Low bandwidth overhead (hash of range, not the range itself)
- Works well against lazy peers who simply don't bother storing
- Serve challenge adds a second dimension (latency) beyond possession

**Weaknesses:**
- Hash ranges provide probabilistic coverage, not completeness — unchallenged ranges could be missing
- The 500ms latency threshold is a heuristic that varies with network topology
- On-demand fetching defeats both challenge types if the proxy is fast enough
- Cannot distinguish "offline" from "cheating" for light nodes at 30% uptime
- No formal security reduction — the scheme is intuitive, not proven

## Alternatives

### 1. Proof of Retrievability (PoR)

**How it works:** Before handing data to a pact partner, the owner encodes it with error-correcting redundancy and embeds sentinel values at random positions. To challenge, the verifier asks for specific sentinels. If the prover has deleted or corrupted even a small fraction of the data, the sentinels will be missing or incorrect with high probability.

The key insight: because of the error-correcting encoding, the verifier can prove the *entire* file is recoverable from a bounded number of challenges — not just that specific pieces exist.

**Formal guarantee:** After k challenges, the probability that the prover has deleted more than a fraction ε of the data and not been caught is at most (1-ε)^k. With k=20 challenges and ε=0.05, the detection probability is >64%. With k=100, it's >99.4%.

**What this buys Gozzip:**
- Completeness guarantee: if challenges pass, the full dataset is recoverable (not just the challenged ranges)
- Formal security reduction: provably hard to cheat under standard assumptions
- Bounded number of challenges for probabilistic completeness (current design has no such bound)

**What this costs:**
- **Pre-processing:** The owner must encode data before handing it to the partner. For Gozzip's event-by-event model (events arrive continuously), this means either re-encoding periodically or using an incremental PoR scheme (more complex)
- **Sentinel management:** The verifier must remember which sentinels were embedded and where. Adds state that must be synced across the owner's devices
- **Complexity:** More code, more edge cases, more opportunities for implementation bugs
- **Incompatible with event-stream model:** PoR was designed for static files, not append-only event streams. Adapting it to continuous event publication requires either periodic re-encoding or a streaming PoR construction (active research area, no mature implementations)

**Verdict:** Stronger guarantees than hash challenges, but the static-file assumption is a poor fit for Gozzip's append-only event stream. The pre-processing and sentinel management overhead may not be justified when storage volumes are small and 20-peer redundancy limits the impact of any single peer cheating.

### 2. Merkle tree challenges

**How it works:** Both parties maintain a Merkle tree over all stored events. The challenger picks a random leaf index, the prover returns the leaf plus the Merkle authentication path. Verification is O(log n) — check the path from leaf to root.

**What this buys Gozzip:**
- Individual event verification (not range-based — can challenge any single event)
- Efficient proofs: O(log n) verification, ~32 bytes per hash in the path
- Well-understood primitive with mature implementations
- Naturally incremental: new events are appended as new leaves, tree is extended

**What this costs:**
- **Tree synchronization:** Both parties must agree on the tree structure. When new events are published, the tree changes. The prover and verifier need to sync their view of the current root hash. This adds a coordination step that hash-range challenges don't need
- **Root hash agreement:** If the two parties disagree on the root, challenges are meaningless. This requires a secure channel for root exchange (already exists via NIP-46, but adds a protocol round)
- **Same probabilistic coverage as hash challenges:** A random leaf challenge covers 1/n of the tree. To achieve high coverage, you need many challenges — same as hash ranges. The Merkle structure doesn't inherently improve coverage; it improves *per-challenge verification cost*

**Verdict:** Better structure than hash ranges, with lower verification cost per challenge. But the coverage problem is identical — you still need enough challenges to cover the dataset probabilistically. The main advantage is that the Merkle tree makes it easy to pinpoint *which* events are missing when a challenge fails, rather than just knowing "something in range [i,j] is wrong." This could improve the current design incrementally without a full redesign.

### 3. Erasure coding + data availability sampling

**How it works:** The owner encodes their data using erasure coding (e.g., Reed-Solomon) into n fragments, of which any k are sufficient to reconstruct the original. Fragments are distributed across pact partners. To verify, the verifier samples random fragments and checks they decode correctly.

This is the approach used by Ethereum's data availability sampling (DAS) for blob data, and by Filecoin for proof of storage.

**What this buys Gozzip:**
- **Resilience without full replication:** Instead of 20 peers each storing a full copy, 20 peers each store 1/10th of the coded data, and any 10 can reconstruct. Storage cost per peer drops dramatically
- **Stronger availability guarantee:** Losing 10 of 20 peers doesn't cause data loss (vs. current design where all 20 store full copies — more redundant but also more wasteful)
- **Data availability proofs:** If the encoding uses KZG polynomial commitments, you get formal proofs that the data is available without downloading it all

**What this costs:**
- **Fundamentally different pact model:** A pact partner no longer holds your complete data. They hold a fragment. This changes the retrieval model — to reconstruct, you need k fragments from k different peers, not just any single peer. Latency for retrieval increases because you need multi-peer coordination
- **Encoding complexity on consumer devices:** Reed-Solomon encoding/decoding is CPU-intensive. KZG commitments require elliptic curve operations. These are feasible on desktop but expensive on mobile devices
- **Fragment management:** When new events are published, new fragments must be generated and distributed. The encoding is not naturally incremental — you either re-encode periodically or use a streaming erasure code (complex)
- **Breaks the social model:** A pact means "you store my data, I store yours." Erasure coding turns this into "you store fragment 7 of my data." The reciprocity model becomes harder to reason about — volume matching now operates on fragment sizes, not event counts

**Verdict:** Solves a different problem. Gozzip's storage volumes are small enough (KB-MB per user per month) that full replication across 20 peers is affordable. Erasure coding trades storage efficiency for retrieval complexity — a trade-off that makes sense at blockchain scale (GB-TB) but is premature for Gozzip's current sizes. If storage volumes grow significantly (e.g., media attachments, large files), this becomes worth revisiting.

### 4. Reputation-only (no cryptographic proofs)

**How it works:** Don't challenge at all. Track empirical delivery: when someone requests your data through gossip and a pact partner serves it, that's evidence they have it. When they fail to serve, that's evidence they don't. Reliability score is based on observed delivery success rate.

**What this buys Gozzip:**
- **Simplicity:** No challenge protocol, no hash computation, no latency measurement
- **Zero overhead:** No bandwidth or computation spent on proofs. All resources go to actual data serving
- **Natural alignment:** The only thing that matters is "can my data be retrieved?" — and this approach directly measures exactly that

**What this costs:**
- **Reactive, not proactive:** You discover a peer lost your data when someone needs it and it's not there. If multiple peers fail simultaneously, data could be lost before you notice
- **No coverage of rarely-requested data:** If nobody requests your old events, you never discover they've been deleted. The challenge system proactively catches this; reputation-only does not
- **Gaming:** A peer could serve recent events (which are requested often) while deleting old events (rarely requested). The reputation score stays high because recent requests succeed

**Verdict:** Attractive for its simplicity but dangerously reactive. The current proactive challenge system catches data loss before it affects availability. Reputation-only is acceptable as a *complement* to challenges (and Gozzip already uses reliability scoring this way), but not as a *replacement*.

### 5. Compact proofs (RSA accumulators / polynomial commitments)

**How it works:** The pact partner maintains a cryptographic accumulator over all stored events. The verifier can check membership (or non-membership) in the accumulator with a single operation. RSA accumulators use modular exponentiation; polynomial commitments (KZG) use elliptic curve pairings.

**What this buys Gozzip:**
- **Constant-size proofs:** Regardless of how many events are stored, the proof is a single group element (~32-256 bytes)
- **Batch verification:** Multiple membership proofs can be verified in a single operation
- **Non-membership proofs:** Can prove that a specific event is *not* in the set (useful for detecting selective deletion)

**What this costs:**
- **Computational expense:** RSA accumulator updates require modular exponentiation over large moduli (~2048 bits). KZG commitments require trusted setup or a powers-of-tau ceremony. Both are heavy for mobile devices
- **Trusted setup (KZG):** Requires a one-time setup phase that produces public parameters. If the setup is compromised, proofs can be forged. RSA accumulators avoid this but are slower
- **Overkill for scale:** These primitives are designed for datasets with millions of elements. Gozzip's per-user event counts are in the hundreds to low thousands per month. The overhead of maintaining an accumulator exceeds the overhead of simple hash challenges at this scale

**Verdict:** Elegant but disproportionate to the problem. The marginal improvement in proof size and verification cost doesn't justify the computational and implementation complexity at Gozzip's current scale.

### 6. Periodic full sync verification

**How it works:** At regular intervals (weekly, monthly), the owner requests a full transfer of all stored events from each pact partner. Compare byte-for-byte against local copy or known hashes.

**What this buys Gozzip:**
- **Complete verification:** No probabilistic gaps. Every event is checked
- **Simple to implement:** Just a bulk data transfer + comparison
- **No cryptographic overhead:** No hash challenges, no accumulators, no encoding

**What this costs:**
- **Bandwidth:** For 20 pact partners, a monthly full sync means transferring 20 × (monthly data volume). At 675 KB/month for an active user, that's ~13.5 MB/month — actually feasible
- **Scales poorly with history:** The 675 KB/month figure is new events only. A user with 2 years of history might have ~16 MB stored per pact partner. Full sync of all 20 = ~320 MB/month. Still feasible but growing
- **Timing exposure:** Full syncs are large, distinctive transfers. A network observer can identify when sync verification is happening. This is a privacy cost the challenge system avoids (challenges are small and blend with normal traffic)

**Verdict:** Surprisingly practical for Gozzip's current scale. The bandwidth numbers are within consumer limits. The main objection is that it scales linearly with history size and creates distinctive traffic patterns. Could work as an occasional complement to the regular challenge system — e.g., a monthly full audit alongside daily random challenges.

## Comparison matrix

| Approach | Completeness | Overhead | Complexity | Fits event stream? | Fits mobile? |
|----------|-------------|----------|------------|-------------------|-------------|
| Hash challenges (current) | Probabilistic (range-based) | Low | Low | Yes (ranges) | Yes |
| Serve challenge (current) | Single event + latency | Low | Low (but heuristic) | Yes | Yes |
| Proof of Retrievability | Provable (bounded challenges) | Medium (pre-processing) | High | Poor (static file model) | Maybe |
| Merkle tree challenges | Probabilistic (per-leaf) | Low (O(log n) per proof) | Medium | Yes (append-only tree) | Yes |
| Erasure coding + DAS | Provable (sampling) | High (encoding/distribution) | High | Poor (re-encoding needed) | No |
| Reputation-only | Empirical (reactive) | Zero | Very low | Yes | Yes |
| Compact proofs (RSA/KZG) | Set membership | High (computation) | Very high | Yes (accumulator updates) | No (expensive) |
| Periodic full sync | Complete (byte-for-byte) | High (bandwidth) | Very low | Yes | Maybe (bandwidth) |

## Recommendations

### What to keep

The current hash challenge + serve challenge + reliability scoring design is well-suited to the threat model. The threats are lazy peers and occasional bad actors, not adversarial miners optimizing against a billion-dollar incentive. The scheme is simple, low-overhead, and works on mobile devices.

### What to consider adding

1. **Merkle tree structure.** Replacing hash-range challenges with Merkle tree challenges would provide the same probabilistic coverage with better diagnostics — when a challenge fails, you know *which event* is missing, not just that "something in range [i,j] is wrong." The append-only tree model fits Gozzip's event stream naturally. This is an incremental improvement, not a redesign.

2. **Periodic full sync as audit.** A monthly full sync alongside daily challenges would catch edge cases that random sampling misses (e.g., a peer who deletes only very old events that are rarely challenged). The bandwidth cost is practical at current volumes. This degrades gracefully — if bandwidth is constrained, skip the full sync and rely on random challenges.

3. **Challenge range bias toward old data.** Challenge range selection should be biased toward older data: 50% of challenges target data >30 days old, 30% target 7-30 day range, 20% target last 7 days. This counters selective deletion of old, rarely-challenged data.

### What to avoid for now

1. **Proof of Retrievability.** The static-file assumption requires awkward adaptation for append-only event streams. The formal guarantees are appealing but the implementation complexity isn't justified until storage volumes grow significantly.

2. **Erasure coding.** Solves the wrong problem at current scale. Full replication across 20 peers is affordable for KB-MB data. Erasure coding adds retrieval complexity for a storage efficiency gain that isn't needed yet.

3. **Compact proofs (RSA/KZG).** Computationally expensive, requires trusted setup (KZG), and optimizes for proof size at scales Gozzip hasn't reached.

### The real vulnerability

The hardest problem in Gozzip's proof-of-storage isn't the proof mechanism — it's the **offline/cheating ambiguity** for light nodes.

A Witness at 30% uptime fails challenges 70% of the time by definition. A lazy Keeper who doesn't bother storing data also fails challenges frequently. The reliability scoring system treats both the same — degraded, then unreliable, then dropped. But one is a reliable partner who happens to be a phone, and the other is a bad actor.

No proof scheme solves this. It's a classification problem:

- **Offline pattern:** Fails challenges during known offline hours (phone locked, no network), passes when online
- **Cheating pattern:** Fails challenges randomly regardless of connectivity, or consistently fails for old events while passing for recent ones

The protocol could improve by tracking *when* and *what* fails, not just *how often*. A Witness that passes 100% of challenges when online but is offline 70% of the time is a good partner. A Keeper that fails 30% of challenges despite 95% uptime is suspicious. The current exponential moving average doesn't capture this distinction.

This is where the next iteration of the design should focus — not on stronger cryptography, but on better classification of failure patterns.
