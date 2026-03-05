# ADR 008: Protocol Hardening

**Date:** 2026-02-28
**Status:** Accepted

## Context

After three rounds of protocol optimization, deep analysis identified 12 remaining structural weaknesses across security (P0), reliability/privacy (P1), and edge cases (P2). Each weakness had a concrete attack vector or failure mode. This ADR documents the decisions made to address all 12.

## Decisions

### 1. DM Key Rotation (P0)

Rotate the DM key every 90 days. Derived via `KDF(root, "dm-decryption-" || rotation_epoch)`. Old keys deleted from devices after a 7-day grace window. Provides bounded forward secrecy (~97 days).

### 2. Gossip Hardening (P0)

Three-layer defense against gossip amplification:
- `request_id` tag on kind 10055 and 10057 for deduplication
- Per-hop rate limiting (10 req/s for 10055, 50 req/s for 10057 per source)
- WoT-only forwarding (2-hop boundary)

### 3. Checkpoint Cross-Verification (P0)

Light nodes verify the per-event hash chain for the most recent M events (default: 20) from each device, alongside the checkpoint Merkle root. Inconsistencies trigger alerts and fallback to alternative sources.

### 4. Most-Recent-Action-Wins Merge Rule (P0)

**Supersedes ADR 003's conflict rule.** Follow list merge conflicts are resolved by comparing `followed_at` / `unfollowed_at` timestamps. The most recent action wins, replacing the previous "follow wins" rule. This makes follows and unfollows equally weighted.

### 5. WoT-Scoped Kind 10059 Distribution (P1)

Kind 10059 (storage endpoint hints) is only sent to followers within 2 WoT hops, limiting topology metadata leakage to trusted contacts.

### 6. Social Recovery (P1)

N-of-M social recovery via two new event kinds:
- Kind 10060 (recovery delegation) — parameterized replaceable, encrypted to each recovery contact
- Kind 10061 (recovery attestation) — published by recovery contacts, points to new root key
- 7-day timelock before new root key is accepted
- Old root key can cancel during timelock

### 7. Per-Device DM Capability (P1)

Kind 10050 device tags gain an optional `dm` flag: `["device", "<pubkey>", "mobile", "dm"]`. Only devices with the `dm` flag receive the DM private key. Reduces attack surface.

### 8. Governance Key Change Notification (P1)

Required client behavior: alert the user when kind 0 or kind 3 is updated from a different device. Client-side only, no protocol change.

### 9. Dual-Day Blind Matching (P2)

Storage peers match kind 10057 blinded pubkeys against both today and yesterday's date, handling clock skew at day boundaries.

### 10. seq Counter Recovery (P2)

On device initialization, query the last known `seq` from peers/relays before publishing. Resume from `last_known_seq + 1`. Client-side initialization step.

### 11. Pact Renegotiation Jitter (P2)

Before broadcasting kind 10055 replacement requests, clients wait `random(0, 48h)`. Standby pacts provide immediate failover during the delay.

### 12. Gossip Topology Exposure (P2)

WoT-only forwarding limits exposure. Relays can offer onion routing as an optional Lightning-paid service — gossip requests wrapped in NIP-44 encrypted layers per hop, hiding the request path from intermediate nodes.

## Consequences

**Positive:**
- All identified P0 security vulnerabilities have concrete mitigations
- Social recovery resolves the root key loss problem (previously an open question)
- DM key rotation provides bounded forward secrecy without full ratchet complexity
- Gossip hardening prevents amplification attacks while preserving the gossip model
- Per-device DM capability reduces the DM key attack surface

**Negative:**
- DM key rotation adds complexity to key management (rotation epochs, grace windows)
- Social recovery requires users to designate trusted contacts
- Most-recent-action-wins requires tracking per-pubkey action timestamps in follow list merge logic
- Two new event kinds (10060, 10061) added to the protocol

**Neutral:**
- All P2 fixes are client-side logic — no protocol changes required
- Gossip topology exposure acknowledged as acceptable tradeoff rather than fully solved
- Checkpoint cross-verification is client-side enforcement, not relay-enforced
