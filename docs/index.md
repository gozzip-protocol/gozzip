# Gozzip Wiki

An open, censorship-resistant protocol for social media and messaging. Inherits Nostr's proven primitives — secp256k1 identity, signed events, relay transport — and adds a storage and retrieval layer where users own their data. Designed to interoperate with Nostr from day one, with data portable to and from ActivityPub (Mastodon), AT Protocol (Bluesky), and other decentralized systems.

## Overview

- [Vision](overview/vision.md) — What Gozzip is, why it exists
- [Nostr Comparison](overview/nostr-comparison.md) — What we keep, what we change, migration strategy
- [Glossary](glossary.md) — Terminology, personas, and protocol concepts

## Actors

- [User](actors/user.md) — Identity, keys, profiles
- [Relay](actors/relay.md) — Stores and forwards messages between clients

## Protocol

- [Messages](protocol/messages.md) — Message types and event structure
- [Identity](protocol/identity.md) — Key management and authentication

## Architecture

- [System Overview](architecture/system-overview.md) — High-level diagram and component relationships
- [Feed Model](architecture/feed-model.md) — WoT-tiered feed construction, interaction-based referral, tiered caching
- [Multi-Device Sync](architecture/multi-device-sync.md) — Fork-and-reconcile model
- [Data Flow](architecture/data-flow.md) — How a message travels from sender to recipient
- [Storage](architecture/storage.md) — Who stores what, persistence model
- [Platform Architecture](architecture/platform-architecture.md) — Desktop, web extension, mobile, and proxy daemon feasibility

## Design

- [Plausibility Analysis](design/plausibility-analysis.md) — Quantitative viability check at different network sizes
- [Simulator Architecture](design/simulator-architecture.md) — Simulator design for validating protocol formulas
- [Incentive Model](design/incentive-model.md) — Karma, contribution scoring, and free-rider prevention
- [Bitchat Integration](design/bitchat-integration.md) — Bluetooth mesh chat with Nostr integration

## Testing

- [Simulation Model](tests/simulation-model.md) — Node state variables, attack vectors, and simulation results

## Decisions

- [Architecture Decision Records](decisions/index.md) — ADR format, numbered decisions log

## Papers

- [Trust-Weighted Gossip for Decentralized Storage and Retrieval](papers/gossip-storage-retrieval.md) — Whitepaper
