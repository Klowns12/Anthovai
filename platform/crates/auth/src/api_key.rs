//! API key generation and verification.
//!
//! Format: `av_{live|test}_{43 base62 chars}` — roughly 190 bits of entropy from
//! the OS random source. The database stores `sha256(full_key)`; the plaintext
//! is shown to the customer exactly once. Entropy this high needs no salt, and
//! a single hash keeps verification to one indexed lookup per request.

use anthovai_core::{DomainError, Result};
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
const SECRET_LEN: usize = 43;
/// How much of the key is stored in the clear for display in the dashboard.
const DISPLAY_PREFIX_LEN: usize = 12;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Environment {
    Live,
    Test,
}

impl Environment {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Live => "live",
            Self::Test => "test",
        }
    }
}

impl std::str::FromStr for Environment {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "live" => Ok(Self::Live),
            "test" => Ok(Self::Test),
            other => Err(DomainError::validation(format!(
                "unknown key environment `{other}`"
            ))),
        }
    }
}

/// A freshly minted key. The plaintext lives only as long as this value.
#[derive(Clone, Debug)]
pub struct GeneratedApiKey {
    pub plaintext: String,
    pub hash: String,
    pub prefix: String,
    pub environment: Environment,
}

pub fn generate(environment: Environment) -> GeneratedApiKey {
    let mut rng = rand::rng();
    let secret: String = (0..SECRET_LEN)
        .map(|_| ALPHABET[rng.random_range(0..ALPHABET.len())] as char)
        .collect();
    let plaintext = format!("av_{}_{}", environment.as_str(), secret);
    let hash = hash_key(&plaintext);
    let prefix = plaintext.chars().take(DISPLAY_PREFIX_LEN).collect();

    GeneratedApiKey {
        plaintext,
        hash,
        prefix,
        environment,
    }
}

pub fn hash_key(plaintext: &str) -> String {
    let digest = Sha256::digest(plaintext.as_bytes());
    hex::encode(digest)
}

/// Pull the key out of an `Authorization` header. Anything other than a bearer
/// token is refused; keys in query strings are refused elsewhere, by the router.
pub fn from_authorization_header(header: &str) -> Result<&str> {
    let token = header
        .strip_prefix("Bearer ")
        .or_else(|| header.strip_prefix("bearer "))
        .ok_or(DomainError::Unauthenticated("missing_bearer_token"))?
        .trim();

    if token.is_empty() {
        return Err(DomainError::Unauthenticated("missing_bearer_token"));
    }
    parse_environment(token)?;
    Ok(token)
}

/// Validate the shape of a key without touching the database, so an obviously
/// malformed key costs no query.
pub fn parse_environment(key: &str) -> Result<Environment> {
    let rest = key
        .strip_prefix("av_")
        .ok_or(DomainError::Unauthenticated("invalid_api_key"))?;
    let (env, secret) = rest
        .split_once('_')
        .ok_or(DomainError::Unauthenticated("invalid_api_key"))?;

    let environment: Environment = env
        .parse()
        .map_err(|_| DomainError::Unauthenticated("invalid_api_key"))?;

    if secret.len() != SECRET_LEN || !secret.bytes().all(|b| ALPHABET.contains(&b)) {
        return Err(DomainError::Unauthenticated("invalid_api_key"));
    }
    Ok(environment)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_keys_have_the_documented_shape() {
        let key = generate(Environment::Live);
        assert!(key.plaintext.starts_with("av_live_"));
        assert_eq!(key.plaintext.len(), "av_live_".len() + SECRET_LEN);
        assert_eq!(key.prefix.len(), DISPLAY_PREFIX_LEN);
        assert!(key.plaintext.starts_with(&key.prefix));
    }

    #[test]
    fn two_keys_are_never_the_same() {
        let a = generate(Environment::Live);
        let b = generate(Environment::Live);
        assert_ne!(a.plaintext, b.plaintext);
        assert_ne!(a.hash, b.hash);
    }

    #[test]
    fn the_hash_is_deterministic_and_hides_the_key() {
        let key = generate(Environment::Test);
        assert_eq!(hash_key(&key.plaintext), key.hash);
        assert_eq!(key.hash.len(), 64);
        assert!(!key.hash.contains(&key.plaintext));
    }

    #[test]
    fn accepts_a_bearer_header() {
        let key = generate(Environment::Live);
        let header = format!("Bearer {}", key.plaintext);
        assert_eq!(from_authorization_header(&header).unwrap(), key.plaintext);
    }

    #[test]
    fn rejects_headers_that_are_not_bearer_tokens() {
        let key = generate(Environment::Live);
        assert!(from_authorization_header(&key.plaintext).is_err());
        assert!(from_authorization_header("Basic abc123").is_err());
        assert!(from_authorization_header("Bearer   ").is_err());
    }

    #[test]
    fn rejects_malformed_keys_without_a_lookup() {
        assert!(parse_environment("sk-openai-style-key").is_err());
        assert!(parse_environment("av_live_tooshort").is_err());
        assert!(parse_environment("av_prod_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").is_err());
        assert!(parse_environment("av_live_!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!").is_err());
    }

    #[test]
    fn recognises_both_environments() {
        assert_eq!(
            parse_environment(&generate(Environment::Live).plaintext).unwrap(),
            Environment::Live
        );
        assert_eq!(
            parse_environment(&generate(Environment::Test).plaintext).unwrap(),
            Environment::Test
        );
    }
}
