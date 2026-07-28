# Gozzip Wiki

An open, censorship-resistant protocol for social media and messaging. Inherits Nostr's proven primitives — secp256k1 identity, signed events, relay transport — and adds a storage and retrieval layer where users own their data. Designed to interoperate with Nostr from day one, with data portable to and from ActivityPub (Mastodon), AT Protocol (Bluesky), and other decentralized systems.

**New here?** The shortest path to understanding the protocol: [Vision](overview/vision.md) → [Glossary](glossary.md) → [System Overview](architecture/system-overview.md) → [Storage](architecture/storage.md) → [Messages](protocol/messages.md) → [Pact State Machine](protocol/pact-state-machine.md).

## Overview

- [Vision](overview/vision.md) — What Gozzip is, why it exists, and how a person grows into the network (the community-evolution arc)
- [Nostr Comparison](overview/nostr-comparison.md) — What we keep, what we change, migration strategy
- [Glossary](glossary.md) — Canonical terminology, personas, phases, and feed tiers

## Actors

- [User](actors/user.md) — Identity, keys, profiles, personas
- [Relay](actors/relay.md) — Heralds: stores and forwards messages between clients

## Protocol

- [Messages](protocol/messages.md) — Canonical event-kind registry and event structure
- [Identity](protocol/identity.md) — Key management and authentication
- [Pact State Machine](protocol/pact-state-machine.md) — The 4-state pact lifecycle and reliability model

## Architecture

- [System Overview](architecture/system-overview.md) — High-level diagram and component relationships
- [Feed Model](architecture/feed-model.md) — WoT-tiered feed construction, interaction-based referral, tiered caching
- [Multi-Device Sync](architecture/multi-device-sync.md) — Fork-and-reconcile model
- [Data Flow](architecture/data-flow.md) — How a message travels from sender to recipient
- [Storage](architecture/storage.md) — Who stores what, pacts, three-tier retrieval, data-availability verification
- [iroh Transport](architecture/iroh.md) — How Gozzip rides iroh (QUIC, gossip, cross-key binding)
- [Platform Architecture](architecture/platform-architecture.md) — Desktop, web extension, mobile, and proxy daemon feasibility

## Design

- [Incentives](design/incentives.md) — Reach-based incentives, the pay-it-forward engine, Guardian/Genesis bootstrap
- [Genesis Bootstrap](design/genesis-bootstrap.md) — How the first community forms when no one can yet be a Guardian
- [Privacy Model](design/privacy-model.md) — The honest metadata story; relay diversity; what NIP-44 does and doesn't protect
- [Moderation](design/moderation.md) — Reports, mutes, labels, deletion
- [Key Management UX](design/key-management-ux.md) — Hiding cryptographic complexity from users
- [Plausibility Analysis](design/plausibility-analysis.md) — Quantitative viability check at different network sizes
- [Simulator Architecture](design/simulator-architecture.md) — Simulator design for validating protocol formulas
- [Push Notifications](design/push-notifications.md) — Mobile delivery under OS background constraints

## Testing

- [Simulation Model](tests/simulation-model.md) — Node state variables, attack vectors, and simulation results

## Decisions

- [Architecture Decision Records](decisions/index.md) — Numbered decisions log with the rationale behind each design choice.

## Reviews

- [Adversarial Reviews](design/reviews/) — Five independent critiques (complexity, protocol design, game theory, cryptography, deployment reality) that informed the current design. See [ADR 019](decisions/019-documentation-simplification.md) for which recommendations were adopted.

## Papers

- [Trust-Weighted Gossip for Decentralized Storage and Retrieval](papers/gossip-storage-retrieval.md) — Whitepaper (§2 is the structural community parallel)

## Future / Post-v1

- [Design explorations beyond v1](future/README.md) — media layer, protocol versioning, BLE mesh, post-quantum, and more. Directions the protocol is built to grow into, not part of the v1 core.
