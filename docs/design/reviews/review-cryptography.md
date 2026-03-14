# Cryptographic Review: Gozzip Protocol

**Date:** 2026-03-14
**Reviewer:** Adversarial cryptography review
**Documents reviewed:** Whitepaper v1.2 (gossip-storage-retrieval.tex), key-management-ux.md, surveillance-surface.md, proof-of-storage-alternatives.md, relay-diversity.md
**Scope:** Key hierarchy soundness, forward secrecy claims, proof-of-storage mechanisms, NIP-44 encryption usage, unnecessary complexity, and alternative design approaches

---

## Executive Summary

The Gozzip protocol makes generally competent cryptographic choices: secp256k1 identity, HKDF-SHA256 key derivation, NIP-44 (ChaCha20-Poly1305) for encryption, SHA-256 Merkle trees for integrity. The architecture is honest about most of its limitations, which is rare and valuable. However, the cryptographic design suffers from a central tension: the key hierarchy is presented as providing compartmentalization, but the deterministic derivation from a single root secret means compromise flows downward unconditionally. The "bounded forward secrecy" claim is technically defensible for device-level compromise only, but will be misunderstood by users and developers as providing Signal-like guarantees. The proof-of-storage mechanism is adequate for the threat model (lazy social peers) but is not a proof of storage in any formal sense. Several mechanisms are more complex than necessary for the security they provide. The protocol would benefit from a cleaner separation between its identity layer (which is strong), its confidentiality layer (which is adequate but misleadingly marketed), and its storage verification layer (which is honest about its limitations).

---

## Issues

### 1. Root Key as Universal Master Secret Defeats Key Hierarchy Purpose

**Severity:** Critical

**Description:**

The key hierarchy derives governance, DM, and device subkeys from the root key via HKDF-SHA256 with deterministic info strings (`"governance"`, `"dm-decryption-" || rotation_epoch`, `"device/0"`, etc.). The documentation presents this as "compartmentalization" -- device compromise is contained, the root key lives in cold storage, DM keys rotate independently.

The compartmentalization claim is half true. Upward compartmentalization works: compromising a device subkey does not reveal the root key, governance key, or DM keys. This is valuable and correctly designed. Downward compartmentalization does not exist: compromising the root key immediately and deterministically yields every governance key, every device subkey, and every DM key for every rotation epoch -- past and future.

This creates a specific, concrete failure mode. The DM key derivation uses `KDF(root, "dm-decryption-" || rotation_epoch)`. The rotation_epoch is deterministic: `floor(unix_timestamp / (90 * 86400))`. An adversary who obtains the root key at any point can:

1. Compute the DM decryption key for every 90-day epoch since account creation.
2. Compute the DM decryption key for every future epoch.
3. Decrypt every NIP-44 encrypted DM ever sent to this user, retroactively.
4. Decrypt all future DMs without any ongoing access.

The key-management-ux.md document stores the root key in platform secure storage (iCloud Keychain, Google Cloud Keystore) encrypted with biometrics. This is reasonable UX, but it means the root key -- which is the universal decryption key for all DMs across all time -- is backed up to cloud infrastructure operated by Apple or Google. A government subpoena, a cloud backup breach, or a platform key escrow policy change exposes the entire DM history. The root key is simultaneously the identity anchor AND the universal DM decryption key AND the device delegation authority AND the governance key generator. This concentration of authority in a single 32-byte secret is the protocol's most significant cryptographic weakness.

The existing agent-01-02 review identified this issue. I am elevating it because the key-management-ux.md document, written after that review, does not address it. The standard mode still derives DM keys from root, stores root in cloud backup, and performs automatic rotation -- meaning the root key is routinely accessible on every device, in every cloud backup, with no option for the DM key chain to be independent.

**Recommendation:**

Decouple the DM key lineage from the root key. Two approaches:

(a) **Hash-chain ratchet for DM keys.** The first DM key is derived from root via HKDF. Each subsequent epoch key is derived as `K_{n+1} = HKDF(K_n, "dm-epoch-advance")`. After deriving the new key, the old key material is securely deleted. Root compromise then reveals only the current and future DM keys, not past ones. This provides genuine forward secrecy at the cost of making DM key recovery impossible without the current epoch key (social recovery would need to include the current DM seed, not just the root key).

(b) **Independent DM seed.** Generate a separate random 32-byte DM seed at account creation, stored alongside the root key but independently. DM epoch keys are derived from this seed. The DM seed can be excluded from cloud backups or stored with a separate passphrase. This adds one more secret to manage but provides genuine compartmentalization between identity authority (root key) and communication confidentiality (DM seed).

Option (a) is strictly better for forward secrecy. Option (b) is simpler and provides compartmentalization without forward secrecy against DM seed compromise. Either is a substantial improvement over the current design where root = everything.

---

### 2. "Bounded Forward Secrecy" Claim Is Misleading

**Severity:** High

**Description:**

The whitepaper states: "DM key rotation provides bounded forward secrecy but not perfect -- contingent on correct client implementation of secure key deletion." The key-management-ux.md describes automatic 90-day rotation with a 7-day overlap window and secure deletion of old key material.

The term "bounded forward secrecy" will be understood by most readers as: "if my key is compromised today, the adversary can read at most ~97 days of past DMs." This is the implied promise. The actual guarantee is narrower:

**What "bounded forward secrecy" actually means in this protocol:**

- If an adversary compromises a *device subkey* (not the root key), they obtain the current DM decryption key, which covers at most the current 90-day epoch plus the 7-day overlap window (~97 days).
- If an adversary compromises the *root key*, they can derive every DM key for every epoch, past and future. Forward secrecy is completely defeated.
- If an adversary compromises the *cloud backup* (iCloud Keychain, Google Backup) in standard mode, they obtain the root key. Same result.
- The secure deletion claim depends entirely on the platform's secure storage API actually deleting key material. On iOS, `SecItemDelete` removes the Keychain entry, but if the device has been backed up (iCloud Backup, not just iCloud Keychain), the old key material may persist in backup snapshots. On Android, `KeyStore.deleteEntry()` removes the key from the Android Keystore, but if the key was also stored in Google Backup, deletion from the local Keystore does not propagate to the backup.

The practical scenario: a user in standard mode has their root key in iCloud Keychain. Apple receives a lawful intercept order. Apple provides the iCloud Keychain contents. The adversary now has the root key and can derive every DM key for every epoch. The "bounded forward secrecy" claim provides zero protection in this scenario, which is arguably the most realistic threat model for a user who needs forward secrecy.

Comparison to Signal's double ratchet:
- Signal's double ratchet derives per-message keys via a Diffie-Hellman ratchet. Compromising the current ratchet state reveals at most a handful of future messages until the next DH ratchet step. Past messages are protected because the DH ratchet keys have been replaced and deleted.
- Signal does not store any master key that can retroactively decrypt all past messages. There is no single secret whose compromise defeats all confidentiality.
- Gozzip's "bounded forward secrecy" provides none of these guarantees. It is rotation, not ratcheting. The distinction matters.

**Recommendation:**

1. Replace the term "bounded forward secrecy" with "periodic key rotation." Forward secrecy has a specific meaning in cryptography (compromise of long-term keys does not compromise past session keys), and this protocol does not provide it. Calling it "bounded forward secrecy" implies a weaker form of a strong guarantee, when in reality it is a different mechanism entirely (key rotation with deletion) that depends on operational assumptions (correct deletion, no backup persistence, no root compromise) that are unlikely to hold in practice.

2. If the protocol wants to claim actual forward secrecy for DMs, implement the hash-chain ratchet from Issue 1. Even then, it would be "forward secrecy contingent on secure deletion of previous epoch keys" -- honest and accurate.

3. Add a clear comparison table to the documentation:

| Property | Signal | Gozzip (current) | Gozzip (with hash-chain ratchet) |
|----------|--------|-------------------|----------------------------------|
| Per-message keys | Yes (DH ratchet) | No (per-epoch) | No (per-epoch) |
| Root key compromise reveals past DMs | N/A (no root key) | Yes (all epochs) | No (only current + future) |
| Device compromise reveals past DMs | Current ratchet state only | Current epoch (~97 days) | Current epoch (~97 days) |
| Requires secure deletion | Yes (ratchet state) | Yes (old epoch keys) | Yes (old epoch keys) |
| Cloud backup defeats FS | N/A | Yes (root in backup) | Partially (DM seed in backup) |

---

### 3. NIP-44 Encryption for Pact Communication Has an Unaddressed Metadata Problem

**Severity:** High

**Description:**

The protocol uses NIP-44 (XChaCha20-Poly1305 with HKDF-derived shared secrets from secp256k1 ECDH) for pact communication via NIP-46 relay-mediated channels. The encryption itself is sound -- NIP-44 is a well-designed authenticated encryption scheme.

The problem is not the encryption but the metadata. NIP-46 messages carry the sender's pubkey in the event author field and the recipient's pubkey in the `p` tag. The relay sees both endpoints of every pact communication message, including:

- Challenge events (kind 10054): the relay sees which two pubkeys are exchanging challenges. Challenge-response creates a distinctive, repeated, bidirectional pattern between the same two pubkeys at regular intervals. Over 30 days, this produces ~30-90 observations per pact pair, making pact relationship identification near-certain (not probabilistic, as the relay-diversity.md frames it).

- Pact negotiation (kinds 10055, 10056, 10053): the relay sees who is negotiating pacts with whom, including the temporal sequence of request -> offer -> accept.

- Event sync: the relay sees the volume and timing of data transfer between pact partners.

The relay-diversity.md requirement to distribute across 3+ relays helps, but each relay still sees the pact pairs that use it with high confidence. With the simulation showing 10-13% of reads going through relays at maturity (for non-sparse topologies), and NIP-46 being the primary communication channel for pact management, relay operators have a permanent, rich metadata stream.

The surveillance-surface.md acknowledges this but describes it as "probabilistic" pact pair identification. This understates the reality: challenge-response patterns are a near-perfect fingerprint. The rotating request token (H(target_pubkey || date)) provides no meaningful privacy against relay operators, as acknowledged in the documentation.

Additionally, the protocol uses NIP-44 encryption for DM communication. NIP-59 gift wraps expose both sender (device pubkey, resolvable to root identity via kind 10050) and recipient (`p` tag) to every relay handling the DM. Content is encrypted, but the communication graph is fully visible. This means:

- A relay operator sees every DM sender-recipient pair.
- Timing analysis reveals conversation patterns (message, response, message).
- Volume analysis reveals the intensity of relationships.

The protocol explicitly states this is a "privacy non-goal," which is honest. But it means NIP-44 encryption provides content confidentiality only, not communication privacy. For users facing adversaries who care about the communication graph (which is most state-level adversaries), the encrypted content is less valuable than the metadata.

**Recommendation:**

1. Upgrade the metadata assessment from "probabilistic" to "near-certain for any relay handling a pact pair's challenge-response traffic." The current framing understates the exposure.

2. For pact communication specifically, consider implementing a scheme where challenge-response messages are addressed to a rotating ephemeral pubkey rather than the pact partner's long-term pubkey. The pact partners share the ephemeral key schedule out-of-band (negotiated during pact formation, rotated periodically). The relay then sees messages to a pubkey it cannot link to any known identity. This is essentially gift-wrapping applied to pact communication.

3. For DMs, the protocol should consider sender-obfuscated routing: the sender publishes the gift-wrapped DM to their own relay list (not the recipient's), and the recipient discovers it via a subscription to a blinded identifier (e.g., H(shared_secret || date)). This hides the sender-recipient pair from any single relay. This is a significant protocol change and may be out of scope, but it should be noted as a future direction.

4. At minimum, the documentation should clearly state: "NIP-44 encryption protects message content. It does not protect the communication graph. Relay operators see who communicates with whom, when, and how often. Users requiring communication graph privacy should use out-of-band tools (Signal, Briar)."

---

### 4. Proof of Storage Is Not a Proof of Storage

**Severity:** Medium

**Description:**

The proof-of-storage-alternatives.md is admirably honest about the limitations of the current scheme. It correctly notes that the hash challenge proves "proof of accessibility" rather than proof of storage, and that the serve challenge's 500ms latency threshold is a heuristic. The document evaluates six alternatives and provides well-reasoned rejections.

However, there are cryptographic concerns with the current challenge scheme that go beyond what the document addresses:

**Predictable challenge structure.** The hash challenge requests a contiguous range `[seq_start, seq_end]` of events. A lazy peer who maintains only an index (event ID -> location of another pact partner who has it) can optimize: when challenged, fetch only the needed range from another partner, compute the hash, and respond. The contiguous range structure makes this efficient -- a single bulk request to another pact partner covers the challenged range.

**Age-biased challenge distribution is circumventable.** The whitepaper specifies 50% of challenges target the oldest third, 30% middle, 20% newest. A rational lazy peer, knowing this distribution, can optimize by storing the oldest third of events (most frequently challenged) and proxying the rest. This inverts the intended defense: the bias makes the challenge pattern predictable.

**No binding between challenge and committed state.** The challenge nonce prevents replay, but there is no commitment to the stored state *before* the challenge arrives. A peer could receive the challenge, fetch the data, compute the response, and delete the data again. The Merkle root in checkpoints provides a commitment, but the challenge does not verify against the checkpoint Merkle root -- it computes an independent hash over the challenged range.

**The serve challenge 500ms threshold is essentially meaningless in practice.** The proof-of-storage-alternatives.md notes this but does not quantify how easy it is to beat. A lazy peer with a single well-connected friend who stores the data can proxy a serve challenge in under 100ms (one HTTP request to a peer on a fast connection). The 500ms threshold would only catch a peer proxying through multiple hops or through a high-latency relay. In the context of NIP-46 relay-mediated communication, where round-trip times of 50-200ms are normal, 500ms is extremely generous.

**Recommendation:**

1. **Bind challenges to checkpoint Merkle roots.** Instead of requesting H(events[i..j] || nonce), request a Merkle inclusion proof for a random event within the most recent checkpoint's Merkle tree. The challenged peer must return the event data plus the Merkle authentication path from the leaf to the root. This forces the peer to maintain the tree structure (or reconstruct it from the full dataset), not just cache individual events. This was recommended in proof-of-storage-alternatives.md as a future direction -- I would prioritize it.

2. **Randomize challenge structure.** Instead of contiguous ranges, challenge with random subsets of event IDs. Request 5-10 events at random positions across the full stored range. A lazy peer now needs to make 5-10 separate fetch requests to proxy the challenge, increasing latency and making the 500ms threshold more meaningful.

3. **Remove the age bias or randomize it.** The fixed 50/30/20 distribution is gameable. Instead, draw the challenge target epoch from a uniform distribution (covering the full stored range) on each challenge. This removes the predictability. If old data is a concern, periodically (monthly) run a full coverage pass where every epoch is challenged at least once.

4. **Acknowledge the scheme honestly.** Call it "liveness verification" or "data accessibility checks," not "proof of storage." The term "proof of storage" has a specific meaning in the cryptographic literature (PoR, PoS, PoRep) that implies formal security guarantees. This scheme does not provide those guarantees, and the proof-of-storage-alternatives.md correctly explains why stronger schemes are inappropriate at this scale. Honest terminology prevents users and implementers from over-relying on the verification.

---

### 5. Social Recovery Threshold Scheme Underspecified

**Severity:** Medium

**Description:**

The whitepaper describes N-of-M social recovery (kinds 10060/10061) with "NIP-44 encrypted shares." The key-management-ux.md describes it as choosing "3 trusted contacts who can help recover your account" with "3-of-5 contacts" as the default threshold. The UX document explicitly hides the concept of Shamir's secret sharing from the user: "No mention of: N-of-M threshold, Shamir's secret sharing, timelocks, or key shards."

The cryptographic mechanism is not specified in enough detail:

1. **What is being shared?** The document says "NIP-44 encrypted shares" in kind 10060 events. Is this Shamir's Secret Sharing (SSS) applied to the root private key? If so, the shares are published to relays (albeit NIP-44 encrypted to each recovery contact individually). This means the encrypted shares persist on relays indefinitely. If the NIP-44 encryption is ever broken (or the recovery contact's key is compromised), the shares are exposed. Over time, the probability that 3 of 5 recovery contacts' keys are compromised increases monotonically.

2. **Share refresh is not mentioned.** Shamir shares should be periodically refreshed (re-sharing the same secret with new random polynomials) so that old shares become useless. Without refresh, the security of the scheme degrades over time as recovery contacts' devices age, change hands, or are compromised.

3. **The 7-day timelock implementation is unspecified.** Who enforces the timelock? In a decentralized system, there is no trusted clock. If the timelock is enforced by clients checking `created_at` timestamps on kind 10061 attestation events, an adversary who compromises 3 recovery contacts can backdate the attestation events to bypass the timelock. The protocol needs to specify that the timelock is measured from the earliest relay-confirmed timestamp of the attestation events (i.e., the relay's receipt timestamp, not the event's `created_at`).

4. **The cancellation race condition** (identified in agent-01-02 review) remains unaddressed: what happens if the old root key publishes a cancellation within the timelock window but network propagation delays cause some clients to see the cancellation after the timelock expires?

**Recommendation:**

1. Specify the sharing scheme precisely: "The root private key is split using Shamir's Secret Sharing (GF(2^256)) into M shares with threshold K. Each share is NIP-44 encrypted to the respective recovery contact's pubkey and published as a kind 10060 event."

2. Mandate share refresh: "Every 180 days, the client generates new Shamir shares for the same root key and publishes updated kind 10060 events. Previous shares are invalidated by including a monotonic share_version counter."

3. Specify timelock enforcement: "The 7-day timelock is measured from the earliest NIP-65 relay timestamp confirming receipt of the K-th attestation event. Clients MUST NOT accept attestation events whose `created_at` is more than 1 hour before the relay's receipt timestamp."

4. Specify cancellation priority: "A cancellation event signed by the old root key with `created_at` before the timelock expiry MUST be honored regardless of when it is received. Clients MUST accept cancellations for 24 hours after the timelock expiry to accommodate propagation delays."

---

### 6. HKDF Key Derivation Lacks Domain Separation for Cross-Protocol Safety

**Severity:** Medium

**Description:**

The HKDF derivation uses:
- IKM = root private key (32 bytes)
- salt = `gozzip-v1` (protocol constant)
- info = purpose label string (e.g., `governance`, `device/0`)
- output = 32 bytes interpreted as secp256k1 scalar

This is correct usage of HKDF per RFC 5869, but the domain separation is insufficient for a protocol that shares the secp256k1 key space with Nostr. A Gozzip user's root key IS their Nostr key. If any other Nostr application uses HKDF with the same salt and info string to derive subkeys, the derived keys collide. The `gozzip-v1` salt provides some protection, but only if no other protocol accidentally (or maliciously) uses the same string.

More specifically: the info strings are human-readable words (`governance`, `device/0`). These are plausible collision candidates with other key derivation schemes. A robust approach would use structured domain separation: `info = "gozzip" || protocol_version || purpose || context_length || context`.

Additionally, the derivation specification says "output = 32 bytes interpreted as a secp256k1 scalar (reduced mod curve order n; retry with incremented counter if zero)." The probability of the output being zero mod n is negligible (1/n, approximately 2^{-256}), so the retry is paranoid but correct. However, the specification does not describe how the counter is incremented. Is the counter appended to the info string? To the IKM? This needs to be precise for interoperable implementations.

**Recommendation:**

1. Use structured domain separation: `info = "gozzip-v1" || uint16_be(purpose_id) || purpose_string` where purpose_id is a fixed numeric identifier (e.g., 0x0001 for governance, 0x0002 for DM, 0x0003 for device) and purpose_string is the human-readable label. This prevents collisions even if another protocol uses the same salt.

2. Specify the retry counter precisely: "If the HKDF output reduced mod n equals zero, increment a uint32 counter (starting at 0) appended to the info string and re-derive. The counter is encoded as 4 big-endian bytes."

3. Consider registering the salt and info format in a Nostr NIP to prevent future collisions with other Nostr-based protocols that might use HKDF key derivation.

---

### 7. Checkpoint Merkle Tree Has No Binding to Previous Checkpoints

**Severity:** Medium

**Description:**

Checkpoints (kind 10051) contain a Merkle root over all events since the previous checkpoint, per-device event heads, and references to the current profile and follow list. The specification describes the Merkle tree construction (SHA-256, canonical ordering by (device_pubkey, seq), power-of-two padding).

However, the checkpoint does not include a hash of the previous checkpoint. This means checkpoints form a linear sequence (each references "all events since the previous checkpoint") but there is no cryptographic binding between consecutive checkpoints. A malicious pact partner could present checkpoint N and checkpoint N+2, omitting checkpoint N+1 entirely, and a verifier could not detect the omission unless they independently know checkpoint N+1 exists.

The per-device event heads include sequence numbers, so a gap in sequence numbers would be detectable. But if the adversary controls the data served to a verifier, they could present a fabricated checkpoint with adjusted sequence numbers that skip the omitted range.

More practically: the per-device hash chain (`prev_hash` tag on events) provides a separate integrity mechanism. But the checkpoint Merkle tree and the event hash chains are not bound to each other. A verifier checking the Merkle tree gets one integrity guarantee; a verifier checking the hash chain gets another. Neither verifier can detect if the other's data has been tampered with unless they perform both checks and cross-reference the results.

**Recommendation:**

1. Include `prev_checkpoint_hash` in each checkpoint event, computed as SHA-256 of the previous checkpoint's serialized content. This creates a hash chain of checkpoints, making omission or reordering detectable.

2. Bind the checkpoint Merkle root to the event hash chain: include the most recent `prev_hash` from each device's event chain as a leaf in the checkpoint Merkle tree. This ensures that any tampering with the event hash chain is detectable via the checkpoint, and vice versa.

---

### 8. Rotating Request Token Adds Complexity Without Security

**Severity:** Low

**Description:**

The rotating request token `bp = H(target_root_pubkey || YYYY-MM-DD)` is used for data requests (kind 10057). The whitepaper, surveillance-surface.md, and multiple design documents all acknowledge that this token is "trivially reversible by any party that knows the target's public key."

Every relay maintains a directory of known pubkeys (since pubkeys appear in every published event). Precomputing all tokens for all known pubkeys for today's date is a trivial operation (one SHA-256 per pubkey, completed in milliseconds for millions of pubkeys). The token therefore provides exactly zero privacy against relay operators.

The token rotates daily, which means storage peers must match against both today's and yesterday's tokens (per the specification). This introduces a day-boundary linkage problem (acknowledged in surveillance-surface.md): a storage peer can link a request using yesterday's token to a request using today's token for the same target, confirming identity continuity across the boundary.

The token does provide one minor benefit: a passive network observer who intercepts a data request in transit and does not know the Nostr pubkey set cannot determine which author is being requested. But this is an extremely narrow threat model -- anyone with access to any Nostr relay (which are public by default) can obtain the pubkey set.

The mechanism adds protocol complexity (token computation, dual-token matching, day-boundary handling) for a security benefit that is essentially zero in practice.

**Recommendation:**

Either:

(a) Remove the rotating token entirely and use the target pubkey directly in data requests. This is simpler, equally secure in practice, and eliminates the day-boundary linkage issue (which is worse than no token at all because it provides a false sense of privacy while introducing a new correlation vector).

(b) If read privacy is genuinely desired, implement a real privacy-preserving retrieval mechanism. The simplest option: the requester generates a random request ID and sends a NIP-44 encrypted request to the storage peer, who responds with encrypted data. The relay sees an opaque request and response between two pubkeys, with no ability to determine the target author. This is trivially implementable within the existing NIP-46 framework and provides genuine read privacy against the relay.

Option (b) is recommended if read privacy matters. Option (a) is recommended if it does not.

---

### 9. Device Delegation Model Has Unclear Cryptographic Boundaries

**Severity:** Low

**Description:**

The key hierarchy specifies three key types: root (cold storage), governance (profile/follow changes), device subkeys (day-to-day signing). Device subkeys are derived via HKDF from the root key with `info = "device/N"`.

The key-management-ux.md proposes device-to-device delegation where an existing device signs a "temporary delegation" for a new device, valid for 7 days, without root key involvement. This creates an ambiguity: the temporary delegation is signed by a device subkey, not the root key. But device subkeys are authorized only for "day-to-day events (posts, reactions, DMs)" -- they are not authorized to delegate authority to other keys.

This means either:

(a) The protocol must define a new delegation authority level for device subkeys (they can grant temporary, limited delegations to other devices), which expands the authority model beyond the three-level hierarchy.

(b) The temporary delegation is not cryptographically verified by other clients (they simply ignore the temporarily delegated device's events until the root key confirms via kind 10050), which means the new device's events are unverifiable for up to 7 days.

(c) The governance key (not just device subkeys) is available on-device in standard mode, and the temporary delegation is signed by the governance key -- but the key-management-ux.md says governance is derived and "stored in platform secure storage," which implies it IS on-device. If governance can delegate, the hierarchy has effectively two levels, not three.

The cryptographic authority model needs to be precisely defined: which keys can sign which kinds of delegations, and what are the verification rules for events signed by delegated keys at each authority level?

**Recommendation:**

1. Define a precise delegation authority table:

| Action | Required signer | Verifiable by third parties? |
|--------|----------------|------------------------------|
| Permanent device delegation | Root key | Yes (kind 10050) |
| Temporary device delegation (7-day) | Governance key | Yes (new kind 10066) |
| Profile/follow changes | Governance key | Yes |
| Posts, reactions | Device subkey | Yes |
| DMs | DM key | Yes |

2. Specify that temporary delegations use a new event kind (e.g., kind 10066) signed by the governance key, with an explicit expiry timestamp. Third-party clients verify temporary delegations by checking that the kind 10066 event exists, is signed by the governance key listed in the current kind 10050, and has not expired.

3. Explicitly state that device subkeys CANNOT delegate authority. Only the governance key (for temporary) and root key (for permanent) can add new devices.

---

### 10. NIP-44 Shared Secret Derivation Lacks Per-Conversation Uniqueness

**Severity:** Low

**Description:**

NIP-44 derives the shared encryption key from ECDH between the sender and recipient's secp256k1 keys. For pact communication, DMs, and other encrypted messages, the shared secret is `ECDH(sender_privkey, recipient_pubkey)`. This means every message between the same two parties uses the same underlying shared secret (though NIP-44 derives per-message keys from the shared secret via HKDF with a random nonce).

For DMs, this is standard and acceptable -- NIP-44's per-message nonce ensures unique encryption keys per message. However, for pact communication, the same shared secret (and thus the same ECDH computation) is used for challenge-response messages, pact negotiation messages, event sync messages, and any other NIP-46 communication between the same two parties. This means:

- All pact communication between Alice and Bob uses the same underlying ECDH shared secret as their DMs.
- If the shared secret is compromised (e.g., through a side-channel attack on the ECDH computation), all communication types are compromised simultaneously.

This is not a vulnerability in NIP-44 itself (the per-message nonce provides IND-CPA security), but it means there is no cryptographic separation between communication contexts. A compromise of the DM channel between two users also compromises their pact communication channel.

**Recommendation:**

This is a low-severity concern because NIP-44's per-message nonce derivation prevents practical attacks. However, if the protocol evolves to include more sensitive pact communication (e.g., challenge-response proofs that reveal information about stored data), consider deriving context-specific shared secrets: `K_dm = HKDF(ECDH_shared, "gozzip-dm")`, `K_pact = HKDF(ECDH_shared, "gozzip-pact")`. This provides cryptographic isolation between communication contexts at minimal additional cost.

---

## What Is Overcomplicated

### The Four-Level Key Hierarchy Is Effectively Two Levels

In practice, the key hierarchy has two meaningful levels, not four:

- **Root key**: the universal master secret from which everything is derived
- **Device subkeys**: the keys that sign day-to-day events

The governance key is derived from root and stored on-device in standard mode. It does not provide additional compartmentalization because any attacker who compromises the device gets both the device subkey and the governance key. Moving the governance key to a separate device (Advanced Mode) does create a meaningful third level, but this is opt-in and expected to be rare.

The DM key is derived from root. It rotates, but the rotation is not a ratchet -- it is deterministic re-derivation from root. Any compromise of root defeats all DM keys. The DM key is therefore not a separate level; it is a derived view of the root key.

**Simplification:** The protocol would be clearer with an explicit two-level model for standard users (root + devices) and a three-level model for advanced users (root + governance-on-separate-device + devices). The DM key should be independent (see Issue 1) rather than derived, making it a genuine third level for all users.

### Checkpoint Merkle Trees Alongside Per-Event Hash Chains Is Redundant

The protocol specifies two integrity mechanisms:

1. Per-device hash chains: each event carries `seq` and `prev_hash`, forming a chain that detects missing or reordered events.
2. Checkpoint Merkle trees: periodic Merkle roots over all events since the last checkpoint.

Both mechanisms detect the same class of attacks (missing events, tampered events). The hash chain is simpler, incremental, and provides real-time verification. The Merkle tree provides efficient range proofs but requires periodic computation and synchronization of the tree structure between parties.

**Simplification:** Keep the per-device hash chains as the primary integrity mechanism. Replace checkpoint Merkle trees with a simpler checkpoint hash: `SHA-256(prev_checkpoint_hash || latest_event_hash_per_device)`. This chains the checkpoints together and binds them to the event hash chains without maintaining a full Merkle tree. The Merkle tree only becomes valuable if the protocol implements Merkle proof challenges (recommended in Issue 4), in which case keep both.

### Volume Balancing Tolerance Is Unnecessary Complexity

The 30% volume tolerance (`|V_p - V_q| / max(V_p, V_q) <= 0.30`) adds a continuous monitoring requirement that triggers renegotiation when activity levels drift. In practice, all pact partners store the same thing: the other party's events. The storage cost is dominated by the number of events, not the volume asymmetry. A user who posts 100 events/month and a user who posts 70 events/month have a negligible storage difference at 925 bytes average.

**Simplification:** Replace volume balancing with a simple event count ceiling per pact partner (e.g., max 1000 events/month). Any user under the ceiling is eligible for pacts with any other user under the ceiling. This eliminates the volume tracking, tolerance computation, and renegotiation logic.

---

## Alternative Approach: How I Would Design the Crypto Layer

Starting fresh, I would make the following changes:

### Identity Layer

Keep secp256k1 keypairs and the Nostr event model. These are battle-tested and interoperable. Keep the device delegation model (root + device subkeys). Simplify to two levels for standard users:

- **Root key**: signs device delegations and recovery operations. Stored in platform secure storage with biometric protection. Backed up to iCloud/Google.
- **Device subkeys**: sign all day-to-day events (posts, profile changes, follow lists). Each device gets a unique subkey derived via HKDF from root with device-specific info.

Remove the governance key as a separate concept for standard users. It adds protocol complexity without meaningful security benefit when it is stored on the same device as the device subkey.

### Confidentiality Layer

**DM keys: independent, ratcheted.**

- At account creation, generate a random 32-byte DM seed, independent of the root key.
- The first DM epoch key is `HKDF(dm_seed, "dm-epoch-0")`.
- Each subsequent epoch key is `HKDF(prev_epoch_key, "dm-epoch-advance")`.
- After derivation, the previous epoch key is securely deleted.
- The DM seed is included in social recovery shares but stored separately from the root key. A user can choose to exclude the DM seed from cloud backups (accepting that DM history is unrecoverable if all devices are lost).
- This provides genuine forward secrecy: root key compromise does not reveal past DM keys.

**Per-context encryption:**

- Derive context-specific shared secrets from the NIP-44 ECDH output: `K_dm = HKDF(ECDH, "gozzip-dm")`, `K_pact = HKDF(ECDH, "gozzip-pact")`.
- This isolates DM confidentiality from pact communication confidentiality.

**Sender-blinded DM routing (future):**

- Instead of putting the recipient's pubkey in the `p` tag (visible to relay), use a blinded identifier: `H(ECDH_shared_secret || YYYY-MM-DD)`. The recipient polls for messages matching their blinded identifiers with each sender. The relay cannot determine sender-recipient pairs.
- This is a meaningful privacy improvement that is achievable within the existing relay model.

### Storage Verification Layer

**Random subset challenges with Merkle binding:**

- Challenges request Merkle inclusion proofs for N random events (default N=5).
- The challenged peer returns each event plus its Merkle authentication path to the checkpoint root.
- The verifier checks: (a) each event is valid (signature, hash), (b) each Merkle path is valid to the agreed checkpoint root, (c) the response arrives within the latency bound.
- This is strictly better than hash-range challenges: it verifies individual events, is resistant to range-proxying, and binds to the checkpoint state.

**Remove the serve challenge.** The 500ms latency threshold is not a meaningful security measure. If the Merkle challenge response time is tracked (it should be), it inherently measures response latency. A separate "serve challenge" with a different latency threshold adds complexity without adding security.

**Remove age-biased distribution.** Use uniform random sampling across the full stored range. The age bias is gameable (Issue 4) and adds protocol complexity. If old data verification is a concern, run a periodic full sweep (monthly) where every event is challenged at least once.

### Recovery Layer

**Shamir's Secret Sharing with verifiable shares:**

- Use Feldman's Verifiable Secret Sharing (VSS) instead of plain Shamir. VSS allows each share holder to verify their share is valid without learning the secret. This prevents a malicious share distributor from giving some contacts invalid shares.
- Mandate share refresh every 180 days with a monotonic version counter.
- Specify the timelock enforcement mechanism precisely: relay-confirmed timestamps, 24-hour grace period for cancellation propagation.

### What I Would Remove

1. **Rotating request tokens.** Replace with NIP-44 encrypted request/response if read privacy matters, or use direct pubkey addressing if it does not.
2. **Volume balancing tolerance.** Replace with a simple event count ceiling.
3. **The governance key** (for standard users). It is not a separate security level when stored on the same device.
4. **The 500ms serve challenge.** Replace with latency tracking on Merkle challenges.
5. **Age-biased challenge distribution.** Replace with uniform random sampling + periodic full sweep.

### What I Would Add

1. **Independent DM seed with hash-chain ratchet** (most impactful single change).
2. **Merkle proof challenges** bound to checkpoint roots.
3. **Context-specific ECDH key derivation** for cryptographic isolation between DMs and pact communication.
4. **Checkpoint hash chaining** (prev_checkpoint_hash in each checkpoint).
5. **Verifiable secret sharing** for social recovery.
6. **Precise specification** of counter increment for HKDF retry, domain separation structure, and timelock enforcement.

---

## Summary Table

| # | Issue | Severity | One-line summary |
|---|-------|----------|------------------|
| 1 | Root key as universal master secret | Critical | Deterministic derivation means root compromise defeats all key hierarchy benefits including all DM forward secrecy |
| 2 | "Bounded forward secrecy" is misleading | High | The guarantee applies only to device compromise, not root or cloud backup compromise; terminology implies Signal-like properties |
| 3 | NIP-44 pact communication metadata | High | Challenge-response patterns provide near-certain pact identification to relay operators; framed as "probabilistic" |
| 4 | Proof of storage is not a proof of storage | Medium | Hash-range challenges are gameable; no binding to committed state; 500ms latency threshold is meaningless |
| 5 | Social recovery scheme underspecified | Medium | Share type, refresh mechanism, timelock enforcement, and cancellation semantics are not precisely defined |
| 6 | HKDF lacks robust domain separation | Medium | Human-readable info strings risk collision; counter increment behavior unspecified |
| 7 | Checkpoint Merkle tree not chained | Medium | No cryptographic binding between consecutive checkpoints; gaps are undetectable |
| 8 | Rotating request token adds complexity for zero security | Low | Trivially reversible by relay operators; false sense of privacy; day-boundary linkage is worse than no token |
| 9 | Device delegation authority boundaries unclear | Low | Temporary delegation signed by device subkey contradicts three-level hierarchy; verification rules unspecified |
| 10 | NIP-44 shared secret lacks per-context derivation | Low | All communication types between two parties share the same ECDH secret; no isolation between contexts |

---

## Overall Assessment

The Gozzip protocol makes reasonable cryptographic choices for a system targeting social peers rather than adversarial miners. The primitives are standard and well-chosen (secp256k1, HKDF-SHA256, NIP-44 ChaCha20-Poly1305, SHA-256 Merkle trees). The documentation is unusually honest about limitations, which is the most important property of a security design.

The critical weakness is the concentration of authority in the root key. A single 32-byte secret controls identity, governs profile changes, derives all device keys, and enables retroactive decryption of all DMs across all time periods. The key hierarchy is presented as four levels, but it functions as one level (root) with derived views. Decoupling the DM key lineage from root is the single most impactful improvement available.

The forward secrecy claim should be retracted or reframed. "Bounded forward secrecy" is a term that sounds like a weaker form of a strong guarantee, when it is actually a different mechanism (rotation with deletion) that provides no forward secrecy against the most realistic compromise scenario (root key exposure via cloud backup or device compromise that reaches the root key). Honest terminology -- "periodic key rotation" -- sets correct expectations.

The proof-of-storage and request token mechanisms are examples of designs that add protocol complexity without proportional security benefit. The proof-of-storage mechanism is adequate for the threat model (the document is honest about this) but should not be called "proof of storage." The request token provides zero privacy in practice and should be replaced with either nothing (simpler) or genuine encrypted retrieval (more secure).

The overall architecture is sound. The issues identified are fixable without redesigning the core protocol. The most valuable changes, in priority order, are: (1) decouple DM keys from root key, (2) rename "bounded forward secrecy" to "periodic key rotation," (3) specify social recovery precisely, (4) upgrade to Merkle proof challenges. These four changes would address all critical and high-severity issues while simplifying the protocol.
