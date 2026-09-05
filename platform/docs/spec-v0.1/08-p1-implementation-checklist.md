# 08 — P1 Implementation Checklist

**เป้าหมาย P1:** สร้าง Agent → อัปโหลด PDF → ถามผ่าน `POST /v1/chat` ด้วย API key ได้คำตอบพร้อม citations โดย OpenAI และ Claude เป็น provider และ tenant isolation ทำงานจริง

ยังไม่ทำใน P1: Dashboard UI สวย (ใช้หน้าเรียบ ๆ หรือ curl ก็ได้), billing, webhooks, team members, streaming (อยู่ P2 แต่วาง trait ไว้)

Definition of Done ของทุก task: โค้ด + unit/integration test + `clippy -D warnings` ผ่าน + อัปเดตเอกสาร 04/05 ถ้า schema/API เปลี่ยน

---

## Milestone 0 — Workspace Bootstrap (วัน 1–2)

- [x] 0.1 สร้าง Cargo workspace ตาม 06 §1 ครบทุก crate (ว่างได้) + `rust-toolchain.toml`
- [x] 0.2 `docker-compose.yml`: postgres:16 + pgvector, minio, (redis ปิดไว้)
- [x] 0.3 `config/default.toml` + loader (figment) + env override; `Settings` struct ใน `core`
- [x] 0.4 `migrations/0001_init.sql` ตาม 04 §3 ทั้งหมด (รวมตาราง P4 ที่ว่าง) + `sqlx migrate run` ผ่าน
- [x] 0.5 CI: fmt, clippy, test, deny, sqlx offline check
- [x] 0.6 `testkit`: testcontainers PG+pgvector fixture, `FakeChatProvider`, `FakeEmbeddingProvider`
- **Acceptance:** `cargo test --workspace` เขียวบน CI ด้วย DB จริงใน container

## Milestone 1 — Core Types, DB, Tenant (วัน 3–5)

- [x] 1.1 `core`: typed ids (`typed_id!` macro, prefix serialize, sqlx Type), `TenantCtx`, `Actor`, `Plan`, `DomainError`
- [x] 1.2 `db`: `Db`, `TenantDb` (SET LOCAL app.tenant_id), `SystemDb`
- [x] 1.3 RLS policies migration `0002_rls.sql` + roles `anthovai_app`, `anthovai_system`
- [x] 1.4 `tenant`: organizations/workspaces/memberships repos + services (create org auto-create Default workspace)
- [x] 1.5 `config/plans.toml` + `Plan::limits()`
- [x] 1.6 Test: RLS block cross-tenant read เมื่อ query ตรงโดยไม่มี WHERE
- **Acceptance:** สร้าง org 2 ราย, query ผ่าน `TenantDb` ของ A ไม่เห็นข้อมูล B แม้ตั้งใจลืม WHERE

## Milestone 2 — Auth (วัน 6–8)

- [x] 2.1 `auth`: argon2id password hash/verify, session create/verify/revoke (hashed id), cookie helpers
- [x] 2.2 API key generate (`av_live_`/`av_test_`), sha256 hash, verify + in-memory cache 60s, scopes, agent scope
- [x] 2.3 RBAC: `Role`, `Permission`, `ctx.require()`
- [x] 2.4 `api` extractors: `ApiKeyAuth → TenantCtx`, `SessionAuth + X-Org-Id → TenantCtx`
- [x] 2.5 Dashboard endpoints: signup, login, logout, me, organizations create, workspaces CRUD, api_keys create/list/revoke/rotate
- [x] 2.6 Login rate limit (in-memory per instance P1), Origin check for non-GET
- [x] 2.7 Tests: invalid/expired/revoked key → 401 ถูก code; scope missing → 403; secret แสดงครั้งเดียว
- **Acceptance:** curl signup → login → create org → create API key → เรียก `/v1/agents` ด้วย key ได้ 200 (list ว่าง)

## Milestone 3 — Agents (วัน 9–11)

- [x] 3.1 `agent`: `AgentConfig` struct + serde + JSON Schema validation (04 §4) + plan gating ของ `model_policy`
- [x] 3.2 Repos: agents, agent_versions; service create (draft v1), update (new draft version), publish, rollback, pause/resume/archive
- [x] 3.3 `load_published(ctx, agent_id)` ตรวจ AgentScope ของ key, status active/paused (paused → 403 `agent_paused`)
- [x] 3.4 Dashboard endpoints 05 §9.3 (ยกเว้น test/stream) + Public `GET /v1/agents`, `/v1/agents/{id}`
- [x] 3.5 Tests: version increments, publish pointer, archived → 410 บน public API
- **Acceptance:** สร้าง agent ผ่าน dashboard API, publish, อ่านผ่าน public API เห็น status active

## Milestone 4 — Knowledge, Storage, Jobs (วัน 12–15)

- [x] 4.1 `storage`: `ObjectStorage` trait + S3 impl (opendal หรือ aws-sdk-s3), key builder `tenant/{org}/{kb}/{doc}/v{n}/original`
- [x] 4.2 `knowledge`: knowledge_bases CRUD, documents create (status uploading→queued), status machine enforcement, version bump on re-upload
- [x] 4.3 `jobs`: `JobQueue` (enqueue, fetch FOR UPDATE SKIP LOCKED, complete, fail+backoff, dead), `JobHandler`, `WorkerRuntime`, LISTEN/NOTIFY
- [x] 4.4 Upload endpoint multipart (file | text) พร้อม Content-Length check + stream count, extension detect, plan limits → 202 · **`url` เลื่อนไป Phase E** พร้อม HTML parser เพื่อให้ SSRF guard มาคู่กัน
- [ ] 4.5 URL fetch guard: SSRF checks ตาม 07 §5 — **Phase E** (มาพร้อม HTML parser)
- [x] 4.6 Dashboard/Public endpoints 05 §6, §9.4 (ยกเว้น chunks/events → poll GET)
- [x] 4.7 `apps/worker` binary: load config, register handlers, graceful shutdown
- [x] 4.8 Tests: job retry/backoff/dead/reap; upload เกิน plan → 413 (ทั้งจาก header และจาก stream); cross-tenant kb → 404 · private IP URL อยู่ใน Phase E
- **Acceptance:** อัปโหลด PDF → document `queued` → worker หยิบ job (handler ยัง stub ที่ set `ready`)

## Milestone 5 — Ingestion Pipeline (วัน 16–20)

- [~] 5.1 Parsers: **TXT/MD เสร็จแล้ว** · PDF, DOCX, HTML/URL, JSON, CSV อยู่ใน Phase E (upload ปฏิเสธชนิดที่ยังอ่านไม่ได้ตั้งแต่ต้นทาง)
- [x] 5.2 Normalizer (NFC, whitespace, hyphenation) + language detect
- [~] 5.3 Chunkers: recursive text ด้วย `tiktoken-rs` + structure-aware สำหรับ MD + contextual header **เสร็จ** · record chunker (JSON/CSV) อยู่ใน Phase E
- [x] 5.4 `embeddings`: `EmbeddingProvider` trait, batching (64, concurrency 4), retry 429, content_hash reuse
- [x] 5.5 `providers/openai` embeddings impl · **ยืนยันกับ OpenAI จริงแล้ว** (`text-embedding-3-small`, 1536 มิติ)
- [x] 5.6 Index: insert chunks (embedding 1536, metadata), version swap ใน transaction, mark old deleted, update documents counters + kb storage_bytes
- [x] 5.7 `IngestPipeline::run` อัปเดต status/progress ทุกขั้น, FAILED พร้อม error_code (`no_extractable_text`, `fetch_failed`, `parse_timeout`, `embedding_failed`)
- [x] 5.8 Cleanup jobs: purge_deleted_chunks
- [x] 5.9 Tests: chunker sizes/overlap, hash reuse ไม่เรียก embed ซ้ำ, pipeline end-to-end บน DB จริง (version swap, failure ไม่ทำลายเวอร์ชันเดิม, cross-tenant ไม่ reuse vector ข้ามกัน)
- **Acceptance (Phase B):** อัปโหลด Markdown ภาษาไทย → READY พร้อม chunks ที่มี heading path และ vector 1536 มิติจริง ✅ · เกณฑ์ PDF ย้ายไป Phase E

## Milestone 6 — Retrieval (วัน 21–23)

- [x] 6.1 `retrieval`: vector search (pgvector cosine, tenant+kb filter), keyword search (tsvector simple), RRF (k=60), MMR (λ=0.7), min_relevance threshold, token budget selection
- [x] 6.2 HNSW + GIN indexes ตรวจ `EXPLAIN` ใช้ index จริง
- [x] 6.3 Multi-KB ต่าง embedding model → embed query ต่อ model (P1 อนุญาต model เดียวต่อ org ก็ได้ แต่โค้ดรองรับ)
- [x] 6.4 `ContextBuilder`: sources numbering, escape `<>`, budget
- [x] 6.5 Tests: RRF ordering, MMR ลด duplicate, threshold → empty, **cross_tenant_isolation_test (retrieval)** · เพิ่ม `tests/relevance.rs` (`--ignored`) สำหรับวัดคุณภาพจริงด้วย model จริง
- **Acceptance ✅** วัดด้วย `text-embedding-3-small` จริง (2026-09-05): คำถามภาษาไทย 5 ข้อ รวมคำถามภาษาอังกฤษที่ถามเนื้อหาไทย เจอย่อหน้าที่ตอบได้ที่ **อันดับ 1 ทุกข้อ** และคำถามนอกเรื่องถูก `min_relevance = 0.25` ปฏิเสธทั้งหมด — ยืนยันด้วยว่า vector search รับภาระภาษาไทยได้จริงตามที่คาดไว้ใน 03 (keyword search ภาษาไทยยังใช้ไม่ได้)

## Milestone 7 — Inference & Providers (วัน 24–27)

- [ ] 7.1 `inference`: `ChatProvider` trait, `ChatRequest/Response/Event`, `ProviderError`, `ModelRegistry` จาก `models.toml`, `ModelPolicy` planner, `HealthTracker` (circuit breaker), `ModelRouter::chat` พร้อม retry+fallback
- [ ] 7.2 `providers/anthropic`: Messages API non-stream (headers, adaptive thinking, effort map, stop_reason map, usage) + live test
- [ ] 7.3 `providers/openai`: chat non-stream + usage + finish map + live test
- [ ] 7.4 `chat_stream` implement ทั้งสอง provider (SSE parse) — ทำเลยถ้าเวลาพอ ไม่งั้นย้าย P2 แต่ trait ต้องมี
- [ ] 7.5 Cost calc: `cost_usd_micro` จาก registry prices
- [ ] 7.6 Tests: planner ตาม policy/plan; breaker เปิดหลัง 5 fails; fallback ถูกเรียกเมื่อ primary Upstream 5xx; BadRequest ไม่ retry
- **Acceptance:** เรียก router ด้วย `anthovai_auto/balanced` → ได้ response จาก provider จริง; ปิด OpenAI key → fallback ไป Claude สำเร็จ

## Milestone 8 — Chat API & RAG Orchestration (วัน 28–31)

- [ ] 8.1 `conversation`: get_or_create, append messages, history window
- [ ] 8.2 `guardrails`: input length/injection flags, output verbatim-prompt check, fallback
- [ ] 8.3 `rag::ChatService::chat` ตาม 06 §6 รวม strict-knowledge short-circuit, PromptBuilder (03 §B.5), PostProcessor (citations → sources, grounded)
- [ ] 8.4 `usage`: record + counters rollup + `check_quota` (messages/month ตาม plan)
- [ ] 8.5 Public `POST /v1/chat` + Dashboard `POST /dashboard/v1/agents/{id}/test` (draft config, `debug` → retrieval_debug)
- [ ] 8.6 Public conversations endpoints (list/get/delete), `GET /v1/usage` แบบง่าย
- [ ] 8.7 Rate limit middleware per key (in-memory sliding window P1) + headers
- [ ] 8.8 Error mapping ครบตาม 05 §2.2, `X-Request-Id` ทุก response
- [ ] 8.9 Tests: HTTP integration ทุก endpoint ด้วย fake providers; citation parser; quota exceeded → 429; **cross_tenant_isolation_test (end-to-end chat)**
- **Acceptance (P1 Demo):**
  ```
  1. signup → org → API key
  2. POST /dashboard/v1/agents (strict, citations on)
  3. POST /dashboard/v1/documents (course-catalog.pdf) → poll → READY
  4. PUT agents/{id}/knowledge_bases; POST publish
  5. curl POST /v1/chat "หลักสูตร Rust ใช้เวลาเรียนกี่สัปดาห์?"
     → answer มี "12 สัปดาห์" + sources[0].page ถูก + grounded=true
  6. ถามเรื่องนอกเอกสาร → fallback message, grounded=false, ไม่เรียก LLM (ดูจาก usage)
  7. ใช้ key ของ org อื่นถาม → 404 agent_not_found
  ```

## Milestone 9 — Observability, OpenAPI, Hardening (วัน 32–34)

- [ ] 9.1 tracing spans + redaction layer; metrics endpoint; `/internal/health`
- [ ] 9.2 `utoipa` OpenAPI → `/v1/openapi.json` + snapshot test
- [ ] 9.3 Dockerfiles (multi-stage, distroless) + compose ใช้ image
- [ ] 9.4 Load test เบื้องต้น (k6/oha): 50 rps chat ด้วย fake provider → p95 (excl. LLM) < 400ms
- [ ] 9.5 `cargo audit`, `gitleaks` ใน CI
- [ ] 9.6 README: run locally ใน 5 คำสั่ง
- **Acceptance:** staging deploy ด้วย compose ทำ P1 Demo ผ่านด้วย provider จริง

---

## ลำดับความสำคัญถ้าเวลาไม่พอ
1. Milestones 0–8 ต้องครบ (เป็น core value)
2. Streaming (7.4) เลื่อนได้ → P2
3. URL/HTML/CSV parsers เลื่อนได้ (PDF, TXT/MD, JSON, DOCX ต้องมี)
4. Rate limit เลื่อนเป็น quota อย่างเดียวได้
5. Observability ขั้นต่ำต้องมี request_id + structured logs

## ประมาณการ
~34 วันทำงานสำหรับ 1 senior Rust dev + AI coding agent; แนะนำทำ Milestone 0–2 ด้วยคนก่อนเพื่อวาง pattern แล้วให้ AI agent ทำ 3–9 ตาม pattern

## Handoff ให้ AI Coding Agent
เมื่อสั่งงานแต่ละ milestone ให้แนบ: เอกสาร 06 (ทั้งฉบับ), ส่วนที่เกี่ยวข้องของ 03/04/05, และ checklist ของ milestone นั้น; ระบุว่า "ห้ามเปลี่ยน schema/API โดยไม่อัปเดต 04/05 ก่อน" และ "ทุก repo fn ต้องรับ TenantCtx"

## P2 Preview (ไม่ผูกมัด)
Streaming SSE (05 §3.2) · API key `test` env · Idempotency-Key · Webhooks · Query rewrite + conversation summary · Redis rate limit · Feedback endpoint · `/v1/search` · Audit log UI · Agent versioning UI · Security checklist 07 §12 ทั้งหมด
