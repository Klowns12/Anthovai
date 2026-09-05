//! Password hashing.
//!
//! Argon2id with the parameters from `config/default.toml`. The salt and the
//! parameters travel inside the PHC string stored in `users.password_hash`, so
//! raising the cost later verifies old hashes fine and re-hashes on next login.

use anthovai_core::{DomainError, Result};
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::{Algorithm, Argon2, Params, Version};

/// Long enough to matter, short enough that people still use a passphrase.
const MIN_LENGTH: usize = 10;
/// bcrypt's 72-byte truncation does not apply to Argon2, but an unbounded
/// password is an easy way to make the server do expensive work.
const MAX_LENGTH: usize = 256;

#[derive(Clone, Copy, Debug)]
pub struct PasswordHasherConfig {
    pub memory_kib: u32,
    pub iterations: u32,
    pub parallelism: u32,
}

impl Default for PasswordHasherConfig {
    fn default() -> Self {
        Self {
            memory_kib: 65_536,
            iterations: 3,
            parallelism: 1,
        }
    }
}

impl PasswordHasherConfig {
    /// Cheap parameters for tests. Hashing at production cost turns a fast test
    /// suite into a slow one for no extra confidence.
    pub fn fast_for_tests() -> Self {
        Self {
            memory_kib: 8,
            iterations: 1,
            parallelism: 1,
        }
    }

    fn argon2(&self) -> Result<Argon2<'static>> {
        let params = Params::new(self.memory_kib, self.iterations, self.parallelism, None)
            .map_err(|e| DomainError::Internal(anyhow::anyhow!("bad argon2 parameters: {e}")))?;
        Ok(Argon2::new(Algorithm::Argon2id, Version::V0x13, params))
    }
}

/// Reject passwords that are too short or absurdly long, before hashing.
pub fn validate(password: &str) -> Result<()> {
    let length = password.chars().count();
    if length < MIN_LENGTH {
        return Err(DomainError::validation(format!(
            "password must be at least {MIN_LENGTH} characters"
        )));
    }
    if length > MAX_LENGTH {
        return Err(DomainError::validation(format!(
            "password must be at most {MAX_LENGTH} characters"
        )));
    }
    Ok(())
}

pub fn hash(password: &str, config: PasswordHasherConfig) -> Result<String> {
    validate(password)?;
    let salt = SaltString::generate(&mut argon2::password_hash::rand_core::OsRng);
    config
        .argon2()?
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| DomainError::Internal(anyhow::anyhow!("could not hash password: {e}")))
}

/// Verify a password. A malformed stored hash is a failed verification, not an
/// error: it must not be distinguishable from a wrong password.
pub fn verify(password: &str, stored_hash: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(stored_hash) else {
        return false;
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> PasswordHasherConfig {
        PasswordHasherConfig::fast_for_tests()
    }

    #[test]
    fn a_hashed_password_verifies() {
        let stored = hash("correct horse battery", config()).unwrap();
        assert!(verify("correct horse battery", &stored));
    }

    #[test]
    fn a_wrong_password_does_not_verify() {
        let stored = hash("correct horse battery", config()).unwrap();
        assert!(!verify("correct horse batter", &stored));
        assert!(!verify("", &stored));
    }

    #[test]
    fn the_same_password_hashes_differently_each_time() {
        let first = hash("correct horse battery", config()).unwrap();
        let second = hash("correct horse battery", config()).unwrap();
        assert_ne!(first, second, "each hash must carry its own salt");
        assert!(verify("correct horse battery", &first));
        assert!(verify("correct horse battery", &second));
    }

    #[test]
    fn the_stored_hash_does_not_contain_the_password() {
        let stored = hash("hunter2-hunter2", config()).unwrap();
        assert!(!stored.contains("hunter2"));
        assert!(stored.starts_with("$argon2id$"));
    }

    #[test]
    fn short_passwords_are_refused() {
        assert!(validate("short").is_err());
        assert!(hash("short", config()).is_err());
        assert!(validate("just-long-enough").is_ok());
    }

    #[test]
    fn absurdly_long_passwords_are_refused() {
        let long = "a".repeat(MAX_LENGTH + 1);
        assert!(validate(&long).is_err());
    }

    #[test]
    fn length_is_counted_in_characters() {
        // Ten Thai characters is a ten-character password, not a thirty-byte one.
        assert!(validate("รหัสผ่านของฉัน").is_ok());
    }

    #[test]
    fn a_corrupt_stored_hash_fails_closed() {
        assert!(!verify("anything at all", "not-a-phc-string"));
        assert!(!verify("anything at all", ""));
    }

    #[test]
    fn a_hash_made_with_higher_cost_still_verifies() {
        // Raising cost must not lock existing users out: the parameters live
        // in the stored string, so verification reads them from there.
        let expensive = PasswordHasherConfig {
            memory_kib: 1024,
            iterations: 2,
            parallelism: 1,
        };
        let stored = hash("correct horse battery", expensive).unwrap();
        assert!(verify("correct horse battery", &stored));
    }
}
