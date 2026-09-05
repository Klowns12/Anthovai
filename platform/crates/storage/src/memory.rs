//! An in-memory object store for tests and local development.

use std::collections::HashMap;
use std::sync::Mutex;

use anthovai_core::{DomainError, Result};
use async_trait::async_trait;

use crate::ObjectStorage;

#[derive(Default)]
pub struct InMemoryStorage {
    objects: Mutex<HashMap<String, Vec<u8>>>,
}

impl InMemoryStorage {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.objects.lock().expect("storage mutex poisoned").len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[async_trait]
impl ObjectStorage for InMemoryStorage {
    async fn put(&self, key: &str, bytes: Vec<u8>, _content_type: &str) -> Result<()> {
        self.objects
            .lock()
            .expect("storage mutex poisoned")
            .insert(key.to_owned(), bytes);
        Ok(())
    }

    async fn get(&self, key: &str) -> Result<Vec<u8>> {
        self.objects
            .lock()
            .expect("storage mutex poisoned")
            .get(key)
            .cloned()
            .ok_or(DomainError::NotFound("object"))
    }

    async fn delete(&self, key: &str) -> Result<()> {
        self.objects
            .lock()
            .expect("storage mutex poisoned")
            .remove(key);
        Ok(())
    }

    async fn exists(&self, key: &str) -> Result<bool> {
        Ok(self
            .objects
            .lock()
            .expect("storage mutex poisoned")
            .contains_key(key))
    }

    async fn delete_prefix(&self, prefix: &str) -> Result<u64> {
        let mut objects = self.objects.lock().expect("storage mutex poisoned");
        let doomed: Vec<String> = objects
            .keys()
            .filter(|k| k.starts_with(prefix))
            .cloned()
            .collect();
        for key in &doomed {
            objects.remove(key);
        }
        Ok(doomed.len() as u64)
    }
}

#[cfg(test)]
mod tests {
    use anthovai_core::{DocumentId, KnowledgeBaseId, OrgId};

    use super::*;
    use crate::StorageKey;

    #[tokio::test]
    async fn stores_and_reads_back() {
        let store = InMemoryStorage::new();
        store
            .put("a", b"hello".to_vec(), "text/plain")
            .await
            .unwrap();
        assert_eq!(store.get("a").await.unwrap(), b"hello");
        assert!(store.exists("a").await.unwrap());
    }

    #[tokio::test]
    async fn a_missing_object_is_not_found() {
        let store = InMemoryStorage::new();
        assert!(store.get("nope").await.is_err());
        assert!(!store.exists("nope").await.unwrap());
    }

    #[tokio::test]
    async fn deleting_a_tenant_prefix_leaves_other_tenants_alone() {
        let store = InMemoryStorage::new();
        let mine = StorageKey::new(OrgId::new(), KnowledgeBaseId::new(), DocumentId::new(), 1);
        let theirs = StorageKey::new(OrgId::new(), KnowledgeBaseId::new(), DocumentId::new(), 1);
        store.put(&mine.original(), vec![1], "x").await.unwrap();
        store.put(&mine.extracted(), vec![2], "x").await.unwrap();
        store.put(&theirs.original(), vec![3], "x").await.unwrap();

        let removed = store
            .delete_prefix(&StorageKey::tenant_prefix(mine.org_id))
            .await
            .unwrap();

        assert_eq!(removed, 2);
        assert!(store.exists(&theirs.original()).await.unwrap());
        assert!(!store.exists(&mine.original()).await.unwrap());
    }
}
