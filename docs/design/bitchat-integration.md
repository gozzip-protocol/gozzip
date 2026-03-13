# Bitchat Integration — Design Document

**Date:** 2026-03-01
**Status:** Approved
**Inspiration:** [bitchat](https://github.com/permissionlesstech/bitchat) — Bluetooth mesh chat with Nostr integration

## Overview

Five features adapted from bitchat's architecture, integrated into Gozzip's protocol. The goal is offline resilience, local discovery, and emergency safety — without compromising Gozzip's identity model or privacy guarantees.

### Why BitChat

BitChat is a well-engineered protocol for the problem it solves: encrypted, ephemeral, peer-to-peer messaging over BLE with no servers, no accounts, and no internet. Its cryptographic layer — Noise Protocol (XX pattern) with Curve25519/ChaChaPoly/SHA256, per-session forward secrecy, and Ed25519 signing — is rigorous. Its compact binary framing (13-byte fixed header, PKCS#7 padding to standard block sizes) is purpose-built for BLE's constrained MTU. And its Bloom filter gossip deduplication solves routing loops efficiently with minimal memory.

Rather than reimplementing these primitives, Gozzip adopts BitChat as its Tier 0 transport. The two protocols are complementary: BitChat excels as a real-time ephemeral mesh for immediate local communication. Gozzip adds the persistence layer — storage pacts, Web of Trust routing, long-term identity with key hierarchy and social recovery — that turns ephemeral messaging into sovereign social infrastructure. BitChat handles "send a message to someone nearby right now." Gozzip handles "own your social life permanently." Together they form a complete stack from Bluetooth radio to persistent social media.

## 1. BLE Mesh Transport (Interop with Bitchat)

### Problem

Gozzip's delivery stack (cached endpoints → gossip → relay) requires internet connectivity. No transport exists for offline scenarios — protests, disasters, connectivity-limited regions.

### Design

Add BLE (Bluetooth Low Energy) mesh as "Layer 0" in the delivery stack, interoperable with bitchat's existing mesh network.

**Transport priority becomes:**
1. **BLE mesh** — nearby devices, zero internet required
2. **Cached endpoints** — direct connection to known storage peers
3. **Gossip** — TTL-based forwarding through WoT
4. **Relay fallback** — traditional relay query

**Interop with bitchat:**
- Gozzip clients participate in bitchat's BLE mesh network using bitchat's Noise Protocol sessions for local transport
- Gozzip events are wrapped in bitchat's compact binary format for BLE transmission
- Bitchat devices relay Gozzip events without understanding them (opaque payload relay)
- When a Gozzip event reaches another Gozzip client via the mesh, it's unwrapped and verified normally (signature check against root_identity)

**BLE mesh properties (inherited from bitchat):**
- Multi-hop relay: up to 7 hops through intermediate devices. While the protocol supports up to 7 relay hops, effective throughput degrades exponentially per hop (~0.15 KB/s at 7 hops). The practical maximum for usable data exchange is 3-4 hops. The primary everyday use case is 1-hop direct peer exchange.
- Automatic peer discovery and connection management
- Battery-optimized adaptive power cycling
- LZ4 message compression for bandwidth optimization
- Noise Protocol encryption between mesh peers (forward secrecy per session)

**What Gozzip adds on top:**
- Events are still signed by device subkeys — BLE is just a transport, not an authentication layer
- Storage pact partners who are physically nearby can sync pact data via BLE without internet
- Checkpoint reconciliation works the same — BLE-delivered events are treated identically to relay-delivered events

### Scope

Client-side transport layer. No new event kinds. No protocol changes. The BLE mesh is a delivery mechanism, not a protocol extension.

## 2. Store-and-Forward Queuing

### Problem

Gozzip assumes "publish and it goes somewhere." If the device has no connectivity (no internet, no BLE peers nearby), the event is lost or the user must retry manually.

### Design

Events are queued locally when no transport is available, with automatic delivery when connectivity returns.

**Queue behavior:**
- Events are signed and stored in a local outbound queue
- The queue is ordered by `created_at` timestamp
- When any transport becomes available (BLE, internet), the queue drains automatically
- Events in the queue are valid Nostr events — they can be delivered to any transport
- Queue persists across app restarts (stored on disk)
- Maximum queue size: client-configurable (default: 1,000 events or 50MB)

**Delivery priority:**
- BLE mesh is tried first (if enabled and peers are nearby)
- Then internet transports (cached endpoints → gossip → relay)
- Events that fail delivery are retried with exponential backoff
- Events older than 7 days in the queue are flagged for user review (stale content)

### Scope

Client-side logic. No protocol changes.

## 3. Noise Protocol for Local Sessions

### Problem

NIP-44 provides authenticated encryption but no forward secrecy for real-time sessions. The 90-day DM key rotation provides bounded forward secrecy at best. For real-time local exchanges (BLE mesh), session-based forward secrecy is needed.

### Design

Use the Noise Protocol framework for BLE mesh sessions, complementing NIP-44 for internet-based encryption.

**Two encryption layers:**
- **BLE transport:** Noise Protocol (XX handshake pattern) between mesh peers. Forward secrecy per session. Session keys are ephemeral — compromising a session doesn't reveal past or future sessions.
- **Event payload:** NIP-44 encryption for DM content (unchanged). The Noise session encrypts the transport; NIP-44 encrypts the content.

**Why both layers:**
- Noise Protocol secures the BLE link (prevents eavesdropping on the mesh)
- NIP-44 secures the message content (end-to-end between sender and recipient, regardless of transport)
- A compromised mesh relay can see that encrypted events are being transmitted but cannot read content

### Scope

Client-side BLE transport encryption. No protocol changes. The Noise layer is between adjacent BLE peers; the NIP-44 layer is between the event author and the intended recipient.

## 4. Geohash Discovery with Ephemeral Subkeys

### Problem

Gozzip's discovery relies on WoT — you find content through people you follow. There's no way to discover nearby users or local content without already knowing their identity. Adding location-based discovery naively would create a surveillance surface linking identities to physical locations.

### Design

Client-side "nearby mode" using ephemeral subkeys that are completely disconnected from the user's root identity.

**Ephemeral identity:**
- When the user activates nearby mode, the client generates a fresh ephemeral keypair
- This key is NOT derived from the root key, NOT listed in kind 10050, NOT linked to the user's identity
- Events signed by the ephemeral key include a geohash tag but NO `root_identity` tag
- The ephemeral key is disposable — discarded when nearby mode is deactivated

**Geohash tagging:**
- Events in nearby mode include a `["g", "<geohash>"]` tag at a user-chosen precision level:
  - Block (7 chars): ~150m radius
  - Neighborhood (6 chars): ~1.2km radius
  - City (5 chars): ~5km radius
- The user controls the precision — lower precision = more privacy, less specific discovery

**Discovery flow:**
1. User activates nearby mode → ephemeral key generated
2. Client broadcasts presence via BLE mesh and/or gossip with geohash tag
3. Other nearby users in nearby mode see ephemeral-key-signed content
4. Content is anonymous — no link to any persistent identity
5. If two users want to connect, they initiate a private identity reveal:
   - Encrypted NIP-44 exchange via the ephemeral keys
   - Each side reveals their root pubkey to the other
   - Both can then follow each other via normal Gozzip identity
6. Deactivate nearby mode → ephemeral key discarded, no trace

**Privacy properties:**
- Location data never touches the main identity
- Ephemeral keys are disposable — no history to correlate across sessions
- The user controls when (and if) to reveal their real identity
- Geohash precision is user-controlled
- No relay involvement needed — works via BLE mesh or local gossip

**Geohash precision levels (from bitchat):**
| Level | Geohash length | Approximate area |
|-------|---------------|-----------------|
| Block | 7 chars | ~150m × 150m |
| Neighborhood | 6 chars | ~1.2km × 0.6km |
| City | 5 chars | ~5km × 5km |
| Province | 4 chars | ~39km × 20km |
| Region | 2 chars | ~1,250km × 625km |

### Scope

Client-side feature. The ephemeral key and geohash tag are standard Nostr event conventions — no new kinds needed. The identity reveal is a private NIP-44 exchange between two ephemeral keys.

## 5. Panic Wipe

### Problem

Users in hostile environments (protests, authoritarian regimes) need the ability to instantly destroy all local data if their device is at risk of being seized.

### Design

Client-side emergency data destruction triggered by a configurable gesture.

**Trigger options (client-configurable):**
- Triple-tap on app logo (bitchat's approach)
- Specific gesture (swipe pattern, shake)
- Decoy PIN: entering a specific PIN at app launch wipes data instead of unlocking

**What gets wiped:**
- All private keys (device key, DM key, governance key)
- All cached events (own and followed users')
- Outbound queue
- Storage pact data
- All local state

**What survives (not on device):**
- Root key (in cold storage — hardware wallet, air-gapped)
- Published events (on storage peers, relays)
- Storage pacts (partners still hold your data)
- Identity (root pubkey is public, can be recovered via social recovery)

**Recovery after wipe:**
1. User accesses root key from cold storage
2. Publishes new kind 10050 with a new device key
3. Fetches events from storage peers (pacts still active)
4. Syncs via checkpoint reconciliation
5. Back to normal — the wipe only destroyed local state

**Decoy mode (optional):**
- Instead of wiping to a blank state (which signals "something was here"), the client can wipe to a decoy profile with innocuous content
- The decoy profile has its own keypair and minimal follow list
- An adversary inspecting the device sees a normal-looking app with harmless content

### Scope

Client-side feature. No protocol changes. The recovery path uses existing protocol mechanisms (cold storage root key, kind 10050, storage peers, checkpoint sync).

## Summary

| Feature | Transport | Privacy | Protocol change? |
|---------|-----------|---------|-----------------|
| BLE mesh | Offline delivery via bitchat mesh | Noise Protocol encryption | No — transport only |
| Store-and-forward | Offline queuing with auto-delivery | Events signed before queuing | No — client logic |
| Noise Protocol | Forward secrecy for BLE sessions | Session-based keys, ephemeral | No — transport encryption |
| Geohash discovery | BLE mesh or local gossip | Ephemeral subkeys, no identity link | No — event conventions |
| Panic wipe | N/A | Destroys all local state | No — client feature |

All five features are client-side. Zero protocol changes. Zero new event kinds. The BLE mesh is a transport layer, geohash discovery uses ephemeral keys with standard tags, and panic wipe uses existing recovery mechanisms.

## Rejected Elements from Bitchat

- **No accounts / no persistent identity** — contradicts Gozzip's root key identity model. Gozzip needs persistent identity for WoT, storage pacts, and reputation.
- **Ephemeral keys as primary identity** — only used for nearby mode, not as the user's main identity.
- **Geohash channels as relay infrastructure** — too high a surveillance surface. Client-side ephemeral discovery is safer.
