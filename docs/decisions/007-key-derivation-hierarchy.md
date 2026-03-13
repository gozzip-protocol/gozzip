# 007 — Key Derivation Hierarchy

## Status

Accepted

## Context

Device delegation alone provides identity isolation but not key isolation. If all devices hold the root private key, a compromised device can:

- Re-authorize itself by publishing a new kind 10050
- Forge profile updates (kind 0) and follow list changes (kind 3)
- Read all DMs (root key is the encryption target)
- Undo its own revocation — it holds the key needed to re-add itself

The root key on every device is a single point of failure.

## Decision

Move the root key to cold storage and derive purpose-specific keys using HKDF-SHA256 (RFC 5869):

**KDF specification:**
- IKM: root private key (32 bytes)
- Salt: `gozzip-v1` (fixed protocol constant)
- Info: purpose label string (e.g., `device-0`, `dm`, `governance`)
- Output: 32 bytes, interpreted as secp256k1 scalar (reduced mod n; retry with incremented counter if result is zero)

```
Root key (cold storage — hardware wallet, air-gapped, or secure enclave)
│
├── DM key = HKDF-SHA256(root, "dm-decryption-" || rotation_epoch)
│   └── On DM-capable devices only — decrypts DMs. Rotates every 90 days (default; configurable).
│
├── Governance key = HKDF-SHA256(root, "governance")
│   └── On trusted devices only — signs kind 0 (profile), kind 3 (follows)
│       Supports event-driven rotation if compromise is suspected.
│
├── Device subkeys (independent per-device)
│   └── Sign kind 1, 6, 7, 9, 14, 30023 — day-to-day events
│
└── Root key itself
    └── Signs kind 10050 (delegation) only — add/revoke devices
        Used rarely, from cold storage
```

Kind 10050 is extended with `dm_key`, `governance_key`, and `checkpoint_delegate` tags to publish derived key public keys. DMs encrypt to the `dm_key` pubkey instead of the root pubkey.

### Governance Key Rotation

The governance key supports rotation via a signed rotation event (kind TBD). The current governance key signs an event containing the new governance key's public key. Nodes verify the chain: root → old governance → new governance. Rotation should be performed if governance key compromise is suspected. Unlike the DM key (which rotates on a schedule), governance key rotation is event-driven.

### DM Key Rotation Period

The DM key rotation period defaults to 90 days. Users in high-threat environments may reduce this to 30 days for a smaller exposure window on device compromise. The trade-off is more frequent key distribution events (kind 10050 updates). The rotation epoch is computed as `floor(unix_timestamp / (rotation_period * 86400))`.

## Rejected Alternative: Root Key on All Devices

Share the root private key across all devices (the standard Nostr approach). Rejected because a single compromised device can re-authorize itself, forge profile/follow updates, read all DMs, and undo its own revocation. No way to limit blast radius.

## Consequences

**Positive:**
- Compromised device (device key + DM key) can post and read DMs but CANNOT re-authorize itself, change follows, or forge profile
- Root key exposure surface drops to rare delegation updates from cold storage
- Governance key can be restricted to trusted devices (e.g., desktop only)
- Checkpoint publishing can be delegated to any device without root key

**Negative:**
- Users must manage cold storage for the root key (hardware wallet, secure enclave)
- DM senders must look up `dm_key` from kind 10050 instead of using root pubkey directly
- More complex key management UX

**Neutral:**
- Existing Nostr keys still work as root keys — the hierarchy is additive
- Nostr users migrating to Gozzip can adopt the hierarchy incrementally
