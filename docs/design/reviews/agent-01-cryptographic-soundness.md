# Cryptographic Soundness Review: Gozzip Protocol

**Reviewer:** Agent-01 (adversarial cryptographic review)
**Date:** 2026-03-12
**Scope:** Key hierarchy, encryption, blinding, proofs of storage, delegation, social recovery, channel security
**Documents reviewed:** Whitepaper (gossip-storage-retrieval.tex), ADRs 005-008, protocol/identity.md, protocol/messages.md, design/proof-of-storage-alternatives.md, design/surveillance-surface.md

---

## Executive Summary

The Gozzip protocol demonstrates above-average design discipline for a social-layer protocol. The key hierarchy is structurally sound, the threat model is honestly stated, and the protocol is self-aware about its limitations (e.g., the proof-of-storage-alternatives.md document). The remaining issues are primarily around NIP-46 metadata exposure, revocation latency, key deletion enforcement, and the fundamental limitations of social recovery.

**Remaining issue count by severity:**
- Significant: 1
- Minor: 2
- Nitpick: 1

---

## 1. Key Hierarchy

### 1.1 Device Key Compromise Does Not Lead to Root Key Compromise

**Claim:** "Compromised device (device key + DM key) can post and read DMs but CANNOT re-authorize itself, change follows, or forge profile."

**Analysis:** This claim holds, with an important caveat. Device keys are generated independently (not derived from root), so compromising a device key reveals nothing about the root key. The DM key and governance key are derived from root, but the derivation is one-way (assuming a proper KDF -- see 1.1). Possessing a derived key does not enable recovery of the root key under standard assumptions.

However, the DM key IS on every DM-capable device. If an attacker compromises a DM-capable device, they obtain:
- The device private key (can forge posts as that device)
- The current DM private key (can read all current and recent DMs)
- Potentially the governance key if the device is "trusted"

The blast radius is correctly analyzed in the protocol documentation. The key hierarchy successfully prevents a compromised device from escalating to root-level authority.

**Does the claim hold?** Yes, assuming a secure KDF (see 1.1).

**Severity:** N/A (claim holds)

### 1.2 Root Key in Cold Storage is the Right Call but Creates a Liveness Problem

**Claim:** The root key lives in cold storage and is used rarely.

**Analysis:** This is cryptographically sound but creates a practical tension. Revoking a compromised device requires the root key. If the root key is truly air-gapped (hardware wallet, offline machine), the user cannot revoke a compromised device quickly. An attacker with a stolen device key could post malicious content for hours or days before the user accesses cold storage.

The protocol acknowledges this implicitly through the governance key change notification (ADR 008, item 8), but doesn't address the revocation latency.

**Does the claim hold?** The security claim holds. The availability/responsiveness claim is unaddressed.

**Severity:** Minor

**Suggested fix:** Document expected revocation latency. Consider an "emergency revocation" mechanism where the governance key can temporarily suspend (not permanently revoke) a device, with the root key required to confirm within a time window.

---

## 2. NIP-44 Encryption and DM Forward Secrecy

### 2.1 NIP-44 Is Adequate for Message Encryption but Provides No Forward Secrecy

**Claim:** "NIP-44 uses authenticated encryption (AEAD). Tampered ciphertext fails decryption with an authentication error."

**Analysis:** NIP-44 uses XChaCha20-Poly1305 with a shared secret derived from ECDH on secp256k1. This provides IND-CCA2 security (semantic security under adaptive chosen-ciphertext attack). The AEAD construction is sound. However, NIP-44 is a static Diffie-Hellman construction -- the same shared secret is used for all messages between a sender-recipient pair until one of them rotates their key. This means:

- Compromising either party's private key retroactively decrypts all past messages encrypted under that key pair.
- There is no per-message ephemeral key exchange (no ratchet, no Double Ratchet, no Signal protocol).

The protocol is honest about this: "DM key rotation provides bounded forward secrecy but not perfect." This is accurate.

**Does the claim hold?** The AEAD claim holds. The forward secrecy is correctly described as bounded.

**Severity:** N/A (accurately described)

### 2.2 DM Key Deletion Relies on Client Compliance

**Claim:** "Old keys deleted from devices after a 7-day grace window."

**Analysis:** This is a client-side policy, not a cryptographic guarantee. The protocol has no mechanism to enforce that old DM keys are actually deleted. A malicious or buggy client could retain old keys indefinitely, defeating the forward secrecy guarantee entirely. A forensic examiner who images a device's storage could recover deleted keys from disk if the deletion was not performed with secure erasure (overwriting with random data).

**Does the claim hold?** Only if clients correctly implement secure key deletion.

**Severity:** Minor

**Suggested fix:** Acknowledge that forward secrecy depends on correct client implementation of key deletion. Recommend that client implementations use platform-specific secure storage APIs (iOS Keychain with `kSecAttrAccessibleWhenUnlockedThisDeviceOnly`, Android Keystore) that provide hardware-backed key storage with deletion guarantees.

---

## 3. NIP-46 Encrypted Channels

### 3.1 The Relay Is a Metadata-Aware Intermediary, Not a Trusted Third Party

**Claim:** "Any two peers can establish an encrypted channel through a shared relay without exchanging IP addresses or requiring simultaneous online presence."

**Analysis:** The encryption claim is correct -- NIP-46 uses NIP-44 encryption, so the relay cannot read message contents. However, the relay is NOT merely an oblivious pipe:

1. **The relay sees who communicates with whom.** NIP-46 messages have `p` tags indicating the recipient. The relay sees both the sender's pubkey (in the event's `pubkey` field) and the recipient's pubkey (in the `p` tag). This is sufficient to build a communication graph.

2. **The relay sees timing.** When Alice stores a message and when Bob fetches it reveals their online patterns and interaction frequency.

3. **The relay can selectively deny service.** A malicious relay can drop NIP-46 messages for specific pubkeys, disrupting pact management, challenge-response, and event sync.

4. **The relay can correlate NIP-46 traffic with other activity.** If Alice uses the same relay for NIP-46 pact communication and for publishing kind 1 events, the relay can correlate her social activity with her pact management.

The surveillance-surface.md document acknowledges points 1-2: "Timing of all operations... NIP-46 store/fetch timing correlation." This is accurate. But the protocol's description of NIP-46 channels as providing privacy guarantees should be more precise.

**Does the claim hold?** The confidentiality claim holds (relay cannot read content). The privacy claim is overstated (relay sees who communicates with whom and when).

**Actual guarantee:** NIP-46 provides message confidentiality and IP address hiding between peers. It does not provide metadata privacy -- the relay learns the communication graph and timing patterns.

**Severity:** Significant

**Suggested fix:** Use NIP-59 gift wrapping for NIP-46 messages. This hides the recipient's pubkey from the relay (the `p` tag is in the encrypted inner event, not the outer wrapper). The relay sees the sender's pubkey and an opaque blob, but not the recipient. This is already used for kind 10059 endpoint hints -- apply the same pattern to NIP-46 pact communication.

---

## 4. Device Delegation

### 4.1 Delegation Is Sound but Revocation Is Slow

**Claim:** "Revoking a device: publish new 10050 without it. Replaceable event -- only the latest version matters."

**Analysis:** The delegation mechanism is straightforward and correct. A kind 10050 event signed by the root key lists authorized device pubkeys. Revocation removes the device from the list. The replaceable event semantics (only the latest version matters) ensure that revocation is permanent once published.

However, revocation propagation depends on relay availability and client polling:

1. The root key signs a new kind 10050 and publishes it to relays.
2. Relays that receive it replace the old version.
3. Other clients fetch the updated kind 10050 on their next poll.
4. Until a client fetches the update, it accepts events from the revoked device.

The window between revocation and propagation is the vulnerability. A revoked device that still has network access can publish events during this window, and clients that haven't fetched the updated kind 10050 will accept them.

**Does the claim hold?** Yes, eventually. Revocation is correct but not instant.

**Severity:** Minor (acknowledged trade-off of eventual consistency)

**Suggested fix:** Consider a short-lived credential model where device keys have an expiry (e.g., 7 days) and must be re-authorized by the root key periodically. This bounds the window even if revocation propagation fails. This adds complexity and root key usage frequency, so it's a trade-off.

---

## 5. Social Recovery

### 5.1 Recovery Attestation Has No Proof of Identity Verification

**Claim:** "Recovery contacts must verify the user's identity through a channel outside the protocol."

**Analysis:** The out-of-band verification is critical, but the protocol provides no way to verify that it actually happened. A recovery contact can publish a kind 10061 attestation without performing any identity verification. The attestation contains no proof of verification -- it is simply a signed statement saying "I believe this new key belongs to the same person."

This means the security of social recovery is entirely dependent on the diligence and honesty of the recovery contacts. The protocol provides no cryptographic enforcement of the verification step.

**Does the claim hold?** The mechanism works as designed. The verification is a social obligation, not a cryptographic one.

**Severity:** Minor (this is a fundamental limitation of social recovery schemes, not specific to Gozzip)

**Suggested fix:** Consider adding a challenge-response element to the recovery flow. For example: the recovery delegation (kind 10060) could include a secret encrypted to both the recovery contact and the user. During recovery, the user must present this secret to the recovery contact, proving they are the same person who set up the recovery. This is not foolproof (the secret could be compromised), but it adds a verification step beyond "I trust you."

### 5.2 Old Root Key Cancellation Creates a Paradox

**Claim:** "The old root key can cancel the recovery at any time during the 7-day timelock."

**Analysis:** If the user lost the root key (the recovery scenario), they cannot cancel. This is by design -- the recovery exists precisely because the root key is unavailable. But this creates a paradox:

- If the user can cancel, they have the root key and don't need recovery.
- If they can't cancel, a fraudulent recovery by colluding contacts succeeds after the timelock.

The timelock's purpose is to handle the case where the root key was NOT lost but someone is attempting a fraudulent recovery. The user notices, uses their root key to cancel, and the attack fails. This is a reasonable design for the "my contacts are trying to steal my identity" threat.

But it provides zero protection for the "I actually lost my key and my contacts are colluding against me" threat. This is a fundamental limitation of social recovery, not a protocol bug, but the documentation should state it clearly.

**Does the claim hold?** The mechanism is logically consistent but cannot protect against simultaneous key loss and contact collusion.

**Severity:** Nitpick (fundamental limitation, not fixable)

---

## 6. Additional Cryptographic Issues

### 6.1 Per-Device Hash Chain (seq + prev_hash) is Sound but Seq Recovery is Fragile

**Claim:** "seq -- monotonically increasing sequence number per device. prev_hash -- hash of the previous event ID from this device."

**Analysis:** The per-device hash chain is a correct construction for detecting gaps and reordering. However, the seq recovery mechanism (ADR 008, item 10) introduces a vulnerability:

"On device initialization, query the last known seq from peers/relays before publishing. Resume from last_known_seq + 1."

If all queried sources have been compromised or are maliciously returning a low seq value, the device will reuse sequence numbers, creating a fork in the hash chain. This is detectable (prev_hash will mismatch), but the detection relies on someone holding the original chain.

**Does the claim hold?** The hash chain itself is sound. The seq recovery mechanism is a best-effort heuristic.

**Severity:** Nitpick

---

## Summary Table

| # | Issue | Claim | Holds? | Severity | Fix Priority |
|---|-------|-------|--------|----------|-------------|
| 1.1 | Root key liveness | Cold storage is secure | Yes, but slow | Minor | Low |
| 2.1 | Key deletion enforcement | Old keys are deleted | Client-dependent | Minor | Low |
| 3.1 | Relay sees NIP-46 metadata | Channels are private | Confidentiality yes, metadata no | Significant | Medium |
| 6.1 | Seq recovery fragility | Hash chain detects gaps | Best-effort recovery | Nitpick | Low |

---

## Overall Assessment

The Gozzip protocol's cryptographic architecture is structurally sound for its stated threat model (social-layer storage among WoT peers, not adversarial financial actors). The key hierarchy successfully isolates device, DM, governance, and root key authority. The choice of NIP-44 for encryption and secp256k1 for identity is well-established and appropriate.

The remaining significant issue (NIP-46 metadata exposure) is a design trade-off that the protocol team has largely already identified and documented. The main recommendation is to make the actual guarantees more precise in the documentation, so that implementers and users do not overestimate the security properties.

The protocol's greatest cryptographic strength is its honesty: the proof-of-storage-alternatives.md and surveillance-surface.md documents demonstrate that the designers understand their limitations.
