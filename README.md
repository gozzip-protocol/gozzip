# Gozzip

An open, censorship-resistant protocol for social media and messaging. Inherits Nostr's proven primitives — secp256k1 identity, signed events, relay transport — and adds a storage and retrieval layer where users own their data.

Your social graph *is* your infrastructure: friends store your data, you store theirs, and content propagates through trust-weighted gossip. Data is self-authenticating (signed by your keys) and portable — it can be converted to and from ActivityPub (Mastodon), AT Protocol (Bluesky), and other decentralized systems. Works with existing Nostr relays and clients from day one.

**Read the whitepaper:** [Trust-Weighted Gossip for Decentralized Storage and Retrieval](docs/papers/gossip-storage-retrieval.pdf) ([markdown version](docs/papers/gossip-storage-retrieval.md))

## The Problem

Decentralized social protocols still depend on servers that control what gets stored, served, and censored. Nostr relays, Mastodon instances, Bluesky's relay infrastructure — your data lives on infrastructure you don't control. If a server goes down, changes its terms, or censors your content — your data disappears. Even protocols that call themselves "decentralized" centralize custody of your data.

## The Approach

- **Storage pacts** — bilateral agreements where WoT peers store each other's events, verified by data-availability challenges (capped-asymmetric, renewal-by-default)
- **Tiered retrieval** — reads cascade through three tiers: local pact storage, a direct encrypted query to the author's pact partners, then relay fallback — keeping most reads local and relay dependency sharply reduced, not eliminated
- **WoT-filtered gossip** *(optional extension)* — an opt-in censorship-resistance path where information propagates through trust boundaries rather than broadcast channels
- **Transport independence** — via [iroh](https://docs.iroh.computer/) integration, the protocol operates over QUIC with peer-to-peer NAT traversal, with future paths to BLE mesh, radio, and Tor via iroh's multipath QUIC transport

The relay doesn't die — its role shrinks to discovery, bootstrap, mobile-to-mobile mailbox, and fallback: reduced custody, not optional infrastructure.

## Project Structure

```
gozzip/
├── gozzip-types/      Shared wire-format types (Rust library crate)
├── gozzip-node/       Real network node using iroh P2P transport
│   ├── discovery.rs — NIP-05 identity verification
│   └── net/blobs.rs — Content-addressed blob storage (iroh-blobs)
├── simulator/          Rust discrete-event network simulator
│   ├── src/            Simulation engine (async actor model)
│   └── config/         TOML configuration profiles
├── docs/
│   ├── papers/         Protocol paper (LaTeX + markdown + PDF)
│   ├── overview/       Vision and Nostr comparison
│   ├── architecture/   System design, data flow, multi-device sync
│   ├── protocol/       Wire format, messages, identity
│   ├── actors/         Participant roles (users, relays)
│   ├── decisions/      Architecture Decision Records (ADRs)
│   ├── design/         Design documents (incentives, privacy, moderation, keys)
│   ├── future/         Deferred post-v1 explorations (see ADR 019)
│   └── glossary.md     Terminology and persona definitions
└── CLAUDE.md           Development workflow instructions
```

### Protocol Paper

The primary technical document is the protocol paper:

- **Markdown**: [`docs/papers/gossip-storage-retrieval.md`](docs/papers/gossip-storage-retrieval.md)
- **LaTeX/PDF**: [`docs/papers/gossip-storage-retrieval.pdf`](docs/papers/gossip-storage-retrieval.pdf)

Covers the full protocol: storage pacts, three-tier retrieval, WoT-filtered gossip (optional), network-theoretic foundations, iroh transport integration, comparison with Nostr/Mastodon, and a phased implementation roadmap. (Empirical results live in the [plausibility analysis](docs/design/plausibility-analysis.md) and [simulation model](docs/tests/simulation-model.md), not the paper.)

### Design Documents

The `docs/` directory contains working documentation developed alongside the protocol:

- **ADRs** (`docs/decisions/`) — key design choices with rationale (custom protocol over pure Nostr, checkpoint reconciliation, key hierarchy, protocol hardening, etc.)
- **Architecture** (`docs/architecture/`) — system overview, storage design, multi-device sync, data flow
- **Design** (`docs/design/`) — deeper explorations of incentive models, storage pact risk mitigations, simulator architecture, plausibility analysis

### Simulator

The [`simulator/`](simulator/) directory contains a Rust discrete-event network simulator that models thousands of nodes as independent async actors — following each other, publishing content, forming pacts, gossiping, and reading posts — with realistic latency and uptime distributions. It exercises the protocol's analytical model and stresses it under adversarial conditions: Sybil attacks, network partitions, churn storms, and viral content spikes.

```bash
cd simulator && cargo build --release
./target/release/gozzip-sim --nodes 5000 --ba-edges 50 validate
```

The simulator is being rebuilt against the current protocol (capped pacts, presence-aware scoring, renewal-by-default, three-tier retrieval); empirical figures will follow that rebuild. See [`simulator/README.md`](simulator/README.md) for documentation and the [plausibility analysis](docs/design/plausibility-analysis.md) for the analytical model.

## Current Status

The protocol's design is complete and its analytical model is documented; the simulator is being rebuilt to validate it against the current mechanisms. A real network node (`gozzip-node`) using iroh for peer-to-peer transport is under development. The architecture is designed for a gradual transition from today's relay model — storage decentralization first (invisible, background), retrieval decentralization later — with each phase delivering value independently.

### Features

- **Gossip privacy** (optional extension): Batch-and-shuffle forwarding breaks temporal correlation in message propagation, where the WoT gossip tier is enabled
- **NIP-44 encrypted DMs** (planned): Point-to-point encrypted direct messages with NIP-17 gift wrapping
- **NIP-28 channels** (planned): Group conversations over gossip topics with NIP-10 threading
- **NIP-05 identity verification**: DNS-based identity claims linking Nostr pubkeys to human-readable identifiers
- **iroh-blobs content addressing** (planned): Large content stored as BLAKE3-addressed blobs with chunked transfer
- **Post-quantum ready type system**: Wire format types support classical and PQ algorithm variants for future migration

### Recent

- **xx.network evaluation** (ADR 012): Evaluated cMix mixnet protocol for traffic analysis resistance. Adopted gossip-native privacy enhancements (batch-and-shuffle, partial HyParView rotation) instead of full mixnet infrastructure.

## License

MIT License. See [LICENSE](LICENSE).
