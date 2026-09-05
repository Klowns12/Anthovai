# 09 — แผนพัฒนา Milestone 4–9 (ฉบับลงมือทำ)

สถานะ ณ 2026-09-05: Phase A–G เสร็จครบ (รวม F), 532 tests เขียว, flow signup → org → knowledge base → upload (txt/md/pdf/docx/json/csv/html/url) → worker ingest → agent → publish → API key → `POST /v1/chat` ได้คำตอบพร้อม citation ใช้งานได้จริงผ่าน HTTP (embeddings จาก OpenAI จริง, chat ตอบด้วย OpenAI จริงแล้ว — คำถามไทย 3 ข้อได้คำตอบไทยสั้น ๆ ถูกต้อง อ้างอิงหัวข้อที่ถูก; **ราคายังไม่ยืนยัน** ทุกแถวใน `config/models.toml` ตั้งไว้ 0 และไม่มี `priced_on` → production ไม่สตาร์ท dev เตือนทุกครั้ง; PDF ไทยจริงยังไม่ได้ทดสอบ — รอไฟล์จากทีมธุรกิจ, test อยู่ที่ `crates/ingestion/src/parsers/pdf.rs` แบบ `#[ignore]`); observability ครบ (`/internal/ready`, `/internal/metrics`), OpenAPI ที่ `/v1/openapi.json`, Docker image + compose stack, `cargo audit` เขียวและอยู่ใน CI, p95 ทุกอย่างยกเว้น model call ราว 100ms ที่ 50 rps (งบ 400ms)
เอกสารนี้ต่อจาก [08](08-p1-implementation-checklist.md) แต่จัดลำดับใหม่ตามหลักการเดียว: **ทำเส้นแนวดิ่งที่บางที่สุดให้ครบก่อน แล้วค่อยขยายด้านข้าง**

---

## 0. หลักการจัดลำดับ

แผนเดิมใน 08 เรียงตาม layer (storage → ingestion ทุก parser → retrieval → provider → chat) ซึ่งหมายความว่าจะไม่มีอะไร "ถามแล้วตอบได้" จนกว่าจะถึง Milestone 8 และความเสี่ยงเรื่องการประกอบชิ้นส่วน (integration risk) ทั้งหมดจะไปกองอยู่ท้ายสุด

แผนนี้กลับลำดับ:

```
Phase A  Storage + Knowledge + Job queue + Worker          (โครง Milestone 4)
Phase B  Parser แค่ text/markdown + chunk + embed(fake) + index   (Milestone 5 แบบบาง)
Phase C  Retrieval SQL บน pgvector + cross-tenant retrieval test  (Milestone 6)
Phase D  ChatService + POST /v1/chat + playground ด้วย FakeChatProvider (Milestone 8)
         ───── จุดนี้ระบบ "ถามตอบได้ครบวงจร" โดยไม่ต้องใช้ provider key ─────
Phase E  Parser ที่เหลือ: PDF, DOCX, HTML/URL, JSON, CSV        (Milestone 5 เต็ม)
Phase F  ต่อ provider จริงเมื่อได้ key + live tests             (Milestone 7)
Phase G  Observability, OpenAPI, Docker, load test              (Milestone 9)
```

เหตุผลสามข้อ:
1. **Integration risk ถูกกำจัดตั้งแต่ Phase D** — ถ้า retrieval, prompt, citations, usage, conversation ประกอบกันไม่ลงตัว เราจะรู้ตอนที่ยังแก้ถูก
2. **ไม่ต้องรอ provider key** — `FakeChatProvider` และ `FakeEmbeddingProvider` มีอยู่แล้วใน `testkit` ใช้ได้ทั้ง dev และ demo; ตอน key มา (Phase F) เปลี่ยนแค่ config
3. **Demo ได้เร็ว** — หลัง Phase D สาธิตให้ลูกค้าดูได้ว่า upload → ถาม → ได้คำตอบพร้อม citation แม้คำตอบจะมาจาก fake provider ที่ echo chunk กลับมา

ข้อแลกเปลี่ยนที่รับไว้: PDF ซึ่งเป็น use case จริงของลูกค้าจะมาช้ากว่าแผนเดิมประมาณ 1 สัปดาห์ แต่ตอนที่มาจะมี pipeline ที่พิสูจน์แล้วรอรับอยู่

---

## 1. สิ่งที่มีอยู่แล้ว (ไม่ต้องเขียนใหม่)

| Crate | มีแล้ว | ยังขาด |
|-------|--------|--------|
| `storage` | `ObjectStorage` trait, `StorageKey` builder, `InMemoryStorage` | S3/MinIO impl, local-disk impl |
| `knowledge` | `DocumentStatus`, `SourceType` + magic bytes, struct ต่าง ๆ | repositories, service, upload |
| `jobs` | `Job`, `JobPayload`, `JobError`, `JobHandler`, backoff policy | `JobQueue` (SQL), `WorkerRuntime`, LISTEN/NOTIFY |
| `ingestion` | `Parser` trait, `chunk()` พร้อม header/overlap, `normalize()`, error codes | parser จริงทุกตัว, `IngestPipeline` |
| `embeddings` | `EmbeddingProvider` trait, `plan_batches`, `content_hash` | batching runner ที่ยิง provider พร้อม hash-reuse |
| `retrieval` | RRF, MMR, budget selection, `ContextBuilder`, citation parser, `rank()` | SQL vector/keyword search, `Retriever` ที่ต่อ DB |
| `rag` | `short_circuit`, `fallback_output`, `model_output` | `ChatService`, `PostProcessor`, persist |
| `conversation` | structs, `history_window` | repositories |
| `usage` | structs, `check_message_quota`, `period_start` | repositories, rollup |
| `inference` | router, policy, registry, circuit breaker ครบ | — (พร้อมใช้) |
| `providers/*` | chat non-stream + OpenAI embeddings ครบ | streaming (P2) |
| `testkit` | `FakeChatProvider`, `FakeEmbeddingProvider`, `db_test!` | — |

**ข้อควรระวังที่พิสูจน์แล้ว** (จาก 07 §2): FK ไม่เคารพ RLS → ทุกจุดที่ insert แถวอ้างถึงตารางลูกค้า (documents→knowledge_bases, chunks→documents, conversations→agents) ต้องตรวจความเป็นเจ้าของด้วย query ที่ scope tenant ก่อน

---

## 2. Dependencies ที่ต้องเพิ่ม (ตัดสินใจแล้ว)

| ใช้ทำอะไร | Crate | เหตุผลที่เลือก |
|-----------|-------|----------------|
| pgvector ↔ sqlx | `pgvector = { version = "0.4", features = ["sqlx"] }` | binding ทางการ, รองรับ sqlx 0.8 |
| Object storage | `object_store = { version = "0.12", features = ["aws"] }` | abstraction เดียวใช้ได้ทั้ง local disk (dev), MinIO และ S3/R2 (prod) โดยไม่ต้องดึง aws-sdk ทั้งชุด |
| Token count | `tiktoken-rs` | ใช้ cl100k_base เป็น approximation กลาง; แทน `estimate_tokens()` เมื่อมี |
| PDF | `pdf-extract` | pure Rust ไม่ต้องมี binary ภายนอก; **ต้องทดสอบกับ PDF ภาษาไทยจริงก่อนตัดสินใจขั้นสุดท้าย** (ดู §8 ความเสี่ยง) |
| DOCX | `zip` + `quick-xml` | DOCX คือ zip ที่มี `word/document.xml`; ไม่ต้องใช้ crate เฉพาะทางที่ดูแลน้อย |
| HTML | `scraper` | selector-based, ตัด nav/footer ได้ |
| CSV | `csv` | มาตรฐาน |
| Language detect | `whatlang` | เบา, รองรับไทย |
| OpenAPI | `utoipa` + `utoipa-axum` | generate จาก type จริง ป้องกัน contract เพี้ยน |
| Metrics | `metrics` + `metrics-exporter-prometheus` | ใช้กับ `/internal/metrics` |
| Multipart | (มีใน axum feature `multipart` แล้ว) | — |

ไม่เพิ่ม: Redis (P2), Qdrant, testcontainers (ใช้ compose + env var ตามที่ทำอยู่แล้วได้ผลดี)

---

## 3. Phase A — Storage, Knowledge, Job Queue, Worker (ประมาณ 4 วัน)

เป้าหมาย: อัปโหลดข้อความหรือไฟล์ → เก็บลง object storage → document อยู่สถานะ `queued` → worker หยิบ job ไปได้ (handler ยัง stub)

### A.1 `storage`: S3 และ local impl
- `ObjectStoreStorage` ห่อ `object_store::ObjectStore` (dyn) — ตัวเดียวรองรับ `LocalFileSystem`, `AmazonS3` (MinIO ใช้ endpoint override + `allow_http`)
- Factory `Storage::from_settings(&StorageSettings)` เลือกจาก `provider = "local" | "s3"`
- เพิ่ม `provider = "local"` + `local_path` ใน `StorageSettings` และ `config/default.toml` (dev ไม่ต้องรัน MinIO ก็ได้)
- Test: contract test ชุดเดียวรันกับทั้ง `InMemoryStorage` และ `LocalFileSystem` (ใน temp dir) — put/get/delete/delete_prefix/isolation ของ prefix

### A.2 `knowledge`: repositories + service
Repositories (ทุกตัวรับ `TenantDb`, bind tenant จาก transaction):
- `insert_knowledge_base`, `list_knowledge_bases(workspace?)`, `find_knowledge_base`, `soft_delete_knowledge_base`
- `insert_document(status=uploading)`, `find_document`, `list_documents(kb, status?)`, `set_document_status(progress, error)`, `bump_document_version`, `soft_delete_document`
- `update_kb_counters(kb, delta_bytes, delta_docs)`
- **ownership check** ก่อน insert document: `knowledge_bases` ต้องอยู่ใน tenant (FK ไม่ช่วย)

Service `KnowledgeService`:
- `create_knowledge_base(ctx, workspace, name)` — ตั้ง `embedding_model` และ `embedding_dim` จาก `EmbeddingSettings` ณ เวลาสร้าง (ล็อกต่อ KB ตลอดอายุ)
- `begin_upload(ctx, kb, title, source_type, size, mime)` → ตรวจ plan limits (max_file_bytes, documents_per_kb, storage_bytes) → insert document `uploading` → คืน `StorageKey`
- `complete_upload(ctx, document, bytes_written, content_hash)` → status `queued` → enqueue `IngestDocument`
- `retry(ctx, document)` — เฉพาะ `failed`; enqueue ใหม่
- `delete_document` → status `deleted` + enqueue `DeleteDocumentChunks`
- Re-upload = `bump_document_version` + `begin_upload` เดิม; chunks version เก่ายังใช้ได้จนกว่า version ใหม่ `ready` (swap ทำใน Phase B)

### A.3 `jobs`: `JobQueue` บน PostgreSQL
- `enqueue(system_db, org_id, payload) -> JobId` — insert `jobs` + `NOTIFY jobs_new`
- `fetch_batch(worker_id, n)`:
  ```sql
  UPDATE jobs SET status='running', locked_by=$1, locked_at=now(), attempts=attempts+1
  WHERE id IN (SELECT id FROM jobs WHERE status='pending' AND run_after <= now()
               ORDER BY priority, run_after LIMIT $2 FOR UPDATE SKIP LOCKED)
  RETURNING *
  ```
- `complete(job_id)`, `fail(job, error)` → ถ้า `should_retry()` set `pending` + `run_after = now()+backoff` ไม่งั้น `dead`; `Permanent` error → `dead` ทันทีไม่ retry
- `reap_stale(locked_older_than)` — worker ตายกลางทาง → คืนเป็น `pending`
- `WorkerRuntime { queue, handlers, concurrency }`: loop `LISTEN jobs_new` + poll interval fallback; semaphore คุม concurrency; graceful shutdown รอ job ค้างจบภายใน 60s
- Handler ทำงานภายใต้ `db.tenant_for(job.org_id)` เสมอ — worker ไม่เคยแตะข้อมูลข้าม tenant นอกจากตาราง `jobs`
- Tests (DB จริง): สอง worker ดึงงานพร้อมกันไม่ได้งานซ้ำ; backoff ตามลำดับ 30s/2m/10m; permanent → dead; reap_stale คืนงาน

### A.4 Upload endpoints
- Dashboard: `GET/POST /dashboard/v1/knowledge_bases`, `GET/PATCH/DELETE /knowledge_bases/{id}`, `GET /knowledge_bases/{id}/documents`, `POST /dashboard/v1/documents` (multipart: `knowledge_base_id` + `file` | `text`+`title`), `GET /documents/{id}`, `POST /documents/{id}/retry`, `DELETE /documents/{id}`
- Public: เหมือนกันภายใต้ `/v1` ด้วย scope `knowledge:read` / `knowledge:write`
- Multipart: ตรวจ `Content-Length` เทียบ plan **ก่อน** อ่าน body; stream ลง storage พร้อมนับ bytes + sha256 ระหว่างทาง; magic bytes จาก 4KB แรก; `url` field ยังไม่รับใน Phase นี้ (มาพร้อม HTML parser ใน Phase E เพื่อให้ SSRF guard มาพร้อมกัน)
- Body limit layer ต้องแยกจาก JSON 1MB: ใส่ `RequestBodyLimitLayer` ระดับ route ให้ upload ตาม `max_file_bytes` ของ plan สูงสุด (200MB) แล้วให้ service ตัดตาม plan จริง

### A.5 Worker binary
- `apps/worker/main.rs` สร้าง `Db`, `Storage`, `JobQueue`, register `IngestDocumentHandler` (Phase B) และ `DeleteDocumentChunksHandler`, `PurgeDeletedChunksHandler` (scheduled ทุกชั่วโมงผ่าน job ที่ enqueue ตัวเองซ้ำ)

### Acceptance Phase A
- HTTP test: สร้าง KB → `POST /documents` ด้วย text → 202 `queued` → worker (รันใน test ด้วย stub handler ที่ set `ready`) → `GET /documents/{id}` เห็น `ready`
- HTTP test: อัปโหลดเกิน `max_file_bytes` ของ free plan → 413 โดยไม่อ่าน body
- HTTP test: tenant B อ้าง `knowledge_base_id` ของ A ตอน upload → 404 `knowledge_base_not_found`
- DB test: job queue concurrency ตามข้างบน

---

## 4. Phase B — Ingestion แบบบาง (ประมาณ 2 วัน)

เป้าหมาย: text/markdown → chunk → embed (fake หรือ OpenAI ตาม config) → `document_chunks` พร้อม vector

### B.1 `ingestion`
- `TextParser` (txt) และ `MarkdownParser` (md → heading blocks ด้วย `pulldown-cmark`)
- `IngestPipeline::run(job)` ตามลำดับสถานะ `processing → chunking → embedding → indexing → ready` อัปเดต `progress` ทุกขั้น; error → `failed` พร้อม `error_code` จาก `error_codes`
- แทน `estimate_tokens()` ด้วย `tiktoken-rs` cl100k (เก็บ estimate ไว้เป็น fallback ถ้า tokenizer โหลดไม่ได้)

### B.2 `embeddings`: `EmbeddingRunner`
- รับ `Vec<ChunkDraft>` → คำนวณ `content_hash` → query `document_chunks` ใน tenant ด้วย hash เดิม (reuse vector) → เหลือเท่าไรจึงยิง provider เป็น batch 64, concurrency 4, retry 429 ตาม `Retry-After`
- คืน `Vec<(ChunkDraft, Vec<f32>, reused: bool)>` และ `embedding_tokens` สำหรับ usage record kind `embedding_ingest`

### B.3 `retrieval` (ส่วน index): `chunk_repo`
- `insert_chunks(tenant_db, kb, doc, version, chunks_with_vectors)` ใช้ `pgvector::Vector`; batch insert 100 แถว/statement
- `swap_version(tenant_db, doc, new_version)` — mark chunks version เก่า `deleted_at=now()` ใน transaction เดียวกับ `documents.current_version = new`
- `purge_deleted_chunks(tenant_db, older_than_24h)`

### B.4 Provider selection ใน worker/api
- `EmbeddingProviderFactory::from_settings`: ถ้า `OPENAI_API_KEY` ว่างและ `ANTHOVAI_ENV != production` → `FakeEmbeddingProvider(1536)` พร้อม warn log ชัด ๆ; production ที่ไม่มี key → **fail fast ตอน start**
- `knowledge_bases.embedding_model` ของ KB ที่สร้างช่วง fake จะเป็น `fake:hash-1536` — query จะ embed ด้วย model เดียวกันจาก field นี้เสมอ จึงถูกต้องในตัวเอง; ตอนสลับเป็น OpenAI ต้อง re-embed KB เหล่านั้น (job `ReembedKnowledgeBase` มี payload อยู่แล้ว)

### Acceptance Phase B
- อัปโหลด markdown 3 หัวข้อ → `ready` → `GET /dashboard/v1/documents/{id}/chunks` เห็น chunks พร้อม `heading_path`
- re-upload → version 2 ready → chunks version 1 มี `deleted_at`; ระหว่าง processing ยังค้นเจอ version 1
- upload เนื้อหาเดิมซ้ำ → `reused = true` ทุก chunk ไม่เรียก provider (นับจาก fake provider call counter)

---

## 5. Phase C — Retrieval บน pgvector (ประมาณ 2 วัน)

### C.1 `retrieval::Retriever`
```rust
pub struct Retriever { embeddings: EmbeddingRegistry /* model_id -> provider */ }
impl Retriever {
  pub async fn retrieve(&self, db: &mut TenantDb, agent: &ResolvedAgent, query: &str, filters: &Filters, cfg: &RetrievalConfig) -> Result<Vec<Candidate>>
}
```
- จัดกลุ่ม KB ของ agent ตาม `embedding_model` → embed query หนึ่งครั้งต่อ model
- Vector SQL (ตาม 03 §B.3) + `WHERE tenant_id = $1 AND knowledge_base_id = ANY($2) AND deleted_at IS NULL` + optional `document_id = ANY($3)`
- Keyword SQL ด้วย `plainto_tsquery('simple', $q)`
- ส่งเข้า `rank()` ที่มีอยู่แล้ว → `ContextBuilder`
- เติม `page`/`title`/`url` จาก `metadata` jsonb ลง `Source` (ตอนนี้ `page_of()` คืน None — เปลี่ยนตรงนี้)

### C.2 Index sanity
- Test ที่รัน `EXPLAIN (FORMAT JSON)` แล้ว assert ว่าใช้ `chunks_embedding_idx` (HNSW) ไม่ใช่ seq scan เมื่อมี ≥ 1,000 chunks (seed ใน test)
- ตั้ง `SET LOCAL hnsw.ef_search = 40` ใน tenant transaction ของ retrieval

### C.3 Cross-tenant retrieval test (เพิ่มเข้า CI job `database`)
- org A และ B อัปโหลดข้อความที่คล้ายกันมาก (ต่างกันคำเดียว) → ถามด้วย ctx ของ A → ต้องไม่มี chunk ของ B เลย ทั้งทาง vector และ keyword
- ทดสอบ query ที่ตั้งใจลืม `knowledge_base_id` filter ผ่าน `TenantDb` ของ A → RLS ยังกันได้

### Acceptance Phase C
- corpus 3 เอกสารภาษาไทย ถาม "หลักสูตร Rust กี่สัปดาห์" ได้ chunk ถูกใน top-3 ด้วย fake embeddings (bag-of-words) — ถ้าไม่ผ่านด้วย fake ให้ mark `#[ignore]` และผูกกับ live embeddings ใน Phase F แทน (fake ไม่ใช่ตัววัดคุณภาพ)

---

## 6. Phase D — ChatService และ `/v1/chat` (ประมาณ 3 วัน)

### D.1 `conversation` + `usage` repositories
- conversations: `get_or_create(ctx, agent, conversation_id?, external_user_id?)` — conversation ต้องอยู่ใน tenant **และ** เป็นของ agent เดียวกัน; `append_messages`, `list_recent(conversation, limit)`, `list_conversations(agent?, external_user?, cursor)`, `delete_conversation` (hard delete สำหรับ PDPA)
- usage: `insert_record`, `increment_counter(period)`, `get_counters(period)`

### D.2 `rag::ChatService` ตาม 06 §6
ลำดับ: `load_published` → `check_message_quota` → `get_or_create conversation` → `guardrails.check_input` → `retriever.retrieve` → `short_circuit` → `PromptBuilder` (org name, วันที่จาก Clock) → `router.chat` → `PostProcessor` (citations, grounded, `check_output` leak) → persist (messages + usage) → `ChatOutput`
- persist แบบ synchronous ใน P1 (ง่ายกว่าและ test ได้แน่นอน); ย้ายไป background + retry ใน P2 เมื่อมี metrics วัด latency จริง
- `RoutingHints.context_tokens` = tokens ของ system prompt + history + question

### D.3 Endpoints
- `POST /v1/chat` (scope `chat`) ตาม 05 §3.1 ครบทุก field ยกเว้น `options.model_policy` override (P2)
- `POST /dashboard/v1/agents/{id}/test` ใช้ `load_draft` + `debug: true` คืน `retrieval_debug` (chunk ids + scores)
- `GET/DELETE /v1/conversations`, `GET /v1/conversations/{id}`
- `GET /v1/usage` แบบ totals + quota
- Response header `X-RateLimit-*` จาก verdict ใน `ApiKeyAuth`
- `model.provider`/`model.name` โผล่เฉพาะ plan ≥ business (`Feature::RevealProviderInResponse` มีแล้ว)

### D.4 Wiring
- `AppState` เพิ่ม `chat: Arc<ChatService>`, `knowledge`, `storage`, `queue`
- `apps/api/main.rs` ประกอบ `ModelRouter` จาก `config/models.toml` + providers ที่มี key; ไม่มี key เลย + ไม่ใช่ production → `FakeChatProvider` พร้อม warn

### Acceptance Phase D = **P1 Demo ตาม 08 §Milestone 8** ด้วย fake providers
1. signup → org → API key (มี HTTP test อยู่แล้ว)
2. สร้าง agent strict + citations, อัปโหลด markdown, ผูก KB, publish
3. `POST /v1/chat` "หลักสูตร Rust ใช้เวลาเรียนกี่สัปดาห์?" → `FakeChatProvider::answering("12 สัปดาห์ [1]")` → `grounded=true`, `sources[0]` ชี้ chunk ถูก
4. ถามนอกเอกสาร → fallback, `grounded=false`, **ไม่มี usage record kind `chat` ที่มี provider** (พิสูจน์ว่าไม่เรียก LLM)
5. key ของ org อื่น → 404 `agent_not_found`
6. quota: set plan free แล้วยิงจน `messages_per_month` เต็ม → 429 `quota_exceeded` และไม่เรียก provider
7. conversation: ส่ง `conversation_id` กลับ → history เข้า prompt (ตรวจจาก echo ของ fake)

---

## 7. Phase E — Parser ครบชุด (ประมาณ 4 วัน)

ทำทีละตัว แต่ละตัวมี fixture จริงใน `crates/ingestion/tests/fixtures/` และ test ว่า parse → chunk ได้จำนวนสมเหตุสมผล

| ลำดับ | Parser | จุดที่ต้องระวัง |
|-------|--------|-----------------|
| E.1 | **PDF** (`pdf-extract`) | รันใน `spawn_blocking` + timeout 120s; จำกัด 2,000 หน้า; **ทดสอบ PDF ไทยที่ฝัง font แบบ subset ก่อน** — ถ้าถอด glyph ไม่ได้ ต้องตัดสินใจเรื่อง sidecar `pdftotext` (poppler) ใน container แยก; scan PDF → `no_extractable_text` |
| E.2 | **JSON** | schema detection ตาม 03 §A.6: array of objects / object of objects / single; record = chunk; metadata `json_path`, `record_key` |
| E.3 | **CSV** | header row = field names; 1 row = 1 record; จำกัด 100k rows |
| E.4 | **DOCX** | `zip` → `word/document.xml` → `w:p` เป็น paragraph, `w:pStyle` Heading1-6 เป็น heading; ตรวจว่าเป็น DOCX จริงจากโครง zip ไม่ใช่แค่ magic bytes |
| E.5 | **HTML/URL** | `reqwest` + SSRF guard ตาม 07 §5 (resolve DNS แล้วเช็ค private range, ไม่ follow redirect ไป private, timeout 15s, 10MB) → `scraper` ตัด `nav, footer, script, style, aside` → เก็บ `title` และ `url` ลง metadata; เปิดรับ field `url` ใน upload endpoint ตอนนี้ |

Acceptance: ทุก parser มี fixture test; PDF ไทย 1 ไฟล์จริงจากลูกค้าเป้าหมาย (ขอจากทีมธุรกิจ) parse ได้ text ที่อ่านออก

---

## 8. Phase F — Provider จริง (ประมาณ 2 วัน, เมื่อได้ key)

- ยืนยันชื่อ model ของ OpenAI และราคาปัจจุบันจาก docs → เติม `config/models.toml` แล้ว `enabled = true`
- Live tests `#[ignore]` ใน `providers/*/tests/live.rs`: chat ตอบได้, usage ไม่เป็น 0, `stop_reason` map ถูก; embeddings คืน 1536 มิติ
- รัน P1 demo ซ้ำด้วย provider จริง → นี่คือจุดวัดคุณภาพ retrieval ครั้งแรกที่มีความหมาย (Phase C acceptance ที่ `#[ignore]` ไว้เปิดตรงนี้)
- Re-embed KB ที่สร้างด้วย fake: enqueue `ReembedKnowledgeBase` ทุก KB ที่ `embedding_model LIKE 'fake:%'`
- Cost sanity: usage record ต้องมี `cost_usd_micro > 0` และตรงกับ registry

---

## 9. Phase G — พร้อม staging (ประมาณ 3 วัน)

- `tracing` span ต่อ request มี `request_id, tenant_id, agent_id`; redaction layer ห้าม log `Authorization`/cookie/message content ที่ INFO
- `metrics` + `/internal/metrics` ตาม 06 §10; `/internal/health` ตรวจ DB, storage, provider circuit, queue depth
- `utoipa` → `/v1/openapi.json` + snapshot test
- Dockerfiles multi-stage (api, worker), compose ใช้ image; README run ใน 5 คำสั่ง
- Load test `oha` 50 rps `/v1/chat` ด้วย fake → p95 excluding LLM < 400ms
- `cargo audit`, `gitleaks` ใน CI
- ทบทวน checklist 07 §12 ทีละข้อ

---

## 10. ความเสี่ยงที่ต้องจัดการเชิงรุก

| ความเสี่ยง | ผลกระทบ | แผน |
|-----------|---------|-----|
| **Fake embeddings ทำให้ retrieval tests หลอก** | เชื่อว่า retrieval ดีทั้งที่ยังไม่เคยวัดจริง | **จัดการแล้ว**: เทสต์ที่ใช้ fake เป็น structural ล้วน (isolation, filter, budget) ส่วน relevance อยู่ใน `tests/relevance.rs` ที่ `--ignored` และใช้ model จริง — พบตอน Phase C ว่าเทสต์สองข้อแอบพึ่ง ranking ของ fake จึงเขียนใหม่ |
| **Keyword search ไม่ทำงานกับภาษาไทย** — `to_tsvector('simple')` แยกคำตามช่องว่าง ภาษาไทยไม่มีช่องว่าง → tsvector ไทยเกือบไร้ค่า | hybrid ลดเหลือ vector อย่างเดียวสำหรับไทย | **ยืนยันแล้ว 2026-09-05** วัดด้วย model จริง: vector search รับภาระได้ครบ คำถามไทย 5 ข้อเจอคำตอบอันดับ 1 ทุกข้อ → ยอมรับใน P1; P2 พิจารณา `pg_trgm` หรือตัดคำด้วย ICU |
| **Multipart streaming + plan limit** ทำผิดแล้วอ่าน body ทั้งก้อนเข้า memory | OOM จากไฟล์ใหญ่ | **จัดการแล้ว**: ตรวจ Content-Length ก่อนอ่าน + นับ byte ระหว่าง stream (header เป็นคำกล่าวอ้าง ไม่ใช่หลักประกัน) + body limit ต่อ route |
| **Worker ตายกลาง ingestion** ทิ้ง document ค้าง `processing` | ลูกค้าเห็นค้างตลอดกาล | **จัดการแล้ว**: `reap_stale` คืนงาน + pipeline idempotent (`discard_version` ก่อน insert ใหม่) |
| **HNSW index ไม่ถูกใช้เมื่อมี tenant filter** — pgvector ไม่กรองตาม tenant ใน index planner อาจเลือก scan แทน | latency โตเงียบ ๆ ตอนมีลูกค้าเยอะ | มี `EXPLAIN` test คุมอยู่ (เคยเห็น flake หนึ่งครั้งตอนสถิติยังไม่นิ่ง); ถ้าโตขึ้นแล้วมีปัญหา ให้พิจารณา partial index ต่อ tenant หรือ partition |
| **FK ข้าม tenant** ที่จุดใหม่ (documents→kb, chunks→doc, conversations→agent) | ช่องโหว่แบบเดียวกับที่พบใน M2 | ownership check ทุกจุด + test ใน isolation suite ต่อ Phase |
| **Scope creep ไป streaming/tools/webhooks** | P1 ไม่จบ | ทุกอย่างนอกตารางนี้ = P2 ตามที่ 08 ระบุ |

---

## 11. ประมาณการและลำดับส่งงาน

| Phase | วัน | ผลลัพธ์ที่มองเห็นได้ |
|-------|-----|------------------------|
| A | 4 | อัปโหลดแล้วเห็นสถานะเปลี่ยน, worker ทำงาน |
| B | 2 | เห็น chunks ใน dashboard API |
| C | 2 | retrieval คืน chunk ถูกต้อง + isolation test ผ่าน |
| D | 3 | **ถาม–ตอบครบวงจร (fake provider)** — demo ได้ |
| E | 4 | PDF/DOCX/JSON/CSV/URL ใช้ได้จริง |
| F | 2 | คำตอบจาก Claude/OpenAI จริง |
| G | 3 | staging พร้อม |
| รวม | ~20 วันทำงาน | |

จุดตรวจรับที่แนะนำ 2 จุด: **หลัง Phase D** (ตัดสินใจเรื่อง UX ของคำตอบ/citation ก่อนลงทุน parser) และ **หลัง Phase F** (ประเมินคุณภาพ retrieval จริงก่อนไป staging)

---

## 12. สิ่งที่ต้องการจากฝั่งธุรกิจ (ไม่บล็อกงานพัฒนา แต่มีเดดไลน์)

| ต้องการ | ใช้ใน | ต้องได้ก่อน |
|---------|-------|-------------|
| PDF ภาษาไทย 2–3 ไฟล์จริงจากลูกค้าเป้าหมาย (หลักสูตร, คู่มือ) | E.1 | เริ่ม Phase E |
| OpenAI API key + Anthropic API key | F | เริ่ม Phase F |
| ตัดสินใจ: อนุญาต `FakeChatProvider` บน staging สำหรับ demo ภายในหรือไม่ | D.4 | ก่อน deploy staging |
| ชื่อองค์กรและ tone สำหรับ default `instructions` template ใน onboarding | D.3 | ไม่บล็อก |
