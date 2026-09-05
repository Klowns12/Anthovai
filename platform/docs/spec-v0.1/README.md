# Anthovai AI Platform — Technical Specification v0.1

สถานะ: **Draft** · วันที่: 2026-09-03 · เจ้าของ: Anthovai

เอกสารชุดนี้เป็น "สัญญา" ทางเทคนิคของ Anthovai AI Platform (Multi-tenant RAG-as-a-Service / AI Agent SaaS) เขียนขึ้นเพื่อใช้เป็น input ให้ AI Coding Agent และทีมพัฒนาก่อนเริ่มเขียนโค้ด Rust จริง เป้าหมายคือให้ architecture ถูกล็อกไว้ก่อน ลดปัญหาโค้ดกระจัดกระจายและเปลี่ยนแบบกลางทาง

## สารบัญ

| # | เอกสาร | เนื้อหา |
|---|--------|---------|
| 01 | [System Architecture](01-system-architecture.md) | ภาพรวมระบบ, 3 layers, component, deployment topology, หลักการออกแบบ |
| 02 | [User Flow](02-user-flow.md) | Flow ตั้งแต่สมัคร → สร้าง Agent → Upload → Test → API Key → Go Live |
| 03 | [RAG Flow](03-rag-flow.md) | Ingestion pipeline และ Query runtime pipeline แบบละเอียด รวม chunking/embedding/rerank/prompt |
| 04 | [Database ERD](04-database-erd.md) | ตาราง, ความสัมพันธ์, DDL PostgreSQL + pgvector, index, RLS |
| 05 | [API Specification](05-api-specification.md) | REST API v1, auth, request/response schema, streaming, error format, rate limit |
| 06 | [Rust Workspace Architecture](06-rust-workspace-architecture.md) | Cargo workspace, crates, dependency rules, trait หลัก, config, worker |
| 07 | [Security & Multi-tenancy](07-security-multitenancy.md) | Tenant isolation, API key, RBAC, secrets, guardrails, audit, threat model |
| 08 | [P1 Implementation Checklist](08-p1-implementation-checklist.md) | รายการงาน P1 ตามลำดับ พร้อม acceptance criteria และ Definition of Done |
| 09 | [Development Plan M4–M9](09-development-plan-m4-m9.md) | แผนลงมือทำต่อจาก Milestone 3 จัดลำดับแบบ vertical slice, dependencies ที่เลือก, ความเสี่ยง, สิ่งที่ต้องการจากฝั่งธุรกิจ |

## หลักการที่ล็อกไว้ (Non-negotiable ใน v0.1)

1. **Customer Knowledge ≠ Model Training** — ใช้ RAG ไม่ fine-tune ต่อลูกค้า
2. **Tenant isolation ตั้งแต่วันแรก** — ทุก query ต้องมี `tenant_id` filter, ห้าม retrieval ข้าม tenant
3. **API ของ Anthovai ไม่ผูกกับ provider** — ลูกค้าเห็น `model_policy` ไม่เห็นชื่อ model ของ OpenAI/Anthropic
4. **Rust modular monolith ก่อน** — แยก crate ตาม domain แต่ deploy เป็น `api` + `worker` 2 binaries
5. **PostgreSQL + pgvector เป็น store เดียว** ในระยะแรก, S3-compatible สำหรับไฟล์ต้นฉบับ
6. **Default model policy = `anthovai_auto`** — Anthovai คุม cost และคุณภาพเอง

## คำศัพท์

| คำ | ความหมาย |
|----|-----------|
| Tenant | Organization หนึ่งราย = ขอบเขต isolation ของข้อมูล |
| Workspace | โปรเจกต์ย่อยภายใน Organization (เช่น Support, HR) |
| Agent | Configuration ของผู้ช่วย AI (instructions, model policy, knowledge bases, guardrails) ไม่ใช่ model |
| Knowledge Base (KB) | กลุ่มของ documents ที่ถูก index แล้วสำหรับ retrieval |
| Document | ไฟล์/URL/JSON หนึ่งชิ้นที่ลูกค้าอัปโหลด |
| Chunk | ส่วนย่อยของ document ที่มี embedding |
| Model Policy | นโยบายเลือก model: `anthovai_auto`, `openai_only`, `claude_only`, `custom` |
| Provider | ผู้ให้บริการ model ภายนอก (OpenAI, Anthropic) |

## Versioning ของเอกสาร

- v0.1 = ร่างแรกสำหรับ P0/P1 เท่านั้น เนื้อหา P2+ ระบุเป็น "Future" และไม่ผูกมัด
- เปลี่ยนแปลง schema/API ต้องอัปเดตเอกสาร 04/05 ก่อน merge โค้ด
