//! NIP-05 identity verification and peer discovery.
//!
//! Implements Nostr NIP-05 DNS-based identity verification for gozzip nodes.
//! Nodes can verify claimed identities by resolving NIP-05 addresses and
//! checking the returned public key against the claimed identity.

use gozzip_types::PubKey;
use serde::Deserialize;
use tracing::debug;

/// A verified NIP-05 identity.
#[derive(Debug, Clone)]
pub struct Nip05Identity {
    /// The NIP-05 address (e.g., "alice@example.com").
    pub address: String,
    /// The local part (e.g., "alice").
    pub name: String,
    /// The domain (e.g., "example.com").
    pub domain: String,
    /// The verified public key.
    pub pubkey: PubKey,
}

/// Response from a NIP-05 well-known endpoint.
#[derive(Debug, Deserialize)]
struct Nip05Response {
    /// Map of local name -> hex-encoded pubkey.
    names: std::collections::HashMap<String, String>,
}

/// Errors from NIP-05 verification.
#[derive(Debug, thiserror::Error)]
pub enum Nip05Error {
    #[error("invalid NIP-05 address format: {0}")]
    InvalidFormat(String),

    #[error("HTTP request failed: {0}")]
    Http(String),

    #[error("invalid JSON response: {0}")]
    InvalidResponse(String),

    #[error("name '{name}' not found at {domain}")]
    NameNotFound { name: String, domain: String },

    #[error("pubkey mismatch: expected {expected}, got {actual}")]
    PubkeyMismatch { expected: String, actual: String },

    #[error("invalid hex pubkey: {0}")]
    InvalidPubkey(String),
}

/// Parse a NIP-05 address into (name, domain).
///
/// Valid format: "name@domain" or "_@domain" (for domain-level identity).
pub fn parse_nip05(address: &str) -> Result<(String, String), Nip05Error> {
    let parts: Vec<&str> = address.splitn(2, '@').collect();
    if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
        return Err(Nip05Error::InvalidFormat(address.to_string()));
    }

    let name = parts[0].to_lowercase();
    let domain = parts[1].to_lowercase();

    // Basic domain validation
    if !domain.contains('.') || domain.starts_with('.') || domain.ends_with('.') {
        return Err(Nip05Error::InvalidFormat(format!(
            "invalid domain: {}",
            domain
        )));
    }

    Ok((name, domain))
}

/// Build the well-known URL for NIP-05 verification.
pub fn nip05_url(name: &str, domain: &str) -> String {
    format!(
        "https://{}/.well-known/nostr.json?name={}",
        domain, name
    )
}

/// Decode a 64-character hex string into a 32-byte PubKey.
fn hex_to_pubkey(hex: &str) -> Result<PubKey, Nip05Error> {
    if hex.len() != 64 {
        return Err(Nip05Error::InvalidPubkey(format!(
            "expected 64 hex chars, got {}",
            hex.len()
        )));
    }

    let mut bytes = [0u8; 32];
    for (i, chunk) in hex.as_bytes().chunks(2).enumerate() {
        let s = std::str::from_utf8(chunk)
            .map_err(|_| Nip05Error::InvalidPubkey("invalid utf8".to_string()))?;
        bytes[i] = u8::from_str_radix(s, 16)
            .map_err(|_| Nip05Error::InvalidPubkey(format!("invalid hex at position {}", i * 2)))?;
    }

    Ok(bytes)
}

/// Verify a NIP-05 identity by fetching the well-known endpoint.
///
/// This performs an HTTP GET to `https://{domain}/.well-known/nostr.json?name={name}`
/// and checks that the returned pubkey matches the expected one.
///
/// Requires an HTTP client to be provided (to avoid tying to a specific HTTP library).
/// The `fetch_fn` takes a URL and returns the response body as a string.
pub async fn verify_nip05<F, Fut>(
    address: &str,
    expected_pubkey: &PubKey,
    fetch_fn: F,
) -> Result<Nip05Identity, Nip05Error>
where
    F: FnOnce(String) -> Fut,
    Fut: std::future::Future<Output = Result<String, String>>,
{
    let (name, domain) = parse_nip05(address)?;
    let url = nip05_url(&name, &domain);

    debug!(url = %url, "Verifying NIP-05 identity");

    let body = fetch_fn(url).await.map_err(Nip05Error::Http)?;

    let response: Nip05Response =
        serde_json::from_str(&body).map_err(|e| Nip05Error::InvalidResponse(e.to_string()))?;

    let hex_pubkey = response
        .names
        .get(&name)
        .ok_or_else(|| Nip05Error::NameNotFound {
            name: name.clone(),
            domain: domain.clone(),
        })?;

    let resolved_pubkey = hex_to_pubkey(hex_pubkey)?;

    if &resolved_pubkey != expected_pubkey {
        let expected_hex: String = expected_pubkey.iter().map(|b| format!("{:02x}", b)).collect();
        return Err(Nip05Error::PubkeyMismatch {
            expected: expected_hex,
            actual: hex_pubkey.clone(),
        });
    }

    debug!(
        address = %address,
        "NIP-05 identity verified"
    );

    Ok(Nip05Identity {
        address: address.to_string(),
        name,
        domain,
        pubkey: resolved_pubkey,
    })
}

/// Format a PubKey as hex string.
pub fn pubkey_to_hex(pubkey: &PubKey) -> String {
    pubkey.iter().map(|b| format!("{:02x}", b)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_nip05_valid() {
        let (name, domain) = parse_nip05("alice@example.com").unwrap();
        assert_eq!(name, "alice");
        assert_eq!(domain, "example.com");
    }

    #[test]
    fn test_parse_nip05_underscore() {
        let (name, domain) = parse_nip05("_@relay.damus.io").unwrap();
        assert_eq!(name, "_");
        assert_eq!(domain, "relay.damus.io");
    }

    #[test]
    fn test_parse_nip05_invalid() {
        assert!(parse_nip05("alice").is_err());
        assert!(parse_nip05("@example.com").is_err());
        assert!(parse_nip05("alice@").is_err());
        assert!(parse_nip05("alice@com").is_err());
    }

    #[test]
    fn test_nip05_url() {
        assert_eq!(
            nip05_url("alice", "example.com"),
            "https://example.com/.well-known/nostr.json?name=alice"
        );
    }

    #[test]
    fn test_hex_to_pubkey() {
        let hex = "0000000000000000000000000000000000000000000000000000000000000001";
        let key = hex_to_pubkey(hex).unwrap();
        assert_eq!(key[31], 1);
        assert_eq!(key[0], 0);
    }

    #[test]
    fn test_hex_to_pubkey_invalid() {
        assert!(hex_to_pubkey("short").is_err());
        assert!(hex_to_pubkey("gg00000000000000000000000000000000000000000000000000000000000000").is_err());
    }

    #[tokio::test]
    async fn test_verify_nip05_success() {
        let expected_key = [0u8; 32];
        let hex_key = "0000000000000000000000000000000000000000000000000000000000000000";
        let mock_response = format!(r#"{{"names":{{"alice":"{}"}}}}"#, hex_key);

        let result = verify_nip05("alice@example.com", &expected_key, |_url| async {
            Ok(mock_response)
        })
        .await;

        assert!(result.is_ok());
        let identity = result.unwrap();
        assert_eq!(identity.name, "alice");
        assert_eq!(identity.domain, "example.com");
    }

    #[tokio::test]
    async fn test_verify_nip05_mismatch() {
        let expected_key = [1u8; 32];
        let wrong_hex = "0000000000000000000000000000000000000000000000000000000000000000";
        let mock_response = format!(r#"{{"names":{{"alice":"{}"}}}}"#, wrong_hex);

        let result = verify_nip05("alice@example.com", &expected_key, |_url| async {
            Ok(mock_response)
        })
        .await;

        assert!(matches!(result, Err(Nip05Error::PubkeyMismatch { .. })));
    }
}
