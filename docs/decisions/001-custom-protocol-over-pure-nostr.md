# ADR 001: Custom Protocol Over Pure Nostr

**Date:** 2026-02-28
**Status:** Accepted

## Context

We need an open, censorship-resistant protocol for social media and messaging that gives users ownership of their data. Nostr exists and has an active ecosystem of relays, clients, and users with the best primitives for censorship-resistant identity. We could build purely on Nostr, fork it, or design something new.

## Decision

Design a custom protocol from scratch that maintains interoperability with the Nostr ecosystem, minimizing transition cost for existing Nostr users and infrastructure.

## Consequences

- We can innovate beyond Nostr's constraints where needed
- Existing Nostr relays and clients work from day one — no modifications needed
- We inherit the benefits of Nostr's key/identity model (secp256k1, signed events)
- Data is self-authenticating and protocol-portable — convertible to ActivityPub, AT Protocol, and other decentralized systems
- We take on the cost of designing and documenting a new protocol layer
- We need to maintain a clear NIP compatibility map
