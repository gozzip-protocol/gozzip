# Messages

Event kinds and their structure in Gozzip. Inherits all Nostr event kinds. Gozzip adds kinds 10050–10065 and 10070; see the [Canonical Kind Registry](#canonical-kind-registry) below.

## Canonical Kind Registry

This section is the single authoritative list of Gozzip custom event kinds. Other documents reference this registry rather than restating ranges.

| Kind | Name | Signed by | Notes |
|------|------|-----------|-------|
| 10050 | Device delegation | root | One per identity; device fleet + derived key mappings |
| 10051 | Checkpoint | checkpoint delegate | Light-node sync + device reconciliation |
| 10052 | Conversation state (DM read) | device | Per-partner read-state sync (ADR 004) |
| 10053 | Storage pact | root | Reciprocal storage commitment; `type` ∈ {standard, bootstrap, guardian} |
| 10054 | Data-availability challenge | — (ephemeral) | Challenge-response verification that a partner can produce stored data on demand |
| 10055 | Storage pact request/advert | root | Broadcast requesting partners; includes guardian advertisements |
| 10056 | Storage pact offer | — | Offer to form a reciprocal pact |
| 10057 | Data request | device | NIP-44-encrypted direct request to a storage peer |
| 10058 | Data offer/response | — | Private response routed to requester via `p` tag |
| 10060 | Recovery delegation | root | Hardened social-recovery contact designation (ADR 008) |
| 10061 | Recovery attestation | recovery contact | Attests a root key rotation |
| 10062 | Push notification token | device | Privacy-preserving wake-up registration |
| 10063 | Deletion request | root/governance | `reason` ∈ {user_request, gdpr, content_violation, error} |
| 10064 | Content report | device (+ `root_identity`) | Report to relay operators; encrypted detail |
| 10065 | Temporary suspension | governance | Governance emergency device suspension |
| 10070 | iroh cross-key binding | root | secp256k1 ↔ Ed25519 endpoint attestation (ADR 011) |

## Inherited Nostr Kinds

All standard Nostr events work unchanged. Device-signed events add a `root_identity` tag to attribute authorship.

| Kind | Name | Signed by | Notes |
|------|------|-----------|-------|
| 0 | Profile metadata | governance | `prev` tag for fork detection |
| 1 | Short note | device | `root_identity` tag added |
| 3 | Follow list | governance | `prev` tag for fork detection |
| 4 | DM (deprecated) | — | Use kind 14 instead |
| 6 | Repost | device | `root_identity` tag added |
| 7 | Reaction | device | `root_identity` tag added |
| 9 | Group chat message | device | `root_identity` tag added |
| 13 | Seal (NIP-17) | device | Signed, unencrypted-to-relay wrapper around a kind 14 chat message |
| 14 | Chat message (NIP-17) | device | Encrypted DM payload, sealed (kind 13) then gift-wrapped (kind 1059) |
| 1059 | Gift wrap (NIP-59) | ephemeral key | Outer wrapper hiding sender/recipient metadata; carries the sealed DM |
| 40–44 | Public channels (NIP-28) | device | Channel create/metadata/message/hide/mute |
| 30023 | Long-form article | device | `root_identity` tag added |
| 9734 | Zap request | device | Targets root pubkey for payment |
| 9735 | Zap receipt | LNURL server | Standard NIP-57 |
| 39000-39009 | Group metadata | group relay | Standard NIP-29 |

**Reference implementation (v2).** This document describes the v1 protocol core. The reference implementation is at `PROTOCOL_VERSION = 2`, which layers NIP-17/NIP-44 encrypted DMs (kinds 1059 GiftWrap, 13 Seal, 14 chat), NIP-28 public channels, and iroh transport on top of that core. NIP-44 is the encryption primitive used throughout Gozzip (device metadata, recovery, data requests, DMs). The deferred versioning strategy is in [protocol-versioning.md](../future/protocol-versioning.md).

## New Kind: Device Delegation (10050)

Replaceable event — one per identity, updated when devices are added or revoked.

Device metadata events (kind 10050) MUST place device type, uptime classification, and DM capability flags in NIP-44 encrypted content. Only the user's pact partners (who need this information for pact management) should be able to decrypt it. Public tags should contain only the device subkey pubkey and root identity reference. This prevents adversaries from fingerprinting a user's device fleet, inferring uptime patterns, or identifying which devices handle DMs.

```json
{
  "kind": 10050,
  "pubkey": "<root_pubkey>",
  "tags": [
    ["device", "<device_pubkey>"],
    ["device", "<device_pubkey_2>"],
    ["device", "<device_pubkey_3>"],
    ["dm_key", "<derived_dm_pubkey>"],
    ["governance_key", "<derived_governance_pubkey>"],
    ["checkpoint_delegate", "<device_pubkey>"],
    ["checkpoint_delegate", "<device_pubkey_2>"],
    ["protocol_version", "1"]
  ],
  "content": "<NIP-44 encrypted to pact partners: {\"devices\": [{\"pubkey\": \"<device_pubkey>\", \"type\": \"mobile\", \"dm\": true, \"uptime\": 0.28, \"uptime_ts\": <unix_timestamp>}, {\"pubkey\": \"<device_pubkey_2>\", \"type\": \"desktop\", \"dm\": true, \"uptime\": 0.91, \"uptime_ts\": <unix_timestamp>}, {\"pubkey\": \"<device_pubkey_3>\", \"type\": \"extension\", \"dm\": false, \"uptime\": 0.52, \"uptime_ts\": <unix_timestamp>}]}>",
  "sig": "<signed by root>"
}
```

- Device type, uptime, and DM capability are in NIP-44 encrypted `content` — only pact partners can decrypt
- Public `device` tags contain only the device subkey pubkey (no type, no capability flags)
- `uptime` — per-device rolling 7-day uptime average (0.0–1.0) and timestamp of last measurement. Computed locally by each device from its own online/offline log. Updated at most once per day. Used by pact partners for device-priority routing: 90%+ uptime devices are treated as full nodes (primary pact endpoint), 50-89% as active light nodes, 10-49% as intermittent, <10% as passive. The protocol automatically promotes/demotes devices as their uptime changes — no user configuration needed. Device types: `mobile`, `desktop`, `extension`, `server`. Users can omit uptime data to opt out (protocol falls back to treating all devices as intermittent).
- `dm_key` — public key for DM encryption. Derived from root via `HKDF-SHA256(root, "dm-decryption-" || rotation_epoch)`. Rotates every 90 days (default; configurable) — a new kind 10050 is published with the updated key. Old keys are deleted from devices after a 7-day grace window. Only devices with the `dm` capability (in encrypted content) hold the corresponding private key.
- The `dm` flag in encrypted content indicates the device holds the DM private key. Devices without this flag cannot read or send DMs. See [ADR 008](../decisions/008-protocol-hardening.md).
- `governance_key` — public key that signs kind 0, kind 3. Only trusted devices hold this private key.
- `checkpoint_delegate` — devices authorized to sign kind 10051. Any delegated device can publish checkpoints without holding the root key.
- Clients fetch this to resolve device→root mappings
- Followers never see it directly — clients resolve it transparently
- Optimized relays can cache this for faster oracle resolution (optional — see [Relay](../actors/relay.md))
- Revoke a device by publishing a new 10050 without it

## New Kind: Checkpoint (10051)

Replaceable event — published periodically or on demand. Enables light node sync and serves as the device reconciliation mechanism (see [ADR 002](../decisions/002-checkpoint-reconciliation.md)).

```json
{
  "kind": 10051,
  "pubkey": "<root_pubkey>",
  "tags": [
    ["device", "<device_pubkey_1>", "<head_event_id>", "<sequence_n>"],
    ["device", "<device_pubkey_2>", "<head_event_id>", "<sequence_n>"],
    ["follow_list_ref", "<kind_3_event_id>"],
    ["profile_ref", "<kind_0_event_id>"],
    ["merkle_root", "<root_hash>", "<event_count>"],
    ["protocol_version", "1"]
  ],
  "sig": "<signed by checkpoint_delegate>"
}
```

- `profile_ref` — event ID of the current kind 0, enables fast profile sync
- `merkle_root` — Merkle root of all events in the current checkpoint window. Enables completeness verification: requesters compute the root from received events and compare. `event_count` provides expected total. See [ADR 006](../decisions/006-storage-pact-risk-mitigations.md).

  **Merkle tree construction:** SHA-256 binary hash tree. Leaves are the 32-byte event IDs (the SHA-256 hash that Nostr already computes for each event), ordered by `(device_pubkey, seq)` — device pubkey lexicographic first, then sequence number ascending. If the number of leaves is not a power of two, duplicate the last leaf to fill the next power of two. Internal nodes: `SHA-256(left_child || right_child)`. Two implementations given the same event set MUST produce the same root.
- Light nodes fetch this + last N events per device to sync
- Full nodes ignore it and use full history
- Checkpoint delegates (authorized devices) publish these
- Optimized relays can index these for faster light node sync (optional)
- Any checkpoint delegate (listed in kind 10050) publishes an updated checkpoint after reconciling sibling events

## New Kind: Conversation State (10052)

Parameterized replaceable event — one per conversation partner. Tracks read-state for DM sync across devices. See [ADR 004](../decisions/004-dm-conversation-state.md).

```json
{
  "kind": 10052,
  "pubkey": "<root_pubkey>",
  "tags": [
    ["d", "<conversation_partner_root_pubkey>"],
    ["read_until", "<unix_timestamp>"],
    ["protocol_version", "1"]
  ],
  "content": "<NIP-44 encrypted to own root pubkey>",
  "sig": "<signed by device key>"
}
```

- **Parameterized replaceable** — `d` tag keys one event per conversation
- **Encrypted content** — `read_until` is also stored encrypted in `content` for privacy (tags may be visible to relays). Clients use the encrypted value; the tag is a hint for relay-side filtering.
- **Self-addressed** — encrypted to own root pubkey, so all devices can decrypt
- `read_until` only moves forward — latest timestamp always wins, no conflict possible

## Storage Pact Events (10053–10058)

Six event kinds for the reciprocal storage pact layer. Users commit to storing each other's recent events, verified by challenge-response. See [ADR 005](../decisions/005-storage-pact-layer.md).

### Kind 10053 — Storage Pact (private)

Parameterized replaceable (keyed by partner pubkey). Declares a reciprocal storage commitment. Exchanged directly between partners — never published to relays.

```json
{
  "kind": 10053,
  "pubkey": "<root_pubkey>",
  "tags": [
    ["d", "<partner_root_pubkey>"],
    ["type", "standard"],
    ["status", "active"],
    ["since_checkpoint", "<checkpoint_event_id>"],
    ["volume", "<bytes>"],
    ["expires", "<unix_timestamp>"],
    ["protocol_version", "1"]
  ]
}
```

- Both parties publish their own 10053 referencing each other
- The pact exists when both sides have published
- Private — exchanged via encrypted DM or direct connection

**Pact types** (`type` tag):
- `standard` — default reciprocal pact, covers events since last checkpoint (~monthly)
- `bootstrap` — one-sided temporary pact for new users. The followed user stores the new user's data. Auto-expires after 90 days or when the new user reaches 10 reciprocal pacts. See [ADR 006](../decisions/006-storage-pact-risk-mitigations.md).
- `guardian` — one-sided sponsorship pact from an established member (Guardian) to a new user (Seedling). No volume matching, no mutual-follow requirement. Auto-expires after 90 days or when the Seedling reaches the Hybrid phase. See [Pact State Machine](pact-state-machine.md#guardian-pact-lifecycle).

**Pact status** (`status` tag) — reflects the 4-state pact lifecycle (Forming → Active → Failing → Ended; see [Pact State Machine](pact-state-machine.md)):
- `active` — pact is in effect: the partner is challenged and expected to serve data requests. Covers the Active and Failing lifecycle states (Failing is a local scoring condition, not a distinct wire status).
- `ended` — pact is dissolved. Published (encrypted to the former partner) when a pact reaches the Ended state through sustained failure or explicit exit.

Forming is implicit: the pact exists once both sides have published a `status: active` 10053. Standby and archival pacts are deferred and are not part of protocol v1.

### Kind 10054 — Data-Availability Challenge (ephemeral)

Challenge-response data availability verification. Supports two modes. Verifies that a partner can serve the data on demand, not that they store it locally — a well-provisioned proxy could pass either challenge type. See [ADR 006](../decisions/006-storage-pact-risk-mitigations.md).

```json
{
  "kind": 10054,
  "tags": [
    ["p", "<challenged_peer_pubkey>"],
    ["type", "hash"],
    ["challenge", "<nonce>"],
    ["range", "<start_seq>", "<end_seq>"],
    ["protocol_version", "1"]
  ]
}
```

**Challenge types** (`type` tag):
- `hash` — "give me H(events[start..end] || nonce)." Proves possession of the event range. Nonce prevents pre-computation. Works asynchronously — response can arrive hours later. **Exact serialization:** `SHA-256(canonical_json(event_start) || canonical_json(event_start+1) || ... || canonical_json(event_end) || nonce_bytes)` where `canonical_json` is Nostr's event serialization format (the same `[0, pubkey, created_at, kind, tags, content]` array used to compute the event ID). The nonce is 32 random bytes provided in the challenge. Events are ordered by `(device_pubkey, seq)` — the same ordering used for Merkle tree construction.
- `serve` — "give me the full event at position N within the range." Measures response latency. Consistently slow responses (> 500ms) suggest the peer is fetching remotely instead of storing locally. **Only used when both pact partners have direct, persistent connections (both 90%+ uptime).** See [Pact Communication Matrix](../architecture/storage.md#pact-communication-matrix).

**Pair-aware response windows:** The challenge type and response window are determined by the weaker device in the pact pair. Clients read their partner's Kind 10050 uptime tags to select the appropriate mode:

| Pair (weaker device) | Challenge type | Response window |
|----------------------|---------------|----------------|
| Both Full (90%+) | `serve` + `hash` | 500ms / 1h |
| One Active (50-89%) | `hash` only | 4-8h |
| One Intermittent (10-49%) | `hash` only | 24h |

Clients track a rolling latency score per peer. Peers flagged as likely proxying get challenged 3x more frequently. Latency scoring only applies to `serve` challenges (Full↔Full pairs).

### Kind 10055 — Storage Pact Request (DVM-style)

Public broadcast requesting storage partners. Any WoT peer with similar volume can respond.

```json
{
  "kind": 10055,
  "pubkey": "<root_pubkey>",
  "tags": [
    ["volume", "<bytes>"],
    ["min_pacts", "<number_needed>"],
    ["ttl", "3"],
    ["request_id", "<unique_id>"],
    ["protocol_version", "1"]
  ]
}
```

- `ttl` — hop count for gossip forwarding. Each peer decrements and forwards if they can't respond. Reaches up to 20³ ≈ 8,000 nodes naively in a 20-peer network; ≈4,500 after clustering adjustment (see whitepaper §3.1). Hardened with per-hop rate limiting (10 req/s per source), WoT-only forwarding (2-hop boundary), and request deduplication via `request_id`.
- `request_id` — unique identifier for deduplication. Nodes track seen request_ids (LRU cache) and drop duplicates. See [ADR 008](../decisions/008-protocol-hardening.md).

### Kind 10056 — Storage Pact Offer

Response to a kind 10055 request. Peer offers to form a reciprocal pact.

```json
{
  "kind": 10056,
  "tags": [
    ["p", "<requester_root_pubkey>"],
    ["volume", "<bytes>"],
    ["tz", "UTC+9"],
    ["protocol_version", "1"]
  ]
}
```

- `tz` (optional) — timezone offset. Used by clients for geographic diversity in peer selection. Target 3+ timezone bands across storage peers to protect against correlated regional failures.

### Kind 10057 — Data Request (NIP-44 encrypted, direct)

A direct request for a target user's events, sent to a chosen storage peer that is known to hold them (a pact partner of the target). The request is NIP-44-encrypted to the storage peer and addressed to it by pubkey; iroh handles the underlying connection. This is a direct query, not a broadcast — there is no gossip fan-out. See [ADR 017](../decisions/017-three-tier-retrieval.md).

```json
{
  "kind": 10057,
  "pubkey": "<requester_device_pubkey>",
  "tags": [
    ["p", "<storage_peer_pubkey>"],
    ["root_identity", "<requester_root_pubkey>"],
    ["protocol_version", "1"]
  ],
  "content": "<NIP-44 encrypted to storage peer: {\"target\": \"<target_root_pubkey>\", \"since\": \"<checkpoint_event_id_or_timestamp>\"}>"
}
```

- The request is signed by the requester's device key and encrypted to the storage peer. The storage peer learns who is asking (the request is signed so it can rate-limit and route the response) and what is being requested, because it must to answer.
- **Reader privacy relative to vanilla Nostr comes from *where* the metadata flows** — a WoT peer chosen by the requester, not an arbitrary relay operator — not from requester anonymity. The requester's pubkey is visible to the storage peer, exactly as in a Nostr relay query, but only to that one trusted peer.
- Storage peers respond privately via kind 10058.
- Hardened with per-source rate limiting (50 req/s per source pubkey). See [ADR 008](../decisions/008-protocol-hardening.md).

### Kind 10058 — Data Offer (private response)

Private response to a kind 10057 data request. Tells the requester where to connect for the data.

```json
{
  "kind": 10058,
  "tags": [
    ["p", "<requester_pubkey>"],
    ["relay", "<connection_endpoint>"],
    ["protocol_version", "1"]
  ]
}
```

## New Kind: Recovery Delegation (10060)

Parameterized replaceable event — one per recovery contact. Designates a trusted WoT member as a recovery contact for social recovery of the root key. See [ADR 008](../decisions/008-protocol-hardening.md).

Recovery delegation events (kind 10060) MUST encrypt the recovery contact set. The `d` tag contains a pseudonymous identifier `H(recovery_contact_pubkey || owner_pubkey)` rather than the plaintext pubkey. The `threshold` and `total` values MUST be placed in NIP-44 encrypted content, not in public tags. Only the recovery contact and the identity owner can determine the relationship exists. This prevents an attacker from enumerating the recovery set and targeting specific contacts.

```json
{
  "kind": 10060,
  "pubkey": "<root_pubkey>",
  "tags": [
    ["d", "<H(recovery_contact_pubkey || owner_pubkey)>"],
    ["protocol_version", "1"]
  ],
  "content": "<NIP-44 encrypted to recovery contact: {\"role\": \"recovery_contact\", \"threshold\": 3, \"total\": 5, \"instructions\": \"...\"}>"
}
```

- `d` tag — `H(recovery_contact_pubkey || owner_pubkey)`, a pseudonymous identifier (parameterized replaceable key). Neither the recovery contact's identity nor the relationship is visible to third parties.
- `threshold` and `total` — placed in NIP-44 encrypted content to prevent attackers from learning the recovery scheme parameters
- `content` — encrypted instructions, threshold, and total for the recovery contact
- Signed by root key — only the identity owner can designate recovery contacts
- Revoke a recovery contact by publishing a new 10060 without them (or updating threshold/total)

## New Kind: Recovery Attestation (10061)

Regular event published by a recovery contact to attest a root key rotation. Part of the social recovery flow.

```json
{
  "kind": 10061,
  "pubkey": "<recovery_contact_root_pubkey>",
  "tags": [
    ["p", "<old_root_pubkey>"],
    ["new_root", "<new_root_pubkey>"],
    ["timelock_start", "<unix_timestamp>"],
    ["protocol_version", "1"]
  ]
}
```

- `p` — the old root pubkey being recovered
- `new_root` — the proposed new root pubkey
- `timelock_start` — when the 7-day timelock begins
- When N valid attestations (from designated recovery contacts) exist for the same `new_root`, the timelock activates
- The old root key can cancel the recovery at any time during the 7-day timelock by publishing a cancellation event
- After timelock expiry with no cancellation, clients and relays accept the new root key as the identity successor

## New Kind: Push Notification Registration (10062)

Parameterized replaceable event — one per notification relay. Registers a user's push token with a notification relay for privacy-preserving wake-up notifications. See [Push Notifications](../design/push-notifications.md).

```json
{
  "kind": 10062,
  "pubkey": "<root_pubkey>",
  "tags": [
    ["d", "<notification_relay_pubkey>"],
    ["relay", "wss://notify.example.com"],
    ["protocol_version", "1"]
  ],
  "content": "<NIP-44 encrypted to notification_relay_pubkey: {\"token\": \"<apns_or_fcm_token>\", \"platform\": \"ios|android\", \"filters\": {\"dm\": true, \"mention\": true, \"reply\": true, \"reaction\": false}}>",
  "sig": "<signed by device key>"
}
```

- **Parameterized replaceable** — `d` tag keys one registration per notification relay
- **Encrypted content** — push token and filter preferences are NIP-44 encrypted to the notification relay's pubkey. Only the notification relay can decrypt. Relays storing this event see an opaque blob.
- **Filter preferences** — `dm`, `mention`, `reply`, `reaction` — which event types trigger a push
- **Platform** — `ios` (APNs) or `android` (FCM)
- **Token rotation** — publish a new 10062 with the updated token when the OS rotates it
- **Deregistration** — publish a 10062 with empty content to deregister
- Push payload contains no message content — only a "sync now" signal. App wakes, syncs from relays/pact partners, generates local notification with actual content.
- Users SHOULD register with 2-3 notification relays for redundancy

## New Kind: Deletion Request (10063)

Regular event requesting deletion of the author's own events. Provides a protocol-level mechanism for GDPR Article 17 ("right to erasure") compliance. See [Moderation Framework](../design/moderation.md).

```json
{
  "kind": 10063,
  "pubkey": "<root_pubkey>",
  "tags": [
    ["e", "<event_id_1>"],
    ["e", "<event_id_2>"],
    ["reason", "gdpr"],
    ["protocol_version", "1"]
  ],
  "content": "",
  "sig": "<signed by root key or governance key>"
}
```

- **`e` tags** — reference specific event IDs to delete. One request can cover multiple events.
- **`reason` tag** (optional) — one of `user_request`, `gdpr`, `content_violation`, `error`, or omitted. Informational only.
- **`all_events` tag** (optional) — `["all_events", "true"]` requests deletion of *every* event the author has published, rather than the specific `e`-tagged events. Used for full account erasure (GDPR right-to-erasure). When present, `e` tags may be omitted. Pact partners, read-caches, and relays SHOULD purge all stored events for the author's root identity.
- **Signed by root key or governance key** — only the author can request deletion of their own events. Device keys cannot issue deletion requests (prevents a compromised device from wiping history).
- Pact partners, read-caches, and relays SHOULD honor deletion requests
- Deletion is best-effort — signed events may have been copied beyond the author's reach. This is the honest reality of any decentralized system.
- Pact partners that systematically ignore deletion requests can be flagged and replaced through standard pact renegotiation
- Compatible with NIP-09 (kind 5) — relays MAY treat kind 10063 equivalently for retention purposes

## New Kind: Content Report (10064)

Regular event reporting content to relay operators and community moderators. This is the canonical 10064 format; the [Moderation](../design/moderation.md) doc references it.

```json
{
  "kind": 10064,
  "pubkey": "<reporter_device_pubkey>",
  "tags": [
    ["e", "<reported_event_id>"],
    ["p", "<reported_author_pubkey>"],
    ["report", "<category>"],
    ["root_identity", "<reporter_root_pubkey>"],
    ["protocol_version", "1"]
  ],
  "content": "<NIP-44 encrypted to relay operator: {\"detail\": \"<free-text context>\"}>",
  "sig": "<signed by device key>"
}
```

- **Signed by the reporter's device key**, with a `root_identity` tag attributing the report to the reporter's root identity (same authorship model as any device-signed event).
- **`e` tag** — the event being reported
- **`p` tag** — the author of the reported event (for relay operator routing)
- **`report` tag** — category: `spam`, `harassment`, `illegal_content`, `csam`, `impersonation`, `other`
- **`content`** — free-text detail is **NIP-44-encrypted to the relay operator's pubkey**, not placed in a public tag. Only the operator can read the reporter's explanation; the fact that a report exists and its category are visible for routing, but the narrative is not public.
- Published to reporter's relays and the reported author's relays (via NIP-65)
- Relay operators receive reports through normal subscription flow and act at their discretion
- Pact partners receiving a credible `csam` or `illegal_content` report MAY delete the content and drop the pact with no reliability score penalty
- Compatible with NIP-32 labeling services — third-party labeling services can tag content with labels that clients use for filtering

## New Kind: Temporary Suspension (10065)

Replaceable event published by the governance key to declare an emergency, time-boxed suspension of a device — for example when a device is suspected compromised but the root key is not immediately available from cold storage to publish a full kind 10050 revocation. Clients treat events from a suspended device as untrusted until the suspension expires or a new kind 10050 supersedes it.

```json
{
  "kind": 10065,
  "pubkey": "<root_pubkey>",
  "tags": [
    ["device", "<suspended_device_pubkey>"],
    ["expires", "<unix_timestamp>"],
    ["protocol_version", "1"]
  ],
  "content": "<optional NIP-44 encrypted note>",
  "sig": "<signed by governance key>"
}
```

- Signed by the governance key so it can be issued from a warm, trusted device without the cold-storage root key
- `expires` — end of the suspension window; a definitive kind 10050 revocation (root-signed) supersedes it
- Governance authority is a subset of root authority: a suspension can freeze a device but cannot re-authorize one — only a root-signed kind 10050 adds or permanently revokes devices

## New Kind: iroh Cross-Key Binding (10070)

Replaceable event binding a user's Nostr identity (secp256k1) to an iroh endpoint identity (Ed25519). iroh uses Ed25519 endpoint keys for its QUIC transport, distinct from the secp256k1 keys Nostr uses for event signing; this attestation lets peers verify that an iroh endpoint legitimately belongs to a given root identity. See [ADR 011](../decisions/011-iroh-transport-integration.md).

```json
{
  "kind": 10070,
  "pubkey": "<root_pubkey>",
  "tags": [
    ["iroh_endpoint", "<ed25519_node_id>"],
    ["protocol_version", "1"]
  ],
  "content": "<ed25519 signature by the iroh endpoint key over the root pubkey>",
  "sig": "<signed by root key>"
}
```

- Double-signed: the outer event is signed by the secp256k1 root key; the `content` carries the Ed25519 endpoint key's signature over the root pubkey, so each key attests to the other and neither can be claimed unilaterally
- Replaceable — publishing a new 10070 rotates the endpoint binding
- Peers verify both signatures before accepting an iroh endpoint as belonging to a root identity

## The prev Tag

Convention on replaceable events (kind 0 and kind 3). Enables multi-device fork detection and automatic merge. See [ADR 003](../decisions/003-replaceable-event-merge.md).

```json
{
  "kind": 0,
  "pubkey": "<root_pubkey>",
  "content": "{\"name\": \"alice\", \"about\": \"...\"}",
  "tags": [["prev", "<event_id_of_replaced_event>"]]
}
```

- Every kind 0 or kind 3 update includes a `prev` tag pointing to the event it replaces
- A fork exists when two events share the same `prev` value
- Clients use this to detect forks and apply automatic merge (follow list: set-union; profile: per-field latest-timestamp)
- The first event in a chain (no predecessor) omits the `prev` tag
- Compatible with standard Nostr relays (they ignore unknown tags)

## The root_identity Tag

Convention on all device-signed events. The `root_identity` tag is a routing hint, not an authentication proof. It lets relays and clients locate the correct kind 10050 event without scanning all possible delegation chains. Clients MUST verify that the signing device pubkey is listed in the referenced kind 10050 before treating the event as authenticated. Caching kind 10050 locally is recommended to avoid repeated lookups.

```json
{
  "kind": 1,
  "pubkey": "<device_pubkey>",
  "content": "hello world",
  "tags": [["root_identity", "<root_pubkey>"]]
}
```

## The seq and prev_hash Tags

Convention on all device-signed events. Creates a per-device hash chain for real-time completeness verification without waiting for checkpoints.

```json
{
  "kind": 1,
  "pubkey": "<device_pubkey>",
  "tags": [
    ["root_identity", "<root_pubkey>"],
    ["seq", "47"],
    ["prev_hash", "<H(previous_event_id)>"]
  ]
}
```

- `seq` — monotonically increasing sequence number per device, starting at 0
- `prev_hash` — hash of the previous event ID from this device
- First event in chain: `seq` = 0, `prev_hash` omitted

**Completeness verification:**
- Gap in sequence numbers → missing event
- `prev_hash` mismatch → tampered or reordered chain
- Checkpoint Merkle root provides cross-device verification; the per-event chain handles single-device stream completeness

## The protocol_version Tag

All Gozzip custom event kinds (10050–10065 and 10070; see the [Canonical Kind Registry](#canonical-kind-registry)) MUST include a `protocol_version` tag:

```json
["protocol_version", "1"]
```

- Version `1` is the initial protocol version described in this document
- Clients receiving an event with an unknown protocol version SHOULD process it on a best-effort basis and MAY ignore fields they do not understand
- Breaking changes (incompatible event format changes) increment the version number
- Clients advertise their supported version range in kind 10055 pact requests via `["protocol_version", "1"]` so potential partners can verify compatibility before forming pacts
- Version negotiation is implicit: if a pact partner publishes events with an unsupported protocol version, the client flags the pact as incompatible and begins replacement

The full deferred versioning strategy (negotiation, migration windows) lives in [protocol-versioning.md](../future/protocol-versioning.md).

## The media Tag

Convention on events that reference media (images, video, audio). Events contain content-addressed `media` tags — a SHA-256 hash of the media plus a best-effort URL hint — and clients verify the hash on every fetch. The event itself remains small (~1 KB with media references); media is fetched separately by hash. This content-addressed tag convention is part of v1; dedicated media *pacts* (reciprocal storage of media blobs) are deferred — see [Media Layer](../future/media-layer.md).

```json
{
  "kind": 1,
  "pubkey": "<device_pubkey>",
  "content": "sunset from the balcony",
  "tags": [
    ["root_identity", "<root_pubkey>"],
    ["seq", "48"],
    ["prev_hash", "<H(previous_event_id)>"],
    ["media", "a1b2c3d4e5f6...", "image/jpeg", "347291", "https://cdn.example.com/a1b2c3d4e5f6.jpg"],
    ["media_thumb", "f7e8d9c0b1a2...", "image/jpeg", "8192", "https://cdn.example.com/thumb.jpg"]
  ]
}
```

**Tag format:**

| Tag | Fields | Description |
|-----|--------|-------------|
| `media` | `sha256_hex`, `mime_type`, `size_bytes`, `url_hint` | Full-resolution media reference |
| `media_thumb` | `sha256_hex`, `mime_type`, `size_bytes`, `url_hint` | Thumbnail reference (optional, recommended for images/video) |

- `sha256_hex` — SHA-256 hash of the raw media bytes, hex-encoded (64 chars). Clients verify this hash on every fetch.
- `mime_type` — IANA media type (`image/jpeg`, `video/mp4`, `audio/ogg`, etc.)
- `size_bytes` — exact byte count of the raw file
- `url_hint` — URL where the author uploaded the media. Best-effort pointer; may go stale. Clients fall back to media pact partners, IPFS, or storage peer caches if the URL is unavailable.
- Multiple `media` tags per event are allowed. Display order follows tag order.
- Thumbnails: 200px wide, JPEG quality 60, max 15 KB. Generated client-side before upload.
- Compatible with standard Nostr relays and clients (unknown tags are ignored)
