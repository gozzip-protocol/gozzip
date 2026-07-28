# Media Layer Design
**Date:** 2026-03-14
**Status:** Draft

## Problem Statement

The whitepaper and plausibility analysis model text-only events (925 bytes weighted average). Real social networks are media-heavy: approximately 30% of posts include images (200KB-2MB each), with video and audio adding further volume. When media is accounted for, storage and bandwidth requirements increase by orders of magnitude.

This document specifies how Gozzip separates media from event metadata, introduces optional media pacts for peer-to-peer media storage, defines the media retrieval protocol, and quantifies the impact on storage and bandwidth at three media-intensity levels.

## Design Principle

**Events are metadata. Media is data. They travel through different channels.**

A Gozzip event containing an image is ~1 KB (text content plus content-addressed hash references). The image itself (100KB-5MB) lives elsewhere — on a CDN, S3 bucket, IPFS, self-hosted server, or optionally on a pact partner's storage. Clients fetch media by hash from the best available source. The hash guarantees integrity regardless of source.

This mirrors how every production social protocol handles media (Nostr via NIP-94/NIP-96, Mastodon via ActivityPub media attachments, AT Protocol via blob references). Gozzip adds content-addressed integrity verification and an optional peer-to-peer media pact layer.

---

## 1. Content-Addressed Media Separation

### Event Structure

Events reference media via `media` tags. The event body stays small (~925 bytes on average, plus ~150 bytes per media reference). Media blobs are stored and served separately.

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

### Media Tag Format

| Tag | Fields | Description |
|-----|--------|-------------|
| `media` | `sha256_hex`, `mime_type`, `size_bytes`, `url_hint` | Full-resolution media reference |
| `media_thumb` | `sha256_hex`, `mime_type`, `size_bytes`, `url_hint` | Thumbnail reference (optional, recommended) |

| Field | Description | Example |
|-------|-------------|---------|
| `sha256_hex` | SHA-256 hash of the raw media bytes, hex-encoded (64 chars) | `a1b2c3d4e5...` |
| `mime_type` | IANA media type | `image/jpeg`, `video/mp4`, `audio/ogg` |
| `size_bytes` | Exact byte count of the raw media file | `347291` |
| `url_hint` | URL where the author uploaded the media (may go stale) | `https://cdn.example.com/a1b2c3.jpg` |

Multiple `media` tags per event are allowed. Display order follows tag order.

### Why Content-Addressed

Content addressing (SHA-256 hash as the identifier) provides three properties:

1. **Source independence.** Any node serving the correct bytes is equivalent. A CDN, IPFS gateway, pact partner, or random cache all produce the same verified result.
2. **Integrity without trust.** A malicious CDN serving corrupted data is detected and bypassed. No trust in any source is required.
3. **Natural deduplication.** The same image shared by 100 users produces one blob, fetched once, verified once.

### Thumbnail Generation

Thumbnails are generated client-side before upload:

- Target: 200px wide, JPEG quality 60, max 15 KB
- Purpose: instant display in feeds while full media loads progressively
- A `media_thumb` tag accompanies each `media` tag (recommended, not required)
- For video: thumbnail is a single representative frame (first non-black frame or 2-second mark)

---

## 2. Media Manifest Event (Kind 10065)

For media requiring richer metadata than fits in tags (multi-resolution images, video with multiple quality levels, album groupings), a dedicated media manifest event provides structured metadata.

```json
{
  "kind": 10065,
  "pubkey": "<device_pubkey>",
  "tags": [
    ["root_identity", "<root_pubkey>"],
    ["d", "<media_sha256_hex>"],
    ["protocol_version", "1"]
  ],
  "content": "{\"hash\": \"a1b2c3d4e5f6...\", \"mime\": \"image/jpeg\", \"size\": 347291, \"width\": 2048, \"height\": 1536, \"blurhash\": \"LEHV6nWB2yk8pyoJadR*.7kCMdnj\", \"thumbnails\": [{\"hash\": \"f7e8d9c0b1a2...\", \"width\": 200, \"height\": 150, \"size\": 8192}], \"alt\": \"Sunset over the Mediterranean\"}",
  "sig": "<signed by device key>"
}
```

### Manifest Fields

| Field | Required | Description |
|-------|----------|-------------|
| `hash` | Yes | SHA-256 of the full-resolution media blob |
| `mime` | Yes | IANA media type |
| `size` | Yes | Byte count of full-resolution blob |
| `width`, `height` | For images/video | Pixel dimensions |
| `blurhash` | Recommended | BlurHash placeholder string for instant display |
| `thumbnails` | Recommended | Array of `{hash, width, height, size}` for progressive loading |
| `alt` | Recommended | Accessibility text description |
| `duration` | For audio/video | Duration in seconds |

The manifest is a **parameterized replaceable event** keyed by the `d` tag (the media hash). This means one manifest per media blob. If the author re-uploads with different metadata, the old manifest is replaced.

### NIP-94 Compatibility

NIP-94 defines file metadata events (kind 1063) with `url`, `m` (mime), `x` (hash), `size`, `dim`, `blurhash`, and `alt` tags. The kind 10065 manifest stores equivalent data in its content field. Clients can translate between formats:

| NIP-94 tag | Kind 10065 field | Notes |
|------------|-----------------|-------|
| `x` | `hash` | Both SHA-256 hex |
| `m` | `mime` | IANA media type |
| `size` | `size` | Byte count |
| `dim` | `width` x `height` | NIP-94 uses "WxH" string format |
| `blurhash` | `blurhash` | Same encoding |
| `alt` | `alt` | Accessibility text |

---

## 3. Media Pacts (Optional)

Media pacts are an extension of storage pacts. Standard storage pacts (kind 10053, `type: standard`) cover events only. Media pacts cover the media blobs referenced by those events.

### Why Separate from Text Pacts

- **Scale mismatch.** Events are small and predictable (~925 bytes). Media blobs range from 100KB to 50MB each. A single image exceeds 400 events worth of data.
- **Volume predictability.** Text event volume is steady (varies by ~30% month-to-month for most users). Media volume is bursty — a vacation week might produce 200MB of images; the next month, zero.
- **Device constraints.** Mobile devices can participate in text pacts but should never be obligated to store multi-gigabyte media collections.
- **User diversity.** Text-only users and media-heavy users coexist. Forcing them into the same pact structure breaks volume matching.

### Media Pact Event (Kind 10053, type: media)

Media pacts reuse kind 10053 with a `type: media` tag to distinguish from text pacts:

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
    ["retention", "90"],
    ["protocol_version", "1"]
  ]
}
```

| Field | Description |
|-------|-------------|
| `type: media` | Distinguishes from `type: standard` (text pacts) |
| `volume` | Media-specific volume commitment (separate from event volume) |
| `retention` | Days of media to retain (default: 90). Older media ages out. |
| `expires` | Pact expiration timestamp |

### Volume Matching for Media

Media volume is matched **separately** from text volume. A user producing 50MB/month of media seeks partners producing similar media volume.

**Tolerance: +/- 100% (not +/- 30%).** Media is bursty. A user who posted 30MB last month might post 80MB this month (a vacation) or 5MB (a quiet month). The tighter +/-30% tolerance used for text events would cause constant renegotiation. The wider tolerance accommodates natural variance while still preventing extreme asymmetry (a 10MB/month user paired with a 500MB/month user).

Volume is calculated as the 30-day trailing sum of media blob sizes referenced in the user's events:

```
media_volume = sum of size_bytes for all unique media hashes referenced
               in events published in the last 30 days
```

Deduplication by hash: the same image posted in 5 events counts once.

### Media Pact Challenges

Standard text pact challenges (kind 10054, `type: hash`) prove possession of events. Media pact challenges prove possession of blobs using a **random byte-range challenge**:

```json
{
  "kind": 10054,
  "tags": [
    ["p", "<challenged_peer_pubkey>"],
    ["type", "media_range"],
    ["media_hash", "<sha256_hex>"],
    ["challenge", "<nonce>"],
    ["offset", "<byte_offset>"],
    ["length", "<byte_count>"],
    ["protocol_version", "1"]
  ]
}
```

**Challenge:** "Compute `SHA-256(bytes[offset..offset+length] || nonce)` for media blob `<sha256_hex>`."

**Why byte-range instead of full-file hash:** The full-file hash is the media tag hash itself — the peer could pre-compute and cache just the hash without storing the file. Random byte-range challenges require actual possession of the data.

**Parameters:**
- `offset`: random value in `[0, file_size - length)`
- `length`: 4096 bytes (4KB). Large enough to be meaningful, small enough that the challenged peer doesn't need to read the entire file.
- `nonce`: 32 random bytes, hex-encoded

**Response window:** Same as the pair's text challenge window (determined by the weaker device's uptime class).

### Keeper-Only Constraint

Media pacts are restricted to **Keepers** (full nodes, 90%+ uptime). Mobile devices and browser extensions are never obligated to store media for pact partners.

Rationale: A 500MB media collection stored on a phone with 2% uptime is useless to the partner. The phone drains its battery and data plan serving content that a server handles trivially.

Enforcement is client-side: clients only offer media pacts from devices classified as full nodes.

---

## 4. Media Retrieval Protocol

Clients fetch media lazily. The event is displayed immediately with a placeholder (blurhash or solid color). Media loads progressively.

### Tiered Retrieval

Media is retrieved through a prioritized cascade:

| Priority | Source | When Used | Latency |
|----------|--------|-----------|---------|
| 1 | Local cache | Hash already fetched and cached | 0 ms |
| 2 | `url_hint` from media tag | First fetch attempt | 50-200 ms |
| 3 | Media pact partners | url_hint stale or failed | 100-500 ms |
| 4 | Text pact partners (read-cache) | Partners may have cached it | 100-500 ms |
| 5 | IPFS gateway | If hash matches a known CID | 200-2000 ms |
| 6 | Relay CDN | Relay serves cached media | 50-200 ms |

At every step, the client computes `SHA-256(downloaded_bytes)` and compares against the hash from the `media` tag. Mismatch: discard, try next source. Match: display and cache.

### Range Requests for Large Media

For media over 1MB, clients use HTTP range requests for progressive loading:

1. **Fetch thumbnail** (from `media_thumb` tag) — display immediately
2. **Fetch full media in chunks** — progressive JPEG renders partial quality; video streams first seconds
3. **User can cancel** mid-download if they scroll past

Range request support depends on the source:
- CDN/S3: HTTP `Range` header (standard)
- Pact partners: custom range protocol via NIP-46 message: `{"type": "media_range", "hash": "<sha256>", "offset": 0, "length": 65536}`
- IPFS: natively supported via byte range

### Progressive Loading Rules

| Context | Behavior |
|---------|----------|
| WiFi, scrolling feed | Auto-load thumbnails. Auto-load full images < 500KB. Tap for larger. |
| Cellular, scrolling feed | Auto-load thumbnails only. Tap for full image. |
| Video/audio | Always tap to play. Never auto-load. |
| Offline/BLE mesh | Text only. Media loads when connectivity returns. |

These are client-side defaults. Users can override (e.g., "always load full images on cellular").

---

## 5. Relay Media Integration

Relays serve as the CDN tier for media — the fallback when peer-to-peer sources are unavailable.

### Relay as Media CDN

Relays can optionally host and serve media blobs. Three models:

| Model | Description | Economics |
|-------|-------------|-----------|
| **Free tier** | Relay caches popular media, serves to anyone | Ad-supported or community-funded |
| **Pay-per-upload** | Author pays relay to host their media | Sats via Lightning (NIP-96 compatible) |
| **Pay-per-serve** | Requester pays relay per download | Micropayment per MB |

Relays are not required to host media. The protocol functions without relay media support — authors use CDNs, S3, or media pacts instead.

### Relay Media Caching

Relays that opt into media hosting implement LRU caching:

- **Popular content cached:** Media referenced by many events or frequently requested stays in cache
- **Rare content evicted:** Infrequently accessed blobs are evicted under storage pressure
- **Size-bounded:** Relay operators configure max media cache size
- **Hash-indexed:** Media is stored and retrieved by SHA-256 hash, identical to peer-to-peer retrieval

### NIP-94 / NIP-96 Compatibility

- **NIP-94** (file metadata): Kind 10065 manifests translate bidirectionally to NIP-94 kind 1063 events
- **NIP-96** (file upload): Relays supporting NIP-96 can accept Gozzip media uploads. The returned URL becomes the `url_hint` in the media tag. The client computes the SHA-256 hash locally before upload.

Gozzip clients interoperate with existing Nostr media infrastructure without modification.

---

## 6. Volume Matching Implications

The separation of text and media into independent pact types has specific implications for volume matching and pact health.

### Independent Budgets

Text pacts and media pacts are **fully independent**:

- A text-only user can form text pacts with a media-heavy user. The text pact covers events only, and the media-heavy user's images do not affect text pact volume matching.
- A user producing 50MB/month of media and 675KB/month of text has:
  - **Text pact volume:** 675KB (matches other active text users)
  - **Media pact volume:** 50MB (matches other medium-media users)

### Pact Health Independence

Media volume does not affect text pact health:

- Text pact challenges verify event possession (kind 10054, `type: hash`)
- Media pact challenges verify blob possession (kind 10054, `type: media_range`)
- A partner who fails media challenges but passes text challenges remains a healthy text pact partner
- A partner who fails text challenges but passes media challenges has their text pact degraded independently

### Media Pact Formation

Media pacts are **opt-in** and form separately from text pacts:

1. User broadcasts kind 10055 with `type: media` tag and media volume
2. Keeper-class peers with similar media volume respond with kind 10056
3. Both exchange kind 10053 (`type: media`) pact events
4. Both begin storing each other's media blobs

A user can have text pacts with one set of peers and media pacts with a different set. Or the same peer can hold both a text pact and a media pact (two independent kind 10053 events).

### Volume Matching Summary

| Dimension | Text Pacts | Media Pacts |
|-----------|-----------|-------------|
| What is stored | Event JSON (~925 bytes each) | Media blobs (100KB-5MB each) |
| Volume calculation | Sum of event bytes, 30-day trailing | Sum of unique media blob sizes, 30-day trailing |
| Matching tolerance | +/- 30% | +/- 100% |
| Challenge type | `hash` (event range hash) | `media_range` (random byte-range) |
| Device eligibility | All devices | Keepers only (90%+ uptime) |
| Pact count | 12-43 (equilibrium-seeking) | 3-10 (fewer needed, Keepers only) |

---

## 7. Mobile Considerations

Mobile devices are first-class citizens for text but restricted participants for media.

### Mobile Media Rules

| Rule | Rationale |
|------|-----------|
| Mobile nodes **never** serve media to others | Battery, bandwidth, storage constraints |
| Mobile nodes **never** form media pacts | Keeper-only constraint |
| WiFi-only download for media > 500KB | Cellular data preservation |
| Thumbnails always load (any connection) | Small enough (~10KB) for any connection |
| Configurable media quality | User controls resolution/quality tradeoffs |
| Progressive download with cancel | User can abort large downloads mid-stream |

### Mobile Media Cache

Mobile clients maintain a bounded media cache:

| Setting | Default | Description |
|---------|---------|-------------|
| `media_cache_max_mb` | 500 | Maximum local media cache |
| `media_cache_ttl_days` | 30 | Maximum age before eviction |
| `cellular_media_threshold_kb` | 500 | Max auto-download size on cellular |
| `media_quality` | `auto` | `thumbnail`, `medium`, `full`, or `auto` |

Eviction is LRU within the TTL boundary. Thumbnails are evicted last (small and re-fetching full media is expensive).

### Mobile Bandwidth Budget

With lazy loading and thumbnails, mobile media consumption is user-controlled:

| Activity | Data consumed |
|----------|---------------|
| Scroll 100 posts (30% with images), thumbnails only | 30 x 10KB = 300KB |
| Tap to view 5 full images | 5 x 350KB = 1.75MB |
| Watch 1 video (2 min, 720p) | ~15MB |
| Typical session (browse + view a few images) | ~2MB |

This keeps the user in control. The 3.3 MB/day text-event budget (from the plausibility analysis) is unaffected by media — media consumption is additive and user-initiated.

---

## 8. Storage and Bandwidth Calculations

The following tables compare three scenarios: text-only (the whitepaper baseline), light-media (10% of posts include an image), and heavy-media (30% of posts include an image).

### Assumptions

```
Text event constants (from plausibility analysis):
  E_AVG              = 925 B      # weighted average text event
  E_d (active user)  = 30         # events per day
  CHECKPOINT_WINDOW  = 30         # days
  PACTS_DEFAULT      = 20         # text pact count

Media constants:
  MEDIA_AVG_SIZE     = 500 KB     # average image size (range: 200KB-2MB)
  MEDIA_THUMB_SIZE   = 10 KB      # thumbnail size
  MEDIA_PACTS        = 5          # media pact count (Keepers only)
  MEDIA_RETENTION    = 90         # days

User profiles:
  Active user: 30 events/day, 150 follows
  Text-only: 0% media posts
  Light-media: 10% of posts have an image (3 images/day)
  Heavy-media: 30% of posts have an image (9 images/day)
```

### Per-User Monthly Storage Production

| Scenario | Text events/month | Media blobs/month | Text volume | Media volume | Total volume |
|----------|-------------------|-------------------|-------------|--------------|--------------|
| **Text-only** | 900 | 0 | 675 KB | 0 | **675 KB** |
| **Light-media (10%)** | 900 | 90 | 675 KB | 45 MB | **~45 MB** |
| **Heavy-media (30%)** | 900 | 270 | 675 KB | 135 MB | **~135 MB** |

Media dominates by 2-3 orders of magnitude. At 30% media, a user produces 200x more media volume than text volume per month.

### Pact Storage Obligations

**Text pact storage** (stored for 20 partners, per plausibility analysis):

| Scenario | Per partner | 20 partners | Notes |
|----------|------------|-------------|-------|
| Text-only | 675 KB | 13.5 MB | Same as whitepaper baseline |
| Light-media | 675 KB | 13.5 MB | Media excluded from text pacts |
| Heavy-media | 675 KB | 13.5 MB | Media excluded from text pacts |

Text pact storage is **unchanged** regardless of media intensity. This is the core benefit of separation.

**Media pact storage** (stored for 5 Keeper partners, 90-day retention):

| Scenario | Per partner (90 days) | 5 partners | Notes |
|----------|----------------------|------------|-------|
| Text-only | 0 | 0 | No media pacts needed |
| Light-media | 135 MB | 675 MB | 90 images/month x 3 months |
| Heavy-media | 405 MB | 2.03 GB | 270 images/month x 3 months |

**Combined pact storage:**

| Scenario | Text pacts (20) | Media pacts (5) | Total pact storage |
|----------|----------------|----------------|-------------------|
| **Text-only** | 13.5 MB | 0 | **13.5 MB** |
| **Light-media** | 13.5 MB | 675 MB | **~690 MB** |
| **Heavy-media** | 13.5 MB | 2.03 GB | **~2.05 GB** |

### Full Node Storage (Annual)

A Keeper running a full node stores text pacts + media pacts + own history + read cache:

| Component | Text-only | Light-media | Heavy-media |
|-----------|-----------|-------------|-------------|
| Text pact storage (80 effective pacts) | 53 MB | 53 MB | 53 MB |
| Media pact storage (5 media partners, 90-day) | 0 | 675 MB | 2.03 GB |
| Own history (12 months text) | 7.9 MB | 7.9 MB | 7.9 MB |
| Own media (12 months) | 0 | 540 MB | 1.62 GB |
| Read cache (text) | 80 MB | 80 MB | 80 MB |
| Media cache | 0 | 500 MB | 500 MB |
| **Total** | **~141 MB** | **~1.86 GB** | **~4.29 GB** |
| **Annual (extrapolated)** | **~640 MB** | **~15 GB** | **~36 GB** |

These are well within consumer hardware limits. A 1TB disk allocates <4% to heavy-media full-node operations.

### Bandwidth: Daily Budget Per User

**Active user on a full node (broadband):**

| Activity | Text-only | Light-media | Heavy-media |
|----------|-----------|-------------|-------------|
| Publish events to text pact partners | 518 KB | 518 KB | 518 KB |
| Upload media to CDN/hosting | 0 | 1.5 MB | 4.5 MB |
| Serve text pact data | 300 MB | 300 MB | 300 MB |
| Serve media pact data (est. 50 requests/partner/day) | 0 | 9.4 MB | 28.1 MB |
| Fetch follows' text events | 1.1 MB | 1.1 MB | 1.1 MB |
| Fetch follows' media (thumbnails) | 0 | 450 KB | 1.35 MB |
| Fetch follows' media (full, user-initiated) | 0 | 8.75 MB | 26.25 MB |
| **Total outbound** | **~301 MB** | **~312 MB** | **~333 MB** |
| **Total inbound** | **~6.4 MB** | **~16.7 MB** | **~35 MB** |

Media adds 4-11% to full-node outbound and 160-450% to inbound. Outbound is dominated by text pact serving (unchanged). Inbound increases significantly but remains small relative to broadband capacity.

**Active user on a light node (mobile):**

| Activity | Text-only | Light-media | Heavy-media |
|----------|-----------|-------------|-------------|
| Text events (publish + fetch) | 3.3 MB/day | 3.3 MB/day | 3.3 MB/day |
| Media thumbnails (auto-load) | 0 | 450 KB/day | 1.35 MB/day |
| Media full (user-initiated, est. 10/day) | 0 | 5 MB/day | 5 MB/day |
| **Total daily** | **3.3 MB** | **~8.8 MB** | **~9.7 MB** |
| **Total monthly** | **~99 MB** | **~264 MB** | **~291 MB** |
| **% of 5GB plan** | 2.0% | 5.3% | 5.8% |

Mobile media consumption is bounded because it is user-initiated. The user controls how many full images they tap to view. Thumbnails add modest overhead (~450KB-1.35MB/day).

### Bandwidth: Typical User Summary

| Metric | Text-only | Light-media (10%) | Heavy-media (30%) |
|--------|-----------|-------------------|-------------------|
| Monthly storage produced | 675 KB | 45 MB | 135 MB |
| Text pact storage (20 partners) | 13.5 MB | 13.5 MB | 13.5 MB |
| Media pact storage (5 partners, 90d) | 0 | 675 MB | 2.03 GB |
| Full node annual storage | 640 MB | 15 GB | 36 GB |
| Mobile daily bandwidth | 3.3 MB | 8.8 MB | 9.7 MB |
| Mobile monthly bandwidth | 99 MB | 264 MB | 291 MB |
| Full-node daily outbound | 301 MB | 312 MB | 333 MB |

---

## 9. Sovereignty Implications

Media introduces a sovereignty gradient. Not all data sovereignty is equal, and the protocol makes deliberate tradeoffs.

### Text Sovereignty: Preserved Unconditionally

Text events — the user's posts, DMs, reactions, follow lists, and profile — are fully sovereign regardless of media strategy:

- Stored by 20 text pact partners (peer-to-peer)
- Verified by challenge-response
- Available at ~100% through pact redundancy
- No CDN or relay dependency for text

This is the critical data. A user's identity, social graph, and communication history are sovereign.

### Media Sovereignty: Tiered

| Tier | How | Sovereignty Level | Cost |
|------|-----|-------------------|------|
| **Full sovereignty** | Media pacts with Keepers + self-hosted media relay | Complete — no third-party dependency | Storage on full node, 5 media pact partners |
| **Partial sovereignty** | Media pacts + CDN backup | High — CDN adds availability, pacts add resilience | CDN hosting cost |
| **Relay-dependent** | CDN/relay only, no media pacts | Low — media availability depends on third parties | CDN hosting cost |
| **Ephemeral** | No media hosting, url_hint only | None — media disappears when host removes it | Free |

### Acceptable Tradeoff

Most users will rely on relay CDN for media. This is an acceptable tradeoff:

- **Text sovereignty** (identity, social graph, communication) is preserved regardless
- Media CDNs are abundant, interchangeable, and cheap ($0.02/GB/month on S3/R2)
- Media loss is recoverable (re-upload) while text/identity loss is not
- Users who need media sovereignty (journalists, activists) can opt into media pacts

The protocol never forces media through a single provider. The content-addressed model means any source serving the correct bytes is equivalent. If a CDN goes down, the same hash resolves from a pact partner, IPFS, or another CDN.

---

## 10. Media Hosting Options

The author chooses where to host media. The protocol does not mandate a hosting backend.

| Option | Who runs it | Cost | Availability | Notes |
|--------|-------------|------|--------------|-------|
| Public CDN (nostr.build, void.cat) | Third party | Free/paid | High | Existing Nostr media hosts work unchanged |
| S3 / R2 / GCS | Self-provisioned | ~$0.02/GB/month | High | Author controls retention |
| IPFS | Pinning service or self | Variable | Variable | Content-addressed natively; hash = CID |
| Self-hosted | Author's server | Electricity | Depends on uptime | Full control |
| Media pact partner | WoT peer (Keeper) | Reciprocal | Depends on partner uptime | Peer-to-peer sovereignty |

The `url_hint` in the media tag is a best-effort pointer. It may go stale. Clients treat it as the first place to try, not the only place.

### Fallback Resolution Order

1. **Local cache** — already fetched this hash before
2. **`url_hint`** from the media tag
3. **Media pact partners** — query by hash
4. **Text pact partners** (read-cache) — may have cached it
5. **IPFS gateway** — if hash matches a known CID
6. **Relay CDN** — relay serves cached copy

At every step: `SHA-256(downloaded_bytes) == hash_from_tag`. Mismatch: discard, next source.

---

## 11. Caching

### Client-Side Media Cache

All clients maintain a local media cache:

| Setting | Default | Description |
|---------|---------|-------------|
| `media_cache_max_mb` | 500 | Maximum media cache size |
| `media_cache_ttl_days` | 30 | Maximum age before eviction |

Eviction: LRU within the TTL boundary. Thumbnails are evicted last.

### Read-Cache Serving (Opt-In)

The cascading read-cache mechanism used for text events applies to media. If Bob has Alice's image cached and Carol requests it by hash, Bob can serve it. Carol verifies the hash.

Media read-cache is **opt-in** and size-bounded separately from text read-cache:

| Setting | Default | Description |
|---------|---------|-------------|
| `media_read_cache_enabled` | false | Whether to serve cached media to others |
| `media_read_cache_max_mb` | 200 | Storage limit for media served to others |

Default is `false` because serving media consumes significantly more bandwidth than serving text events. Users on servers or unmetered connections can opt in.

---

## 12. Integrity Model

Every media fetch is verified:

1. Client downloads bytes from any source
2. Client computes `SHA-256(bytes)`
3. Client compares against the hash in the `media` tag
4. **Match:** display and cache. **Mismatch:** discard, try next source.

This is the same integrity model as text events (signature verification regardless of source), applied to binary blobs. A CDN, IPFS node, pact partner, or random peer can serve the media — the hash proves it is exactly what the author referenced.

**No trust in the source.** The `url_hint` is a convenience, not an endorsement. A malicious CDN serving corrupted or substituted content is detected and bypassed. A pact partner serving the correct bytes is indistinguishable from a CDN serving the correct bytes.

---

## 13. Interaction with Text Pacts

Media is explicitly excluded from text pact obligations:

| Dimension | Text Pacts | Media Pacts |
|-----------|-----------|-------------|
| Volume counted | Event JSON bytes only | Media blob bytes only |
| Challenge type | `hash` (event range) | `media_range` (byte-range) |
| Volume matching | +/- 30% | +/- 100% |
| Device types | All | Keepers only |
| Failure impact | Text pact degraded | Media pact degraded (text unaffected) |

A user with 100MB of events and 5GB of media has a 100MB text pact volume. The 5GB is either self-hosted, CDN-hosted, or covered by separate media pacts.

This separation keeps text pact obligations tractable for all device types. A phone can hold 100MB of text events for 20 pact partners (2GB total) but cannot hold 5GB of media for each.

---

## 14. Event Kind Summary

| Kind | Name | Purpose |
|------|------|---------|
| 10053 (`type: media`) | Media Pact | Reciprocal media storage commitment between Keepers |
| 10054 (`type: media_range`) | Media Challenge | Byte-range proof-of-storage for media blobs |
| 10065 | Media Manifest | Rich metadata for media blobs (dimensions, blurhash, thumbnails) |

All other media functionality (tags, retrieval, caching) operates within existing event kinds and protocol mechanisms.

---

## 15. Open Questions

1. **Video streaming.** The current design handles images well. Video files (10MB-500MB) may need a chunked storage model where media pacts store individual chunks rather than whole files. This is deferred to a future extension.

2. **Media deduplication across pacts.** If Alice and Bob both post the same image, their media pact partners store it twice (once per user's obligation). Cross-user deduplication is possible but adds complexity. Deferred.

3. **Media encryption.** Public media blobs are unencrypted (anyone with the hash can fetch and verify them). DM media should be encrypted to the recipient before hashing. The hash then references the ciphertext, and only the recipient can decrypt. Encryption scheme: NIP-44 symmetric encryption with the media blob as payload.

4. **Retention incentives.** Media pact retention defaults to 90 days. Users wanting longer retention must either self-host, use a CDN, or find partners willing to commit to longer retention. The protocol currently has no mechanism to incentivize long-term media retention beyond reciprocity.

---

## 16. Summary

| Concern | Resolution |
|---------|-----------|
| Events stay small | Media tags add ~150 bytes per reference. Event remains ~925 bytes. |
| Text pact obligations unchanged | Media excluded from text pacts. Separate media pacts optional. |
| Mobile-friendly | Phones never store media for pact partners. Lazy loading controls bandwidth. |
| No single point of failure | Content-addressed. Any source with the correct bytes works. |
| Integrity guaranteed | SHA-256 verification on every fetch. |
| Text sovereignty preserved | Media strategy does not affect text pact health or sovereignty. |
| Backward compatible | Standard Nostr clients ignore unknown tags. Media tags are additive. |
| Scalable | Full-node annual storage ~36GB even at 30% media — trivial for consumer hardware. |
| No new infrastructure required | Existing CDNs, IPFS, S3 all work. Media pacts are optional peer-to-peer layer. |
| NIP-94/NIP-96 compatible | Manifest translates to NIP-94. Upload via NIP-96 produces url_hint. |
