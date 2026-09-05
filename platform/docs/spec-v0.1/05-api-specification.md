# 05 — API Specification v1

Base URL: `https://api.anthovai.com`
Content type: `application/json; charset=utf-8` (ยกเว้น upload = `multipart/form-data`)
เวลาทั้งหมดเป็น ISO-8601 UTC

## 1. สอง API Surface

| Surface | Prefix | Auth | ผู้ใช้ |
|---------|--------|------|--------|
| **Public API** | `/v1/*` | `Authorization: Bearer av_live_...` | แอปของลูกค้า, developer |
| **Dashboard API** | `/dashboard/v1/*` | Session cookie (`__Host-av_session`, HttpOnly, SameSite=Lax) หรือ `Authorization: Bearer <jwt>` | Next.js dashboard |

Public API เป็น **contract ที่สัญญาไว้กับลูกค้า** เปลี่ยนแบบ breaking ต้องออก `/v2`. Dashboard API เปลี่ยนได้ตามต้องการ (ใช้กับ frontend ของเราเท่านั้น)

## 2. Common Conventions

### 2.1 Headers
| Header | ทิศทาง | ความหมาย |
|--------|--------|----------|
| `Authorization` | req | Bearer API key หรือ JWT |
| `Idempotency-Key` | req (P2) | UUID; ซ้ำภายใน 24h → คืน response เดิม |
| `X-Request-Id` | req/res | ถ้า client ส่งมาจะ echo กลับ ไม่งั้น server สร้าง `req_01J...` |
| `X-RateLimit-Limit` / `-Remaining` / `-Reset` | res | ต่อ API key ต่อนาที |
| `X-Anthovai-Degraded` | res | เช่น `embedding` เมื่อทำงานแบบลดคุณภาพ |
| `Retry-After` | res | กับ 429/503 |

### 2.2 Error Format
```json
{
  "error": {
    "type": "invalid_request_error",
    "code": "agent_not_found",
    "message": "Agent agt_01J... was not found in this workspace.",
    "param": "agent_id",
    "request_id": "req_01J...",
    "doc_url": "https://docs.anthovai.com/errors#agent_not_found"
  }
}
```

| HTTP | `type` | `code` ตัวอย่าง |
|------|--------|------------------|
| 400 | invalid_request_error | `invalid_json`, `missing_field`, `message_too_long`, `unsupported_file_type` |
| 401 | authentication_error | `invalid_api_key`, `expired_api_key`, `revoked_api_key`, `session_expired` |
| 403 | permission_error | `agent_not_allowed`, `scope_missing`, `plan_required`, `role_insufficient` |
| 404 | not_found_error | `agent_not_found`, `document_not_found`, `conversation_not_found` |
| 409 | conflict_error | `document_processing`, `slug_taken` |
| 410 | gone_error | `agent_archived` |
| 413 | payload_too_large | `file_too_large` |
| 422 | validation_error | `invalid_model_policy` |
| 429 | rate_limit_error | `rate_limited`, `quota_exceeded` |
| 500 | api_error | `internal_error` |
| 503 | service_unavailable | `provider_unavailable`, `retrieval_unavailable` |

### 2.3 Pagination (list endpoints)
`?limit=20&cursor=<opaque>` → response `{ "data": [...], "has_more": true, "next_cursor": "..." }`
Sort default: `created_at desc`

### 2.4 Object IDs
Prefixed ULID: `org_`, `ws_`, `agt_`, `kb_`, `doc_`, `chk_`, `key_`, `conv_`, `msg_`, `req_`, `job_`

---

## 3. Public API — Chat

### 3.1 `POST /v1/chat`
Scope: `chat`

**Request**
```json
{
  "agent_id": "agt_01J...",
  "message": "หลักสูตร Rust ใช้เวลาเรียนกี่สัปดาห์?",
  "conversation_id": "conv_01J...",
  "user": { "id": "student-4471", "metadata": { "grade": "M6" } },
  "filters": { "document_ids": ["doc_01J..."] },
  "options": {
    "include_sources": true,
    "include_usage": true,
    "max_sources": 5,
    "language": "th",
    "model_policy": "anthovai_auto"
  },
  "metadata": { "channel": "website" }
}
```
| Field | Type | Required | หมายเหตุ |
|-------|------|----------|----------|
| agent_id | string | ✓ | ต้องอยู่ใน scope ของ key |
| message | string 1–4000 | ✓ | |
| conversation_id | string | | ถ้าไม่ส่ง สร้างใหม่และคืนใน response |
| user.id | string ≤128 | | external_user_id สำหรับ analytics/memory |
| filters | object | | จำกัด retrieval; document_ids ต้องอยู่ใน KB ของ agent |
| options.model_policy | enum | | override ได้เฉพาะ plan ที่อนุญาต ไม่งั้น 403 `plan_required` |
| options.language | ISO 639-1 | | override agent |

**Response 200**
```json
{
  "id": "msg_01J...",
  "request_id": "req_01J...",
  "conversation_id": "conv_01J...",
  "agent_id": "agt_01J...",
  "answer": "หลักสูตร Rust Programming ใช้เวลาเรียน 12 สัปดาห์ [1]",
  "grounded": true,
  "sources": [
    {
      "index": 1,
      "document_id": "doc_01J...",
      "chunk_id": "chk_01J...",
      "title": "Course Catalog 2026",
      "page": 4,
      "url": null,
      "snippet": "หลักสูตร Rust Programming ใช้เวลาเรียน 12 สัปดาห์ ...",
      "score": 0.83
    }
  ],
  "usage": {
    "input_tokens": 812,
    "output_tokens": 96,
    "total_tokens": 908
  },
  "model": { "policy": "anthovai_auto", "tier": "medium" },
  "latency_ms": 1840,
  "created_at": "2026-09-03T10:00:00Z"
}
```
- `model.provider`/`model.name` จะปรากฏเฉพาะเมื่อ plan ≥ business (ตั้งได้ใน org settings)
- `grounded=false` และ `sources=[]` เมื่อตอบ fallback

### 3.2 `POST /v1/chat/stream`
Request เหมือน 3.1; response `text/event-stream`

```
event: message_start
data: {"id":"msg_01J...","request_id":"req_01J...","conversation_id":"conv_01J..."}

event: sources
data: {"sources":[{"index":1,"document_id":"doc_...","title":"Course Catalog 2026","page":4}]}

event: delta
data: {"text":"หลักสูตร Rust"}

event: delta
data: {"text":" Programming ใช้เวลา"}

event: usage
data: {"input_tokens":812,"output_tokens":96}

event: done
data: {"grounded":true,"finish_reason":"stop","latency_ms":1840}
```
- `event: error` → `data: {"error": {...}}` แล้วปิด stream
- ส่ง `: keepalive` comment ทุก 15s ระหว่างรอ
- `finish_reason ∈ {stop, length, content_filter, fallback}`

### 3.3 `POST /v1/messages/{message_id}/feedback` (P2)
```json
{ "rating": 1, "comment": "ถูกต้อง" }
```
→ 204

---

## 4. Public API — Conversations
Scope: `chat`

| Method | Path | คำอธิบาย |
|--------|------|-----------|
| GET | `/v1/conversations?agent_id=&user_id=&limit=&cursor=` | list |
| GET | `/v1/conversations/{id}` | รวม `messages` ล่าสุด 50 รายการ |
| DELETE | `/v1/conversations/{id}` | ลบถาวร (GDPR/PDPA) |

---

## 5. Public API — Agents (read-only ใน P1)
Scope: `agents:read`

| Method | Path | คำอธิบาย |
|--------|------|-----------|
| GET | `/v1/agents` | list agents ที่ key เรียกได้ |
| GET | `/v1/agents/{id}` | `{id, name, description, status, language, knowledge_bases:[{id,name,status}], published_at}` (ไม่เผย instructions) |

การสร้าง/แก้ไข agent ผ่าน Public API = P5 (สำหรับ platform partners)

---

## 6. Public API — Knowledge & Documents
Scope: `knowledge:read`, `knowledge:write` (ให้ลูกค้า sync เอกสารอัตโนมัติจากระบบของตัวเอง)

### 6.1 Knowledge Bases
| Method | Path | Scope |
|--------|------|-------|
| GET | `/v1/knowledge_bases` | read |
| GET | `/v1/knowledge_bases/{id}` | read |
| POST | `/v1/knowledge_bases` `{name, description}` | write |
| DELETE | `/v1/knowledge_bases/{id}` | write |

### 6.2 Documents
**`POST /v1/documents`** (multipart) — scope write
```
knowledge_base_id = kb_01J...
file              = <binary>          (หรือ)
url               = https://...       (หรือ)
text              = "..."  + title
metadata          = {"category":"admission"}   (optional JSON string)
```
→ **202**
```json
{ "id": "doc_01J...", "knowledge_base_id": "kb_01J...", "title": "handbook-2026.pdf",
  "source_type": "pdf", "status": "queued", "progress": 0, "created_at": "..." }
```

**`GET /v1/documents/{id}`** → object เดียวกัน + `chunk_count, token_count, error_code, error_message, current_version`
**`GET /v1/documents?knowledge_base_id=&status=`** → list
**`PUT /v1/documents/{id}`** (multipart) → re-upload = version ใหม่ → 202
**`DELETE /v1/documents/{id}`** → 204 (status→deleted, chunks ถูกลบ async)

### 6.3 `POST /v1/search` (P2) — retrieval อย่างเดียว ไม่เรียก LLM
```json
{ "agent_id": "agt_...", "query": "...", "top_k": 10 }
```
→ `{ "results": [{chunk_id, document_id, title, page, content, score}] }`

---

## 7. Public API — Usage
Scope: ทุก key
`GET /v1/usage?from=2026-09-01&to=2026-09-30&group_by=day|agent`
```json
{ "period": {"from":"...","to":"..."},
  "totals": {"messages": 4321, "input_tokens": 3500000, "output_tokens": 410000},
  "quota": {"messages_limit": 10000, "messages_used": 4321, "resets_at": "2026-10-01T00:00:00Z"},
  "data": [{"date":"2026-09-01","messages":120,"input_tokens":...}] }
```

## 8. Public API — Webhooks (P2)
Events: `document.ready`, `document.failed`, `conversation.created`, `usage.threshold`
Signature: `X-Anthovai-Signature: t=<unix>,v1=<hmac_sha256(secret, t + "." + body)>`

---

## 9. Dashboard API (สรุป endpoints; schema ยืดหยุ่น)

### 9.1 Auth
```
POST /dashboard/v1/auth/signup        {email, password, name}
POST /dashboard/v1/auth/login         {email, password}        → set cookie
POST /dashboard/v1/auth/magic-link    {email}
GET  /dashboard/v1/auth/magic-link/verify?token=
POST /dashboard/v1/auth/logout
GET  /dashboard/v1/me                 → {user, organizations:[{id,name,role}]}
```
ทุก endpoint ถัดไปต้องส่ง header `X-Org-Id: org_...` (org ที่กำลังทำงาน) server ตรวจ membership

### 9.2 Organizations & Workspaces
```
POST /dashboard/v1/organizations                 {name, slug}
GET  /dashboard/v1/organizations/{id}
PATCH /dashboard/v1/organizations/{id}           {name, settings}
GET/POST /dashboard/v1/workspaces
PATCH/DELETE /dashboard/v1/workspaces/{id}
GET/POST /dashboard/v1/members                   (P3)  {email, role}
PATCH/DELETE /dashboard/v1/members/{user_id}     (P3)
```

### 9.3 Agents
```
GET    /dashboard/v1/agents?workspace_id=
POST   /dashboard/v1/agents                     {workspace_id, name, description, config}
GET    /dashboard/v1/agents/{id}                → รวม draft_config, published_config, versions[]
PATCH  /dashboard/v1/agents/{id}                {name, description, config}  → สร้าง draft version ใหม่
POST   /dashboard/v1/agents/{id}/publish        → published = draft
POST   /dashboard/v1/agents/{id}/rollback       {version}
POST   /dashboard/v1/agents/{id}/pause | /resume | /archive
POST   /dashboard/v1/agents/{id}/test           {message, conversation_id?, debug?: true} → เหมือน /v1/chat แต่ใช้ draft + retrieval_debug
POST   /dashboard/v1/agents/{id}/test/stream
PUT    /dashboard/v1/agents/{id}/knowledge_bases {knowledge_base_ids:[...]}
```
`config` validate ตาม JSON Schema ใน 04 §4; `model_policy` ที่เกิน plan → 403 `plan_required`

### 9.4 Knowledge
```
GET/POST /dashboard/v1/knowledge_bases
GET/PATCH/DELETE /dashboard/v1/knowledge_bases/{id}
GET  /dashboard/v1/knowledge_bases/{id}/documents
POST /dashboard/v1/documents                     (multipart เหมือน 6.2)
GET  /dashboard/v1/documents/{id}
GET  /dashboard/v1/documents/{id}/chunks?limit=  → ดู chunks (debug)
POST /dashboard/v1/documents/{id}/retry
PUT/DELETE /dashboard/v1/documents/{id}
GET  /dashboard/v1/documents/{id}/events         (SSE progress)  หรือ poll GET ทุก 2s ใน P1
```

### 9.5 API Keys
```
GET  /dashboard/v1/api_keys?workspace_id=
POST /dashboard/v1/api_keys        {workspace_id, name, environment, scopes, all_agents, agent_ids, expires_in_days}
                                   → 201 {id, name, prefix, secret: "av_live_....", ...}  (secret แสดงครั้งเดียว)
POST /dashboard/v1/api_keys/{id}/rotate   → key ใหม่ + เก่า grace 24h
POST /dashboard/v1/api_keys/{id}/revoke
```

### 9.6 Usage & Conversations
```
GET /dashboard/v1/usage?from=&to=&group_by=day|agent|api_key
GET /dashboard/v1/conversations?agent_id=
GET /dashboard/v1/conversations/{id}
GET /dashboard/v1/audit_logs?limit=          (P3)
```

### 9.7 Internal / Ops (staff only, `role=anthovai_staff` ใน JWT)
```
GET  /internal/health          → {status, db, storage, providers:{openai:"healthy",anthropic:"healthy"}, queue_depth}
GET  /internal/metrics         → Prometheus text
POST /internal/orgs/{id}/plan  {plan, limits}
```

---

## 10. Rate Limits (P1 defaults)

| ขอบเขต | Free | Starter | Business | Enterprise |
|--------|------|---------|----------|------------|
| req/min ต่อ API key | 20 | 60 | 300 | custom |
| concurrent streams ต่อ key | 2 | 5 | 20 | custom |
| upload/hour ต่อ org | 20 | 100 | 1,000 | custom |
| messages/month (quota) | 1,000 | 10,000 | 100,000 | custom |

Algorithm: sliding window ใน Redis (P2) / PG advisory + in-memory per instance (P1)

## 11. Versioning & Deprecation
- Path version `/v1`; field ใหม่เพิ่มได้ตลอด (additive); ลบ/เปลี่ยน type = breaking
- Deprecation ประกาศล่วงหน้า 6 เดือน พร้อม header `Deprecation: true`, `Sunset: <date>`

## 12. OpenAPI
- สร้าง `openapi.yaml` จาก Rust types ด้วย `utoipa` และ serve ที่ `/v1/openapi.json`; docs site อ่านจากไฟล์นี้
- Snapshot test: openapi.json ที่ generate ต้องตรงกับไฟล์ที่ commit ไว้ (ป้องกัน contract เปลี่ยนโดยไม่ตั้งใจ)
