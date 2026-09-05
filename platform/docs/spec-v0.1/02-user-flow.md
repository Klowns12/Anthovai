# 02 — Complete User Flow

เอกสารนี้อธิบาย flow ของผู้ใช้ 2 กลุ่ม: **Customer Admin** (ใช้ Dashboard) และ **Developer/End User** (ใช้ Public API) และ state machine ที่เกี่ยวข้อง

## 1. Personas

| Persona | ใช้อะไร | ต้องการอะไร |
|---------|---------|-------------|
| Owner / Admin | Dashboard | สร้าง org, เชิญทีม, สร้าง agent, จัดการ billing |
| Editor | Dashboard | จัดการ knowledge, ปรับ agent, ทดสอบ |
| Developer | Dashboard + API | สร้าง API key, อ่าน docs, ต่อ API |
| End User | แอปของลูกค้า | ถามคำถาม ได้คำตอบที่ถูกต้อง มีแหล่งอ้างอิง |
| Anthovai Staff | Internal admin | ดู usage, health, support, override plan |

## 2. Onboarding Flow

```
[Landing] → Sign up (email + password หรือ magic link)
   ↓
Verify email
   ↓
Create Organization  ──▶ org.slug, plan=free, owner=user
   ↓
Auto-create Workspace "Default"
   ↓
Onboarding wizard (skippable):
   Step 1: Create your first Agent  (name, language, tone)
   Step 2: Add knowledge          (upload file / paste URL / paste text)
   Step 3: Test it                (chat panel, เห็น sources)
   Step 4: Get API key            (copy snippet)
   ↓
Dashboard Home
```

**Acceptance:** ผู้ใช้ใหม่ทำครบ 4 step ได้ภายใน 5 นาทีด้วย PDF 1 ไฟล์

## 3. Organization & Workspace

```
User ──belongs to (role)──▶ Organization ──has──▶ Workspaces ──has──▶ Agents, Knowledge Bases, API Keys
```

- ผู้ใช้หนึ่งคนอยู่ได้หลาย organization
- Role ระดับ org: `owner`, `admin`, `editor`, `viewer` (รายละเอียดใน 07)
- Workspace เป็นเพียงการจัดกลุ่ม ไม่ใช่ขอบเขต isolation (isolation = organization)
- API Key ผูกกับ workspace และมี scope ระบุ agent ที่เรียกได้ (default: ทุก agent ใน workspace)

## 4. Agent Lifecycle

### 4.1 สร้าง Agent
```
[+ Create Agent]
  Name*                      "ABC School Assistant"
  Description                
  Language                   Thai / English / Auto
  Instructions (system)      textarea + template picker
  Model                      ● Anthovai Auto  ○ Advanced (OpenAI / Claude)   ← ตาม plan
  Reasoning                  ○ Fast  ● Balanced  ○ Deep
  Response length            ○ Short ● Balanced  ○ Detailed
  Knowledge Bases            [ ] Student Handbook  [ ] Course Catalog  [+ New]
  Behavior
     [x] Answer only from knowledge
     [x] Show citations
     Fallback message        "ขออภัย ฉันไม่มีข้อมูลเรื่องนี้"
  [Create]
```

- สร้างแล้ว `status = draft` จนกว่าจะมี KB อย่างน้อยหนึ่งที่ READY หรือผู้ใช้กด Publish
- ทุกครั้งที่กด Save จะสร้าง `agent_versions` ใหม่; public API ใช้ `published_version_id`

### 4.2 Agent Status
```
draft ──publish──▶ active ──pause──▶ paused ──resume──▶ active
  │                  │
  └──────delete──────┴──▶ archived (soft delete, keys ปฏิเสธ 410)
```

### 4.3 Test Agent (Playground)
- แผง chat ใน dashboard เรียก `POST /dashboard/v1/agents/{id}/test` ใช้ **draft version** (ไม่ใช่ published)
- แสดง answer, sources (เปิดดู chunk ได้), model ที่ใช้จริง (staff/Business+ เห็นชื่อ provider), latency, tokens
- ปุ่ม "Why this answer?" แสดง retrieved chunks พร้อม score (debug mode)

## 5. Knowledge Flow (มุมมองผู้ใช้)

```
Knowledge Bases
  ● Student Handbook     12 docs   READY
  ● Course Catalog        3 docs   PROCESSING (1 pending)
  [+ New Knowledge Base]

Knowledge Base: Student Handbook
  [Upload files] [Add URL] [Add text] [Add JSON/CSV]

  Name                    Type   Size    Status      Updated
  handbook-2026.pdf       PDF    2.1 MB  READY       2m ago
  faq.json                JSON   40 KB   EMBEDDING ▓▓▓▓▓░░ 70%
  https://abc.ac.th/adm   URL    —       FAILED ⓘ    ...   [Retry]
```

### 5.1 Document Status Machine
```
UPLOADING → QUEUED → PROCESSING → CHUNKING → EMBEDDING → INDEXING → READY
                          │           │           │          │
                          └───────────┴───────────┴──────────┴──▶ FAILED (error_code, retryable?)
READY ──re-upload──▶ (new version) QUEUED ...   (old chunks ยังใช้ได้จน version ใหม่ READY แล้ว swap)
READY ──delete──▶ DELETED (chunks hard-delete ภายใน 24h, original ลบจาก storage)
```

### 5.2 ข้อจำกัดตาม Plan (P1 ใช้ค่า hard-coded, P4 ย้ายไป plans table)
| Plan | KB storage | Docs/KB | Max file | Agents | Messages/mo | Model choice |
|------|-----------|---------|----------|--------|-------------|--------------|
| Free | 100 MB | 50 | 10 MB | 1 | 1,000 | Auto |
| Starter | 1 GB | 500 | 25 MB | 3 | 10,000 | Auto |
| Business | 10 GB | 5,000 | 50 MB | 10 | 100,000 | Auto / OpenAI / Claude |
| Enterprise | Custom | Custom | 200 MB | Unlimited | Custom | + model_policy custom |

## 6. API Key Flow

```
[API Keys] → [+ Create key]
   Name           "Production website"
   Workspace      Default
   Agents         ● All in workspace  ○ Selected: [ABC School Assistant]
   Expires        ○ Never ● 90 days ○ Custom
   [Create]
   ┌──────────────────────────────────────────────────────┐
   │ Copy your key now. It will not be shown again.       │
   │ av_live_3f9c...k2Qa                     [Copy]       │
   └──────────────────────────────────────────────────────┘
List: name, prefix (av_live_3f9c…), created, last used, status, [Rotate] [Revoke]
```

- `av_live_` สำหรับ production, `av_test_` สำหรับ sandbox (ไม่นับ quota, จำกัด 100 req/day) — P2
- Rotate = สร้าง key ใหม่ + key เก่าอยู่ใน grace period 24h แล้ว revoke อัตโนมัติ

## 7. Integration Flow (Developer)

```
Dashboard → Agent → [Integrate]
  Tab: cURL | JavaScript | Python | Widget (P3)
```
ตัวอย่างที่แสดง:
```js
const res = await fetch("https://api.anthovai.com/v1/chat", {
  method: "POST",
  headers: { "Authorization": "Bearer av_live_...", "Content-Type": "application/json" },
  body: JSON.stringify({ agent_id: "agt_01J...", message: "สมัครเรียนต้องทำอย่างไร?" })
});
const { answer, sources, conversation_id } = await res.json();
```
- ส่ง `conversation_id` กลับมาในคำถามถัดไปเพื่อให้มี memory ของบทสนทนา
- ใช้ `/v1/chat/stream` เพื่อ SSE

## 8. End-User Flow (ผ่านแอปของลูกค้า)

```
End user พิมพ์คำถาม
  ↓ (app ของลูกค้าเรียก Anthovai API ด้วย server-side key)
Anthovai: auth → agent → RAG → LLM → answer + sources
  ↓
App แสดงคำตอบ + ลิงก์อ้างอิง (title, page)
  ↓ (optional) end user กด 👍/👎 → POST /v1/feedback  (P2)
```
**สำคัญ:** API key ห้ามอยู่ใน browser ของ end user; P3 จะมี Widget token แบบ short-lived สำหรับ embed ตรงในเว็บ

## 9. Team Flow (P3)
- Invite by email → role → accept → เข้าถึง org
- Audit log ทุก action ที่เปลี่ยน agent/KB/key

## 10. Billing Flow (P4, สรุปเพื่อให้ schema รองรับ)
- Free → Upgrade → Stripe Checkout → subscription active → quota update
- เกิน quota: soft-limit แจ้งเตือนที่ 80%, hard-limit ตอบ `429 quota_exceeded` พร้อม `X-Quota-Reset`

## 11. Error/Edge Cases ที่ UI ต้องรองรับ

| กรณี | พฤติกรรม |
|------|----------|
| Upload ไฟล์เกินขนาด | reject ทันทีที่ client + server 413 |
| ไฟล์ parse ไม่ได้ (scan PDF ไม่มี text) | FAILED `no_extractable_text` พร้อมคำแนะนำ (OCR = Future) |
| URL ดึงไม่ได้ / robots block | FAILED `fetch_failed` แสดง HTTP status |
| Agent ไม่มี KB READY | Playground ยังใช้ได้แต่แจ้ง "answering without knowledge"; public API ตอบ fallback |
| Provider ล่มทั้งสอง | 503 `provider_unavailable` + `Retry-After` |
| Quota เกิน | 429 `quota_exceeded` |
