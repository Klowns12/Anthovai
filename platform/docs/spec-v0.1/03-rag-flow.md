# 03 — Complete RAG Flow

แบ่งเป็น 2 pipeline: **Ingestion** (offline, worker) และ **Query Runtime** (online, api)

---

## Part A — Ingestion Pipeline

### A.1 ภาพรวม
```
Upload → Store original → Parse → Normalize → Chunk → Embed → Index → READY
```
ทำงานใน `anthovai-worker` ทั้งหมด (ยกเว้น upload) แต่ละขั้นอัปเดต `documents.status` และ `documents.progress` (0–100)

### A.2 Job Model (P1: PostgreSQL table `jobs`)
```sql
jobs(id, tenant_id, kind='ingest_document', payload jsonb, status, attempts, max_attempts=3,
     run_after, locked_by, locked_at, last_error, created_at)
```
Worker ดึงงานด้วย `SELECT ... FOR UPDATE SKIP LOCKED` ทีละ N งาน; retry แบบ exponential backoff (30s, 2m, 10m); เกิน max_attempts → document FAILED

### A.3 Source Types และ Parser

| Type | MIME / detect | Parser (Rust crate แนะนำ) | Output |
|------|---------------|---------------------------|--------|
| PDF | application/pdf | `pdf-extract` หรือ `lopdf` + fallback เรียก `pdftotext` (poppler) ผ่าน sidecar | text per page |
| DOCX | application/vnd.openxmlformats-officedocument.wordprocessingml.document | `docx-rs` / unzip + parse `word/document.xml` | text with headings |
| TXT / MD | text/plain, text/markdown | direct; MD ใช้ `pulldown-cmark` เพื่อรู้โครงสร้าง heading | text with headings |
| HTML / URL | text/html | `reqwest` + `scraper` + readability heuristic (ตัด nav/footer) | main text + title |
| JSON | application/json | `serde_json` → schema detection → flatten | records |
| CSV | text/csv | `csv` crate; header row = field names | records (1 row = 1 record) |

ข้อจำกัด P1: ไม่มี OCR, ไม่มี image understanding, URL crawl แค่หน้าเดียว (ไม่ follow links; site crawl = P5)

### A.4 Normalization
- Unicode NFC, ตัด control chars, รวม whitespace ซ้ำ, แก้ hyphenation ข้ามบรรทัดใน PDF
- ตรวจภาษา (`whatlang`) เก็บใน `documents.language`
- เก็บ extracted text ที่ `tenant/{org}/{kb}/{doc}/v{n}/extracted.txt` เพื่อ re-chunk ได้โดยไม่ต้อง parse ใหม่

### A.5 Chunking Strategy

| Source type | Strategy | Target size | Overlap |
|-------------|----------|-------------|---------|
| PDF/DOCX/TXT | Recursive split: heading → paragraph → sentence | 400–600 tokens | 15% (~80 tokens) |
| Markdown/HTML | Structure-aware: heading path เป็น metadata, split ภายใต้ heading | 400–600 tokens | 15% |
| JSON record | 1 record = 1 chunk (ถ้า > 600 tokens แยกตาม field ใหญ่) | ≤ 600 | 0 |
| CSV row | 1 row = 1 chunk แสดงเป็น `field: value` บรรทัดละ field | ≤ 400 | 0 |

**Chunk text ที่ส่งไป embedding** = `contextual header + body`
```
[Document: Course Catalog 2026 > Section: Rust Programming]
หลักสูตร Rust Programming ใช้เวลาเรียน 12 สัปดาห์ ...
```
Header ช่วย retrieval แม่นขึ้นและใช้ทำ citation ที่อ่านง่าย

**Token counting**: ใช้ `tiktoken-rs` (cl100k_base) เป็น approximation กลางสำหรับทุก provider

### A.6 JSON Handling (สำคัญตาม use case)
```
JSON input
  ↓ Schema detection
  ├─ Array of objects  → แต่ละ object = record
  ├─ Object of objects → แต่ละ value = record, key เป็น record id
  └─ Single object     → 1 record (หรือแยกตาม top-level key ถ้าใหญ่)
  ↓ Normalization → "field_path: value" lines (nested ใช้ dot path)
  ↓ Metadata: json_path, record_key, field names (สำหรับ metadata filter)
```
ตัวอย่าง record → chunk text:
```
[Document: courses.json > Record: rust-101]
course: Rust Programming
duration: 12 weeks
price: 4900 THB
description: เรียน ownership, borrowing, async ...
```

### A.7 Embedding
- Trait `EmbeddingProvider::embed_batch(texts) -> Vec<Vec<f32>>`
- P1 provider: OpenAI embeddings (dimension 1536 หรือกำหนดที่ 1024 ผ่าน `dimensions` param) — **ล็อก dimension ที่ 1536 ใน v0.1** เก็บใน `knowledge_bases.embedding_model` + `embedding_dim` เพื่อรองรับเปลี่ยน model ต่อ KB ในอนาคต (ต้อง re-embed ทั้ง KB)
- Batch 64 texts/request, concurrency 4, retry 429 ตาม `Retry-After`
- ทุก chunk เก็บ `content_hash` (sha256 ของ chunk text) → ถ้า re-upload แล้ว hash เดิม ให้ reuse embedding (ประหยัด cost)

### A.8 Indexing
```sql
INSERT INTO document_chunks (id, tenant_id, knowledge_base_id, document_id, document_version,
   chunk_index, content, content_hash, token_count, embedding, tsv, metadata)
```
- `tsv` = `to_tsvector('simple', content)` สำหรับ hybrid keyword search (Thai ไม่มี stemmer ดี → ใช้ `simple` + ตัดคำด้วย ICU ก่อนใน Rust ถ้าต้อง (Future))
- Index: HNSW บน `embedding` (`vector_cosine_ops`, m=16, ef_construction=64) และ GIN บน `tsv` และ B-tree `(tenant_id, knowledge_base_id)`
- Swap version: insert chunks ของ version ใหม่ทั้งหมดใน transaction → update `documents.current_version` → mark chunks เก่า `deleted_at` → hard delete โดย cleanup job

### A.9 Metadata ต่อ chunk
```json
{
  "source_type": "pdf",
  "title": "Course Catalog 2026",
  "heading_path": ["Programs", "Rust Programming"],
  "page": 4,
  "url": null,
  "json_path": null,
  "record_key": null,
  "language": "th",
  "created_at": "2026-09-03T10:00:00Z"
}
```

---

## Part B — Query Runtime Pipeline

### B.1 ภาพรวม
```
Request → Auth/Tenant → Agent config → Query processing → Retrieval → Rerank/Select
        → Prompt build → Model Router → LLM → Post-process → Response + Usage
```
Budget เวลา (ไม่รวม generation): auth 5ms · config 10ms · embed query 80–150ms · search 50–150ms · rerank 20ms · build 5ms

### B.2 Query Processing
1. Trim, จำกัดความยาว (max 4,000 chars → 400 `invalid_request`)
2. Guardrail input: ตรวจ pattern prompt-injection ขั้นพื้นฐาน (log + flag, ยังไม่ block ใน P1)
3. **Conversation-aware rewrite (P2)**: ถ้ามี history และคำถามสั้น/มี pronoun → ให้ LLM เล็ก rewrite เป็น standalone query ก่อน embed
4. Embed query ด้วย embedding model เดียวกับ KB (อ่านจาก `knowledge_bases.embedding_model`; ถ้า Agent ผูกหลาย KB ที่ model ต่างกัน → embed ทีละ model)

### B.3 Retrieval (Hybrid)
```sql
-- Vector (top 30)
SELECT id, content, metadata, 1 - (embedding <=> $q) AS vscore
FROM document_chunks
WHERE tenant_id = $tenant AND knowledge_base_id = ANY($kb_ids) AND deleted_at IS NULL
ORDER BY embedding <=> $q LIMIT 30;

-- Keyword (top 30)
SELECT id, ts_rank_cd(tsv, plainto_tsquery('simple', $text)) AS kscore ...
WHERE tenant_id = $tenant AND knowledge_base_id = ANY($kb_ids) AND deleted_at IS NULL
  AND tsv @@ plainto_tsquery('simple', $text) LIMIT 30;
```
- รวมด้วย **Reciprocal Rank Fusion** (k=60): `score = Σ 1/(k + rank_i)`
- Metadata filter จาก agent config หรือ request (`filters: {document_ids, source_type}`) ใส่ใน WHERE
- `tenant_id` filter **บังคับที่ repository layer** ไม่ให้ caller ลืม (ดู 07)

### B.4 Reranking & Selection
- P1: ไม่มี cross-encoder; ใช้ RRF score + **MMR** (λ=0.7) เพื่อลด chunk ซ้ำ
- เลือกจนเต็ม `context_token_budget` (default 6,000 tokens, ปรับตาม reasoning level) สูงสุด `top_k` (default 8)
- Threshold: ถ้า top vscore < `min_relevance` (default 0.25 cosine similarity) → ถือว่า "no relevant knowledge"
- P6: เพิ่ม `Reranker` trait (Cohere/Voyage rerank หรือ LLM-based)

### B.5 Prompt Construction
```
SYSTEM:
  {agent.instructions}

  You are answering on behalf of {org.name}. Today is {date}.
  Rules:
  - Answer in {language or "the user's language"}.
  - Use ONLY the information inside <knowledge>. {if strict: If the answer is not there, reply exactly: "{fallback_message}"}
  - Cite sources using [n] where n is the source number.
  - {response_length instruction}

  <knowledge>
  <source n="1" doc="Course Catalog 2026" page="4">…chunk…</source>
  <source n="2" …>…</source>
  </knowledge>

MESSAGES:
  [last N turns of conversation, N by token budget, default 6 turns]
  user: {question}
```
- Knowledge อยู่ใน **system** เพื่อให้ prompt caching ของ provider ทำงานกับ instructions ส่วนบน (instructions คงที่มาก่อน, knowledge ที่เปลี่ยนตามคำถามอยู่ท้าย)
- ถ้า "no relevant knowledge" และ `strict_knowledge=true` → ไม่เรียก LLM เลย ตอบ fallback ทันที (ประหยัด cost) พร้อม `sources: []` และ `grounded: false`

### B.6 Model Router
```
Input: ModelPolicy (agent) + RoutingHints {reasoning: fast|balanced|deep, context_tokens, has_tools, language}
       + ProviderHealth (circuit breaker state)

anthovai_auto:
  fast      → tier "small"   (cheapest healthy provider)
  balanced  → tier "medium"
  deep      → tier "large"
  ถ้า context_tokens > tier max → ขยับ tier ขึ้น
  ถ้า provider หลักของ tier unhealthy → provider รอง

openai_only / claude_only: เลือก tier ภายใน provider เดียว, ไม่ fallback ข้าม provider (ตอบ 503 ถ้าล่ม)
custom (Enterprise): {primary: {provider, model}, fallback: [...] } ตามที่ตั้งไว้
```
**Model tier table** เก็บใน config (`models.toml`) ไม่ hard-code ในโค้ด:
```toml
[[models]]
id = "openai-small";  provider = "openai";    name = "<openai small model>";  tier = "small";  max_context = 128000; in_price = 0.15; out_price = 0.60
[[models]]
id = "claude-medium"; provider = "anthropic"; name = "claude-sonnet-5";       tier = "medium"; max_context = 1000000; in_price = 2.0;  out_price = 10.0
[[models]]
id = "claude-large";  provider = "anthropic"; name = "claude-opus-5";         tier = "large";  max_context = 1000000; in_price = 5.0;  out_price = 25.0
```
(ชื่อ model ของ OpenAI ให้ยืนยันจาก docs ณ วัน implement; ราคาเป็น USD/1M tokens ใช้คำนวณ internal cost)

### B.7 Provider Call
- **Anthropic**: `POST https://api.anthropic.com/v1/messages` headers `x-api-key`, `anthropic-version: 2023-06-01`; body `{model, max_tokens, system, messages, stream}`; สำหรับรุ่น 4.6+ ใส่ `thinking: {type: "adaptive"}` และควบคุมความลึกด้วย `output_config.effort` (`low` = fast, `medium` = balanced, `high` = deep); อ่าน `usage.input_tokens/output_tokens`; ตรวจ `stop_reason` (`end_turn`, `max_tokens`, `refusal`)
- **OpenAI**: Chat Completions / Responses API ตามที่ SDK-less HTTP รองรับ; map `usage.prompt_tokens/completion_tokens`
- Timeout: connect 5s, total 60s (non-stream) / idle 30s (stream); retry 1 ครั้งเฉพาะ 429/5xx/connection error ก่อนสลับ fallback
- Circuit breaker ต่อ provider: 5 failures ใน 60s → open 30s

### B.8 Streaming
- Provider SSE → normalize เป็น Anthovai SSE events (ดู 05 §7): `message_start`, `delta`, `sources`, `usage`, `done`, `error`
- `sources` event ส่ง **ก่อน** delta แรก (เพราะ retrieval เสร็จก่อน generation) เพื่อให้ UI แสดง citations ได้ทันที
- Guardrail post-check ใน stream ทำแบบ best-effort (ตรวจ citation index เกินช่วง → ตัดออกใน `done`)

### B.9 Post-processing
1. **Citation mapping**: regex `\[(\d+)\]` → map เป็น `sources[]` ที่ถูกอ้างจริง; แนบ `document_id, title, page, url, chunk_id, snippet`
2. **Grounding flag**: `grounded = sources.len() > 0`
3. **Guardrail output**: ตรวจ fallback phrase, ตรวจ PII pattern พื้นฐาน (log only P1)
4. **Usage record**: `input_tokens, output_tokens, embedding_tokens, provider, model, latency_ms, cost_usd_micro, cache_read_tokens`
5. **Persist**: `messages` (user + assistant, พร้อม `retrieved_chunk_ids`, `model_used`) และ `usage_records` — ทำใน background task พร้อม retry; ถ้า persist พลาด response ยังส่งได้ แต่ log error

### B.10 Conversation Memory (P2 รายละเอียด, P1 ขั้นต่ำ)
- P1: เก็บ messages ตาม `conversation_id`, ใส่ N turns ล่าสุดใน prompt
- P2: summary rolling เมื่อ history > budget; query rewrite
- Memory **ไม่** ถูกเขียนกลับ Knowledge Base โดยอัตโนมัติ

### B.11 Evaluation Hooks (P6 แต่วาง log ตั้งแต่ P1)
- ทุก request เก็บ `retrieval_debug` (chunk ids + scores) ใน `messages.metadata` ถ้า `debug=true` หรือ sampling 5%
- Feedback endpoint (P2) ผูกกับ `message_id` → ใช้ทำ eval set ภายหลัง

### B.12 Failure Modes

| จุด | ล้มเหลว | พฤติกรรม |
|-----|---------|----------|
| Embed query | provider error | retry 1 → ถ้ายังพัง: keyword-only retrieval + header `X-Anthovai-Degraded: embedding` |
| Vector search | DB timeout | 503 `retrieval_unavailable` |
| LLM primary | 5xx/timeout | fallback provider ตาม policy |
| LLM ทั้งหมด | ล่ม | 503 `provider_unavailable`, `Retry-After: 10` |
| Output ไม่มี citation แต่ strict | — | ยังส่งคำตอบ, `grounded=false`, log เพื่อ eval |
