# Identity

Key management and authentication in Gozzip. Built on Nostr's secp256k1 keypairs with a device delegation layer.

## Key Hierarchy

```
Root keypair (secp256k1) — cold storage (hardware wallet, air-gapped, or secure enclave)
│
├── DM key = KDF(root, "dm-decryption-" || rotation_epoch)
│   ├── On DM-capable devices only (dm flag in kind 10050)
│   ├── Rotates every 90 days — old keys deleted after 7-day grace
│   └── Published as dm_key in kind 10050
│
├── Governance key = KDF(root, "governance")
│   ├── On trusted devices only — signs kind 0 (profile), kind 3 (follows)
│   └── Published as governance_key in kind 10050
│
├── Device subkeys (independent per-device)
│   ├── Sign kind 1, 6, 7, 9, 14, 30023 — day-to-day events
│   └── Listed as device tags in kind 10050
│
└── Root key itself
    └── Signs kind 10050 only — add/revoke devices, update derived key mappings
        Used rarely, from cold storage
```

**Root key** — the user's identity. Same format as a Nostr keypair. Lives in cold storage (hardware wallet, air-gapped machine, or secure enclave). Only used to sign kind 10050 (device delegation updates). Existing Nostr keys work as Gozzip root keys.

**DM key** — derived from root via KDF with a rotation epoch. Only DM-capable devices (those with the `dm` flag in kind 10050) hold this. Published in kind 10050 so DM senders can encrypt to it. Rotates every 90 days for bounded forward secrecy.

**Governance key** — derived from root via KDF. Only trusted devices hold this. Signs profile and follow list updates.

**Device keys** — per-device keypairs authorized via kind 10050. Sign day-to-day events. If a device is compromised, revoke it without losing identity.

## Compromised Device Impact

| Action | Root key | Governance key | DM key | Device key |
|--------|----------|---------------|--------|------------|
| Post, react, repost | — | — | — | Yes |
| Read DMs | — | — | Yes | — |
| Update profile/follows | — | Yes | — | — |
| Add/revoke devices | Yes | — | — | — |
| Publish checkpoint | — | — | — | Yes (if delegated) |

A compromised DM-capable phone (device key + DM key) can post and read DMs. A compromised non-DM device can only post. Neither can re-authorize itself, change the follow list, or forge the profile. The user revokes it from cold storage (root key signs new kind 10050 without the compromised device). After DM key rotation (90-day cycle), a previously compromised DM key stops working for new messages.

## Device Delegation

The root key publishes a kind 10050 event listing all authorized device pubkeys. See [Messages > Device Delegation](messages.md#new-kind-device-delegation-10050) for the full schema.

- Adding a device: publish new 10050 with the device added
- Revoking a device: publish new 10050 without it
- Replaceable event — only the latest version matters

All delegation changes require signing with the root key from cold storage. See [ADR 007](../decisions/007-key-derivation-hierarchy.md).

## Authentication Flow

When a client subscribes to a root pubkey, it resolves device→root itself:

1. Client fetches kind 10050 → gets authorized device pubkeys
2. Client subscribes to events from each device pubkey
3. Client merges into unified feed attributed to root identity

The `root_identity` tag on device-signed events provides a fast path — clients (and optionally relays) can use this tag without resolving the delegation chain on every query.

**Optional relay optimization (oracle resolution):** An optimized relay can cache Kind 10050 mappings and index by `root_identity` tag, returning a unified feed in one round-trip instead of three. This is an acceleration, not a requirement — the protocol works with any standard Nostr relay.

## Multi-Device Merge for Replaceable Events

When multiple devices update the same replaceable event (kind 0, kind 3) independently, a fork occurs. Gozzip clients detect and resolve forks automatically using the `prev` tag. See [ADR 003](../decisions/003-replaceable-event-merge.md) for rationale.

### Fork detection

Every kind 0 and kind 3 event carries a `prev` tag pointing to the event it replaces. Two events with the same `prev` value are a fork.

### Follow list (kind 3)

Set-based merge against the common ancestor:

- `merged = ancestor + added_by_either_side - removed_by_either_side`
- Conflict (one side follows, other unfollows same pubkey): **most recent action wins** — compare `followed_at` / `unfollowed_at` timestamps from the respective kind 3 events. See [ADR 008](../decisions/008-protocol-hardening.md).

### Profile (kind 0)

Per-field merge of the `content` JSON:

- If only one side changed a field, that value wins
- If both changed the same field, later timestamp wins (with a 60-second tiebreaker — see [Multi-Device Sync](../architecture/multi-device-sync.md#bounded-timestamps))

### Merge execution

- Automatic — whichever device detects the fork first publishes the merged result
- Deterministic — if multiple devices merge the same fork, they produce the same result
- The merged event's `prev` tag points to both fork tips

## DM Encryption

DMs encrypt to the `dm_key` published in kind 10050 (not the root pubkey directly). All devices can decrypt because they all hold the derived DM private key. See [ADR 007](../decisions/007-key-derivation-hierarchy.md).

- NIP-44 encryption targets the recipient's `dm_key` (from their kind 10050)
- NIP-59 sealed gifts wrap the encrypted event for metadata privacy
- Device pubkey signs the outer event
- Compromising a device reveals the DM key but not the root key

**AEAD integrity:** NIP-44 uses authenticated encryption (AEAD). Tampered ciphertext fails decryption with an authentication error. Storage peers holding encrypted DM blobs cannot serve corrupted ciphertext — AEAD proves integrity on the recipient's end.

### DM Key Rotation

The DM key rotates every 90 days for bounded forward secrecy. See [ADR 008](../decisions/008-protocol-hardening.md).

- Rotation epoch: `floor(unix_timestamp / (90 * 86400))`
- New key derived via `KDF(root, "dm-decryption-" || rotation_epoch)`
- Root key signs a new kind 10050 with the updated `dm_key`
- Old DM private keys are deleted from devices after a 7-day grace window post-rotation
- 7-day grace window: devices retain the previous key for 7 days to handle late-arriving messages
- Senders always encrypt to the current `dm_key` from the recipient's latest kind 10050
- Compromise of a DM key reveals at most ~97 days of DMs (90-day window + 7-day grace)

### Per-Device DM Capability

Not all devices need DM access. The `dm` flag in kind 10050 device tags controls which devices receive the DM private key. See [ADR 008](../decisions/008-protocol-hardening.md).

- Device tag format: `["device", "<pubkey>", "<type>", "dm"]`
- Only devices with the `dm` flag hold the DM private key
- Devices without the flag can post, react, repost but cannot read or send DMs
- Users choose which devices are DM-capable during device setup
- Reduces the attack surface: fewer devices hold the DM key means fewer targets

## Social Recovery

N-of-M social recovery for root key loss. Resolves the root key recovery problem via trusted WoT contacts. See [ADR 008](../decisions/008-protocol-hardening.md).

### Setup

1. User designates M recovery contacts (trusted WoT members)
2. Publishes kind 10060 (recovery delegation) for each contact, encrypted to that contact
3. Each 10060 specifies the recovery threshold N (minimum attestations required)

### Recovery Flow

1. User loses root key, generates a new root keypair
2. Contacts N of M recovery contacts out-of-band
3. Each cooperating contact verifies the user's identity and publishes kind 10061 (recovery attestation) pointing to the new root key
4. When N attestations exist for the same new root key, a 7-day timelock begins
5. During the timelock, the old root key can cancel the recovery (proves it wasn't actually lost)
6. After 7 days with no cancellation, clients and relays accept the new root key as the identity successor

### Security Properties

- **Threshold:** N-of-M prevents a single compromised recovery contact from stealing the identity
- **Timelock:** 7-day window gives the legitimate owner time to cancel a fraudulent recovery
- **Out-of-band verification:** Recovery contacts must verify the user's identity through a channel outside the protocol
- **Cancellation:** The old root key always has priority during the timelock

## Governance Key Change Notification

Required client behavior to detect unauthorized governance key usage. See [ADR 008](../decisions/008-protocol-hardening.md).

- Clients monitor kind 0 and kind 3 events for the user's root identity
- When an update is signed by a device pubkey that isn't the current device, show a notification
- Notification includes: which device signed the update, what changed, timestamp
- This is client-side behavior — not a protocol change
- Protects against silent profile or follow list modification from a compromised governance key

## Resolved Questions

- **How do replaceable events merge across devices?** — `prev` tag enables fork detection; follow list uses set-based merge (most recent action wins on conflict — see [ADR 008](../decisions/008-protocol-hardening.md)); profile uses per-field latest-timestamp. See [ADR 003](../decisions/003-replaceable-event-merge.md).
- **Is there a "primary device" for replaceable events?** — No. Any device can update and merge. Deterministic merge rules eliminate the need for a leader.
- **Should device keys be derived from the root key or independent?** — Independent. Device subkeys are generated per-device. The DM key and governance key are derived from root via KDF, but device keys are not — this limits blast radius if a device is compromised. See [ADR 007](../decisions/007-key-derivation-hierarchy.md).
- **Should there be device capability scoping?** — Yes. Governance key restricts kind 0/kind 3 signing to trusted devices. Checkpoint delegation restricts kind 10051 signing. Device keys handle day-to-day events. See [ADR 007](../decisions/007-key-derivation-hierarchy.md).
- **How does a user recover if the root key is lost?** — N-of-M social recovery via kinds 10060/10061. User designates M recovery contacts; N attestations + 7-day timelock enable root key rotation. See [ADR 008](../decisions/008-protocol-hardening.md).
