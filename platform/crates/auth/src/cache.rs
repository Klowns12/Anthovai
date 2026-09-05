//! A short-lived cache of resolved API keys.
//!
//! Every request on the public API needs the key's tenant, plan and scopes.
//! Without a cache that is a database round trip per request, on the hot path,
//! for data that changes only when someone edits a key in the dashboard.
//!
//! The entries are deliberately short-lived: a revoked key must stop working
//! quickly, and the eviction below makes it immediate on the instance that did
//! the revoking. Other instances catch up when their entry ages out, which is
//! why the TTL is a minute rather than an hour.

use std::collections::HashMap;
use std::sync::Mutex;

use anthovai_core::Clock;
use chrono::{DateTime, Duration, Utc};

use crate::ApiKeyRecord;

pub struct ApiKeyCache {
    clock: Clock,
    ttl: Duration,
    entries: Mutex<HashMap<String, Entry>>,
}

struct Entry {
    record: ApiKeyRecord,
    stored_at: DateTime<Utc>,
}

impl ApiKeyCache {
    pub fn new(clock: Clock, ttl_secs: u64) -> Self {
        Self {
            clock,
            ttl: Duration::seconds(ttl_secs as i64),
            entries: Mutex::new(HashMap::new()),
        }
    }

    /// The cached record for this key hash, if it has not aged out.
    pub fn get(&self, key_hash: &str) -> Option<ApiKeyRecord> {
        let now = self.clock.now();
        let mut entries = self.entries.lock().expect("api key cache mutex poisoned");

        match entries.get(key_hash) {
            Some(entry) if now - entry.stored_at < self.ttl => Some(entry.record.clone()),
            Some(_) => {
                entries.remove(key_hash);
                None
            }
            None => None,
        }
    }

    pub fn put(&self, key_hash: &str, record: ApiKeyRecord) {
        let stored_at = self.clock.now();
        self.entries
            .lock()
            .expect("api key cache mutex poisoned")
            .insert(key_hash.to_owned(), Entry { record, stored_at });
    }

    /// Called when a key is revoked, rotated or expired, so it stops working on
    /// this instance at once instead of at the end of its TTL.
    pub fn evict(&self, key_hash: &str) {
        self.entries
            .lock()
            .expect("api key cache mutex poisoned")
            .remove(key_hash);
    }

    pub fn clear(&self) {
        self.entries
            .lock()
            .expect("api key cache mutex poisoned")
            .clear();
    }

    pub fn len(&self) -> usize {
        self.entries
            .lock()
            .expect("api key cache mutex poisoned")
            .len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl std::fmt::Debug for ApiKeyCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ApiKeyCache")
            .field("entries", &self.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use anthovai_core::{AgentScope, ApiKeyId, OrgId, Plan, Scope, WorkspaceId};

    use super::*;
    use crate::{Environment, KeyStatus};

    fn record() -> ApiKeyRecord {
        ApiKeyRecord {
            id: ApiKeyId::new(),
            org_id: OrgId::new(),
            workspace_id: WorkspaceId::new(),
            environment: Environment::Live,
            scopes: vec![Scope::Chat],
            agents: AgentScope::All,
            plan: Plan::Free,
            status: KeyStatus::Active,
            expires_at: None,
        }
    }

    fn cache() -> (ApiKeyCache, anthovai_core::time::FixedClock) {
        let start = DateTime::parse_from_rfc3339("2026-09-04T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let (clock, handle) = Clock::fixed(start);
        (ApiKeyCache::new(clock, 60), handle)
    }

    #[test]
    fn a_miss_returns_nothing() {
        let (cache, _) = cache();
        assert!(cache.get("unknown").is_none());
    }

    #[test]
    fn a_stored_record_comes_back() {
        let (cache, _) = cache();
        let record = record();
        cache.put("hash", record.clone());

        let found = cache.get("hash").expect("the record was just stored");
        assert_eq!(found.id, record.id);
        assert_eq!(found.org_id, record.org_id);
    }

    #[test]
    fn an_entry_ages_out() {
        let (cache, clock) = cache();
        cache.put("hash", record());

        clock.advance(Duration::seconds(59));
        assert!(cache.get("hash").is_some());

        clock.advance(Duration::seconds(2));
        assert!(cache.get("hash").is_none());
    }

    #[test]
    fn an_aged_out_entry_is_not_left_behind() {
        let (cache, clock) = cache();
        cache.put("hash", record());
        clock.advance(Duration::seconds(61));

        cache.get("hash");
        assert!(cache.is_empty(), "reading a stale entry should drop it");
    }

    #[test]
    fn revoking_a_key_takes_effect_immediately() {
        let (cache, _) = cache();
        cache.put("hash", record());
        cache.evict("hash");
        assert!(cache.get("hash").is_none());
    }

    #[test]
    fn keys_do_not_collide() {
        let (cache, _) = cache();
        let first = record();
        let second = record();
        cache.put("a", first.clone());
        cache.put("b", second.clone());

        assert_eq!(cache.get("a").unwrap().id, first.id);
        assert_eq!(cache.get("b").unwrap().id, second.id);
    }
}
