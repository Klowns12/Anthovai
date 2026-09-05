//! Dashboard sessions.
//!
//! The cookie holds a random token; the database holds only its SHA-256. A
//! leaked database backup therefore does not hand anyone a working session, and
//! the token has enough entropy that no salt or slow hash is needed.

use anthovai_core::UserId;
use chrono::{DateTime, Duration, Utc};
use rand::Rng;
use sha2::{Digest, Sha256};

/// The cookie name. `__Host-` forces HTTPS, no `Domain` attribute and a `/`
/// path, so a subdomain cannot set or read it.
pub const COOKIE_NAME: &str = "__Host-av_session";

const TOKEN_BYTES: usize = 32;

#[derive(Clone, Debug)]
pub struct NewSession {
    /// Goes in the cookie. Never stored.
    pub token: String,
    /// Goes in the database.
    pub token_hash: String,
    pub user_id: UserId,
    pub expires_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub struct Session {
    pub token_hash: String,
    pub user_id: UserId,
    pub expires_at: DateTime<Utc>,
}

impl Session {
    pub fn is_expired(&self, now: DateTime<Utc>) -> bool {
        self.expires_at <= now
    }

    /// Sessions slide forward while in use, but only once the halfway point has
    /// passed. Writing to the session table on every request would cost a write
    /// per page view for no security benefit.
    pub fn should_extend(&self, now: DateTime<Utc>, ttl: Duration) -> bool {
        let remaining = self.expires_at - now;
        remaining < ttl / 2
    }
}

pub fn issue(user_id: UserId, ttl: Duration, now: DateTime<Utc>) -> NewSession {
    let mut bytes = [0_u8; TOKEN_BYTES];
    rand::rng().fill(&mut bytes);
    let token = hex::encode(bytes);

    NewSession {
        token_hash: hash_token(&token),
        token,
        user_id,
        expires_at: now + ttl,
    }
}

pub fn hash_token(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

/// The `Set-Cookie` value for a fresh session.
pub fn cookie_for(token: &str, ttl: Duration) -> String {
    format!(
        "{COOKIE_NAME}={token}; Path=/; HttpOnly; Secure; SameSite=Lax; Max-Age={}",
        ttl.num_seconds()
    )
}

/// The `Set-Cookie` value that clears the session on sign-out.
pub fn clearing_cookie() -> String {
    format!("{COOKIE_NAME}=; Path=/; HttpOnly; Secure; SameSite=Lax; Max-Age=0")
}

/// Pull the session token out of a `Cookie` header.
pub fn token_from_cookie_header(header: &str) -> Option<&str> {
    header.split(';').find_map(|pair| {
        let (name, value) = pair.split_once('=')?;
        (name.trim() == COOKIE_NAME).then(|| value.trim())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-09-04T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    #[test]
    fn a_token_is_long_and_unpredictable() {
        let first = issue(UserId::new(), Duration::hours(1), now());
        let second = issue(UserId::new(), Duration::hours(1), now());
        assert_eq!(first.token.len(), TOKEN_BYTES * 2);
        assert_ne!(first.token, second.token);
    }

    #[test]
    fn the_database_never_sees_the_token() {
        let session = issue(UserId::new(), Duration::hours(1), now());
        assert_ne!(session.token, session.token_hash);
        assert_eq!(session.token_hash, hash_token(&session.token));
        assert!(!session.token_hash.contains(&session.token));
    }

    #[test]
    fn expiry_is_enforced_on_the_boundary() {
        let session = Session {
            token_hash: "h".into(),
            user_id: UserId::new(),
            expires_at: now(),
        };
        assert!(session.is_expired(now()));
        assert!(!session.is_expired(now() - Duration::seconds(1)));
    }

    #[test]
    fn a_fresh_session_is_not_extended_on_every_request() {
        let ttl = Duration::hours(168);
        let session = Session {
            token_hash: "h".into(),
            user_id: UserId::new(),
            expires_at: now() + ttl,
        };
        assert!(!session.should_extend(now(), ttl));
    }

    #[test]
    fn a_session_past_halfway_is_extended() {
        let ttl = Duration::hours(168);
        let session = Session {
            token_hash: "h".into(),
            user_id: UserId::new(),
            expires_at: now() + Duration::hours(60),
        };
        assert!(session.should_extend(now(), ttl));
    }

    #[test]
    fn the_cookie_is_locked_down() {
        let cookie = cookie_for("abc123", Duration::hours(2));
        assert!(cookie.starts_with("__Host-av_session=abc123"));
        for attribute in ["HttpOnly", "Secure", "SameSite=Lax", "Path=/"] {
            assert!(cookie.contains(attribute), "missing {attribute}: {cookie}");
        }
        assert!(cookie.contains("Max-Age=7200"));
    }

    #[test]
    fn signing_out_clears_the_cookie() {
        assert!(clearing_cookie().contains("Max-Age=0"));
    }

    #[test]
    fn finds_the_token_among_other_cookies() {
        let header = "theme=dark; __Host-av_session=tok123; locale=th";
        assert_eq!(token_from_cookie_header(header), Some("tok123"));
    }

    #[test]
    fn ignores_cookies_that_merely_look_similar() {
        assert_eq!(token_from_cookie_header("av_session=tok123"), None);
        assert_eq!(token_from_cookie_header("other=1"), None);
        assert_eq!(token_from_cookie_header(""), None);
    }
}
