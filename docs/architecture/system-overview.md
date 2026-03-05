# System Overview

High-level architecture of Gozzip — an open, censorship-resistant protocol for social media and messaging that inherits Nostr's primitives and adds a data ownership layer.

## Components

- **Root identity** — secp256k1 keypair, same as Nostr. The user's permanent identity.
- **Device subkeys** — per-device keypairs, plus derived DM and governance keys. Authorized by root key via kind 10050.
- **Relays** — store and forward events. Standard Nostr relays work without modification. Optimized relays can optionally resolve device → root identity for faster queries.
- **Clients** — user-facing apps (mobile, desktop, web, browser extension). Sign events with device keys. Handle all protocol intelligence: gossip forwarding, blinded matching, WoT filtering, device resolution.
- **Storage peers** — WoT peers that hold your recent events via reciprocal pacts. Serve your data when your devices are offline.
- **Bridges** — connect to other networks (Nostr, ActivityPub, etc.)

## What We Build on Top of Nostr

```
┌──────────────────────────────────────────────────┐
│                 Gozzip Layer                      │
│                                                   │
│  Kind 10050 (device delegation)                  │
│  Kind 10051 (checkpoint)                         │
│  Kind 10052 (conversation state)                 │
│  Kind 10053-10059 (storage pacts + endpoint hints)│
│  Kind 10060-10061 (social recovery)               │
│  root_identity tag convention                     │
│  Client-side device→root resolution               │
│  Follow-as-commitment indexing                   │
│  BLE mesh transport (bitchat interop)              │
├──────────────────────────────────────────────────┤
│                 Nostr Layer                       │
│                                                   │
│  Event structure, signatures, relay protocol     │
│  Kind 0, 1, 3, 6, 7, 14, 30023, 9734/9735       │
│  NIP-10, NIP-23, NIP-29, NIP-44, NIP-57, NIP-59│
│  secp256k1 keys, WebSocket transport             │
└──────────────────────────────────────────────────┘
```

## Design Principles

- **User sovereignty** — users are not dependent on relays. They index their own social graph. Data is self-authenticating and portable across protocols.
- **Follow-as-commitment** — following costs you indexing resources. You curate wisely.
- **Nostr-native** — existing keys, events, and relays work from day one. No relay modifications needed.
- **Protocol-portable** — signed events can be converted to ActivityPub, AT Protocol, and other decentralized formats. Users own their data, not the protocol.
- **Light by default** — checkpoints enable light nodes. Full history is opt-in.
- **Device isolation** — compromise one device, revoke it. Identity survives.
- **Emergent incentives** — contribution to the network (storage, curation) translates to content reach through pact-aware gossip routing. No tokens or subscriptions.

## What Needs to Be Built

| Component | Scope |
|-----------|-------|
| Kind 10050 | NIP draft for device delegation |
| Kind 10051 | NIP draft for checkpoints |
| Kind 10052 | DM read-state sync (conversation state) |
| `root_identity` tag | Convention, documented in NIP |
| Client resolver | Client-side device ↔ root resolution (required). Relay oracle resolution (optional optimization). |
| Kind 10053–10058 | Storage pact events (pact, challenge, request, offer, data request/offer) |
| Kind 10059 | Storage endpoint hints for gossip discovery |
| Kind 10060–10061 | Social recovery (recovery delegation + attestation) |
| Key derivation | KDF for DM key, governance key from root |
| Per-event chain | seq + prev_hash on device-signed events |
