//! Does retrieval actually find the right passage?
//!
//! Every other test in this crate runs against the hash embedder and is
//! deliberately structural: it asks whether the right rows are reachable,
//! filtered and assembled, never whether they are *ranked* well. Ranking is a
//! property of the embedding model, and judging it against a stand-in would
//! measure the stand-in.
//!
//! So this file is the one that answers the question, and it needs a real
//! model. It is ignored by default and costs a fraction of a cent to run:
//!
//! ```text
//! OPENAI_API_KEY=... ANTHOVAI_TEST_DATABASE_URL=... \
//!   cargo test -p anthovai-retrieval --test relevance -- --ignored --nocapture
//! ```

use std::sync::Arc;

use anthovai_core::config::EmbeddingSettings;
use anthovai_core::{KnowledgeBaseId, OrgId, Plan, TenantCtx, WorkspaceId};
use anthovai_db::{sqlx, Db};
use anthovai_embeddings::{EmbeddingProvider, EmbeddingRunner, RunnerConfig};
use anthovai_ingestion::{pipeline, IngestPipeline};
use anthovai_knowledge::{CreateKnowledgeBase, KnowledgeService, UploadTarget};
use anthovai_provider_openai::OpenAiEmbeddings;
use anthovai_retrieval::{RetrievalConfig, Retriever, SearchFilters};
use anthovai_storage::{InMemoryStorage, Storage};

const MODEL: &str = "text-embedding-3-small";
const DIMENSION: usize = 1536;

/// A Thai handbook, because that is the language the first customers write in
/// and the one where a bag-of-words stand-in tells us least.
const HANDBOOK: &str = "# หลักสูตร\n\n\
## Rust Programming\n\n\
หลักสูตร Rust ใช้เวลาเรียน 12 สัปดาห์ เรียนช่วงเย็นวันธรรมดา ตั้งแต่หกโมงเย็นถึงสามทุ่ม \
นักเรียนต้องมีคอมพิวเตอร์พกพาที่คอมไพล์โปรเจกต์ขนาดกลางได้ สี่สัปดาห์แรกเรียนเรื่อง \
ownership และ borrowing ซึ่งเป็นจุดที่ผู้เรียนส่วนใหญ่ติด\n\n\
## Go Programming\n\n\
หลักสูตร Go ใช้เวลาเรียน 8 สัปดาห์ เน้นเรื่อง concurrency เป็นหลัก \
ผู้เรียนควรเคยเขียนโปรแกรมภาษาอื่นมาก่อน\n\n\
## การสมัครเรียน\n\n\
เปิดรับสมัครเดือนมีนาคมและปิดรับปลายเดือนเมษายน ไม่มีข้อสอบเข้า แต่ที่นั่งมีจำกัด \
ผู้สมัครต้องเล่าถึงสิ่งที่เคยสร้างมาก่อน ค่าเล่าเรียนชำระเป็นรายเทอม \
มีทุนการศึกษาจำนวนจำกัดสำหรับผู้ที่ต้องการ\n\n\
## โรงอาหาร\n\n\
โรงอาหารเปิดตั้งแต่เจ็ดโมงเช้าและปิดหลังเลิกเรียนภาคค่ำ อาหารร้อนเสิร์ฟถึงสองทุ่ม \
มีเมนูมังสวิรัติทุกวัน\n";

/// A question, and a phrase that must appear in the passage that answers it.
const QUESTIONS: &[(&str, &str)] = &[
    ("หลักสูตร Rust ใช้เวลาเรียนกี่สัปดาห์", "12 สัปดาห์"),
    ("สมัครเรียนได้ถึงเมื่อไหร่", "ปิดรับปลายเดือนเมษายน"),
    ("โรงอาหารเปิดกี่โมง", "เจ็ดโมงเช้า"),
    ("มีทุนการศึกษาไหม", "ทุนการศึกษา"),
    // Asked in English about Thai content, which is the case a keyword search
    // cannot help with at all.
    ("how long is the Go course", "Go ใช้เวลาเรียน 8 สัปดาห์"),
];

#[tokio::test]
#[ignore = "needs OPENAI_API_KEY and makes a real, billable API call"]
async fn a_real_model_finds_the_passage_that_answers_each_question() {
    let Some((db, ctx, retriever, knowledge_base_id)) = setup().await else {
        return;
    };

    let config = RetrievalConfig::default();
    let mut failures = Vec::new();

    for (question, expected) in QUESTIONS {
        let found = retriever
            .retrieve(
                &ctx,
                &[knowledge_base_id],
                question,
                &SearchFilters::default(),
                &config,
            )
            .await
            .expect("retrieval should succeed");

        let top = found.candidates.first();
        let rank = found
            .candidates
            .iter()
            .position(|c| c.content.contains(expected));

        println!(
            "{question}\n  top: {}\n  answer at rank: {:?}\n",
            top.map(|c| c.content.chars().take(70).collect::<String>())
                .unwrap_or_else(|| "(nothing)".into()),
            rank
        );

        match rank {
            Some(0) => {}
            Some(n) => failures.push(format!("`{question}` ranked the answer {n} places down")),
            None => failures.push(format!("`{question}` did not find the answer at all")),
        }
    }

    // Ranked first every time is the bar for a handbook this small. If this
    // starts failing, the chunking or the relevance floor is what to look at
    // before the model.
    assert!(failures.is_empty(), "{}", failures.join("\n"));

    cleanup(&db, ctx.org_id).await;
}

#[tokio::test]
#[ignore = "needs OPENAI_API_KEY and makes a real, billable API call"]
async fn the_relevance_floor_rejects_an_unrelated_question() {
    let Some((db, ctx, retriever, knowledge_base_id)) = setup().await else {
        return;
    };

    // Nothing in a school handbook answers this. With a real model the default
    // floor should be what stops a strict agent inventing an answer from the
    // nearest paragraph it could find.
    let found = retriever
        .retrieve(
            &ctx,
            &[knowledge_base_id],
            "ราคาหุ้นของบริษัทเทสลาวันนี้เท่าไหร่",
            &SearchFilters::default(),
            &RetrievalConfig::default(),
        )
        .await
        .expect("retrieval should succeed");

    println!(
        "unrelated question returned {} passages, best score {:?}",
        found.candidates.len(),
        found.candidates.first().and_then(|c| c.vector_score)
    );

    assert!(
        found.is_empty(),
        "the floor of {} let {} unrelated passages through",
        RetrievalConfig::default().min_relevance,
        found.candidates.len()
    );

    cleanup(&db, ctx.org_id).await;
}

/// Ingest the handbook with a real model, and return everything needed to
/// search it. `None` when the environment is not set up for a live run.
async fn setup() -> Option<(Db, TenantCtx, Retriever, KnowledgeBaseId)> {
    let database_url = std::env::var("ANTHOVAI_TEST_DATABASE_URL").ok()?;
    let api_key = std::env::var("OPENAI_API_KEY")
        .ok()
        .filter(|k| !k.is_empty())?;

    let db = Db::connect(&database_url, 3).await.expect("connect");
    db.run_migrations().await.expect("migrate");

    let embedder: Arc<dyn EmbeddingProvider> =
        Arc::new(OpenAiEmbeddings::new(api_key, None, MODEL, DIMENSION).expect("open provider"));
    let model_id = embedder.model_id().to_owned();

    let ctx = seed_tenant(&db).await;
    let storage: Storage = Arc::new(InMemoryStorage::new());

    let knowledge = KnowledgeService::new(
        db.clone(),
        Arc::clone(&storage),
        EmbeddingSettings {
            default_model: model_id,
            dimension: DIMENSION,
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

    let start = knowledge
        .start_upload(
            &ctx,
            knowledge_base_id,
            UploadTarget::File {
                filename: "handbook.md".into(),
                mime_type: Some("text/markdown".into()),
                declared_size: Some(HANDBOOK.len() as i64),
            },
        )
        .await
        .expect("start upload");

    storage
        .put(
            &start.storage_key,
            HANDBOOK.as_bytes().to_vec(),
            "text/markdown",
        )
        .await
        .expect("store");
    knowledge
        .finish_upload(
            &ctx,
            start.document_id,
            &start.storage_key,
            HANDBOOK.len() as i64,
            &anthovai_embeddings::content_hash(HANDBOOK),
        )
        .await
        .expect("finish upload");

    let pipeline = IngestPipeline::new(
        db.clone(),
        Arc::clone(&storage),
        Arc::new(EmbeddingRunner::new(
            Arc::clone(&embedder),
            RunnerConfig::default(),
        )),
        pipeline::chunk_config_from(500, 80),
    );

    let outcome = pipeline
        .run(ctx.org_id, start.document_id, 1)
        .await
        .expect("ingest with a real model");
    println!("indexed {} chunks", outcome.chunks);

    let retriever = Retriever::new(db.clone(), vec![embedder]);
    Some((db, ctx, retriever, knowledge_base_id))
}

async fn seed_tenant(db: &Db) -> TenantCtx {
    let org_id = OrgId::new();
    let workspace_id = WorkspaceId::new();

    let mut system = db.system().await.unwrap();
    sqlx::query("INSERT INTO organizations (id, slug, name) VALUES ($1, $2, $3)")
        .bind(org_id.to_db())
        .bind(format!("live-{}", org_id.to_db().to_lowercase()))
        .bind("Relevance test")
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

/// These runs cost money, so they do not leave rows behind for the next one to
/// trip over.
async fn cleanup(db: &Db, org_id: OrgId) {
    let mut system = db.system().await.unwrap();
    let _ = sqlx::query("DELETE FROM organizations WHERE id = $1")
        .bind(org_id.to_db())
        .execute(system.conn())
        .await;
    let _ = system.commit().await;
}
