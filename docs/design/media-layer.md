# Media Layer

How Gozzip handles images, video, audio, and other large blobs. The design separates media from event metadata so that events remain small, pact obligations remain tractable, and mobile devices are never forced to store or serve media for others.

## Design Principle

Events are metadata. Media is data. They travel through different channels.

A Gozzip event containing an image is ~1 KB (text content plus a content-addressed hash reference to the image). The image itself (100 KB - 5 MB) lives elsewhere -- on a CDN, S3 bucket, IPFS, self-hosted server, or optionally on a pact partner's storage. Clients fetch the media by hash from the best available source. The hash guarantees integrity regardless of source.

This mirrors how every production social protocol handles media (Nostr via NIP-94/NIP-96, Mastodon via ActivityPub media attachments, AT Protocol via blob references). Gozzip adds content-addressed integrity verification and an optional peer-to-peer media pact layer.

## Media Reference Format

Events reference media via `media` tags. Each tag contains everything a client needs to locate and verify the blob.

```json
["media", "<sha256_hex>", "<mime_type>", "<size_bytes>", "<url_hint>"]
```

| Field | Description | Example |
|-------|-------------|---------|
| `sha256_hex` | SHA-256 hash of the raw media bytes, hex-encoded | `a1b2c3...` (64 chars) |
| `mime_type` | IANA media type | `image/jpeg`, `video/mp4`, `audio/ogg` |
| `size_bytes` | Exact byte count of the raw media file | `347291` |
| `url_hint` | URL where the author uploaded the media (may go stale) | `https://cdn.example.com/a1b2c3...jpg` |

**Example event with an image:**

```json
{
  "kind": 1,
  "pubkey": "<device_pubkey>",
  "content": "sunset from the balcony",
  "tags": [
    ["root_identity", "<root_pubkey>"],
    ["seq", "48"],
    ["prev_hash", "<H(previous_event_id)>"],
    ["media", "a1b2c3d4e5f6...", "image/jpeg", "347291", "https://cdn.example.com/a1b2c3d4e5f6.jpg"]
  ]
}
```

**Multiple media per event:** Add multiple `media` tags. Order in the tag list determines display order.

**Thumbnail references:** For images and video, a second `media` tag with the suffix `-thumb` in the hash field is optional but recommended for mobile-friendly progressive loading:

```json
["media", "<sha256_hex>", "image/jpeg", "347291", "https://cdn.example.com/full.jpg"],
["media_thumb", "<thumb_sha256_hex>", "image/jpeg", "8192", "https://cdn.example.com/thumb.jpg"]
```

Thumbnails are generated client-side before upload. Target: 200px wide, JPEG quality 60, max 15 KB.

## Media Hosting Model

The author chooses where to host media. The protocol does not mandate a hosting backend.

| Option | Who runs it | Cost | Availability | Notes |
|--------|------------|------|-------------|-------|
| Public CDN (e.g., nostr.build, void.cat) | Third party | Free/paid | High | Existing Nostr media hosts work unchanged |
| S3 / R2 / GCS | Self-provisioned | ~$0.02/GB/month | High | Author controls retention |
| IPFS | Pinning service or self | Variable | Variable | Content-addressed natively; hash in `media` tag = CID |
| Self-hosted | Author's server | Electricity | Depends on uptime | Full control |
| Media pact partner | WoT peer | Reciprocal | Depends on peer uptime | See Media Pacts below |

The `url_hint` in the media tag is a best-effort pointer. It may go stale (CDN deletes the file, server goes down). Clients treat it as the first place to try, not the only place.

**Fallback resolution order:**

1. Local cache (already fetched this hash before)
2. `url_hint` from the media tag
3. Media pact partners (if any -- query by hash)
4. IPFS gateway (if hash matches a known CID)
5. Ask the author's storage pact partners (they may have cached it)

At every step, the client verifies `SHA-256(downloaded_bytes) == hash_from_tag`. If verification fails, discard and try the next source.

## Media Pacts

An optional extension of storage pacts. Standard storage pacts (kind 10053) cover events only. Media pacts cover the media blobs referenced by those events.

### Why separate

- Events are small and predictable (~1 KB). Media is large and variable (100 KB - 50 MB per blob).
- Volume matching for events is tractable. Volume matching for media requires separate accounting -- a user who posts 10 images/day has 100x the media volume of a text-only user.
- Mobile devices can participate in event pacts but should never be obligated to store or serve multi-gigabyte media collections.

### Media pact structure

Media pacts use the same kind 10053 event with `type: media`:

```json
{
  "kind": 10053,
  "pubkey": "<root_pubkey>",
  "tags": [
    ["d", "<partner_root_pubkey>"],
    ["type", "media"],
    ["status", "active"],
    ["volume", "<bytes>"],
    ["expires", "<unix_timestamp>"],
    ["retention", "90"]
  ]
}
```

- `type: media` distinguishes from `type: standard` (event pacts)
- `volume` is the media-specific volume commitment (separate from event volume)
- `retention` is the number of days of media to retain (default: 90). Older media ages out of the pact obligation. Authors who want permanent media availability use CDN/S3/IPFS.

### Media pact volume matching

Media volume is matched separately from event volume. A user producing 50 MB/month of media seeks partners producing similar media volume. The +/- 30% tolerance from event pacts applies independently to media volume.

### Media pact challenges

Standard event pact challenges (kind 10054) prove possession of events. Media pact challenges prove possession of blobs using a random byte-range challenge:

```json
{
  "kind": 10054,
  "tags": [
    ["p", "<challenged_peer_pubkey>"],
    ["type", "media_range"],
    ["media_hash", "<sha256_hex>"],
    ["challenge", "<nonce>"],
    ["offset", "<byte_offset>"],
    ["length", "<byte_count>"]
  ]
}
```

**Challenge:** "Give me `H(bytes[offset..offset+length] || nonce)` for media blob `<sha256_hex>`."

**Why byte-range instead of full-file hash:** Full-file hashing is the media tag hash itself -- the peer could pre-compute and cache just the hash without storing the file. Random byte-range challenges require actual possession of the data.

**Response window:** Same as the pair's event challenge window (determined by the weaker device).

### Keeper-only constraint

Media pacts are restricted to Keepers (full nodes, 90%+ uptime). Mobile devices and browser extensions are never obligated to store media for pact partners. This is enforced client-side: clients only offer media pacts from devices classified as Full nodes.

Rationale: A 500 MB media collection stored on a phone with 2% uptime is useless to the partner. The phone drains its battery and data plan serving content that a server handles trivially.

## Media Retrieval

Clients fetch media lazily. The event is displayed immediately with a placeholder. Media loads progressively.

### Fetch sequence

1. **Check local cache** -- if the hash is cached, display immediately
2. **Fetch thumbnail** (`media_thumb` tag) -- small enough to load on any connection. Display while full image loads
3. **Fetch full media** -- only on user action (tap/click) for large files, or automatically for small images on WiFi

### Progressive loading rules

| Context | Behavior |
|---------|----------|
| WiFi, scrolling feed | Auto-load thumbnails. Auto-load full images < 500 KB. Tap for larger. |
| Cellular, scrolling feed | Auto-load thumbnails only. Tap for full image. |
| Video/audio | Always tap to play. Never auto-load. |
| Offline/BLE mesh | Text only. Media loads when connectivity returns. |

These are client-side defaults. Users can override (e.g., "always load full images").

### Bandwidth impact

With lazy loading and thumbnails:
- Scrolling a feed of 100 posts (30% with images): ~30 thumbnails x 10 KB = 300 KB
- Tapping to view 5 full images: ~5 x 350 KB = 1.75 MB
- Total: ~2 MB for a browsing session vs. ~11 MB if all images loaded eagerly

This keeps mobile bandwidth within the 3.3 MB/day budget established in the plausibility analysis for text events, plus a controlled additional amount for media that the user chooses to view.

## Caching

### Client-side media cache

Clients maintain a local media cache, bounded by size:

| Config key | Default | Description |
|-----------|---------|-------------|
| `media_cache_max_mb` | 500 | Maximum media cache size |
| `media_cache_ttl_days` | 30 | Maximum age before eviction |

Eviction: LRU within TTL boundary. Thumbnails are evicted last (they are small and re-fetching full media is expensive).

### Read-cache serving

The same cascading read-cache mechanism used for events applies to media. If Bob has Alice's image cached and Carol requests it (by hash), Bob can serve it. Carol verifies the hash -- source does not matter.

Media read-cache is opt-in and size-bounded separately from event read-cache:

| Config key | Default | Description |
|-----------|---------|-------------|
| `media_read_cache_enabled` | false | Whether to serve cached media to others |
| `media_read_cache_max_mb` | 200 | Storage limit for media served to others |

Default is `false` because serving media consumes significantly more bandwidth than serving events. Users on servers or unmetered connections can opt in.

## Integrity

Every media fetch is verified:

1. Client downloads the bytes from any source
2. Client computes `SHA-256(bytes)`
3. Client compares against the hash in the `media` tag
4. Match: display. Mismatch: discard, try next source.

This is the same integrity model as events (signature verification regardless of source), applied to binary blobs. A CDN, IPFS node, pact partner, or random peer can serve the media -- the hash proves it is exactly what the author referenced.

**No trust in the source.** The `url_hint` is a convenience, not an endorsement. A malicious CDN serving corrupted data is detected and bypassed. A pact partner serving the correct bytes is indistinguishable from a CDN serving the correct bytes.

## Interaction with Event Pacts

Media is explicitly excluded from event pact obligations:

- **Event pact volume** counts event bytes only (the JSON event including media tags, not the media blobs)
- **Event pact challenges** prove possession of events, not media
- **Volume matching** for event pacts ignores media volume entirely

A user with 100 MB of events and 5 GB of media has a 100 MB event pact volume. The 5 GB is either self-hosted, CDN-hosted, or covered by a separate media pact.

This separation keeps event pact obligations tractable for all device types. A phone can hold 100 MB of events for 20 pact partners (2 GB total) but cannot hold 5 GB of media for each of them.

## Summary

| Concern | Resolution |
|---------|-----------|
| Events stay small | Media tags add ~150 bytes per reference. Event remains ~1 KB. |
| Pact obligations remain tractable | Media excluded from event pacts. Separate media pacts are optional. |
| Mobile-friendly | Phones never store media for pact partners. Lazy loading controls bandwidth. |
| No single point of failure | Content-addressed. Any source with the correct bytes works. |
| Integrity guaranteed | SHA-256 verification on every fetch. |
| Backward compatible | Standard Nostr clients ignore unknown tags. Media tags are additive. |
| No new infrastructure required | Existing CDNs, IPFS, S3 all work. Media pacts are optional peer-to-peer layer. |
