# ADR 010: Bitchat Integration

**Date:** 2026-03-01
**Status:** Accepted

## Context

Gozzip's delivery stack requires internet connectivity. No transport exists for offline scenarios — protests, disasters, connectivity-limited regions. Additionally, there is no way to discover nearby users without already knowing their identity, and no emergency data destruction mechanism for users in hostile environments.

[Bitchat](https://github.com/permissionlesstech/bitchat) solves offline messaging via BLE mesh with Nostr integration. Several of its features can be adapted for Gozzip without protocol changes.

## Decision

Integrate five client-side features inspired by bitchat:

### 1. BLE Mesh Transport

Add BLE (Bluetooth Low Energy) mesh as "Layer 0" in the delivery stack, interoperable with bitchat's mesh network. Transport priority becomes: BLE mesh → cached endpoints → gossip → relay. Gozzip events are wrapped in bitchat's compact binary format for BLE transmission. Multi-hop relay up to 7 hops. Noise Protocol encryption between mesh peers. Events are still signed by device subkeys — BLE is just a transport.

### 2. Store-and-Forward Queuing

Events are signed and stored in a local outbound queue when no transport is available. Queue drains automatically when any transport becomes available (BLE, internet). Persists across app restarts. Maximum: 1,000 events or 50MB (configurable).

### 3. Noise Protocol for BLE Sessions

Noise Protocol (XX handshake) for BLE mesh sessions provides forward secrecy per session. Complements NIP-44: Noise secures the BLE transport link, NIP-44 secures the message content end-to-end.

### 4. Geohash Discovery with Ephemeral Subkeys

Client-side "nearby mode" using ephemeral keypairs completely disconnected from the user's root identity. Ephemeral keys are NOT derived from root, NOT in kind 10050, NOT linked to identity. Events include a geohash tag (`["g", "<geohash>"]`) at user-chosen precision but no `root_identity` tag. Two users can privately reveal their real identities via NIP-44 exchange through ephemeral keys. Key discarded when nearby mode deactivated.

### 5. Panic Wipe

Client-side emergency data destruction. Configurable trigger (triple-tap, decoy PIN, gesture). Wipes all private keys, cached events, and local state. Recovery via cold storage root key + storage peers + checkpoint reconciliation. Optional decoy mode shows an innocuous profile after wipe.

## Rejected Elements

- **No accounts / no persistent identity** — contradicts Gozzip's root key identity model
- **Ephemeral keys as primary identity** — only used for nearby mode
- **Geohash channels as relay infrastructure** — too high a surveillance surface

## Consequences

**Positive:**
- Offline resilience via BLE mesh without internet dependency
- Location-based discovery without compromising identity privacy
- Emergency data protection for users in hostile environments
- Zero protocol changes — all features are client-side
- Interop with bitchat's existing mesh network extends offline reach

**Negative:**
- BLE mesh adds client complexity (Bluetooth stack, battery management)
- Nearby mode requires careful UX to prevent accidental identity reveal
- Store-and-forward queue needs disk management on mobile devices

**Neutral:**
- Noise Protocol is an additional encryption layer, not a replacement for NIP-44
- All features are opt-in — users who don't need offline/nearby/panic can ignore them
