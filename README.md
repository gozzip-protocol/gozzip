# Gozzip

An open, censorship-resistant protocol for social media and messaging. Inherits Nostr's proven primitives — secp256k1 identity, signed events, relay transport — and adds a storage and retrieval layer where users own their data.

Your social graph *is* your infrastructure: friends store your data, you store theirs, and content propagates through trust-weighted gossip. Data is self-authenticating (signed by your keys) and portable — it can be converted to and from ActivityPub (Mastodon), AT Protocol (Bluesky), and other decentralized systems. Works with existing Nostr relays and clients from day one.

**Read the whitepaper:** [Trust-Weighted Gossip for Decentralized Storage and Retrieval](docs/papers/gossip-storage-retrieval.pdf) ([markdown version](docs/papers/gossip-storage-retrieval.md))

## The Problem

Decentralized social protocols still depend on servers that control what gets stored, served, and censored. Nostr relays, Mastodon instances, Bluesky's relay infrastructure — your data lives on infrastructure you don't control. If a server goes down, changes its terms, or censors your content — your data disappears. Even protocols that call themselves "decentralized" centralize custody of your data.

## The Approach

- **Storage pacts** — bilateral agreements where WoT peers store each other's events, enforced by challenge-response verification
- **Tiered retrieval** — reads cascade from local pact storage (92% of reads) through gossip (2.2%) to relay fallback (4.7%), achieving 98.8% success without relay dependency
- **WoT-filtered gossip** — information propagates through trust boundaries, not broadcast channels
- **Transport independence** — via FIPS integration, the protocol operates over IP, BLE mesh, radio, Tor, or any other transport

The relay doesn't die — it becomes an optional discovery layer and performance accelerator instead of a gatekeeper.

## Project Structure

```
gozzip/
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
│   ├── design/         Design documents (incentives, risk, simulator)
│   └── glossary.md     Terminology and persona definitions
└── CLAUDE.md           Development workflow instructions
```

### Protocol Paper

The primary technical document is the protocol paper:

- **Markdown**: [`docs/papers/gossip-storage-retrieval.md`](docs/papers/gossip-storage-retrieval.md)
- **LaTeX/PDF**: [`docs/papers/gossip-storage-retrieval.pdf`](docs/papers/gossip-storage-retrieval.pdf)

Covers the full protocol: storage pacts, tiered retrieval, WoT-filtered gossip, network-theoretic foundations, FIPS transport integration, comparison with Nostr/Mastodon, simulation results at 5,000 nodes, and a phased implementation roadmap.

### Design Documents

The `docs/` directory contains working documentation developed alongside the protocol:

- **ADRs** (`docs/decisions/`) — key design choices with rationale (custom protocol over pure Nostr, checkpoint reconciliation, key hierarchy, protocol hardening, etc.)
- **Architecture** (`docs/architecture/`) — system overview, storage design, multi-device sync, data flow
- **Design** (`docs/design/`) — deeper explorations of incentive models, storage pact risk mitigations, simulator architecture, plausibility analysis

### Simulator

The [`simulator/`](simulator/) directory contains a Rust discrete-event network simulator that validates the protocol's claims. It models thousands of nodes as independent async actors — following each other, publishing content, forming pacts, gossiping, and reading posts — with realistic latency and uptime distributions.

```bash
cd simulator && cargo build --release
./target/release/gozzip-sim --nodes 5000 --ba-edges 50 validate
```

Key results from a 5,000-node, 30-day simulation:

| Metric | Result |
|--------|--------|
| Overall retrieval success | 98.8% |
| Reads from local pact storage | 92.0% |
| Relay fallback usage | 4.7% |
| Relay dependency at 14+ day maturity | 0.2% |
| Mean pacts per node | 18.69 / 20 target |

The simulator also supports stress testing: Sybil attacks, network partitions, churn storms, and viral content spikes. See [`simulator/README.md`](simulator/README.md) for full documentation.

## Current Status

The protocol is validated by simulation (100–5,000 nodes). No production implementation exists yet. The architecture is designed for a gradual transition from today's relay model — storage decentralization first (invisible, background), retrieval decentralization later — with each phase delivering value independently.

## License

MIT License. See [LICENSE](LICENSE).
