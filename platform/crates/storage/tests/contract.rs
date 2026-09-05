//! One suite, run against every implementation.
//!
//! The in-memory store exists so tests are fast; the local one is what a
//! developer actually runs against. If they disagree, a test that passes in one
//! is worth nothing in the other, so both answer the same questions here.

use anthovai_core::{DocumentId, KnowledgeBaseId, OrgId};
use anthovai_storage::{InMemoryStorage, ObjectStorage, ObjectStoreStorage, StorageKey};

/// A fresh local store in a directory this test owns.
struct TempStore {
    store: ObjectStoreStorage,
    path: std::path::PathBuf,
}

impl TempStore {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("anthovai-storage-{name}-{}", OrgId::new()));
        let store = ObjectStoreStorage::local(path.to_str().unwrap()).expect("open local store");
        Self { store, path }
    }
}

impl Drop for TempStore {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn key() -> StorageKey {
    StorageKey::new(OrgId::new(), KnowledgeBaseId::new(), DocumentId::new(), 1)
}

/// Run one scenario against both implementations.
macro_rules! contract_test {
    (async fn $name:ident($store:ident: &dyn ObjectStorage) $body:block) => {
        mod $name {
            use super::*;

            async fn scenario($store: &dyn ObjectStorage) $body

            #[tokio::test]
            async fn in_memory() {
                scenario(&InMemoryStorage::new()).await;
            }

            #[tokio::test]
            async fn on_local_disk() {
                let temp = TempStore::new(stringify!($name));
                scenario(&temp.store).await;
            }
        }
    };
}

contract_test!(
    async fn stores_and_reads_back(store: &dyn ObjectStorage) {
        let key = key();
        store
            .put(&key.original(), b"hello world".to_vec(), "text/plain")
            .await
            .expect("put");

        assert_eq!(store.get(&key.original()).await.unwrap(), b"hello world");
        assert!(store.exists(&key.original()).await.unwrap());
    }
);

contract_test!(
    async fn a_missing_object_is_not_found(store: &dyn ObjectStorage) {
        let key = key();
        assert!(!store.exists(&key.original()).await.unwrap());

        let err = store
            .get(&key.original())
            .await
            .expect_err("should be missing");
        assert_eq!(err.code(), "object_not_found");
    }
);

contract_test!(
    async fn writing_twice_replaces_the_object(store: &dyn ObjectStorage) {
        let key = key();
        store
            .put(&key.original(), b"first".to_vec(), "text/plain")
            .await
            .unwrap();
        store
            .put(&key.original(), b"second".to_vec(), "text/plain")
            .await
            .unwrap();

        assert_eq!(store.get(&key.original()).await.unwrap(), b"second");
    }
);

contract_test!(
    async fn deleting_something_absent_is_not_an_error(store: &dyn ObjectStorage) {
        // Ingestion retries and cleanup jobs both re-delete. Making that an error
        // would turn ordinary retries into failures.
        let key = key();
        store
            .delete(&key.original())
            .await
            .expect("delete is idempotent");
    }
);

contract_test!(
    async fn the_original_and_the_extracted_text_are_separate(store: &dyn ObjectStorage) {
        let key = key();
        store
            .put(&key.original(), b"raw".to_vec(), "application/pdf")
            .await
            .unwrap();
        store
            .put(&key.extracted(), b"text".to_vec(), "text/plain")
            .await
            .unwrap();

        assert_eq!(store.get(&key.original()).await.unwrap(), b"raw");
        assert_eq!(store.get(&key.extracted()).await.unwrap(), b"text");

        store.delete(&key.original()).await.unwrap();
        assert!(store.exists(&key.extracted()).await.unwrap());
    }
);

contract_test!(
    async fn versions_do_not_overwrite_each_other(store: &dyn ObjectStorage) {
        let v1 = key();
        let v2 = StorageKey { version: 2, ..v1 };

        store
            .put(&v1.original(), b"old".to_vec(), "text/plain")
            .await
            .unwrap();
        store
            .put(&v2.original(), b"new".to_vec(), "text/plain")
            .await
            .unwrap();

        assert_eq!(store.get(&v1.original()).await.unwrap(), b"old");
        assert_eq!(store.get(&v2.original()).await.unwrap(), b"new");
    }
);

contract_test!(
    async fn deleting_a_tenant_leaves_other_tenants_alone(store: &dyn ObjectStorage) {
        let mine = key();
        let theirs = key();

        store
            .put(&mine.original(), b"mine".to_vec(), "text/plain")
            .await
            .unwrap();
        store
            .put(&mine.extracted(), b"mine too".to_vec(), "text/plain")
            .await
            .unwrap();
        store
            .put(&theirs.original(), b"theirs".to_vec(), "text/plain")
            .await
            .unwrap();

        let removed = store
            .delete_prefix(&StorageKey::tenant_prefix(mine.org_id))
            .await
            .expect("delete prefix");

        assert_eq!(removed, 2);
        assert!(!store.exists(&mine.original()).await.unwrap());
        assert!(!store.exists(&mine.extracted()).await.unwrap());
        assert!(
            store.exists(&theirs.original()).await.unwrap(),
            "another tenant's objects must survive"
        );
    }
);

contract_test!(
    async fn a_prefix_that_matches_nothing_removes_nothing(store: &dyn ObjectStorage) {
        let removed = store
            .delete_prefix(&StorageKey::tenant_prefix(OrgId::new()))
            .await
            .unwrap();
        assert_eq!(removed, 0);
    }
);

contract_test!(
    async fn holds_binary_content_unchanged(store: &dyn ObjectStorage) {
        // PDFs are not text, and a store that mangles bytes would be found much
        // later, in a parser, looking like a parser bug.
        let key = key();
        let pdf: Vec<u8> = (0..=255).collect();
        store
            .put(&key.original(), pdf.clone(), "application/pdf")
            .await
            .unwrap();

        assert_eq!(store.get(&key.original()).await.unwrap(), pdf);
    }
);
