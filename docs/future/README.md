# Future / Post-v1 Design Explorations

> **Nothing in this directory is part of protocol v1.** These are design
> explorations for directions the protocol is built to grow into — good ideas that
> are premature for a protocol at this stage. The live documents under
> `docs/protocol/`, `docs/architecture/`, and `docs/design/` are authoritative;
> read these as forward-looking design notes.

## Deferred explorations

- [Media layer](media-layer.md) — media pacts, CDN economics. v1 keeps only content-addressed `media` tags + hash verification (in [storage](../architecture/storage.md)).
- [Protocol versioning](protocol-versioning.md) — negotiation/deprecation strategy. v1 keeps only the `protocol_version` tag.
- [BitChat / BLE mesh](bitchat-integration.md) — Bluetooth transport (Tier 0). Deferred by [ADR 011](../decisions/011-iroh-transport-integration.md); no iroh BLE transport exists yet.
- [Post-quantum roadmap](post-quantum-roadmap.md) — ML-KEM/ML-DSA migration (self-assessed priority 3/10, [ADR 012](../decisions/012-xx-network-evaluation.md)).
- [Proof-of-storage alternatives](proof-of-storage-alternatives.md) — survey of verification schemes. The one adopted item (Merkle-proof challenges) lives in [storage](../architecture/storage.md).
- [Spam resistance](spam-resistance.md) — emergent-property analysis (not a spec).
- [Monitoring & diagnostics](monitoring-diagnostics.md) — full observability suite. v1 keeps only a pact list + one health number.

## History

- [history/](history/) — superseded planning docs (the live [feed model](../architecture/feed-model.md) is the sole current telling).
