# iroh Identity Mapping

## Dual-Key Model

Each gozzip node operates with two distinct cryptographic identities:

- **Ed25519** (iroh transport identity): Used by the iroh `Endpoint` for QUIC handshakes, peer authentication, and gossip protocol operations. This is the `NodeId` in iroh terminology.
- **secp256k1** (Nostr application identity): Used for signing Nostr events, authenticating application-layer messages, and establishing social-graph relationships (follows, pacts, WoT).

These keys use different elliptic curves and serve different purposes. The Ed25519 key is the node's *network* identity; the secp256k1 key is the user's *social* identity.

## Cross-Key Binding (Kind 10070)

A signed attestation links both keys into a verifiable identity pair. This is published as a Nostr replaceable event (Kind 10070).

The binding requires **bidirectional signatures**:

1. The secp256k1 key signs the Ed25519 public key (proves the Nostr identity claims ownership of the iroh node).
2. The Ed25519 key signs the secp256k1 public key (proves the iroh node claims association with the Nostr identity).

**Both directions are required.** Without bidirectional binding, identity mismatch attacks are trivial:

- An attacker could claim someone else's Ed25519 key if only the secp256k1 signature were required.
- An attacker could claim someone else's Nostr identity if only the Ed25519 signature were required.

The Kind 10070 event contains both signatures and both public keys, allowing any verifier to confirm the binding without contacting either key holder.

## Key Independence

The Ed25519 key is **NOT** derived from the secp256k1 key (or vice versa). This is a deliberate design choice for compartmentalization:

- Compromise of the secp256k1 root key does not compromise the transport layer.
- Compromise of the Ed25519 key does not compromise the Nostr identity.
- Each key can be rotated independently (though with different operational costs).
- The two keys can use different storage backends with different security properties.

## Key Storage

The Ed25519 key is **permanently hot** — it is needed for every QUIC handshake, which occurs on every new peer connection. This has direct implications for storage:

- **Secure Enclave / TEE**: Use hardware-backed key storage where available (Apple Secure Enclave, Android StrongBox, TPM 2.0). The key never leaves the secure element; signing operations are delegated to it.
- **Encrypted at rest**: When hardware backing is unavailable, the key is encrypted at rest using device biometric (fingerprint, face) or device PIN as the key derivation input.
- **NOT synced to cloud backup**: The Ed25519 key is device-specific. iCloud Keychain, Google Backup, and similar services must be excluded. Syncing the key to the cloud defeats device-level compartmentalization.

## Key Rotation

Changing the Ed25519 key changes the iroh `NodeId`. This is a disruptive operation with cascading effects:

1. **All gossip memberships are lost.** HyParView tracks peers by NodeId. A new NodeId is a new peer — no history, no view membership.
2. **Re-publish Kind 10070.** The cross-key binding must be updated with the new Ed25519 key and fresh bidirectional signatures.
3. **Rejoin all topics.** The node must re-subscribe to discovery, per-author, and any other gossip topics from scratch.
4. **No in-band rotation signal in iroh.** iroh has no built-in mechanism for announcing "NodeId X is now NodeId Y." Rotation must be signaled via a Nostr event (updated Kind 10070) so that peers can discover the new NodeId through the Nostr relay layer.

Key rotation should be treated as an infrequent, planned operation — not a routine action.

## Message Authentication

Every gozzip protocol message is signed by the sender's **secp256k1 key**, adding 96 bytes of overhead per message:

- 32 bytes: secp256k1 compressed public key
- 64 bytes: Schnorr signature

This is necessary because `delivered_from` in iroh-gossip indicates **which neighbor forwarded the message**, NOT who authored it. In a gossip network, messages are relayed through multiple hops. The `delivered_from` field only identifies the last hop.

**Verification on receipt is mandatory.** Every node must:

1. Extract the secp256k1 public key from the message.
2. Verify the Schnorr signature over the message payload.
3. Look up the cross-key binding (Kind 10070) to confirm the secp256k1 key is associated with a known iroh NodeId.
4. Check WoT membership for the claimed identity.

Skipping signature verification — even for messages from "trusted" neighbors — is a critical vulnerability.

## Replay Protection

iroh's built-in deduplication is **content-hash only**. If an attacker replays an identical message, iroh may or may not deduplicate it depending on timing and cache state. This is insufficient for application-layer replay protection.

gozzip implements its own replay protection:

- **Per-sender monotonic nonces**: Each sender maintains a strictly increasing nonce. Receivers track the highest nonce seen per sender and reject any message with a nonce less than or equal to the last seen value.
- **Timestamp windowing (5 minutes)**: Messages with timestamps more than 5 minutes in the past or future are rejected. This bounds the window in which replay is possible and limits the state receivers must maintain.

Both mechanisms are applied together. A message must have a valid (non-replayed) nonce AND fall within the timestamp window to be accepted.
