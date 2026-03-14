# Protocol Versioning Strategy

**Date:** 2026-03-14
**Status:** Draft

## Problem Statement

The protocol currently has no versioning strategy. The `protocol_version` tag exists on custom event kinds (defined in [messages.md](../protocol/messages.md)), but there is no specification for how versions are negotiated between peers, how deprecated versions are sunset, how security fixes propagate, or how clients coordinate upgrades across a decentralized network with no central authority.

Every successful decentralized protocol has a versioning story: Nostr uses NIPs (Nostr Implementation Possibilities), Matrix uses spec versions with a room version negotiation mechanism, and AT Protocol uses lexicon-versioned schemas. Gozzip needs the same.

Without a versioning strategy, any change to event formats, challenge-response semantics, or key derivation creates a hard fork -- some clients understand the new format, others do not, and pacts between incompatible clients silently fail.

## Design Principles

1. **No flag days.** The network must never require all clients to upgrade simultaneously.
2. **Pacts are the unit of compatibility.** Two peers in a pact agree on a protocol version for that pact. Different pacts can run different versions concurrently.
3. **Forward compatibility by default.** Clients SHOULD accept events with unknown protocol versions and process them on a best-effort basis.
4. **Deprecation is gradual.** Old versions are supported for a defined window before clients may refuse them.
5. **Security overrides all.** Critical security fixes can accelerate deprecation timelines.

## 1. Version Field on All Event Kinds

All Gozzip-specific event kinds (10050-10068) MUST include a `protocol_version` tag:

```json
["protocol_version", "1"]
```

**Format:** Simple integer string, monotonically increasing. Version `"1"` is the initial protocol version defined in [messages.md](../protocol/messages.md).

**Rules:**

- Clients MUST include `protocol_version` on all published Gozzip events (kinds 10050-10068).
- Clients SHOULD accept events with unknown (higher) protocol versions and process them on a best-effort basis, ignoring fields they do not understand.
- Clients MUST reject events with `protocol_version` below their minimum supported version.
- A protocol version bump indicates a change to event semantics, tag formats, or cryptographic operations. Additive changes (new optional tags) do not require a version bump.

**What constitutes a version bump:**

| Change type | Version bump? | Example |
|---|---|---|
| New optional tag on existing kind | No | Adding `["region", "eu"]` to kind 10056 |
| New event kind in 10050-10068 range | No (but requires NIP registration) | Adding kind 10067 |
| Changed semantics of existing tag | Yes | Changing challenge hash algorithm |
| New required tag on existing kind | Yes | Making `["tz", ...]` mandatory on kind 10056 |
| Changed key derivation | Yes | New HKDF info string format |
| Removed tag or field | Yes | Dropping `merkle_root` from kind 10051 |

## 2. Capability Advertisement in Pact Requests

Kind 10055 (storage pact request) gains a `supported_versions` tag that advertises which protocol versions the requesting client supports:

```json
{
  "kind": 10055,
  "pubkey": "<root_pubkey>",
  "tags": [
    ["volume", "<bytes>"],
    ["min_pacts", "<number_needed>"],
    ["ttl", "3"],
    ["request_id", "<unique_id>"],
    ["supported_versions", "1", "2"],
    ["protocol_version", "1"]
  ]
}
```

**Tag format:** `["supported_versions", "1", "2", ...]` -- a list of all protocol versions the client supports, as individual string values after the tag name.

**Rules:**

- Clients MUST include `supported_versions` in kind 10055 events.
- Clients SHOULD include `supported_versions` in kind 10056 (pact offer) responses so the requester can verify compatibility before accepting.
- If a kind 10055 omits `supported_versions`, peers SHOULD assume it supports only the version in its `protocol_version` tag.
- The `protocol_version` tag on the event itself indicates which version was used to construct the event. The `supported_versions` tag indicates the full range the client can handle.

**Pact offers (kind 10056) with version advertisement:**

```json
{
  "kind": 10056,
  "tags": [
    ["p", "<requester_root_pubkey>"],
    ["volume", "<bytes>"],
    ["tz", "UTC+9"],
    ["supported_versions", "1", "2"],
    ["protocol_version", "2"]
  ]
}
```

## 3. Version Negotiation

### At Pact Formation

When two peers form a pact, they negotiate the protocol version for that pact:

1. **Requester** publishes kind 10055 with `supported_versions`.
2. **Offerer** publishes kind 10056 with `supported_versions`.
3. **Negotiated version** = highest mutually supported version.
4. Both peers publish kind 10053 (pact event) using the negotiated version as the `protocol_version` tag value.

```json
{
  "kind": 10053,
  "pubkey": "<root_pubkey>",
  "tags": [
    ["d", "<partner_root_pubkey>"],
    ["type", "standard"],
    ["status", "active"],
    ["since_checkpoint", "<checkpoint_event_id>"],
    ["volume", "<bytes>"],
    ["expires", "<unix_timestamp>"],
    ["negotiated_version", "2"],
    ["protocol_version", "2"]
  ]
}
```

The `negotiated_version` tag records the agreed-upon version for the pact. All pact-related events between these two peers (challenges, gossip forwarding, checkpoint references) use this version's semantics.

### Minimum Version for Pact Formation

Clients enforce a minimum version for new pact formation:

- A client running protocol version N MUST support pact formation with peers running version N-1 (backward compatibility window of 1).
- A client MAY refuse pact formation with peers whose highest supported version is below N-2.
- During the bootstrap phase (network < 500 users), clients SHOULD accept any version to avoid partitioning a small network.

### Mid-Pact Upgrades

If a peer upgrades to a new protocol version during an active pact:

1. The upgrading peer begins publishing events with the new `protocol_version`.
2. The other peer continues processing these events on a best-effort basis (forward compatibility).
3. At the next checkpoint (approximately monthly), both peers re-evaluate version compatibility.
4. If both peers now support a higher version, the pact naturally upgrades: the next kind 10053 refresh uses the new `negotiated_version`.
5. No explicit renegotiation event is needed. The checkpoint cycle provides a natural renegotiation point.

**No forced migration:** Active pacts continue operating at their negotiated version until natural expiry or renewal. A peer upgrading to version 3 does not break its version-2 pacts. The pact continues using version 2 semantics for challenges and verification until both sides agree to upgrade.

## 4. Deprecation Timeline

### Standard Deprecation

The protocol maintains a rolling support window of N, N-1, and N-2:

```
Version N   — current (fully supported)
Version N-1 — supported (pact formation allowed)
Version N-2 — minimum supported (existing pacts continue, new pact formation discouraged)
Version N-3 — sunset (90-day countdown begins when version N is released)
```

**Timeline for a version release:**

| Day | Event |
|---|---|
| 0 | Version N released. Version N-3 enters 90-day sunset. |
| 1-90 | Clients log warnings for pacts running version N-3. |
| 90 | Version N-3 sunset complete. Clients MAY refuse new pacts with peers below version N-2. |
| 90+ | Existing version N-3 pacts continue until natural expiry but are not renewed. |

**Client behavior during sunset:**

- SHOULD display a notification to the user that a pact partner is running a deprecated version.
- SHOULD prioritize replacement of deprecated-version pacts when seeking new partners.
- MUST NOT forcibly terminate active pacts solely due to version deprecation.
- MAY refuse new pact formation with peers whose highest supported version is in sunset.

### Example: Version 3 Release

```
Before release:
  Supported: v1, v2
  Minimum for new pacts: v1

After v3 release:
  Supported: v1, v2, v3
  v1 enters 90-day sunset

After 90-day sunset:
  Supported: v2, v3
  Minimum for new pacts: v2 (clients MAY refuse v1)
  Existing v1 pacts: continue until natural expiry, then not renewed
```

## 5. Security Update Handling

Security vulnerabilities may require accelerated deprecation. Three categories:

### Hash Algorithm Weakening

If the hash algorithm used in challenge-response (currently SHA-256) is found to be weakened:

1. A new protocol version introduces the replacement algorithm.
2. A **mandatory upgrade window** of 30 days (instead of the standard 90-day sunset) is announced.
3. After the window, clients MUST reject challenge responses using the weakened algorithm.
4. Existing pacts using the old algorithm transition to Degraded state and are replaced.

The mandatory upgrade window is enforced by client implementations, not by the protocol itself. Client developers coordinate via the NIP amendment process (see section 7).

### Key Derivation Changes

If the HKDF derivation scheme (currently HKDF-SHA256 with salt `gozzip-v1`) needs updating:

1. A new protocol version introduces the new derivation with a new salt (e.g., `gozzip-v2`).
2. Old keys derived under the previous scheme remain valid for their rotation period.
3. New keys are derived under the new scheme.
4. Kind 10050 events published under the new version use new-scheme derived keys.
5. Clients supporting both versions can verify keys under either scheme during the transition.

### Challenge-Response Changes

Challenge-response semantics are negotiated per-pact via the `negotiated_version` tag:

- A version 2 pact uses version 2 challenge serialization and verification.
- A version 3 pact uses version 3 challenge serialization and verification.
- Mixed-version networks work because each pact independently uses its negotiated version.
- No global coordination needed -- peers upgrade their challenge-response behavior pact by pact.

### Emergency Minimum Version Bump

For critical vulnerabilities (e.g., a flaw that allows forging challenge responses):

1. Client developers publish an advisory with a new minimum version.
2. Clients update their minimum supported version immediately (no 90-day sunset).
3. Pacts running below the emergency minimum enter Degraded state.
4. Partners are notified via NIP-46 message with reason `security_upgrade_required`.
5. If the partner upgrades within 7 days, the pact resumes at the new version.
6. If not, the pact transitions to Failed and is replaced.

This is the only scenario where an active pact can be forcibly disrupted by a version change.

## 6. Database Schema Migration

Schema migration is a client-local concern, not a protocol-level specification. However, clients implementing Gozzip SHOULD follow these guidelines to ensure smooth upgrades and rollbacks.

### Migration Principles

1. **Additive changes only.** New fields are added with sensible defaults. Existing fields are never removed in a single version jump.
2. **Two-version field retention.** A field deprecated in version N is retained (but unused) until version N+2, providing a rollback safety net.
3. **Versioned migration scripts.** Each protocol version bump has a corresponding migration script identified by version number.
4. **Rollback support.** Migration scripts MUST be reversible for at least one version back (version N can roll back to N-1).

### Recommended Migration Flow

```
1. Client starts with database at schema version S.
2. Client detects protocol version P > S.
3. Client runs migration scripts S+1, S+2, ..., P in order.
4. Each script:
   a. Adds new columns/tables with default values.
   b. Backfills data where possible (e.g., computing new indexes).
   c. Records the migration version in a schema_version table.
5. If migration fails at step K:
   a. Roll back to step K-1.
   b. Client continues operating at the older schema version.
   c. Log a warning; do not crash.
```

### What Clients Store Per-Pact

Each pact record in the local database SHOULD include:

| Field | Description |
|---|---|
| `partner_pubkey` | Root pubkey of the pact partner |
| `negotiated_version` | Protocol version agreed upon for this pact |
| `partner_supported_versions` | Last known supported versions from the partner |
| `pact_created_at` | Unix timestamp of pact formation |
| `last_challenge_version` | Version used in the most recent challenge-response |

This allows the client to track version compatibility per-pact and detect when renegotiation is appropriate.

## 7. NIP Registry

All Gozzip custom event kinds are registered in the Nostr NIP (Nostr Implementation Possibilities) registry. This provides discoverability, prevents kind number collisions with other Nostr applications, and establishes a formal amendment process for changes.

### Registered Event Kinds

| Kind | Name | NIP | Replaceable? | Description |
|---|---|---|---|---|
| 10050 | Device Delegation | NIP-XX1 | Yes (replaceable) | Device key authorization, DM key, governance key |
| 10051 | Checkpoint | NIP-XX1 | Yes (replaceable) | Per-identity checkpoint with Merkle root |
| 10052 | Conversation State | NIP-XX2 | Yes (parameterized) | DM read-state per conversation partner |
| 10053 | Storage Pact | NIP-XX3 | Yes (parameterized) | Reciprocal storage commitment (private) |
| 10054 | Storage Challenge | NIP-XX3 | No (ephemeral) | Challenge-response proof of storage |
| 10055 | Pact Request | NIP-XX3 | No (regular) | Public broadcast requesting storage partners |
| 10056 | Pact Offer | NIP-XX3 | No (regular) | Response to pact request |
| 10057 | Data Request | NIP-XX4 | No (regular) | Pseudonymous data retrieval request |
| 10058 | Data Offer | NIP-XX4 | No (regular) | Private response with connection endpoint |
| 10059 | Storage Endpoint Hint | NIP-XX5 | No (regular) | Encrypted peer endpoints for followers |
| 10060 | Recovery Delegation | NIP-XX6 | Yes (parameterized) | Social recovery contact designation |
| 10061 | Recovery Attestation | NIP-XX6 | No (regular) | Recovery contact attestation for key rotation |
| 10062 | Push Notification Registration | NIP-XX7 | Yes (parameterized) | Push token registration with notification relay |
| 10063 | Deletion Request | NIP-XX8 | No (regular) | GDPR-compatible event deletion request |
| 10064 | Content Report | NIP-XX9 | No (regular) | Content/user report for moderation |
| 10065 | Temporary Suspension | NIP-XX10 | No (regular) | Emergency device suspension via governance key |
| 10066 | Guardianship Completion | NIP-XX11 | No (regular) | Mutual attestation of successful guardianship |
| 10067 | Reserved | -- | -- | Reserved for future protocol use |
| 10068 | Reserved | -- | -- | Reserved for future protocol use |

NIP numbers (XX1-XX11) are placeholders. Actual NIP numbers are assigned during the Nostr NIP submission process.

### NIP Grouping Strategy

Related kinds are grouped into single NIPs for coherent specification:

- **NIP-XX1 (Identity):** kinds 10050, 10051 -- device delegation and checkpoints
- **NIP-XX3 (Storage Pacts):** kinds 10053-10056 -- the full pact lifecycle
- **NIP-XX4 (Data Retrieval):** kinds 10057, 10058 -- pseudonymous data request/response
- **NIP-XX6 (Social Recovery):** kinds 10060, 10061 -- recovery delegation and attestation

### Amendment Process

Changes to Gozzip event kind formats follow the Nostr NIP amendment process:

1. **Proposal:** Draft a NIP amendment describing the change, the motivation, and the protocol version that introduces it.
2. **Review:** Community review period (minimum 14 days for non-security changes).
3. **Implementation:** At least two independent client implementations must demonstrate the change before the NIP is merged.
4. **Version bump:** If the change modifies existing semantics (see section 1), the protocol version is incremented.
5. **Deprecation:** Old behavior follows the standard deprecation timeline (section 4) unless the change is security-critical (section 5).

## 8. Compatibility Matrix

### Feature Availability by Protocol Version

| Feature | v1 | v2 (planned) | Notes |
|---|---|---|---|
| Device delegation (10050) | Yes | Yes | |
| Checkpoints with Merkle root (10051) | Yes | Yes | |
| DM conversation state (10052) | Yes | Yes | |
| Storage pacts (10053-10058) | Yes | Yes | |
| Storage endpoint hints (10059) | Yes | Yes | |
| Social recovery (10060-10061) | Yes | Yes | |
| Push notification registration (10062) | Yes | Yes | |
| Deletion requests (10063) | Yes | Yes | |
| Content reports (10064) | Yes | Yes | |
| Emergency suspension (10065) | Yes | Yes | |
| Guardianship completion (10066) | Yes | Yes | |
| `supported_versions` in pact requests | No | Yes | v1 clients infer version from `protocol_version` tag |
| `negotiated_version` in pact events | No | Yes | v1 pacts implicitly use v1 |
| Version renegotiation at checkpoint | No | Yes | v1 pacts do not renegotiate |

Version 2 is a placeholder showing how the matrix will evolve. The specific features in v2 will be determined by the first round of protocol changes that require a version bump.

### Client Version Declaration

Client implementations MUST declare their supported version range in a machine-readable format:

```json
{
  "client": "gozzip-ios",
  "client_version": "1.2.0",
  "min_protocol_version": 1,
  "max_protocol_version": 2,
  "deprecated_versions": []
}
```

This declaration is for developer documentation and interoperability testing. It is not published as an event.

### Test Vectors

Each protocol version MUST include test vectors for interoperability verification. Test vectors cover:

1. **Canonical event serialization.** A reference event for each kind with all fields populated, plus the expected serialized bytes.
2. **Challenge hash computation.** A reference event set, nonce, and expected SHA-256 output (as defined in [pact-state-machine.md](../protocol/pact-state-machine.md)).
3. **Merkle root computation.** A reference event set and expected Merkle root (as defined in [messages.md](../protocol/messages.md) for kind 10051).
4. **Key derivation.** A reference root key, derivation parameters, and expected derived keys (DM key, governance key).
5. **Rotating request token.** A reference pubkey, date, and expected token hash.

**Version 1 test vectors** (to be published alongside the first client implementation):

```
Test vector 1: Challenge hash
  Events: [event_a (device_pubkey=0xAA..., seq=0), event_b (device_pubkey=0xAA..., seq=1)]
  Nonce: 0x0000...0001 (32 bytes)
  Expected hash: (to be computed from reference implementation)

Test vector 2: Merkle root
  Events: [event_a, event_b, event_c] (ordered by device_pubkey, seq)
  Expected root: (to be computed from reference implementation)

Test vector 3: Key derivation
  Root private key: 0x0101...01 (32 bytes, test only)
  Salt: "gozzip-v1"
  Info: "dm-decryption-0"
  Expected DM public key: (to be computed)

Test vector 4: Rotating request token
  Target pubkey: 0xAAAA...AA (32 bytes hex)
  Date: "2026-01-15"
  Expected token: (to be computed)
```

Concrete values will be filled in from the reference Rust implementation in `simulator/` once it produces canonical outputs. The structure above defines what each test vector must cover.

## Interaction with Existing Protocol

### Backward Compatibility with Version 1

The current protocol (all events described in [messages.md](../protocol/messages.md)) is version 1. This versioning strategy is itself a version-1 specification -- no existing events need to change.

The additions introduced by this document (the `supported_versions` tag on kind 10055/10056, the `negotiated_version` tag on kind 10053) are optional tags that version-1 clients will ignore. A version-1 client that does not understand `supported_versions` will still form pacts normally; it simply will not participate in explicit version negotiation.

### Relationship to Pact State Machine

The [pact state machine](../protocol/pact-state-machine.md) gains one new consideration: version incompatibility as a reason for pact degradation. If a pact partner publishes events with a `protocol_version` that the client cannot process (below client's minimum or above client's maximum with no backward-compatible interpretation), the pact transitions to Degraded and the client seeks a replacement.

This is not a new state or transition -- it uses the existing Degraded state and replacement-seeking behavior defined in the state machine.

### Relationship to Key Derivation

The HKDF salt `gozzip-v1` (defined in [ADR 007](../decisions/007-key-derivation-hierarchy.md)) is a fixed protocol constant, not tied to the protocol version number. If key derivation changes in a future version, a new salt (e.g., `gozzip-v2-kdf`) would be introduced, but the version in the salt refers to the KDF scheme version, not the protocol version. This avoids coupling key material to protocol versioning.

## Open Questions

1. **Version discovery without pact formation.** Should there be a lightweight version-check mechanism (e.g., a NIP-11 relay information document extension) so clients can discover a peer's supported versions before initiating pact negotiation?

2. **Multi-version event publishing.** Should clients publish critical events (kind 10050, kind 10051) in multiple protocol versions simultaneously during transition periods, or is forward compatibility sufficient?

3. **Relay-side version filtering.** Should relays support filtering by `protocol_version` in subscription filters (e.g., `{"kinds": [10055], "#protocol_version": ["2"]}`), or is client-side filtering adequate?
