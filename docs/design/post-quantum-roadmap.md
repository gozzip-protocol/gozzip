# Post-Quantum Cryptography Roadmap

Migration plan for gozzip's cryptographic primitives to post-quantum algorithms. Priority: **3/10** — plan now, act later.

## Current Cryptographic Primitives

| Primitive | Algorithm | Usage | Key/Sig Size | Quantum Threat |
|---|---|---|---|---|
| Transport identity | Ed25519 | iroh EndpointId, peer authentication | 32B pub / 64B sig | Shor's algorithm breaks in poly-time |
| Event signing | secp256k1 | Nostr event signatures, author identity | 32B pub / 64B sig | Shor's algorithm breaks in poly-time |
| Content hashing | SHA-256 | Event IDs, integrity checks | 32B digest | Grover's algorithm halves security (128-bit effective) — still safe |
| Key exchange | X25519 | QUIC TLS 1.3 handshake (via iroh) | 32B | Shor's algorithm breaks in poly-time |

All signature and key exchange schemes are vulnerable to a sufficiently large quantum computer running Shor's algorithm. SHA-256 is weakened by Grover's algorithm but retains 128-bit security — acceptable.

## Why Not Now

Five reasons to defer active PQ migration:

### 1. Public Data — No Harvest-Now-Decrypt-Later Threat

The primary quantum risk for classical crypto is **HNDL**: an adversary records encrypted traffic today and decrypts it when quantum computers are available. Gozzip gossip messages are public by design — there is nothing to harvest. The only encrypted channel is QUIC transport, which has forward secrecy (ephemeral keys per session). Recorded QUIC sessions cannot be decrypted even with the long-term private key.

### 2. No E2E Encryption Yet

Gozzip does not currently implement E2E encrypted DMs. When it does (NIP-44 + NIP-17, per ADR 012), PQ key exchange should be included from day one (see Tier 3 below). But until then, there is no encryption to protect.

### 3. iroh QUIC Has Forward Secrecy

Even if an adversary obtains a node's Ed25519 private key via a future quantum computer, they cannot decrypt past QUIC sessions. The ephemeral X25519 keys used per-session are not recoverable from the long-term key. The threat is limited to **impersonation** (forging new messages), not retroactive decryption.

### 4. PQ Crates Are Unaudited

The RustCrypto ML-KEM and ML-DSA implementations exist but have not undergone formal security audits. Deploying unaudited PQ crypto could introduce implementation vulnerabilities worse than the theoretical quantum threat.

### 5. Protocol Is Pre-v1.0

Wire format changes are still free. Adding PQ-ready type enums now (Tier 0) costs nothing and preserves the ability to add PQ algorithms without a breaking change later.

## Threat Model: What Quantum Breaks

| Attack | Requires | Timeline Estimate | Impact on Gozzip |
|---|---|---|---|
| Forge Ed25519 signatures | Real-time quantum computer | 2035-2045+ | Impersonate iroh peers |
| Forge secp256k1 signatures | Real-time quantum computer | 2035-2045+ | Forge Nostr events, steal identity |
| Break X25519 key exchange | Real-time quantum computer | 2035-2045+ | MITM future QUIC sessions (not past — forward secrecy) |
| Brute-force SHA-256 | Grover's algorithm | Impractical (2^128 operations) | No practical impact |
| Decrypt recorded QUIC traffic | Quantum computer + recorded traffic | N/A — forward secrecy prevents this | No impact |

The primary risk is **identity theft** — an adversary with a quantum computer could forge signatures to impersonate any Nostr identity. This requires a real-time, fault-tolerant quantum computer, estimated at 2035-2045 at the earliest.

## NIST Post-Quantum Standards

### Adopt

| Standard | FIPS | Type | Primitive | Key Size | Sig/Ciphertext Size | Basis |
|---|---|---|---|---|---|---|
| ML-KEM | FIPS 203 | Key Encapsulation | Kyber (lattice) | 800-1568B | 768-1568B | Module-LWE |
| ML-DSA | FIPS 204 | Digital Signature | Dilithium (lattice) | 1312-2592B | 2420-4595B | Module-LWE + SIS |
| SLH-DSA | FIPS 205 | Digital Signature (backup) | SPHINCS+ (hash) | 32-64B | 7856-49856B | Hash-based (conservative) |

**ML-DSA** is the primary signature target — smallest PQ signatures with fast verification. **SLH-DSA** is the conservative backup — hash-based security assumptions are better understood than lattice assumptions, but signatures are 10-20x larger.

### Rust Crates

| Crate | Algorithm | Maintainer | Status |
|---|---|---|---|
| `ml-kem` | ML-KEM (FIPS 203) | RustCrypto | Implemented, unaudited |
| `ml-dsa` | ML-DSA (FIPS 204) | RustCrypto | Implemented, unaudited |
| `slh-dsa` | SLH-DSA (FIPS 205) | RustCrypto | Implemented, unaudited |
| `rustls-post-quantum` | Hybrid PQ key exchange for TLS | rustls project | Experimental |

## Migration Tiers

### Tier 0: PQ-Ready Type System (Now)

**Cost**: Low. **Dependency**: None.

Evolve fixed-size key and signature types into enums before v1.0 to avoid a breaking wire format change later.

```rust
// Before: fixed-size, algorithm-locked
pub type PubKey = [u8; 32];
pub type Signature = [u8; 64];

// After: extensible, algorithm-agnostic
#[derive(Serialize, Deserialize)]
pub enum CryptoKey {
    Ed25519([u8; 32]),
    Secp256k1([u8; 32]),
    // Future:
    // MlDsa44([u8; 1312]),
    // MlDsa65([u8; 1952]),
}

#[derive(Serialize, Deserialize)]
pub enum CryptoSignature {
    Ed25519([u8; 64]),
    Secp256k1([u8; 64]),
    // Future:
    // MlDsa44(Vec<u8>),  // 2420 bytes
    // MlDsa65(Vec<u8>),  // 3293 bytes
}
```

The enum discriminant adds 1 byte of wire overhead (postcard varint encoding). Verification dispatches on the variant. This is a prerequisite for all subsequent tiers.

### Tier 1: Wait for iroh PQ-Hybrid QUIC (Free)

**Cost**: Zero. **Dependency**: iroh + rustls ecosystem.

The rustls project is working on post-quantum hybrid key exchange for TLS 1.3 (X25519 + ML-KEM). When iroh upgrades its QUIC implementation (quinn, which uses rustls), gozzip gets PQ transport encryption for free — no code changes required.

This protects against future quantum MITM attacks on QUIC connections. It does NOT protect Nostr event signatures (those require Tier 2).

**Track**: `rustls-post-quantum` crate progress and iroh release notes.

### Tier 2: Optional ML-DSA Signatures (When Audited)

**Cost**: Medium. **Dependency**: Audited `ml-dsa` crate.

Add ML-DSA event signatures behind a feature flag:

```toml
[features]
post-quantum = ["ml-dsa"]
```

- Nodes with `post-quantum` enabled sign events with both secp256k1 AND ML-DSA (dual signature).
- Nodes without the flag ignore the ML-DSA signature and verify only secp256k1.
- Dual signatures ensure backward compatibility during transition.
- ML-DSA-44 adds ~2420 bytes per event signature. Acceptable for text events, notable for high-frequency event types.

**Trigger**: Deploy when the `ml-dsa` crate passes a formal security audit by a recognized firm (e.g., NCC Group, Trail of Bits, Cure53).

### Tier 3: Hybrid ML-KEM + X25519 Key Exchange (When E2E DMs Ship)

**Cost**: Medium-high. **Dependency**: E2E DM implementation (NIP-44 + NIP-17) + audited `ml-kem` crate.

When gozzip implements E2E encrypted direct messages, the key exchange MUST use a hybrid scheme from day one:

1. Perform X25519 ECDH (classical).
2. Perform ML-KEM encapsulation (post-quantum).
3. Combine both shared secrets via HKDF to derive the message encryption key.

This ensures that even if ML-KEM is later found to have a flaw, X25519 still protects the conversation. And even if quantum computers break X25519, ML-KEM protects the conversation. Both must be broken simultaneously.

**Do NOT ship classical-only E2E DMs.** Unlike public gossip messages, DM content is private and subject to HNDL attacks. An adversary who records the key exchange today could decrypt the conversation with a future quantum computer if only X25519 is used.

## Why NOT SIDH

**Supersingular Isogeny Diffie-Hellman** was xx.network's quantum-resistant key exchange. In August 2022, Wouter Castryck and Thomas Decru published an attack that recovers SIDH private keys for all standardized parameter sets (SIKEp434, SIKEp503, SIKEp610, SIKEp751) in hours on a single laptop core.

The attack exploits the auxiliary torsion point information that SIDH publishes as part of the public key. This is a fundamental protocol flaw, not an implementation bug — no parameter choice can fix it.

NIST had already advanced SIKE (SIDH's KEM variant) to Round 4 of the PQ competition before the break. It was immediately withdrawn. Any system still using SIDH/SIKE is using broken cryptography.

## Why NOT WOTS+

**Winternitz One-Time Signatures** are hash-based and quantum-resistant, but fundamentally incompatible with gozzip's requirements:

1. **One-time use**: Each WOTS+ keypair can sign exactly one message. Signing a second message with the same key leaks the private key. Nostr identities sign thousands of events over their lifetime.

2. **Large signatures**: WOTS+ signatures are 2-8KB depending on the Winternitz parameter. Compare to 64 bytes for Ed25519/secp256k1. Every gossip message would carry 30-125x more signature data.

3. **Key management complexity**: A node would need to pre-generate and manage thousands of one-time keypairs, track which have been used, and securely delete spent keys. This is fragile and error-prone.

4. **No identity continuity**: The "public key" changes with every signature. There is no persistent identifier to put in a Nostr profile or WoT graph. The entire identity model breaks down.

SLH-DSA (SPHINCS+, FIPS 205) solves these problems — it is hash-based and quantum-resistant like WOTS+ but supports unlimited signatures per keypair, at the cost of larger signature sizes (7-50KB). It remains gozzip's conservative backup if lattice-based ML-DSA is ever compromised.
