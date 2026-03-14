# Adversarial Review: Cryptographic Soundness & Network Topology

**Date:** 2026-03-14
**Scope:** Combined Agent 01 (Cryptographic Soundness) and Agent 02 (Network Topology)
**Documents reviewed:** Whitepaper v1.2, identity.md, messages.md, pact-state-machine.md, surveillance-surface.md, key-management-ux.md, relay-diversity.md
**Verdict:** The protocol is architecturally sound with several strong design choices, but contains a handful of issues ranging from critical (HKDF key derivation leaking subkey linkability) to significant (epidemic threshold argument circularity, proof-of-storage weakness) that should be addressed before production deployment.

---

## Executive Summary

Gozzip presents a well-considered design that maps social trust onto storage infrastructure, inherits Nostr's battle-tested identity primitives, and layers on a key hierarchy, pact mechanism, and gossip routing scheme grounded in real network theory. The cryptographic primitives are generally chosen correctly (secp256k1, HKDF-SHA256, NIP-44 AEAD, SHA-256 Merkle trees), and the network-theoretic arguments are cited accurately. However, a close reading reveals several issues: the HKDF derivation scheme creates deterministic linkage between root, governance, and DM keys that undermines the compartmentalization goal; the epidemic threshold argument conflates WoT-filtered forwarding with a formal spectral radius bound without proving the claim; the proof-of-storage mechanism is acknowledged as weak but the 500ms latency heuristic is weaker than acknowledged; and the social recovery scheme has a subtle race condition during the timelock window. These are fixable issues in a fundamentally sound architecture.

---

## Issues

### 1. HKDF Deterministic Key Linkage Undermines Compartmentalization

**Severity:** Critical

**Description:**

The key hierarchy derives governance and DM keys deterministically from the root key via HKDF-SHA256 with fixed, publicly known info strings (`"governance"`, `"dm-decryption-" || rotation_epoch`). This means any party who obtains the root private key can immediately compute every derived key (governance, DM for every rotation epoch past and future). This is by design for usability (the root key is the master secret), but it creates a problem the documentation does not address: **the public keys of derived keys are deterministically linked to the root public key by anyone who knows the derivation scheme.**

Specifically, an adversary who knows a user's root public key and the HKDF parameters cannot derive the private keys (that requires the root private key). However, the scheme's determinism means that if the adversary ever observes a DM key or governance key in a kind 10050 event (which is published publicly), they can verify a candidate relationship: given a root pubkey, derive the expected governance pubkey via the public derivation path and check if it matches the one in kind 10050. Since HKDF with a secp256k1 scalar does not have a public-key equivalent of the derivation (you cannot compute `HKDF(root_privkey, info)` from `root_pubkey` alone), this specific attack does not work directly.

The real issue is different and more subtle: **all derived keys are computable from the root private key, so compromise of the root key is total compromise.** The identity.md document presents the key hierarchy as providing "compartmentalization," but the derivation direction means compartmentalization only works upward (device compromise does not yield root), never downward (root compromise yields everything). This is standard HD key design, but the documentation should be explicit that root compromise is catastrophic and retroactive -- including all past DM keys for every rotation epoch, which defeats the forward secrecy claim.

More critically: the DM key derivation uses `"dm-decryption-" || rotation_epoch`, where `rotation_epoch = floor(unix_timestamp / (rotation_period * 86400))`. An adversary who compromises the root key at any point can compute DM keys for every past and future epoch. The "bounded forward secrecy" claim (compromise reveals at most ~97 days of DMs) is **only true for device compromise, not root key compromise.** The whitepaper and identity.md do not make this distinction clearly enough.

**Recommendation:**

1. Explicitly document that root key compromise defeats all forward secrecy guarantees for DM keys. The 90-day bounded exposure window applies exclusively to device-level compromise where only the current DM private key is extracted, not the root key.
2. Consider whether the DM key should use a ratcheting scheme (e.g., a hash chain where each epoch's key is derived from the previous epoch's key, not from root) so that root compromise only reveals future keys, not past ones. This would require storing the current DM key seed independently from the root key, which complicates the UX model but provides genuine forward secrecy.
3. At minimum, add a warning to the "Compromised Device Impact" table for the root key row: "Reveals all past and future DM keys (all rotation epochs)."

---

### 2. Epidemic Threshold Argument Contains Circular Reasoning

**Severity:** Significant

**Description:**

Section 3.3 of the whitepaper argues that WoT-only forwarding restores a non-zero epidemic threshold on what would otherwise be a threshold-free scale-free network. The argument cites Castellano and Pastor-Satorras (2010) to claim that the epidemic threshold is governed by the spectral radius of the "realized WoT subgraph," and that this spectral radius is "substantially smaller" than that of the full network because WoT filtering "removes the long-range shortcuts that hub nodes would otherwise provide."

This argument has two problems:

First, **no bound on the spectral radius of the WoT subgraph is computed or cited.** The claim that the WoT subgraph has a smaller spectral radius is stated as intuitive but not proven. In a social network where mutual follows are common among high-degree nodes (which is realistic -- popular accounts often follow each other), the WoT subgraph may preserve much of the hub structure. The spectral radius of the WoT subgraph could still be large enough that the effective epidemic threshold remains near zero for practical purposes.

Second, the argument is partially circular: it claims WoT filtering bounds propagation, then cites TTL=3 as an "independent bound" that caps the reachable set. But the TTL is a design parameter, not a consequence of the WoT structure. The honest statement is: "TTL=3 is the mechanism that prevents runaway gossip; WoT filtering provides a complementary social-trust filter." The paper instead presents TTL and WoT as dual mechanisms that together "restore" a theoretical epidemic threshold, which conflates a protocol-level hop limit with a graph-theoretic property.

**Recommendation:**

1. Either compute or bound the spectral radius of a realistic WoT subgraph (e.g., from the simulation's BA graph with mutual-follow filtering) and show that the resulting epidemic threshold is meaningfully non-zero, or soften the claim to: "WoT filtering and TTL=3 together limit gossip propagation to a bounded neighborhood. We do not claim this formally restores a non-zero epidemic threshold in the Pastor-Satorras sense, but the practical effect is equivalent: gossip does not achieve network-wide epidemic spread."
2. Separate the theoretical contribution (WoT changes the effective propagation topology) from the engineering contribution (TTL=3 caps reachable set regardless of topology).

---

### 3. Proof-of-Storage Hash Challenge Is Vulnerable to Lazy Replication

**Severity:** Significant

**Description:**

The hash challenge (kind 10054, type `hash`) requires the challenged peer to compute `SHA-256(canonical_json(e_i) || ... || canonical_json(e_j) || nonce)` over a range of events. The response window for non-full pairs is 4-24 hours. A lazy peer that does not actually store the data can:

1. Receive the challenge.
2. Fetch the required events from the original author's other pact partners (via kind 10057 data request or direct endpoint from kind 10059 hints).
3. Compute the hash and respond within the window.

The whitepaper acknowledges this in the Related Work section ("a malicious partner could theoretically re-fetch data before responding") but frames the 500ms latency check on `serve` challenges as the countermeasure. However, `serve` challenges are only used for Full-Full pairs (both 95%+ uptime), which the equilibrium model suggests is a minority of pact relationships (only ~25% of nodes are Keepers, so Full-Full pairs are ~6% of all pact relationships). The vast majority of pact pairs use `hash`-only challenges with multi-hour windows, making lazy replication undetectable.

This is not merely a theoretical concern. A rational node could save significant storage by not storing pact partner data and instead maintaining a lightweight index of where to fetch it on demand. The EMA reliability scoring with alpha=0.95 means that as long as the lazy peer responds to challenges (by fetching and computing), it maintains a healthy score indefinitely.

**Recommendation:**

1. Introduce **random subset challenges**: instead of requesting a contiguous range `[seq_start, seq_end]`, request a random subset of event IDs (e.g., 10 random events from the stored range). This forces the lazy peer to fetch 10 individual events from potentially different sources, increasing the latency and making lazy replication detectable even with multi-hour windows.
2. Alternatively, introduce **Merkle proof challenges**: request a Merkle inclusion proof for a random event within the checkpoint Merkle tree. The challenged peer must return the event plus the Merkle path. This is harder to proxy because the Merkle tree structure depends on having the full event set.
3. Consider reducing the response window for `hash` challenges to 1 hour even for intermittent pairs, with the retry-on-alternative-relay mechanism handling transient failures.

---

### 4. Social Recovery Race Condition During Timelock

**Severity:** Significant

**Description:**

The social recovery scheme (kinds 10060/10061) uses a 7-day timelock during which the old root key can cancel the recovery. The cancellation mechanism is described as: "the old root key can cancel the recovery at any time during the 7-day timelock by publishing a cancellation event."

There is a race condition: if an attacker compromises N recovery contacts and initiates a fraudulent recovery, the legitimate user must notice the recovery attestation events (kind 10061) and publish a cancellation within 7 days. The protocol does not specify:

1. **How the legitimate user is notified of a recovery attempt.** If the user's devices are compromised or offline, they may not see the attestation events.
2. **What happens if the cancellation event and the timelock expiry are published near-simultaneously.** Clock skew across relays could cause some relays to accept the cancellation while others accept the new root key.
3. **Whether the cancellation is a replaceable event or a regular event.** If it is a regular event, relay propagation delays could cause the cancellation to arrive at some relays after the timelock has expired from their perspective.

Additionally, the 7-day timelock is a fixed parameter. For a user who is traveling without connectivity for more than 7 days (plausible for off-grid use cases that the protocol explicitly targets), an attacker who compromises recovery contacts has an uncontested window.

**Recommendation:**

1. Specify a mandatory client behavior: clients MUST alert the user (via push notification, kind 10062) when any kind 10061 recovery attestation is published targeting their root pubkey. This is a protocol-level notification, not a nice-to-have.
2. Define the cancellation event as having strict priority: any cancellation signed by the old root key that is timestamped before the timelock expiry MUST be honored, regardless of relay propagation delays. Clients should apply a grace period (e.g., 1 hour after timelock expiry) during which cancellations are still accepted.
3. Make the timelock configurable (e.g., 7-30 days) with the kind 10060 specifying the user's chosen timelock duration. Users in high-latency or off-grid scenarios can set longer timelocks.

---

### 5. Device Delegation QR Code Onboarding Creates a TOCTOU Window

**Severity:** Significant

**Description:**

The key-management-ux.md describes a device-to-device delegation flow where an existing device grants a "temporary delegation" valid for 7 days, and the root key (available via iCloud Keychain/Google Backup) must confirm within that window by publishing an updated kind 10050.

This creates a time-of-check-to-time-of-use (TOCTOU) problem: the temporary delegation is signed by the existing device (using a device subkey), not by the root key. The protocol specification (identity.md) states that "All delegation changes require signing with the root key from cold storage." The UX document proposes an exception to this rule that is not reflected in the protocol specification.

More concretely: during the 7-day temporary delegation window, the new device operates with a delegation that was NOT signed by the root key. Other clients verifying this device's events against the kind 10050 will not find the device listed (since the kind 10050 has not been updated yet). This means either:

(a) Other clients reject events from the temporarily-delegated device (correct security, broken UX), or
(b) The protocol introduces a new event kind for temporary delegations that clients must also check (protocol complexity increase), or
(c) Clients accept events signed by any key that claims a root_identity tag without checking kind 10050 (security regression).

The document does not resolve which of these applies.

**Recommendation:**

1. Define a specific protocol mechanism for temporary delegation: either a new event kind (e.g., kind 10066 "temporary device delegation" signed by an authorized device key, with an expiry timestamp) that clients check in addition to kind 10050, or require that the QR code flow triggers an immediate kind 10050 update from the root key (which the UX document says may not be accessible).
2. If option (a) is chosen (temporary delegations are invisible to other clients), document that events from the new device will not be verifiable by third parties until the root key confirms. This is acceptable for a 7-day window but should be explicit.
3. Reconcile the key-management-ux.md with identity.md on whether device delegation strictly requires root key signing.

---

### 6. Rotating Request Token Provides No Meaningful Privacy

**Severity:** Minor

**Description:**

The rotating request token `H(target_pubkey || YYYY-MM-DD)` is acknowledged throughout the documentation as "trivially reversible by any party that knows the target's public key." The surveillance-surface.md and whitepaper are honest about this limitation.

However, the protocol still treats this as a privacy mechanism (the retrieval section calls it "pseudonymous lookup," and the design documents discuss it as part of the privacy architecture). In practice, any relay operator who maintains a directory of known pubkeys (which every relay does, since pubkeys appear in every published event) can precompute all tokens for all known users in milliseconds. The token provides zero privacy against relay operators and only marginal privacy against a passive observer who does not already know the pubkey set.

The protocol would be better served by either:
(a) Admitting the token is purely a namespace collision avoidance mechanism (not a privacy mechanism) and removing the privacy framing, or
(b) Implementing actual private information retrieval (PIR) or blinded tokens for data requests.

**Recommendation:**

1. Reframe the rotating request token as a deduplication and namespace mechanism, not a privacy mechanism. Remove the term "pseudonymous lookup" -- it sets incorrect expectations.
2. If read privacy is a design goal, investigate lightweight PIR techniques or oblivious transfer protocols for data requests. These are expensive but would provide genuine privacy. If read privacy is not a design goal (which the "Privacy Non-Goals" section suggests), state this clearly in the retrieval section alongside the token description.

---

### 7. Volume Balancing Tolerance Creates Exploitation Window

**Severity:** Minor

**Description:**

Pact volume balancing uses a 30% tolerance: `|V_p - V_q| / max(V_p, V_q) <= 0.30`. A strategic node can exploit this by maintaining posting volume at exactly 70% of their pact partner's volume, receiving full storage while providing 30% less storage in return. Over 20 pact partners, this asymmetry compounds: the exploiting node stores 20 * V_partner bytes but only has 20 * 0.7 * V_partner bytes stored for it.

Wait -- this analysis is backwards. The exploiting node stores *more* for its partners (since V_partner > V_self). The actual exploitation is: a node with low posting volume gets disproportionately more storage coverage relative to its contribution. A lurker who posts very little benefits from 20 pact partners storing their (small) data, while providing storage for 20 partners' (potentially larger) data.

This is actually not an exploit -- it is the intended behavior (volume matching ensures rough parity). But the 30% tolerance means a node can post at 70% of its partners' rate and still maintain pacts, creating a mild free-rider advantage. The renegotiation trigger (volume drift >2x) catches large drift but not persistent 30% asymmetry.

**Recommendation:**

This is a minor design trade-off, not a bug. The 30% tolerance is reasonable for practical matching. If it becomes a concern, consider reducing the tolerance to 20% or implementing a sliding tolerance that tightens over pact lifetime (e.g., 30% for the first 90 days, 20% thereafter).

---

### 8. Checkpoint Merkle Tree Duplication Is Non-Standard and Unnecessary

**Severity:** Nitpick

**Description:**

The Merkle tree specification states: "If the number of leaves is not a power of two, the last leaf is duplicated to fill the next power of two." This is a common but non-standard approach that has a known (minor) issue: if the last event ID happens to appear twice in the canonical ordering (impossible in practice since event IDs are unique SHA-256 hashes), the duplication would be ambiguous.

More practically, the duplication approach means that a tree with 2^n + 1 leaves has the same number of internal nodes as a tree with 2^(n+1) leaves, wasting computation for sets just above a power of two. RFC 6962 (Certificate Transparency) defines a more elegant construction that handles non-power-of-two leaf counts without duplication.

**Recommendation:**

Consider adopting the RFC 6962 Merkle tree construction, which handles arbitrary leaf counts without padding. This is a minor implementation detail but aligns with a well-tested standard. Alternatively, keep the current approach but note that event IDs are guaranteed unique, so the duplication ambiguity does not apply.

---

### 9. NIP-46 Channel Metadata Leakage Is Underestimated

**Severity:** Minor

**Description:**

The relay-diversity.md and surveillance-surface.md documents discuss NIP-46 timing analysis as a way for relay operators to infer pact relationships. The analysis frames this as "probabilistic" and "partial." However, pact challenge-response creates a highly distinctive pattern: one party sends a kind 10054 challenge, and the other party responds within a defined window. This is not generic timing correlation -- it is a specific, repeated, bidirectional message pattern between the same two pubkeys, occurring daily (or 3x daily for degraded pacts).

Over 30 days, a relay that handles even one pact pair's NIP-46 traffic will observe ~30 challenge-response exchanges between the same two pubkeys. This is not "probabilistic" -- it is near-certain identification of a pact relationship. The relay-diversity requirement (distribute across 3+ relays) helps, but each relay still sees the pact pairs that use it with high confidence.

**Recommendation:**

1. Acknowledge that challenge-response timing analysis provides near-certain (not probabilistic) pact pair identification for any relay handling that pair's traffic.
2. Consider wrapping challenge-response in NIP-59 gift wrapping so that the relay cannot see which pubkeys are exchanging challenges. This adds overhead but closes the metadata leak.
3. Alternatively, batch challenge-response messages with other NIP-46 traffic to reduce the distinctive pattern.

---

### 10. Pact State Machine Missing Explicit Handling of Simultaneous Transitions

**Severity:** Minor

**Description:**

The pact-state-machine.md defines transitions between states but does not specify what happens when two transitions are triggered simultaneously or near-simultaneously. For example:

- A pact partner's reliability score drops below 70% (triggering Active -> Degraded) at the same time that a partition is detected (which should suspend scoring). Which takes precedence?
- A standby partner is being promoted (Standby -> Active) at the same time that the pact transitions to Failed due to the 7-day unresponsive timeout. The promotion and failure happen in the same event loop iteration.

The document specifies partition detection as an "exception" to the Active -> Failed transition, but does not formalize the priority ordering of transitions or define atomic state transition semantics.

**Recommendation:**

Add a "Transition Priority" section specifying that: (1) partition detection suspension takes precedence over all scoring-based transitions, (2) transitions are evaluated in a defined order per evaluation cycle (e.g., partition check -> timeout check -> score check -> comfort check), and (3) at most one transition per pact per evaluation cycle.

---

## Summary Table

| # | Issue | Severity | One-line summary |
|---|-------|----------|------------------|
| 1 | HKDF deterministic key linkage | Critical | Root key compromise defeats all DM forward secrecy; documentation understates this |
| 2 | Epidemic threshold argument circularity | Significant | WoT spectral radius claim is stated but not proven; TTL=3 is the real mechanism |
| 3 | Proof-of-storage lazy replication | Significant | Hash challenges with multi-hour windows are trivially proxied by lazy peers |
| 4 | Social recovery timelock race condition | Significant | No mandatory notification of recovery attempts; clock skew at timelock boundary undefined |
| 5 | Device delegation TOCTOU window | Significant | Temporary delegation UX proposal contradicts protocol spec; verifiability gap during 7-day window |
| 6 | Rotating request token privacy framing | Minor | Token is trivially reversible; "pseudonymous lookup" sets incorrect expectations |
| 7 | Volume balancing exploitation | Minor | 30% tolerance allows persistent mild asymmetry; within acceptable bounds |
| 8 | Merkle tree padding approach | Nitpick | Non-standard padding; RFC 6962 construction is cleaner |
| 9 | NIP-46 challenge-response metadata | Minor | Relay pact identification is near-certain, not probabilistic; pattern is highly distinctive |
| 10 | State machine simultaneous transitions | Minor | No priority ordering for concurrent transition triggers |

---

## Overall Assessment

Gozzip is a serious protocol design that demonstrates genuine engagement with the relevant network science, cryptographic, and systems literature. The key hierarchy is well-structured with appropriate compartmentalization (upward only, which is the correct direction). The pact mechanism is cleverly designed with the equilibrium-seeking formation model being a particular strength -- it avoids the trap of prescribing fixed parameters and instead lets the pact count emerge from measurable conditions. The surveillance surface analysis is unusually honest for a protocol whitepaper, openly documenting privacy non-goals and limitations rather than overstating protections.

The critical issue (HKDF-derived DM keys defeating forward secrecy on root compromise) is architecturally significant but fixable with a key derivation redesign. The significant issues cluster around two themes: (1) the epidemic threshold argument's circular reasoning (WoT spectral radius claim is stated but not proven), and (2) the proof-of-storage mechanism's weakness against strategic lazy replication. Both are addressable through the recommended changes without altering the protocol's core architecture. The minor issues are genuine but do not threaten the protocol's viability. Overall, this is a strong design that would benefit from tightening its formal claims to match what it actually delivers, and from strengthening its proof-of-storage against rational adversaries.
