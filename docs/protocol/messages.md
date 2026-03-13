# Messages

Event kinds and their structure in Gozzip. Inherits all Nostr event kinds. Gozzip adds kinds 10050–10064.

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
| 14 | DM (NIP-44) | device | Encrypted to recipient's dm_key (from kind 10050) |
| 30023 | Long-form article | device | `root_identity` tag added |
| 9734 | Zap request | device | Targets root pubkey for payment |
| 9735 | Zap receipt | LNURL server | Standard NIP-57 |
| 39000-39009 | Group metadata | group relay | Standard NIP-29 |

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
    ["checkpoint_delegate", "<device_pubkey_2>"]
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
    ["merkle_root", "<root_hash>", "<event_count>"]
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
    ["read_until", "<unix_timestamp>"]
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
    ["expires", "<unix_timestamp>"]
  ]
}
```

- Both parties publish their own 10053 referencing each other
- The pact exists when both sides have published
- Private — exchanged via encrypted DM or direct connection

**Pact types** (`type` tag):
- `standard` — default reciprocal pact, covers events since last checkpoint (~monthly)
- `bootstrap` — one-sided temporary pact for new users. The followed user stores the new user's data. Auto-expires after 90 days or when the new user reaches 10 reciprocal pacts. See [ADR 006](../decisions/006-storage-pact-risk-mitigations.md).
- `archival` — covers full history or a deep range. Lower challenge frequency (weekly). For power users and archivists.

**Pact status** (`status` tag):
- `active` — peer is challenged and expected to serve data requests
- `standby` — peer receives events but isn't challenged. Promoted to active when an active pact drops, providing instant failover with no discovery delay.

### Kind 10054 — Storage Challenge (ephemeral)

Challenge-response proof of storage. Supports two modes. See [ADR 006](../decisions/006-storage-pact-risk-mitigations.md).

```json
{
  "kind": 10054,
  "tags": [
    ["p", "<challenged_peer_pubkey>"],
    ["type", "hash"],
    ["challenge", "<nonce>"],
    ["range", "<start_seq>", "<end_seq>"]
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
    ["request_id", "<unique_id>"]
  ]
}
```

- `ttl` — hop count for gossip forwarding. Each peer decrements and forwards if they can't respond. Reaches ~8,000 nodes in a 20-peer network (20^3). Hardened with per-hop rate limiting (10 req/s per source), WoT-only forwarding (2-hop boundary), and request deduplication via `request_id`.
- `request_id` — unique identifier for deduplication. Nodes track seen request_ids (LRU cache) and drop duplicates. See [ADR 008](../decisions/008-protocol-hardening.md).

### Kind 10056 — Storage Pact Offer

Response to a kind 10055 request. Peer offers to form a reciprocal pact.

```json
{
  "kind": 10056,
  "tags": [
    ["p", "<requester_root_pubkey>"],
    ["volume", "<bytes>"],
    ["tz", "UTC+9"]
  ]
}
```

- `tz` (optional) — timezone offset. Used by clients for geographic diversity in peer selection. Target 3+ timezone bands across storage peers to protect against correlated regional failures.

### Kind 10057 — Data Request (DVM-style, pseudonymous)

Broadcast requesting events for a specific user. Uses a rotating request token to limit casual surveillance of who reads whom. See [ADR 006](../decisions/006-storage-pact-risk-mitigations.md).

This is a daily-rotating lookup key computed as `H(target_pubkey || YYYY-MM-DD)`. It prevents casual observers from linking requests across days but is trivially reversible by any party that knows the target's public key.

```json
{
  "kind": 10057,
  "tags": [
    ["rt", "<H(target_root_pubkey || YYYY-MM-DD)>"],
    ["since", "<checkpoint_event_id_or_timestamp>"],
    ["ttl", "3"],
    ["request_id", "<unique_id>"]
  ]
}
```

- `rt` (rotating request token) — `H(target_pubkey || YYYY-MM-DD)`. Rotates daily. Storage peers match against both today and yesterday's date to handle clock skew at day boundaries.
- Storage peers compute the same token for each pubkey they store and match incoming requests
- Observers who do not know the target pubkey see a hash that changes daily — cannot link requests across days. However, any party that knows (or can guess) the target's public key can compute the token and identify the request target.
- Storage peers respond privately via kind 10058
- `request_id` — unique identifier for deduplication (same as kind 10055). Hardened with per-hop rate limiting (50 req/s per source), WoT-only forwarding (2-hop boundary). See [ADR 008](../decisions/008-protocol-hardening.md).

### Kind 10058 — Data Offer (private response)

Private response to a kind 10057 data request. Tells the requester where to connect for the data.

```json
{
  "kind": 10058,
  "tags": [
    ["p", "<requester_pubkey>"],
    ["relay", "<connection_endpoint>"]
  ]
}
```

### Kind 10059 — Storage Endpoint Hint (encrypted)

Encrypted message to followers containing current storage peer endpoints. Sent when a follow relationship is established and updated when peers change. Eliminates DVM overhead for routine fetches.

```json
{
  "kind": 10059,
  "pubkey": "<device_pubkey>",
  "content": "<NIP-44 encrypted: {\"peers\": [\"wss://peer1\", \"wss://peer2\"]}>",
  "tags": [
    ["p", "<follower_root_pubkey>"],
    ["root_identity", "<root_pubkey>"]
  ]
}
```

- **NIP-59 gift wrapped** — the inner Kind 10059 is wrapped in a Kind 1059 gift wrap addressed to the follower. The relay stores an opaque blob. Only the intended recipient can decrypt. No relay-side filtering needed — privacy is handled by cryptography, not relay logic.
- Encrypted to the follower — only they can read the peer endpoints
- Sent once on follow, updated when storage peers change
- Followers cache these locally for direct peer access without DVM broadcast
- **WoT-scoped distribution:** only sent to followers within 2 WoT hops. Limits topology metadata leakage to trusted contacts. See [ADR 008](../decisions/008-protocol-hardening.md).

## New Kind: Recovery Delegation (10060)

Parameterized replaceable event — one per recovery contact. Designates a trusted WoT member as a recovery contact for social recovery of the root key. See [ADR 008](../decisions/008-protocol-hardening.md).

Recovery delegation events (kind 10060) MUST encrypt the recovery contact set. The `d` tag contains a blinded identifier `H(recovery_contact_pubkey || owner_pubkey)` rather than the plaintext pubkey. The `threshold` and `total` values MUST be placed in NIP-44 encrypted content, not in public tags. Only the recovery contact and the identity owner can determine the relationship exists. This prevents an attacker from enumerating the recovery set and targeting specific contacts.

```json
{
  "kind": 10060,
  "pubkey": "<root_pubkey>",
  "tags": [
    ["d", "<H(recovery_contact_pubkey || owner_pubkey)>"]
  ],
  "content": "<NIP-44 encrypted to recovery contact: {\"role\": \"recovery_contact\", \"threshold\": 3, \"total\": 5, \"instructions\": \"...\"}>"
}
```

- `d` tag — `H(recovery_contact_pubkey || owner_pubkey)`, a blinded identifier (parameterized replaceable key). Neither the recovery contact's identity nor the relationship is visible to third parties.
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
    ["timelock_start", "<unix_timestamp>"]
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

Regular event requesting deletion of the author's own events. Provides a protocol-level mechanism for GDPR Article 17 ("right to erasure") compliance. See [Moderation Framework](../design/moderation-framework.md).

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
- **`reason` tag** (optional) — `gdpr`, `personal`, `error`, or omitted. Informational only.
- **Signed by root key or governance key** — only the author can request deletion of their own events. Device keys cannot issue deletion requests (prevents a compromised device from wiping history).
- Pact partners, read-caches, and relays SHOULD honor deletion requests
- Deletion is best-effort — signed events may have been copied beyond the author's reach. This is the honest reality of any decentralized system.
- Pact partners that systematically ignore deletion requests can be flagged and replaced through standard pact renegotiation
- Compatible with NIP-09 (kind 5) — relays MAY treat kind 10063 equivalently for retention purposes

## New Kind: Content Report (10064)

Regular event reporting content to relay operators and community moderators. See [Moderation Framework](../design/moderation-framework.md).

```json
{
  "kind": 10064,
  "pubkey": "<reporter_root_pubkey>",
  "tags": [
    ["e", "<reported_event_id>"],
    ["p", "<reported_author_pubkey>"],
    ["report", "<category>"],
    ["protocol_version", "1"]
  ],
  "content": "<optional description>",
  "sig": "<signed by device key>"
}
```

- **`e` tag** — the event being reported
- **`p` tag** — the author of the reported event (for relay operator routing)
- **`report` tag** — category: `spam`, `harassment`, `illegal`, `csam`, `impersonation`, `other`
- **`content`** — optional free-text description providing context
- Published to reporter's relays and the reported author's relays (via NIP-65)
- Relay operators receive reports through normal subscription flow and act at their discretion
- Pact partners receiving a credible `csam` or `illegal` report MAY delete the content and drop the pact with no reliability score penalty
- Compatible with NIP-32 labeling services — third-party labeling services can tag content with labels that clients use for filtering

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

All Gozzip custom event kinds (10050–10064) MUST include a `protocol_version` tag:

```json
["protocol_version", "1"]
```

- Version `1` is the initial protocol version described in this document
- Clients receiving an event with an unknown protocol version SHOULD process it on a best-effort basis and MAY ignore fields they do not understand
- Breaking changes (incompatible event format changes) increment the version number
- Clients advertise their supported version range in kind 10055 pact requests via `["protocol_version", "1"]` so potential partners can verify compatibility before forming pacts
- Version negotiation is implicit: if a pact partner publishes events with an unsupported protocol version, the client flags the pact as incompatible and begins replacement

## The media Tag

Convention on events that reference media (images, video, audio). Events contain content-addressed hash references to media blobs stored externally. The event itself remains small (~1 KB with media references); media is fetched separately by hash. See [Media Layer](../design/media-layer.md).

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
