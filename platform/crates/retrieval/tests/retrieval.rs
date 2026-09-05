//! Retrieval against a real PostgreSQL, real vectors and the real HNSW index.
//!
//! The unit tests cover fusion and budgeting on synthetic candidates. These
//! cover what only the database can answer: that the index is actually used,
//! that a search cannot reach another tenant's chunks, and that what comes back
//! carries enough to build a citation a customer can check.

use std::sync::Arc;

use anthovai_core::config::EmbeddingSettings;
use anthovai_core::{DocumentId, KnowledgeBaseId, OrgId, Plan, TenantCtx, WorkspaceId};
use anthovai_db::{sqlx, Db};
use anthovai_embeddings::{EmbeddingProvider, EmbeddingRunner, HashEmbedder, RunnerConfig};
use anthovai_ingestion::{pipeline, IngestPipeline};
use anthovai_knowledge::{CreateKnowledgeBase, KnowledgeService, UploadTarget};
use anthovai_retrieval::{RetrievalConfig, Retriever, SearchFilters};
use anthovai_storage::{InMemoryStorage, ObjectStorage, Storage};
use anthovai_testkit::db_test;

struct Fixture {
    db: Db,
    knowledge: KnowledgeService,
    storage: Arc<InMemoryStorage>,
    pipeline: IngestPipeline,
    retriever: Retriever,
    ctx: TenantCtx,
    knowledge_base_id: KnowledgeBaseId,
}

const MODEL: &str = "fake:hash-1536";

impl Fixture {
    async fn new(db: &Db) -> Self {
        let storage = Arc::new(InMemoryStorage::new());
        let ctx = seed_tenant(db).await;
        let embedder: Arc<dyn EmbeddingProvider> = Arc::new(HashEmbedder::new(1536));

        let knowledge = KnowledgeService::new(
            db.clone(),
            Arc::clone(&storage) as Storage,
            EmbeddingSettings {
                default_model: MODEL.to_owned(),
                dimension: 1536,
                batch_size: 64,
                concurrency: 4,
            },
        );

        let knowledge_base_id = knowledge
            .create_knowledge_base(
                &ctx,
                CreateKnowledgeBase {
                    workspace_id: ctx.workspace_id.unwrap(),
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
                Arc::clone(&embedder),
                RunnerConfig::default(),
            )),
            pipeline::chunk_config_from(60, 10),
        );

        Self {
            db: db.clone(),
            knowledge,
            storage,
            pipeline,
            retriever: Retriever::new(db.clone(), vec![embedder]),
            ctx,
            knowledge_base_id,
        }
    }

    /// Upload a file and take it all the way to indexed chunks.
    ///
    /// Uploaded as a file rather than pasted text, so the extension picks the
    /// parser the way it does for a customer — Markdown through the Markdown
    /// parser, headings and all.
    async fn index(&self, filename: &str, text: &str) -> DocumentId {
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

        self.storage
            .put(&start.storage_key, text.as_bytes().to_vec(), "text/plain")
            .await
            .expect("store");

        self.knowledge
            .finish_upload(
                &self.ctx,
                start.document_id,
                &start.storage_key,
                text.len() as i64,
                &anthovai_embeddings::content_hash(text),
            )
            .await
            .expect("finish upload");

        self.pipeline
            .run(self.ctx.org_id, start.document_id, 1)
            .await
            .expect("ingest");

        start.document_id
    }

    /// Search with the relevance floor removed.
    ///
    /// These tests run against the hash embedder, whose similarity scores carry
    /// word overlap and nothing else — a paragraph that plainly answers a
    /// question can score 0.16 simply because it is long. Judging the floor
    /// against those numbers would be measuring the stand-in, so tests about
    /// what retrieval *returns* switch it off, and the floor has its own test
    /// with a value the numbers can carry.
    async fn search(&self, query: &str) -> anthovai_retrieval::Retrieved {
        let structural = RetrievalConfig {
            min_relevance: 0.0,
            ..RetrievalConfig::default()
        };
        self.search_with(query, &SearchFilters::default(), &structural)
            .await
    }

    async fn search_with(
        &self,
        query: &str,
        filters: &SearchFilters,
        config: &RetrievalConfig,
    ) -> anthovai_retrieval::Retrieved {
        self.retriever
            .retrieve(&self.ctx, &[self.knowledge_base_id], query, filters, config)
            .await
            .expect("retrieval should succeed")
    }
}

async fn seed_tenant(db: &Db) -> TenantCtx {
    let org_id = OrgId::new();
    let workspace_id = WorkspaceId::new();

    let mut system = db.system().await.unwrap();
    sqlx::query("INSERT INTO organizations (id, slug, name) VALUES ($1, $2, $3)")
        .bind(org_id.to_db())
        .bind(format!("ret-{}", org_id.to_db().to_lowercase()))
        .bind("Retrieval test")
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
    ctx
}

const HANDBOOK: &str = "# Programs\n\n\
## Rust Programming\n\n\
The Rust programming course runs for twelve weeks on weekday evenings. Students \
need a laptop that can compile a moderately large project. The first four weeks \
cover ownership and borrowing, which is where most people struggle.\n\n\
## Admissions\n\n\
Applications open in March and close at the end of April. There is no entrance \
examination, but places are limited and applicants describe something they have \
built. Fees are payable by term.\n\n\
## Cafeteria\n\n\
The cafeteria opens at seven in the morning and closes after the evening class. \
Hot meals are served until eight. There is a vegetarian option every day.\n";

// ---- finding things -------------------------------------------------------

db_test!(async fn a_question_reaches_the_passage_that_answers_it(db) {
    let fixture = Fixture::new(&db).await;
    fixture.index("handbook.md", HANDBOOK).await;

    let found = fixture.search("how many weeks does the Rust course run").await;

    assert!(!found.is_empty(), "the handbook should have an answer");
    // That it is *found* is a fact about the search: the right rows are
    // reachable, filtered correctly, and come back whole. Whether it ranks
    // first is a fact about the embedding model, and cannot be judged against a
    // stand-in — that measurement waits for a real one (Phase F).
    assert!(
        found
            .candidates
            .iter()
            .any(|c| c.content.contains("twelve weeks")),
        "the answering paragraph should be among the results, got: {:?}",
        found
            .candidates
            .iter()
            .map(|c| c.content.chars().take(40).collect::<String>())
            .collect::<Vec<_>>()
    );
});

db_test!(async fn what_comes_back_can_be_turned_into_a_citation(db) {
    let fixture = Fixture::new(&db).await;
    fixture.index("handbook.md", HANDBOOK).await;

    let found = fixture.search("when do applications close").await;
    let sources = &found.context.sources;
    assert!(!sources.is_empty(), "there should be sources");

    for source in sources {
        // A customer checking an answer needs to know which document it came
        // from; a chunk id would tell them nothing.
        assert!(
            source.title.contains("handbook.md"),
            "every citation names its document, got {}",
            source.title
        );
        assert!(source.document_id.starts_with("doc_"));
        assert!(source.chunk_id.starts_with("chk_"));
        assert!(!source.snippet.is_empty());
    }

    // And where in the document. Which section is a question of ranking, so
    // this asks only that the section survived ingestion into the citation.
    assert!(
        sources.iter().any(|s| s.title.contains(" — ")),
        "a citation should carry the section it came from, got: {:?}",
        sources.iter().map(|s| &s.title).collect::<Vec<_>>()
    );
});

db_test!(async fn the_knowledge_block_is_numbered_and_escaped(db) {
    let fixture = Fixture::new(&db).await;
    fixture.index("handbook.md", HANDBOOK).await;

    let found = fixture.search("cafeteria opening hours").await;

    assert!(found.context.block.starts_with("<knowledge>"));
    assert!(found.context.block.contains("n=\"1\""));
    assert!(found.context.token_estimate > 0);
});

db_test!(async fn an_unrelated_question_finds_nothing_relevant(db) {
    let fixture = Fixture::new(&db).await;
    fixture.index("handbook.md", HANDBOOK).await;

    // Nothing in the handbook is about this. With the relevance floor raised to
    // where a real model would put it, the honest answer is no passages —
    // which is what lets a strict agent say "I do not know" rather than
    // improvise from the closest paragraph it could find.
    let strict = RetrievalConfig {
        min_relevance: 0.9,
        ..RetrievalConfig::default()
    };
    let found = fixture
        .search_with("what is the airspeed of an unladen swallow", &SearchFilters::default(), &strict)
        .await;

    assert!(found.is_empty(), "got {:?}", found.candidates.len());
});

db_test!(async fn a_search_with_no_knowledge_bases_returns_nothing(db) {
    let fixture = Fixture::new(&db).await;
    fixture.index("handbook.md", HANDBOOK).await;

    let found = fixture
        .retriever
        .retrieve(
            &fixture.ctx,
            &[],
            "anything at all",
            &SearchFilters::default(),
            &RetrievalConfig::default(),
        )
        .await
        .unwrap();

    assert!(found.is_empty());
});

db_test!(async fn an_empty_question_is_not_searched_for(db) {
    let fixture = Fixture::new(&db).await;
    fixture.index("handbook.md", HANDBOOK).await;

    assert!(fixture.search("   ").await.is_empty());
});

// ---- limits ---------------------------------------------------------------

db_test!(async fn the_context_stays_inside_its_budget(db) {
    let fixture = Fixture::new(&db).await;
    fixture.index("handbook.md", HANDBOOK).await;

    let tight = RetrievalConfig {
        context_token_budget: 80,
        top_k: 10,
        ..RetrievalConfig::default()
    };
    let found = fixture
        .search_with("courses and admissions and food", &SearchFilters::default(), &tight)
        .await;

    // The budget is what stops a long document pushing the question out of the
    // model's context window.
    assert!(
        found.context.token_estimate <= 80,
        "context was {} tokens",
        found.context.token_estimate
    );
});

db_test!(async fn no_more_than_top_k_passages_come_back(db) {
    let fixture = Fixture::new(&db).await;
    fixture.index("handbook.md", HANDBOOK).await;
    fixture.index("second.md", HANDBOOK).await;

    let narrow = RetrievalConfig {
        top_k: 2,
        ..RetrievalConfig::default()
    };
    let found = fixture
        .search_with("courses", &SearchFilters::default(), &narrow)
        .await;

    assert!(found.candidates.len() <= 2);
});

db_test!(async fn a_search_can_be_narrowed_to_one_document(db) {
    let fixture = Fixture::new(&db).await;
    let handbook = fixture.index("handbook.md", HANDBOOK).await;
    fixture.index("other.md", HANDBOOK).await;

    let filters = SearchFilters {
        document_ids: vec![handbook.to_db()],
    };
    let found = fixture
        .search_with("admissions", &filters, &RetrievalConfig::default())
        .await;

    assert!(!found.is_empty());
    assert!(
        found
            .candidates
            .iter()
            .all(|c| c.document_id == handbook.to_string()),
        "only the named document should be searched"
    );
});

db_test!(async fn deleted_chunks_are_not_searched(db) {
    let fixture = Fixture::new(&db).await;
    let document_id = fixture.index("handbook.md", HANDBOOK).await;

    assert!(!fixture.search("Rust course").await.is_empty());

    fixture
        .knowledge
        .delete_document(&fixture.ctx, document_id)
        .await
        .unwrap();

    let mut db_tx = fixture.db.tenant(&fixture.ctx).await.unwrap();
    anthovai_retrieval::chunk_repo::mark_document_chunks_deleted(&mut db_tx, document_id)
        .await
        .unwrap();
    db_tx.commit().await.unwrap();

    assert!(
        fixture.search("Rust course").await.is_empty(),
        "a deleted document must stop being an answer immediately"
    );
});

// ---- isolation ------------------------------------------------------------

db_test!(async fn a_search_never_reaches_another_tenants_chunks(db) {
    // Both tenants hold near-identical text, which is exactly the case where a
    // missing tenant filter would go unnoticed: the answers would look right.
    let alice = Fixture::new(&db).await;
    let bob = Fixture::new(&db).await;

    alice.index("handbook.md", HANDBOOK).await;
    bob.index(
        "handbook.md",
        &HANDBOOK.replace("twelve weeks", "twenty weeks"),
    )
    .await;

    let found = alice.search("how many weeks does the Rust course run").await;

    assert!(!found.is_empty());
    assert!(
        found.candidates.iter().all(|c| !c.content.contains("twenty weeks")),
        "Alice must not see Bob's version"
    );
    assert!(
        found.candidates.iter().any(|c| c.content.contains("twelve weeks")),
        "Alice should see her own"
    );
});

db_test!(async fn another_tenants_knowledge_base_is_reported_missing(db) {
    let alice = Fixture::new(&db).await;
    let bob = Fixture::new(&db).await;
    alice.index("handbook.md", HANDBOOK).await;

    // Bob names Alice's knowledge base. Row-level security hides it, and the
    // count check turns "hidden" into a plain not-found rather than an empty
    // result that looks like the base is simply empty.
    let error = bob
        .retriever
        .retrieve(
            &bob.ctx,
            &[alice.knowledge_base_id],
            "Rust course",
            &SearchFilters::default(),
            &RetrievalConfig::default(),
        )
        .await
        .expect_err("Bob must not search Alice's knowledge base");

    assert_eq!(error.code(), "knowledge_base_not_found");
});

db_test!(async fn a_knowledge_base_built_with_another_model_is_refused(db) {
    let fixture = Fixture::new(&db).await;
    fixture.index("handbook.md", HANDBOOK).await;

    // A retriever configured with a different model than the base was built
    // with. Searching anyway would compare vectors from two models, whose
    // distances are meaningless rather than merely inaccurate.
    let mismatched = Retriever::new(db.clone(), vec![Arc::new(HashEmbedder::new(768))]);

    let error = mismatched
        .retrieve(
            &fixture.ctx,
            &[fixture.knowledge_base_id],
            "Rust course",
            &SearchFilters::default(),
            &RetrievalConfig::default(),
        )
        .await
        .expect_err("mismatched models must not be searched across");

    assert_eq!(error.code(), "knowledge_base_needs_reembedding");
});

// ---- the index ------------------------------------------------------------

db_test!(async fn the_vector_index_is_reachable_through_row_level_security(db) {
    // The risk this guards is specific: `pgvector` cannot see the tenant
    // predicate, and row-level security wraps the scan in a filter of its own.
    // If that barrier made the HNSW index unreachable, every search would
    // silently fall back to scanning every chunk the tenant owns — correct
    // answers, found in production as latency, months later.
    //
    // What is asserted is reachability, not preference. Whether the planner
    // *chooses* the index depends on how many rows the tenant has, which a test
    // cannot hold still: an earlier version seeded 1,200 chunks and asserted the
    // index was chosen, and it passed only because other tests had left the
    // table large enough to make a scan look expensive. On a clean database the
    // same 1,200 rows are cheaper to scan, and it failed — correctly, and for a
    // reason that had nothing to do with the index.
    //
    // So the scan is discouraged rather than the data inflated. `enable_seqscan`
    // is a cost penalty, not a prohibition: if the index were unreachable the
    // planner would still return a sequential scan here, and this would fail.
    let fixture = Fixture::new(&db).await;
    seed_many_chunks(&fixture, 1_200).await;

    // Without this the planner works from whatever statistics autovacuum last
    // collected, which after a large delete elsewhere in the table can be very
    // wrong — and the test then measures autovacuum timing rather than whether
    // the index is reachable. That was this test's one historical flake.
    {
        let mut analyze = fixture.db.system().await.unwrap();
        sqlx::query("ANALYZE document_chunks")
            .execute(analyze.conn())
            .await
            .expect("refresh the planner statistics");
        analyze.commit().await.unwrap();
    }

    let embedder = HashEmbedder::new(1536);
    let query = embedder.embed_one("ownership and borrowing").await.unwrap();

    let mut db_tx = fixture.db.tenant(&fixture.ctx).await.unwrap();
    for setting in ["SET LOCAL hnsw.ef_search = 40", "SET LOCAL enable_seqscan = off"] {
        sqlx::query(setting).execute(db_tx.conn()).await.unwrap();
    }

    let plan: String = sqlx::query_scalar(
        "EXPLAIN (FORMAT TEXT) SELECT id FROM document_chunks
         WHERE tenant_id = $1 AND deleted_at IS NULL
         ORDER BY embedding <=> $2 LIMIT 8",
    )
    .bind(fixture.ctx.org_id.to_db())
    .bind(pgvector::Vector::from(query))
    .fetch_all(db_tx.conn())
    .await
    .map(|rows: Vec<String>| rows.join("\n"))
    .expect("explain the query");
    db_tx.commit().await.unwrap();

    assert!(
        plan.contains("chunks_embedding_idx") || plan.to_lowercase().contains("index scan"),
        "the vector index should be used, plan was:\n{plan}"
    );
});

/// Enough chunks that PostgreSQL prefers an index over a scan.
async fn seed_many_chunks(fixture: &Fixture, count: usize) {
    use anthovai_retrieval::chunk_repo::{insert_chunks, ChunkToInsert};

    let embedder = HashEmbedder::new(1536);
    let document_id = DocumentId::new();

    let mut db = fixture.db.tenant(&fixture.ctx).await.unwrap();
    sqlx::query(
        "INSERT INTO documents (id, tenant_id, knowledge_base_id, title, source_type, status,
                                current_version)
         VALUES ($1, $2, $3, 'bulk', 'text', 'ready', 1)",
    )
    .bind(document_id.to_db())
    .bind(fixture.ctx.org_id.to_db())
    .bind(fixture.knowledge_base_id.to_db())
    .execute(db.conn())
    .await
    .unwrap();

    let mut batch = Vec::with_capacity(count);
    for i in 0..count {
        let content = format!("filler passage number {i} about various unrelated subjects");
        let vector = embedder.embed_one(&content).await.unwrap();
        batch.push(ChunkToInsert {
            chunk_index: i as i32,
            content_hash: anthovai_embeddings::content_hash(&content),
            content,
            token_count: 10,
            vector,
            metadata: serde_json::json!({"title": "bulk"}),
        });
    }

    insert_chunks(&mut db, fixture.knowledge_base_id, document_id, 1, &batch)
        .await
        .unwrap();
    db.commit().await.unwrap();

    // The planner needs statistics before it will trust the index.
    sqlx::query("ANALYZE document_chunks")
        .execute(fixture.db.pool())
        .await
        .unwrap();
}
