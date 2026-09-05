//! Object storage for original uploads and extracted text.
//!
//! Keys are always built by [`StorageKey`], never from a customer-supplied
//! filename, so a hostile name cannot escape its tenant prefix.

use std::sync::Arc;

use anthovai_core::config::StorageSettings;
use anthovai_core::{DocumentId, KnowledgeBaseId, OrgId, Result};
use async_trait::async_trait;

pub mod memory;
pub mod object;

pub use memory::InMemoryStorage;
pub use object::ObjectStoreStorage;

/// What the rest of the platform holds.
pub type Storage = Arc<dyn ObjectStorage>;

/// Open the store this deployment is configured for.
pub fn from_settings(settings: &StorageSettings) -> Result<Storage> {
    Ok(Arc::new(ObjectStoreStorage::from_settings(settings)?))
}

#[async_trait]
pub trait ObjectStorage: Send + Sync {
    async fn put(&self, key: &str, bytes: Vec<u8>, content_type: &str) -> Result<()>;
    async fn get(&self, key: &str) -> Result<Vec<u8>>;
    async fn delete(&self, key: &str) -> Result<()>;
    async fn exists(&self, key: &str) -> Result<bool>;
    /// Remove everything under a prefix. Used when a tenant is deleted.
    async fn delete_prefix(&self, prefix: &str) -> Result<u64>;
}

/// Builds the documented layout:
/// `tenant/{org}/{kb}/{doc}/v{n}/{original|extracted.txt}`
#[derive(Clone, Copy, Debug)]
pub struct StorageKey {
    pub org_id: OrgId,
    pub knowledge_base_id: KnowledgeBaseId,
    pub document_id: DocumentId,
    pub version: i32,
}

impl StorageKey {
    pub fn new(
        org_id: OrgId,
        knowledge_base_id: KnowledgeBaseId,
        document_id: DocumentId,
        version: i32,
    ) -> Self {
        Self {
            org_id,
            knowledge_base_id,
            document_id,
            version,
        }
    }

    fn dir(&self) -> String {
        format!(
            "tenant/{}/{}/{}/v{}",
            self.org_id.to_db(),
            self.knowledge_base_id.to_db(),
            self.document_id.to_db(),
            self.version
        )
    }

    pub fn original(&self) -> String {
        format!("{}/original", self.dir())
    }

    pub fn extracted(&self) -> String {
        format!("{}/extracted.txt", self.dir())
    }

    /// Everything belonging to one tenant, for deletion.
    pub fn tenant_prefix(org_id: OrgId) -> String {
        format!("tenant/{}/", org_id.to_db())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key() -> StorageKey {
        StorageKey::new(OrgId::new(), KnowledgeBaseId::new(), DocumentId::new(), 2)
    }

    #[test]
    fn keys_start_with_the_tenant_prefix() {
        let key = key();
        let prefix = StorageKey::tenant_prefix(key.org_id);
        assert!(key.original().starts_with(&prefix));
        assert!(key.extracted().starts_with(&prefix));
    }

    #[test]
    fn versions_get_their_own_directory() {
        let v1 = StorageKey::new(OrgId::new(), KnowledgeBaseId::new(), DocumentId::new(), 1);
        let v2 = StorageKey { version: 2, ..v1 };
        assert_ne!(v1.original(), v2.original());
        assert!(v2.original().contains("/v2/"));
    }

    #[test]
    fn the_original_and_the_extracted_text_are_separate_objects() {
        let key = key();
        assert_ne!(key.original(), key.extracted());
    }

    #[test]
    fn keys_are_built_only_from_ids_so_a_filename_cannot_escape() {
        // There is no path here a caller could influence: every segment is a ULID.
        let key = key();
        assert!(!key.original().contains(".."));
        assert_eq!(key.original().matches('/').count(), 5);
    }
}
