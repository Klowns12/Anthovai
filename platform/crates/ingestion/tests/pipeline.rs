//! Ingestion end to end, against a real PostgreSQL and a real vector column.
//!
//! What the unit tests cannot show is the part that matters most here: that a
//! re-upload never leaves a gap where the document is unsearchable, that a
//! failure leaves the previous version serving, and that a vector round-trips
//! through `pgvector` unchanged.

use std::sync::Arc;

use anthovai_core::config::EmbeddingSettings;
use anthovai_core::{
    Clock, DocumentId, KnowledgeBaseId, OrgId, Plan, TenantCtx, UserId, WorkspaceId,
};
use anthovai_db::{sqlx, Db};
use anthovai_embeddings::{EmbeddingRunner, HashEmbedder, RunnerConfig};
use anthovai_ingestion::{pipeline, IngestPipeline};
use anthovai_knowledge::{
    repo as knowledge_repo, CreateKnowledgeBase, DocumentStatus, KnowledgeService, UploadTarget,
};
use anthovai_retrieval::chunk_repo;
use anthovai_storage::{InMemoryStorage, ObjectStorage, Storage};
use anthovai_testkit::db_test;

struct Fixture {
    db: Db,
    knowledge: KnowledgeService,
    storage: Arc<InMemoryStorage>,
    pipeline: IngestPipeline,
    ctx: TenantCtx,
    knowledge_base_id: KnowledgeBaseId,
}

impl Fixture {
    async fn new(db: &Db) -> Self {
        let storage = Arc::new(InMemoryStorage::new());
        let ctx = seed_tenant(db).await;

        let knowledge = KnowledgeService::new(
            db.clone(),
            Arc::clone(&storage) as Storage,
            EmbeddingSettings {
                default_model: "fake:hash-1536".to_owned(),
                dimension: 1536,
                batch_size: 64,
                concurrency: 4,
            },
        );

        let knowledge_base_id = knowledge
            .create_knowledge_base(
                &ctx,
                CreateKnowledgeBase {
                    workspace_id: ctx.workspace_id.expect("seeded with a workspace"),
                    name: "Student Handbook".into(),
                    description: None,
                },
            )
            .await
            .expect("create knowledge base")
            .id;

        let pipeline = IngestPipeline::new(
            db.clone(),
            Arc::clone(&storage) as Storage,
            Arc::new(EmbeddingRunner::new(
                Arc::new(HashEmbedder::new(1536)),
                RunnerConfig::default(),
            )),
            // Small, so a short document still produces several chunks.
            pipeline::chunk_config_from(60, 10),
        );

        Self {
            db: db.clone(),
            knowledge,
            storage,
            pipeline,
            ctx,
            knowledge_base_id,
        }
    }

    /// Upload a file and return the document it produced.
    ///
    /// A file rather than pasted text, so the extension picks the parser the
    /// way it does for a customer. Uploading Markdown as plain text runs the
    /// text parser instead, and the headings quietly never appear.
    async fn upload(&self, filename: &str, text: &str) -> DocumentId {
        let start = self
            .knowledge
            .start_upload(
                &self.ctx,
                self.knowledge_base_id,
                UploadTarget::File {
                    filename: filename.to_owned(),
                    mime_type: Some("text/markdown".to_owned()),
                    declared_size: Some(text.len() as i64),
                },
            )
            .await
            .expect("start upload");

        let bytes = text.as_bytes().to_vec();
        let hash = anthovai_embeddings::content_hash(text);

        self.storage
            .put(&start.storage_key, bytes.clone(), "text/plain")
            .await
            .expect("store the bytes");

        self.knowledge
            .finish_upload(
                &self.ctx,
                start.document_id,
                &start.storage_key,
                bytes.len() as i64,
                &hash,
            )
            .await
            .expect("finish upload");

        start.document_id
    }

    /// Replace a document's stored bytes, as a re-upload would.
    async fn replace_bytes(&self, document_id: DocumentId, version: i32, text: &str) {
        let key = anthovai_storage::StorageKey::new(
            self.ctx.org_id,
            self.knowledge_base_id,
            document_id,
            version,
        );
        self.storage
            .put(&key.original(), text.as_bytes().to_vec(), "text/plain")
            .await
            .expect("store the replacement");

        let mut db = self.db.tenant(&self.ctx).await.unwrap();
        knowledge_repo::record_upload(
            &mut db,
            document_id,
            &key.original(),
            text.len() as i64,
            &anthovai_embeddings::content_hash(text),
        )
        .await
        .unwrap();
        db.commit().await.unwrap();
    }

    async fn ingest(
        &self,
        document_id: DocumentId,
        version: i32,
    ) -> anthovai_ingestion::IngestOutcome {
        self.pipeline
            .run(self.ctx.org_id, document_id, version)
            .await
            .expect("ingestion should succeed")
    }

    async fn document(&self, document_id: DocumentId) -> anthovai_knowledge::Document {
        let mut db = self.db.tenant(&self.ctx).await.unwrap();
        let document = knowledge_repo::find_document(&mut db, document_id)
            .await
            .expect("read the document");
        db.commit().await.unwrap();
        document
    }

    /// Live chunks for a document, in order.
    async fn chunks(&self, document_id: DocumentId) -> Vec<(i32, String, i32)> {
        let mut db = self.db.tenant(&self.ctx).await.unwrap();
        let rows: Vec<(i32, String, i32)> = sqlx::query_as(
            "SELECT chunk_index, content, document_version
             FROM document_chunks
             WHERE document_id = $1 AND tenant_id = $2 AND deleted_at IS NULL
             ORDER BY chunk_index",
        )
        .bind(document_id.to_db())
        .bind(self.ctx.org_id.to_db())
        .fetch_all(db.conn())
        .await
        .expect("read chunks");
        db.commit().await.unwrap();
        rows
    }

    /// Chunk text with the metadata stored beside it.
    async fn chunk_metadata(&self, document_id: DocumentId) -> Vec<(String, serde_json::Value)> {
        let mut db = self.db.tenant(&self.ctx).await.unwrap();
        let rows: Vec<(String, serde_json::Value)> = sqlx::query_as(
            "SELECT content, metadata FROM document_chunks
             WHERE document_id = $1 AND tenant_id = $2 AND deleted_at IS NULL
             ORDER BY chunk_index",
        )
        .bind(document_id.to_db())
        .bind(self.ctx.org_id.to_db())
        .fetch_all(db.conn())
        .await
        .expect("read chunk metadata");
        db.commit().await.unwrap();
        rows
    }

    /// The stored vector for one chunk, straight out of the vector column.
    async fn first_vector(&self, document_id: DocumentId) -> Vec<f32> {
        let mut db = self.db.tenant(&self.ctx).await.unwrap();
        let row: (pgvector::Vector,) = sqlx::query_as(
            "SELECT embedding FROM document_chunks
             WHERE document_id = $1 AND tenant_id = $2 AND deleted_at IS NULL
             ORDER BY chunk_index LIMIT 1",
        )
        .bind(document_id.to_db())
        .bind(self.ctx.org_id.to_db())
        .fetch_one(db.conn())
        .await
        .expect("read a vector");
        db.commit().await.unwrap();
        row.0.to_vec()
    }
}

/// A tenant with one workspace, created directly: onboarding has its own tests.
async fn seed_tenant(db: &Db) -> TenantCtx {
    let org_id = OrgId::new();
    let workspace_id = WorkspaceId::new();
    let user_id = UserId::new();

    let mut system = db.system().await.unwrap();
    sqlx::query("INSERT INTO organizations (id, slug, name) VALUES ($1, $2, $3)")
        .bind(org_id.to_db())
        .bind(format!("ing-{}", org_id.to_db().to_lowercase()))
        .bind("Ingestion test")
        .execute(system.conn())
        .await
        .unwrap();
    sqlx::query("INSERT INTO workspaces (id, tenant_id, name, slug) VALUES ($1, $2, $3, $4)")
        .bind(workspace_id.to_db())
        .bind(org_id.to_db())
        .bind("Default")
        .bind("default")
        .execute(system.conn())
        .await
        .unwrap();
    system.commit().await.unwrap();

    let mut ctx = TenantCtx::system(org_id, Plan::Enterprise);
    ctx.workspace_id = Some(workspace_id);
    let _ = user_id;
    let _ = Clock::system();
    ctx
}

/// Long enough that each section becomes its own chunk. A document small
/// enough to fit in one chunk cannot show that an edit to one paragraph leaves
/// the others alone, which is what several of these tests are about.
const HANDBOOK: &str = "# Programs\n\n\
## Rust Programming\n\n\
The Rust programming course runs for twelve weeks, on weekday evenings from six \
until nine. Students need a laptop that can compile a moderately large project, \
and rather more patience than the marketing copy suggests. The first four weeks \
cover ownership and borrowing, which is where most people struggle, and the rest \
is spent building something real.\n\n\
## Go Programming\n\n\
The Go course runs for eight weeks and covers concurrency in depth. It assumes \
you have written software before in some other language, though not necessarily \
a compiled one. Goroutines and channels take up the middle four weeks, and the \
course finishes with a small networked service that students deploy themselves.\n\n\
## Admissions\n\n\
Applications open in March and close at the end of April. There is no entrance \
examination, but places are limited and applicants are asked to describe \
something they have built, however small. Fees are payable by term, and a \
limited number of scholarships are available for students who need them.\n";

// ---- the happy path -------------------------------------------------------

db_test!(async fn a_document_becomes_searchable_chunks(db) {
    let fixture = Fixture::new(&db).await;
    let document_id = fixture.upload("handbook.md", HANDBOOK).await;

    let outcome = fixture.ingest(document_id, 1).await;

    assert!(outcome.chunks > 0, "the document should produce chunks");
    assert!(outcome.tokens > 0);

    let document = fixture.document(document_id).await;
    assert_eq!(document.status, DocumentStatus::Ready);
    assert_eq!(document.progress, 100);
    assert_eq!(document.chunk_count, outcome.chunks as i32);
    assert!(document.token_count > 0);

    let chunks = fixture.chunks(document_id).await;
    assert_eq!(chunks.len(), outcome.chunks);
    assert!(chunks.iter().all(|(_, _, version)| *version == 1));
});

db_test!(async fn chunks_carry_the_heading_they_came_from(db) {
    let fixture = Fixture::new(&db).await;
    let document_id = fixture.upload("handbook.md", HANDBOOK).await;
    fixture.ingest(document_id, 1).await;

    let rust_chunk = fixture
        .chunk_metadata(document_id)
        .await
        .into_iter()
        .find(|(content, _)| content.contains("twelve weeks"))
        .expect("the Rust paragraph should be indexed");

    // The contextual header is what makes a retrieved paragraph make sense on
    // its own.
    assert!(
        rust_chunk.0.contains("Section: Programs > Rust Programming"),
        "the chunk text should carry its section: {}",
        rust_chunk.0
    );

    // And the same path is stored as data, because that is what a citation is
    // built from — asserting only on the text would pass even if the Markdown
    // parser never ran and the heading was just another line of prose.
    let heading_path = rust_chunk.1["heading_path"]
        .as_array()
        .expect("heading_path should be stored");
    assert_eq!(
        heading_path
            .iter()
            .filter_map(|v| v.as_str())
            .collect::<Vec<_>>(),
        vec!["Programs", "Rust Programming"]
    );
});

db_test!(async fn a_vector_round_trips_through_the_database(db) {
    let fixture = Fixture::new(&db).await;
    let document_id = fixture.upload("handbook.md", HANDBOOK).await;
    fixture.ingest(document_id, 1).await;

    let stored = fixture.first_vector(document_id).await;

    assert_eq!(stored.len(), 1536, "the column width must match the model");
    assert!(
        stored.iter().any(|v| *v != 0.0),
        "a vector of zeros would retrieve nothing and look like it worked"
    );

    // Normalised by the embedder, and unchanged by the round trip.
    let magnitude: f32 = stored.iter().map(|v| v * v).sum::<f32>().sqrt();
    assert!((magnitude - 1.0).abs() < 0.01, "magnitude was {magnitude}");
});

db_test!(async fn thai_text_is_chunked_and_indexed(db) {
    let fixture = Fixture::new(&db).await;
    let thai = "# หลักสูตร\n\nหลักสูตร Rust ใช้เวลาเรียน 12 สัปดาห์ เรียนช่วงเย็นวันธรรมดา\n\n\
                นักเรียนต้องมีคอมพิวเตอร์พกพาของตนเอง\n";

    let document_id = fixture.upload("th.md", thai).await;
    let outcome = fixture.ingest(document_id, 1).await;

    assert!(outcome.chunks > 0);
    let document = fixture.document(document_id).await;
    assert_eq!(document.language.as_deref(), Some("tha"));
    assert!(
        document.token_count > 10,
        "Thai must not be counted as one token per paragraph"
    );
});

// ---- versions -------------------------------------------------------------

db_test!(async fn the_old_version_keeps_serving_until_the_new_one_is_ready(db) {
    let fixture = Fixture::new(&db).await;
    let document_id = fixture.upload("handbook.md", HANDBOOK).await;
    fixture.ingest(document_id, 1).await;

    let before = fixture.chunks(document_id).await;
    assert!(before.iter().all(|(_, _, v)| *v == 1));

    // A re-upload with the Rust course rewritten.
    let revised = HANDBOOK.replace("twelve weeks", "fourteen weeks");
    fixture.replace_bytes(document_id, 2, &revised).await;
    fixture.ingest(document_id, 2).await;

    let after = fixture.chunks(document_id).await;
    assert!(
        after.iter().all(|(_, _, v)| *v == 2),
        "only the new version should be live"
    );
    assert!(
        after.iter().any(|(_, c, _)| c.contains("fourteen weeks")),
        "the revision should be searchable"
    );
    assert!(
        !after.iter().any(|(_, c, _)| c.contains("twelve weeks")),
        "the old text should no longer be live"
    );

    let document = fixture.document(document_id).await;
    assert_eq!(document.current_version, 2);
});

db_test!(async fn unchanged_text_is_not_embedded_twice(db) {
    let fixture = Fixture::new(&db).await;
    let document_id = fixture.upload("handbook.md", HANDBOOK).await;

    let first = fixture.ingest(document_id, 1).await;
    assert_eq!(first.reused_vectors, 0, "nothing to reuse the first time");
    assert!(first.billable_tokens > 0);

    // Re-upload with one paragraph changed: only that paragraph should cost.
    let revised = HANDBOOK.replace("eight weeks", "nine weeks");
    fixture.replace_bytes(document_id, 2, &revised).await;
    let second = fixture.ingest(document_id, 2).await;

    assert!(
        second.reused_vectors > 0,
        "the unchanged paragraphs should have been reused"
    );
    assert!(
        second.billable_tokens < first.billable_tokens,
        "a one-paragraph change should cost less than the whole document: \
         {} vs {}",
        second.billable_tokens,
        first.billable_tokens
    );
});

db_test!(async fn re_ingesting_the_same_version_does_not_duplicate_chunks(db) {
    // What a retry looks like: the same job runs twice for the same version.
    let fixture = Fixture::new(&db).await;
    let document_id = fixture.upload("handbook.md", HANDBOOK).await;

    let first = fixture.ingest(document_id, 1).await;
    let second = fixture.ingest(document_id, 1).await;

    assert_eq!(first.chunks, second.chunks);
    assert_eq!(
        fixture.chunks(document_id).await.len(),
        first.chunks,
        "a retry must replace the previous attempt, not add to it"
    );
});

// ---- failures -------------------------------------------------------------

db_test!(async fn a_document_that_yields_no_text_fails_permanently(db) {
    let fixture = Fixture::new(&db).await;

    // Bytes that are not UTF-8: decoding them as text would produce nonsense
    // that gets embedded and quietly retrieved for months.
    let start = fixture
        .knowledge
        .start_upload(
            &fixture.ctx,
            fixture.knowledge_base_id,
            UploadTarget::Text {
                title: "broken.txt".into(),
            },
        )
        .await
        .unwrap();

    fixture
        .storage
        .put(&start.storage_key, vec![0x48, 0xE9, 0x6C], "text/plain")
        .await
        .unwrap();
    fixture
        .knowledge
        .finish_upload(&fixture.ctx, start.document_id, &start.storage_key, 3, "hash")
        .await
        .unwrap();

    let error = fixture
        .pipeline
        .run(fixture.ctx.org_id, start.document_id, 1)
        .await
        .expect_err("this document cannot be read");

    assert!(!error.is_retryable(), "retrying will not make it readable");
    assert_eq!(error.code(), "no_extractable_text");

    // And the customer can see why.
    let document = fixture.document(start.document_id).await;
    assert_eq!(document.status, DocumentStatus::Failed);
    assert_eq!(document.error_code.as_deref(), Some("no_extractable_text"));
    assert!(document.error_message.is_some());
});

db_test!(async fn a_failed_re_ingestion_leaves_the_previous_version_serving(db) {
    let fixture = Fixture::new(&db).await;
    let document_id = fixture.upload("handbook.md", HANDBOOK).await;
    fixture.ingest(document_id, 1).await;

    let before = fixture.chunks(document_id).await;
    assert!(!before.is_empty());

    // Version 2's bytes are unreadable.
    let key = anthovai_storage::StorageKey::new(
        fixture.ctx.org_id,
        fixture.knowledge_base_id,
        document_id,
        2,
    );
    fixture
        .storage
        .put(&key.original(), vec![0xE9, 0xE9], "text/plain")
        .await
        .unwrap();

    let mut db_tx = fixture.db.tenant(&fixture.ctx).await.unwrap();
    knowledge_repo::record_upload(&mut db_tx, document_id, &key.original(), 2, "hash")
        .await
        .unwrap();
    db_tx.commit().await.unwrap();

    fixture
        .pipeline
        .run(fixture.ctx.org_id, document_id, 2)
        .await
        .expect_err("version 2 cannot be read");

    // The customer's document is still searchable, and still says what it said.
    let after = fixture.chunks(document_id).await;
    assert_eq!(
        after.len(),
        before.len(),
        "a failed re-upload must not empty the knowledge base"
    );
    assert!(after.iter().all(|(_, _, v)| *v == 1));
    assert!(after.iter().any(|(_, c, _)| c.contains("twelve weeks")));
});

db_test!(async fn a_document_deleted_mid_ingestion_stays_deleted(db) {
    let fixture = Fixture::new(&db).await;
    let document_id = fixture.upload("handbook.md", HANDBOOK).await;

    // Deleted while its job sat in the queue.
    fixture
        .knowledge
        .delete_document(&fixture.ctx, document_id)
        .await
        .unwrap();

    let outcome = fixture.ingest(document_id, 1).await;

    assert_eq!(outcome.chunks, 0, "there was nothing to do");
    assert!(
        fixture.chunks(document_id).await.is_empty(),
        "a deleted document must not come back with fresh chunks"
    );
});

// ---- isolation ------------------------------------------------------------

db_test!(async fn one_tenant_never_reuses_anothers_vectors(db) {
    // Reuse is keyed by content hash, and identical text across two tenants is
    // entirely plausible — a standard policy, a copied FAQ. The lookup must
    // still not cross the boundary.
    let alice = Fixture::new(&db).await;
    let bob = Fixture::new(&db).await;

    let alice_doc = alice.upload("shared.md", HANDBOOK).await;
    alice.ingest(alice_doc, 1).await;

    let bob_doc = bob.upload("shared.md", HANDBOOK).await;
    let outcome = bob.ingest(bob_doc, 1).await;

    assert_eq!(
        outcome.reused_vectors, 0,
        "Bob must not reuse vectors Alice paid for, or read from her rows"
    );
    assert!(outcome.chunks > 0);
});

db_test!(async fn chunks_are_only_visible_to_their_own_tenant(db) {
    let alice = Fixture::new(&db).await;
    let bob = Fixture::new(&db).await;

    let alice_doc = alice.upload("private.md", HANDBOOK).await;
    alice.ingest(alice_doc, 1).await;

    // Bob asks for Alice's document by id, through his own tenant context.
    let mut db_tx = bob.db.tenant(&bob.ctx).await.unwrap();
    let count = chunk_repo::count_live_chunks(&mut db_tx, alice_doc)
        .await
        .unwrap();
    db_tx.commit().await.unwrap();

    assert_eq!(count, 0, "Alice's chunks must be invisible to Bob");
});

// ---- housekeeping ---------------------------------------------------------

db_test!(async fn retired_chunks_survive_long_enough_for_running_requests(db) {
    let fixture = Fixture::new(&db).await;
    let document_id = fixture.upload("handbook.md", HANDBOOK).await;
    fixture.ingest(document_id, 1).await;

    let mut marking = fixture.db.tenant(&fixture.ctx).await.unwrap();
    chunk_repo::mark_document_chunks_deleted(&mut marking, document_id)
        .await
        .unwrap();
    marking.commit().await.unwrap();

    // Nothing purged yet: a request that started a moment ago is still reading
    // these rows.
    let mut early = fixture.db.tenant(&fixture.ctx).await.unwrap();
    let purged_now = chunk_repo::purge_retired(&mut early, 24).await.unwrap();
    early.commit().await.unwrap();
    assert_eq!(purged_now, 0);

    // A day later they go. The purge runs in its own transaction, as the
    // housekeeping job does — inside the marking transaction `now()` is still
    // the moment it began, and nothing would ever look old enough.
    let mut later = fixture.db.tenant(&fixture.ctx).await.unwrap();
    let purged_later = chunk_repo::purge_retired(&mut later, 0).await.unwrap();
    later.commit().await.unwrap();

    assert!(purged_later > 0);
    assert!(fixture.chunks(document_id).await.is_empty());
});

// ---- re-embedding ----------------------------------------------------------

db_test!(async fn a_base_built_by_the_stand_in_is_found_and_can_be_repointed(db) {
    // Everything a developer indexes before a provider key exists is embedded
    // by the hash stand-in. Those bases answer questions and mean nothing by
    // them, so the worker sweeps for them at startup once a real embedder is
    // configured.
    //
    // The sweep runs as the system role, across tenants, and its first version
    // found nothing at all: `knowledge_bases` forces row-level security and
    // had no policy for that role, so the query returned an empty set and
    // reported success. That is what this test is really guarding.
    let fixture = Fixture::new(&db).await;

    let mut system = db.system().await.unwrap();
    let found = knowledge_repo::knowledge_bases_needing_reembedding(&mut system)
        .await
        .expect("the sweep should be able to read across tenants");
    system.commit().await.unwrap();

    assert!(
        found
            .iter()
            .any(|(_, kb_id)| *kb_id == fixture.knowledge_base_id),
        "a base built with `fake:hash-1536` was not found by the sweep"
    );

    // Repointing is the first thing the handler does, before any document is
    // re-ingested: retrieval groups a base's chunks by the model the base
    // names, so new vectors written under the old name would be searched by
    // the embedder that did not produce them.
    let mut tenant = db.tenant(&fixture.ctx).await.unwrap();
    knowledge_repo::set_embedding_model(
        &mut tenant,
        fixture.knowledge_base_id,
        "openai:text-embedding-3-small",
    )
    .await
    .expect("repoint the base");
    tenant.commit().await.unwrap();

    let mut system = db.system().await.unwrap();
    let found = knowledge_repo::knowledge_bases_needing_reembedding(&mut system)
        .await
        .unwrap();
    system.commit().await.unwrap();

    assert!(
        !found
            .iter()
            .any(|(_, kb_id)| *kb_id == fixture.knowledge_base_id),
        "a repointed base should not be swept again"
    );
});
