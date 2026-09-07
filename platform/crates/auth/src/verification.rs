//! Email-verification tokens.
//!
//! The same shape as a session token and for the same reasons: 32 random bytes
//! shown once, only the SHA-256 stored, so the table is worthless to anyone who
//! reads it.
//!
//! The lifetime is short by comparison. A session is a convenience and is
//! extended as it is used; this is a single-use proof that somebody can read a
//! mailbox, and a link that still works a month later is a link that has been
//! sitting in an inbox a month long enough to be forwarded, archived, or read
//! by whoever inherited the address.

use chrono::{DateTime, Duration, Utc};
use rand::Rng;
use sha2::{Digest, Sha256};

use anthovai_core::UserId;

const TOKEN_BYTES: usize = 32;

/// How long a verification link is good for.
pub const TTL_HOURS: i64 = 24;

pub struct NewVerification {
    /// Shown once, in the email. Never stored.
    pub token: String,
    pub token_hash: String,
    pub user_id: UserId,
    pub email: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub struct Verification {
    pub user_id: UserId,
    pub email: String,
    pub expires_at: DateTime<Utc>,
    pub consumed_at: Option<DateTime<Utc>>,
}

impl Verification {
    pub fn is_expired(&self, now: DateTime<Utc>) -> bool {
        self.expires_at <= now
    }

    pub fn is_consumed(&self) -> bool {
        self.consumed_at.is_some()
    }
}

pub fn issue(user_id: UserId, email: &str, now: DateTime<Utc>) -> NewVerification {
    let mut bytes = [0_u8; TOKEN_BYTES];
    rand::rng().fill(&mut bytes);
    let token = hex::encode(bytes);

    NewVerification {
        token_hash: hash_token(&token),
        token,
        user_id,
        email: email.to_owned(),
        expires_at: now + Duration::hours(TTL_HOURS),
    }
}

pub fn hash_token(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user() -> UserId {
        UserId::new()
    }

    #[test]
    fn the_token_is_never_the_thing_that_is_stored() {
        let issued = issue(user(), "somchai@example.com", Utc::now());
        assert_ne!(issued.token, issued.token_hash);
        assert_eq!(issued.token_hash, hash_token(&issued.token));
    }

    #[test]
    fn two_tokens_are_never_the_same() {
        let a = issue(user(), "a@example.com", Utc::now());
        let b = issue(user(), "a@example.com", Utc::now());
        assert_ne!(a.token, b.token, "a predictable token is not a proof of anything");
    }

    #[test]
    fn a_token_is_long_enough_to_be_worth_guessing_at() {
        let issued = issue(user(), "a@example.com", Utc::now());
        // Hex of 32 bytes. Stated as a number rather than a range because a
        // shorter token is the kind of change that looks harmless in a diff.
        assert_eq!(issued.token.len(), 64);
    }

    #[test]
    fn expiry_is_a_day_and_the_boundary_belongs_to_the_past() {
        let now = Utc::now();
        let issued = issue(user(), "a@example.com", now);
        let v = Verification {
            user_id: issued.user_id,
            email: issued.email.clone(),
            expires_at: issued.expires_at,
            consumed_at: None,
        };

        assert!(!v.is_expired(now));
        assert!(!v.is_expired(now + Duration::hours(TTL_HOURS) - Duration::seconds(1)));
        // Exactly at the expiry the link is dead: `<=`, so there is no second
        // in which a token is both expired and usable.
        assert!(v.is_expired(now + Duration::hours(TTL_HOURS)));
    }
}
