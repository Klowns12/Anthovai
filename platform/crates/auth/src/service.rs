//! Authentication services: sign-up, sign-in, session verification, and turning
//! an API key into a [`TenantCtx`].
//!
//! This crate is the only place a `TenantCtx` is built. Everything downstream
//! trusts it and never re-derives a tenant from a request.

use anthovai_core::{
    Actor, AgentScope, ApiKeyId, Clock, DomainError, OrgId, Permission, Plan, RequestId, Result,
    Role, Scope, TenantCtx, UserId, WorkspaceId,
};
use anthovai_db::Db;
use chrono::Duration;

use crate::api_key::{self, Environment, GeneratedApiKey};
use crate::cache::ApiKeyCache;
use crate::password::{self, PasswordHasherConfig};
use crate::repo::{self, ApiKeySummary, NewApiKey};
use crate::session::{self, NewSession};
use crate::verification::{self, NewVerification};
use crate::User;

#[derive(Clone, Copy, Debug)]
pub struct AuthConfig {
    pub session_ttl_hours: i64,
    pub api_key_cache_secs: u64,
    pub password: PasswordHasherConfig,
    /// How long a rotated key keeps working, so a deployment can roll over
    /// without a window where neither key is valid.
    pub rotation_grace_hours: i64,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            session_ttl_hours: 168,
            api_key_cache_secs: 60,
            password: PasswordHasherConfig::default(),
            rotation_grace_hours: 24,
        }
    }
}

pub struct AuthService {
    db: Db,
    clock: Clock,
    config: AuthConfig,
    key_cache: ApiKeyCache,
}

/// A key, the once-only secret, and where it lives.
#[derive(Clone, Debug)]
pub struct IssuedApiKey {
    pub id: ApiKeyId,
    pub name: String,
    pub prefix: String,
    pub environment: Environment,
    /// Shown to the customer exactly once. Never stored, never logged.
    pub secret: String,
}

#[derive(Clone, Debug)]
pub struct CreateApiKey {
    pub workspace_id: WorkspaceId,
    pub name: String,
    pub environment: Environment,
    pub scopes: Vec<Scope>,
    pub agents: AgentScope,
    pub expires_in_days: Option<i64>,
}

impl AuthService {
    pub fn new(db: Db, clock: Clock, config: AuthConfig) -> Self {
        let key_cache = ApiKeyCache::new(clock.clone(), config.api_key_cache_secs);
        Self {
            db,
            clock,
            config,
            key_cache,
        }
    }

    // ---- users and sessions -----------------------------------------------

    pub async fn sign_up(
        &self,
        email: &str,
        raw_password: &str,
        name: Option<&str>,
    ) -> Result<UserId> {
        let email = normalise_email(email)?;
        let hash = password::hash(raw_password, self.config.password)?;
        let user_id = UserId::new();

        let mut db = self.db.system().await?;
        repo::insert_user(&mut db, user_id, &email, Some(&hash), name).await?;
        db.commit().await?;

        Ok(user_id)
    }

    /// Sign in. Every failure returns the same error, whether the account does
    /// not exist, has no password set, or the password is wrong: a caller must
    /// not be able to enumerate registered addresses.
    pub async fn sign_in(
        &self,
        email: &str,
        raw_password: &str,
        ip: Option<&str>,
        user_agent: Option<&str>,
    ) -> Result<NewSession> {
        let email = normalise_email(email)?;

        let mut db = self.db.system().await?;
        let user = repo::find_user_by_email(&mut db, &email).await?;

        let user = match user {
            Some(user) => user,
            None => {
                // Hash anyway, so a missing account is not measurably faster
                // than a wrong password.
                let _ = password::hash(raw_password, self.config.password);
                return Err(DomainError::Unauthenticated("invalid_credentials"));
            }
        };

        let stored = user
            .password_hash
            .as_deref()
            .ok_or(DomainError::Unauthenticated("invalid_credentials"))?;
        if !password::verify(raw_password, stored) {
            return Err(DomainError::Unauthenticated("invalid_credentials"));
        }

        let ttl = Duration::hours(self.config.session_ttl_hours);
        let new_session = session::issue(user.id, ttl, self.clock.now());
        repo::insert_session(&mut db, &new_session, ip, user_agent).await?;
        db.commit().await?;

        Ok(new_session)
    }

    /// Verify a session cookie, sliding its expiry when it is past halfway.
    pub async fn verify_session(&self, token: &str) -> Result<User> {
        let token_hash = session::hash_token(token);
        let now = self.clock.now();
        let ttl = Duration::hours(self.config.session_ttl_hours);

        let mut db = self.db.system().await?;
        let stored = repo::find_session(&mut db, &token_hash)
            .await?
            .ok_or(DomainError::Unauthenticated("session_expired"))?;

        if stored.is_expired(now) {
            repo::delete_session(&mut db, &token_hash).await?;
            db.commit().await?;
            return Err(DomainError::Unauthenticated("session_expired"));
        }

        if stored.should_extend(now, ttl) {
            repo::extend_session(&mut db, &token_hash, now + ttl).await?;
        }

        let user = repo::find_user(&mut db, stored.user_id)
            .await?
            .ok_or(DomainError::Unauthenticated("session_expired"))?;
        db.commit().await?;

        Ok(user)
    }

    /// Record that the address behind an account has been proved.
    ///
    /// The magic-link endpoint calls this when a token comes back (P3). Until
    /// that exists it is also how a deployment gets its first live API key, so
    /// it stays a first-class service method rather than a test-only hook.
    /// Look a user up by id. The dashboard reads the session's user from the
    /// session; this is for the paths that hold an id and nothing else.
    pub async fn find_user(&self, user_id: UserId) -> Result<Option<User>> {
        let mut db = self.db.system().await?;
        let user = repo::find_user(&mut db, user_id).await?;
        db.commit().await?;
        Ok(user)
    }

    pub async fn mark_email_verified(&self, user_id: UserId) -> Result<()> {
        let mut db = self.db.system().await?;
        repo::mark_email_verified(&mut db, user_id).await?;
        db.commit().await
    }

    /// Issue a fresh verification token for a user, voiding any outstanding
    /// ones.
    ///
    /// Returns the token to be put in a link. It is never stored and never
    /// returned again, so a caller that drops it has to ask for another.
    ///
    /// An already-verified account is not an error and does not get a token —
    /// there is nothing left to prove, and issuing one anyway would email a
    /// working link to an address on the strength of a request that anybody
    /// holding a session could repeat.
    pub async fn request_email_verification(
        &self,
        user_id: UserId,
    ) -> Result<Option<NewVerification>> {
        let now = self.clock.now();
        let mut db = self.db.system().await?;

        let user = repo::find_user(&mut db, user_id)
            .await?
            .ok_or(DomainError::NotFound("user"))?;

        if user.email_verified_at.is_some() {
            db.commit().await?;
            return Ok(None);
        }

        repo::invalidate_verifications(&mut db, user_id, now).await?;
        let issued = verification::issue(user_id, &user.email, now);
        repo::insert_verification(&mut db, &issued).await?;
        db.commit().await?;

        Ok(Some(issued))
    }

    /// Accept a token and mark the address proved.
    ///
    /// Every failure returns the same error. A caller learns whether the token
    /// worked and nothing else: distinguishing "expired" from "never existed"
    /// would let somebody test guesses against the table and learn which ones
    /// were once real.
    ///
    /// The exception is a token that this same request already consumed a
    /// moment ago, which is reported as success — mail clients and security
    /// scanners follow links before a person does, and a customer whose link
    /// "stopped working" because their own mail provider opened it first is
    /// being punished for something they cannot see.
    pub async fn verify_email(&self, token: &str) -> Result<UserId> {
        let now = self.clock.now();
        let hash = verification::hash_token(token);
        let mut db = self.db.system().await?;

        let Some(found) = repo::find_verification(&mut db, &hash).await? else {
            return Err(DomainError::rejected(
                "verification_token_invalid",
                "That confirmation link is not valid. It may have expired or already been used.",
            ));
        };

        if found.is_expired(now) {
            return Err(DomainError::rejected(
                "verification_token_invalid",
                "That confirmation link is not valid. It may have expired or already been used.",
            ));
        }

        // The address may have changed since the token was issued, in which
        // case it proves an address the account no longer has.
        let user = repo::find_user(&mut db, found.user_id)
            .await?
            .ok_or_else(|| {
                DomainError::rejected(
                "verification_token_invalid",
                "That confirmation link is not valid. It may have expired or already been used.",
            )
            })?;

        if !user.email.eq_ignore_ascii_case(&found.email) {
            return Err(DomainError::rejected(
                "verification_token_invalid",
                "That confirmation link is not valid. It may have expired or already been used.",
            ));
        }

        if user.email_verified_at.is_some() {
            // Already done, by this token or another. Nothing to change.
            db.commit().await?;
            return Ok(found.user_id);
        }

        if !repo::consume_verification(&mut db, &hash, now).await? {
            // Consumed by a concurrent request. That request is doing the same
            // work, so this one has nothing to add and nothing to complain of.
            db.commit().await?;
            return Ok(found.user_id);
        }

        repo::mark_email_verified(&mut db, found.user_id).await?;
        db.commit().await?;

        Ok(found.user_id)
    }

    /// Remove verification tokens that expired. Housekeeping, like sessions.
    pub async fn purge_expired_verifications(&self) -> Result<u64> {
        let mut db = self.db.system().await?;
        let removed = repo::purge_expired_verifications(&mut db, self.clock.now()).await?;
        db.commit().await?;
        Ok(removed)
    }

    pub async fn sign_out(&self, token: &str) -> Result<()> {
        let mut db = self.db.system().await?;
        repo::delete_session(&mut db, &session::hash_token(token)).await?;
        db.commit().await
    }

    /// Build the context for a dashboard request. The role comes from the
    /// membership, so a user who is not a member of the organization they named
    /// gets `NotFound` — not a hint that it exists.
    pub fn dashboard_context(
        &self,
        user_id: UserId,
        org_id: OrgId,
        role: Role,
        plan: Plan,
    ) -> TenantCtx {
        TenantCtx {
            org_id,
            workspace_id: None,
            actor: Actor::User { user_id, role },
            plan,
            request_id: RequestId::new(),
        }
    }

    // ---- API keys ---------------------------------------------------------

    /// Mint a key. The secret is returned once and never again; only its hash
    /// is stored, so not even we can recover it afterwards.
    pub async fn create_api_key(
        &self,
        ctx: &TenantCtx,
        request: CreateApiKey,
    ) -> Result<IssuedApiKey> {
        ctx.require(Permission::ApiKeyManage)?;
        if request.name.trim().is_empty() {
            return Err(DomainError::validation("api key name is required"));
        }
        if request.scopes.is_empty() {
            return Err(DomainError::validation(
                "an api key needs at least one scope",
            ));
        }

        let generated: GeneratedApiKey = api_key::generate(request.environment);
        let expires_at = request
            .expires_in_days
            .map(|days| self.clock.now() + Duration::days(days));

        let record = NewApiKey {
            id: ApiKeyId::new(),
            workspace_id: request.workspace_id,
            name: request.name.trim().to_owned(),
            key_hash: generated.hash.clone(),
            prefix: generated.prefix.clone(),
            environment: request.environment,
            scopes: request.scopes,
            agents: request.agents,
            expires_at,
            created_by: ctx.user_id(),
            rotated_from: None,
        };

        let mut db = self.db.tenant(ctx).await?;
        repo::insert_api_key(&mut db, &record).await?;
        db.commit().await?;

        Ok(IssuedApiKey {
            id: record.id,
            name: record.name,
            prefix: generated.prefix,
            environment: generated.environment,
            secret: generated.plaintext,
        })
    }

    pub async fn list_api_keys(
        &self,
        ctx: &TenantCtx,
        workspace_id: Option<WorkspaceId>,
    ) -> Result<Vec<ApiKeySummary>> {
        ctx.require(Permission::ApiKeyManage)?;
        let mut db = self.db.tenant(ctx).await?;
        let keys = repo::list_api_keys(&mut db, workspace_id, self.clock.now()).await?;
        db.commit().await?;
        Ok(keys)
    }

    pub async fn revoke_api_key(&self, ctx: &TenantCtx, key_id: ApiKeyId) -> Result<()> {
        ctx.require(Permission::ApiKeyManage)?;
        let mut db = self.db.tenant(ctx).await?;
        let key_hash = repo::revoke_api_key(&mut db, key_id).await?;
        db.commit().await?;

        // Evict before returning, so the key stops working on this instance the
        // moment the dashboard says it has.
        self.key_cache.evict(&key_hash);
        Ok(())
    }

    /// Issue a replacement and put the old key on a deadline rather than
    /// killing it outright, so a running deployment can be updated first.
    pub async fn rotate_api_key(
        &self,
        ctx: &TenantCtx,
        key_id: ApiKeyId,
        request: CreateApiKey,
    ) -> Result<IssuedApiKey> {
        ctx.require(Permission::ApiKeyManage)?;

        let issued = self.create_api_key(ctx, request).await?;

        let grace_until = self.clock.now() + Duration::hours(self.config.rotation_grace_hours);
        let mut db = self.db.tenant(ctx).await?;
        let old_hash = repo::set_api_key_expiry(&mut db, key_id, grace_until).await?;
        db.commit().await?;

        self.key_cache.evict(&old_hash);
        Ok(issued)
    }

    /// Turn a bearer token into a tenant context. This is the front door of the
    /// public API, so it is deliberately strict: malformed keys are rejected
    /// before any query, and every failure is the same shape of 401.
    pub async fn authenticate_api_key(
        &self,
        bearer_header: &str,
        request_id: RequestId,
    ) -> Result<TenantCtx> {
        let key = api_key::from_authorization_header(bearer_header)?;
        let key_hash = api_key::hash_key(key);

        let record = match self.key_cache.get(&key_hash) {
            Some(cached) => cached,
            None => {
                let mut db = self.db.system().await?;
                let found = repo::find_api_key_by_hash(&mut db, &key_hash).await?;
                db.commit().await?;

                let record = found.ok_or(DomainError::Unauthenticated("invalid_api_key"))?;
                self.key_cache.put(&key_hash, record.clone());
                record
            }
        };

        // Checked on every request, cached or not, so a key that expires mid-TTL
        // stops working at its deadline rather than at the end of the cache entry.
        if let Err(err) = record.check_usable(self.clock.now()) {
            self.key_cache.evict(&key_hash);
            return Err(err);
        }

        let ctx = TenantCtx {
            org_id: record.org_id,
            workspace_id: Some(record.workspace_id),
            actor: Actor::ApiKey {
                key_id: record.id,
                scopes: record.scopes.clone(),
                agents: record.agents.clone(),
            },
            plan: record.plan,
            request_id,
        };

        // Best-effort: a failure to record usage must not fail the request.
        if let Ok(mut db) = self.db.tenant(&ctx).await {
            if repo::touch_api_key(&mut db, record.id).await.is_ok() {
                let _ = db.commit().await;
            }
        }

        Ok(ctx)
    }

    /// Exposed for tests and for the sign-out-everywhere path.
    pub fn cache(&self) -> &ApiKeyCache {
        &self.key_cache
    }
}

impl std::fmt::Debug for AuthService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthService")
            .field("cached_keys", &self.key_cache.len())
            .finish()
    }
}

fn normalise_email(email: &str) -> Result<String> {
    let trimmed = email.trim().to_lowercase();
    // Deliberately shallow: the address is proved by the verification mail, not
    // by a regular expression.
    let looks_like_an_address = trimmed
        .split_once('@')
        .is_some_and(|(local, domain)| !local.is_empty() && domain.contains('.'));

    if !looks_like_an_address || trimmed.len() > 254 {
        return Err(DomainError::validation("a valid email address is required"));
    }
    Ok(trimmed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emails_are_normalised() {
        assert_eq!(
            normalise_email("  Owner@ABC.ac.th ").unwrap(),
            "owner@abc.ac.th"
        );
    }

    #[test]
    fn obviously_invalid_addresses_are_refused() {
        for bad in ["", "no-at-sign", "@example.com", "user@localhost", "   "] {
            assert!(normalise_email(bad).is_err(), "should reject {bad:?}");
        }
    }

    #[test]
    fn absurdly_long_addresses_are_refused() {
        let long = format!("{}@example.com", "a".repeat(250));
        assert!(normalise_email(&long).is_err());
    }
}
