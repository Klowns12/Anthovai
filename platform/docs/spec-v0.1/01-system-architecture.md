# 01 — System Architecture

## 1. เป้าหมายของระบบ

Anthovai AI Platform ให้ธุรกิจสร้าง **AI Agent ที่รู้จักข้อมูลขององค์กรตัวเอง** โดยไม่ต้องสร้าง RAG infrastructure เอง ลูกค้าอัปโหลด knowledge → ตั้งค่า Agent → ได้ API Key → ต่อกับเว็บไซต์/LMS/แอปของตัวเอง

สิ่งที่ Anthovai เป็นเจ้าของ: **Agent layer, Knowledge/RAG layer, API platform layer**
สิ่งที่ Anthovai ไม่ได้เป็นเจ้าของ (ระยะแรก): **Foundation model** (ใช้ OpenAI และ Anthropic ผ่าน Model Router)

## 2. Architecture ระดับสูง

```
                              CLIENTS
        ┌──────────────────────┬──────────────────────┐
        │  Dashboard (Next.js) │  Customer Apps        │
        │  (Anthovai staff +   │  (website chatbot,    │
        │   customer admins)   │   LMS, mobile, etc.)  │
        └──────────┬───────────┴──────────┬───────────┘
                   │ Session/JWT          │ API Key (av_live_...)
                   ▼                      ▼
        ┌──────────────────────────────────────────────┐
        │              anthovai-api (Rust/axum)         │
        │  ┌──────────┐ ┌───────────┐ ┌──────────────┐ │
        │  │ Auth     │ │ Rate Limit│ │ Tenant Ctx   │ │
        │  └────┬─────┘ └─────┬─────┘ └──────┬───────┘ │
        │       └─────────────┼──────────────┘         │
        │                     ▼                         │
        │  ┌─────────┐ ┌───────────┐ ┌───────────────┐ │
        │  │ Agents  │ │ Knowledge │ │ Conversations │ │
        │  └────┬────┘ └─────┬─────┘ └───────┬───────┘ │
        │       │            │               │         │
        │       ▼            ▼               ▼         │
        │  ┌──────────────────────────────────────────┐│
        │  │            RAG Engine (retrieval crate)  ││
        │  │  query embed → vector search → filter →  ││
        │  │  rerank → context builder → prompt       ││
        │  └────────────────────┬─────────────────────┘│
        │                       ▼                       │
        │  ┌──────────────────────────────────────────┐│
        │  │      Model Router (inference crate)      ││
        │  │  policy → provider select → fallback     ││
        │  └───────┬──────────────────────┬───────────┘│
        └──────────┼──────────────────────┼────────────┘
                   ▼                      ▼
           ┌──────────────┐       ┌──────────────┐
           │ OpenAI API   │       │ Anthropic API│
           │ chat+embed   │       │ Messages API │
           └──────────────┘       └──────────────┘

        ┌──────────────────────────────────────────────┐
        │             anthovai-worker (Rust)           │
        │  queue consumer → parse → chunk → embed →    │
        │  index → update document status              │
        └──────────┬────────────────────┬──────────────┘
                   ▼                    ▼
        ┌──────────────────┐   ┌──────────────────┐
        │ PostgreSQL 16    │   │ S3-compatible     │
        │ + pgvector       │   │ Object Storage    │
        │ (relational +    │   │ (original files,  │
        │  chunks + vectors│   │  extracted text)  │
        │  + job queue)    │   │                   │
        └──────────────────┘   └──────────────────┘
```

## 3. สาม Layer หลัก

### 3.1 AI Model Layer (crate: `inference`, `providers/*`)
- **Model Router**: รับ `ModelPolicy` + `RoutingHints` (task type, context size, reasoning level) → เลือก `ProviderId + model_name`
- **Providers**: implement trait `ChatProvider`, `EmbeddingProvider` (แยกกัน)
- **Fallback**: ถ้า provider หลัก error/timeout → ลอง provider รอง ตาม policy
- **Guardrails**: pre-check (prompt injection heuristics, input size) และ post-check (grounding, fallback message)
- ไม่มี foundation model ของตัวเองใน P1–P5

### 3.2 Knowledge Layer (crate: `knowledge`, `ingestion`, `retrieval`, `embeddings`)
- **Knowledge Base** เป็น unit ของการจัดกลุ่ม document และเป็น unit ของ permission ที่ Agent อ้างถึง
- **Ingestion pipeline** ทำงานใน worker เท่านั้น ไม่ทำใน HTTP request
- **Vector store** = pgvector ใน PostgreSQL เดียวกับ relational data (ระยะแรก)
- **Versioning**: document มี `version` เพิ่มเมื่อ re-upload; chunks ของ version เก่าถูก soft-delete

### 3.3 Agent / API Layer (crate: `agent`, `api`, `auth`, `tenant`, `usage`)
- **Agent** = configuration (ไม่ใช่ model): identity, instructions, model policy, KB list, retrieval config, guardrails, output settings
- **Public API** `/v1/*` ใช้ API Key
- **Dashboard API** `/dashboard/v1/*` ใช้ session JWT (สิทธิ์ตาม RBAC ใน organization)
- **Usage** บันทึกทุก request: tokens in/out, provider, model, latency, cost (internal)

## 4. Component Inventory

| Component | Runtime | ความรับผิดชอบ | Scale |
|-----------|---------|----------------|-------|
| `anthovai-api` | Rust binary (axum + tokio) | HTTP API ทั้ง public และ dashboard, RAG runtime, model routing | horizontal (stateless) |
| `anthovai-worker` | Rust binary (tokio) | consume job queue, ingestion pipeline, scheduled tasks | horizontal ตาม queue depth |
| `dashboard` | Next.js | UI สำหรับลูกค้าและ staff | static/edge |
| PostgreSQL 16 + pgvector | managed DB | relational data, chunks, vectors, job queue (P1) | vertical ก่อน, read replica ภายหลัง |
| Object Storage | S3-compatible (MinIO dev / S3 or R2 prod) | ไฟล์ต้นฉบับ, extracted text | managed |
| Redis (optional P2) | managed | rate limit counters, cache, queue (ถ้าย้ายจาก PG) | managed |

## 5. หลักการออกแบบ

1. **Modular monolith** — crates แยกตาม bounded context, binaries มีแค่ `api` และ `worker`; ห้าม crate หนึ่ง import internal ของอีก crate โดยตรง ให้ผ่าน public API ของ crate เท่านั้น
2. **Tenant context เป็น first-class** — ทุก service function รับ `TenantCtx` เป็น argument ตัวแรก; ทุก SQL มี `WHERE tenant_id = $1`; เสริมด้วย PostgreSQL RLS
3. **Provider-agnostic contract** — response format เป็นของ Anthovai; ชื่อ model จริงเก็บใน `usage_records` เท่านั้น
4. **Async everything, no blocking in request path** — upload → job; embedding batch ใน worker
5. **Idempotency** — public write endpoints รองรับ `Idempotency-Key` header (P2)
6. **Observability ตั้งแต่ P1** — structured logs (tracing), `request_id` ทุก response, metrics พื้นฐาน (latency, provider errors, queue depth)
7. **Config ผ่าน env** — ไม่มี secrets ใน repo; provider keys เป็น env ของ Anthovai (ระยะแรกไม่รองรับ BYOK)

## 6. Request Lifecycle (Public Chat)

```
Client ──POST /v1/chat──▶ api
  1. Extract Bearer key → hash → lookup api_keys (cache 60s)
  2. Build TenantCtx {org_id, workspace_id, api_key_id, scopes}
  3. Rate limit check (per api_key, per org)
  4. Load Agent (must belong to tenant) + resolve allowed KB ids
  5. Load/create Conversation (optional conversation_id)
  6. RAG: embed query → vector search (tenant_id + kb_ids filter) → rerank → build context
  7. Model Router: policy → provider/model → call (with fallback)
  8. Post-process: citations mapping, guardrail, usage record
  9. Persist messages + usage (async fire-and-forget with retry)
 10. Return {request_id, answer, sources, usage}
```

## 7. Ingestion Lifecycle

```
Dashboard ──POST /v1/documents (multipart)──▶ api
  1. Validate size/type, create documents row (status=UPLOADING)
  2. Stream file to object storage: tenant/{org}/{kb}/{doc}/original
  3. status=QUEUED, enqueue job {document_id, tenant_id}
  4. Return 202 {document_id, status}

worker ──poll queue──▶
  5. status=PROCESSING → download → parse (PDF/DOCX/TXT/MD/JSON/CSV/HTML)
  6. Normalize text → store extracted text to object storage
  7. status=CHUNKING → chunk with strategy per type
  8. status=EMBEDDING → batch embed (provider embedding model)
  9. status=INDEXING → insert document_chunks (tenant_id, kb_id, embedding, metadata)
 10. status=READY (or FAILED with error_message); emit webhook (P2)
```

## 8. Deployment Topology (P1 → P2)

**P1 (dev/staging):** docker-compose: `api`, `worker`, `postgres+pgvector`, `minio`, `dashboard`
**P2 (production):**
- `api` ×2+ behind load balancer (TLS termination ที่ LB)
- `worker` ×1–N
- Managed PostgreSQL พร้อม pgvector, daily backup, PITR
- S3/R2 พร้อม bucket policy แยก prefix ตาม tenant
- Redis สำหรับ rate limit และ cache

## 9. Non-Functional Targets (P1)

| Metric | Target |
|--------|--------|
| Chat p95 latency (excluding LLM generation) | < 400 ms |
| Retrieval p95 (top-20, 1M chunks) | < 150 ms |
| Ingestion 10 MB PDF → READY | < 3 min |
| API availability | 99.5% (P2 → 99.9%) |
| Tenant data leak | 0 — มี integration test cross-tenant ทุก PR |

## 10. สิ่งที่ตั้งใจ *ไม่* ทำใน v0.1

- Microservices / message broker แยก (ใช้ PG job table ก่อน)
- Vector DB ภายนอก (Qdrant/Pinecone)
- Fine-tuning
- BYOK (ลูกค้าเอา provider key มาเอง)
- Multi-region

## 11. Open Questions

| # | คำถาม | Default ที่ใช้ใน v0.1 |
|---|-------|------------------------|
| Q1 | Embedding provider หลัก? | OpenAI embeddings (Anthropic ไม่มี embedding model) มี trait รองรับเพิ่ม Voyage/local ภายหลัง |
| Q2 | Reranker ใน P1? | ไม่ใช้ external reranker; ใช้ hybrid score (vector + BM25 ผ่าน `tsvector`) และ MMR |
| Q3 | Dashboard auth provider? | Email+password + magic link ทำเอง (P1), OAuth Google (P3) |
| Q4 | Billing provider? | Stripe (P4) |
