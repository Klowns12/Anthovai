# แผนเสียบ Thai OCR เข้า Anthovai Platform

> สถานะ (2026-09-16): **เสร็จและ merge เข้า main แล้ว** — ทุกอย่างในเอกสารนี้ทำครบ
> และพิสูจน์ผ่านสแตกจริง ไม่ใช่แค่ unit test
>
> - PR #2 · OCR sidecar + `PdfParser::with_ocr` + `[ocr]` config + worker wiring
> - PR #2 รอบ 2 · `SourceType::Image` (PNG/JPEG/WebP), `ImageParser`,
>   migration `0008_image_documents.sql`, API ปฏิเสธรูปด้วย `ocr_not_enabled` เมื่อ OCR ปิด
> - PR #6 · แก้ image MinIO ที่ถูกถอนจาก Docker Hub + ให้ API ประกาศสถานะ OCR ตอน start
>
> **พิสูจน์ end-to-end บนสแตกเต็ม** (`--profile ocr`, API + worker + sidecar เป็น container):
> อัปโหลด PDF สแกนที่ไม่มี text layer และรูปถ่ายหน้ากระดาษผ่าน dashboard API จริง
> ทั้งคู่ถึงสถานะ `ready` — อย่างละ 3 chunks, `language = tha`, `ocr: true` ทุก chunk,
> และ PDF 2 หน้าเก็บเลขหน้า 1,2 ไว้ครบ · ข้อความไทยอยู่ใน index จริง:
> `DN-690912-03`, `อิฐมอญ`, `279,007.85`, `QT-6909-0087`, `ประยูร`
>
> ผลข้างเคียงที่แก้ไปด้วย: `parse_timeout` เดิมถูก map เป็น permanent ทั้งที่ doc บอกว่า
> retry — ตอนนี้ `IngestError::from_parse` แยก transient ออกจริง

## หลักการ

Platform เป็น Rust; Typhoon OCR เป็นโมเดล vision ที่รันผ่าน Ollama/vLLM
→ ทำเป็น **OCR sidecar**: service เล็ก ๆ คุยกับ inference server แล้วให้
`anthovai-worker` เรียกผ่าน HTTP ภายในเครื่อง ไม่ต้องยัด ML runtime เข้า Rust

```
anthovai-worker (Rust)
   └─ ingestion crate
        ├─ PDF มี text layer  → pdf-extract (เดิม, ไม่แตะ)
        ├─ PDF สแกน / รูปภาพ  → POST ocr-sidecar:9090/ocr  ← ใหม่
        └─ DOCX/TXT/HTML/...  → เดิม, ไม่แตะ

ocr-sidecar (Python FastAPI, container เดียว)
   ├─ pdftoppm แปลงหน้า PDF → PNG 200dpi
   ├─ เรียก Ollama /api/generate (typhoon-ocr) ทีละหน้า
   └─ คืน markdown ต่อหน้า + confidence + เวลา
```

## เกณฑ์เข้า OCR path

ใน `ingestion`: ถ้า `pdf-extract` ได้ข้อความ < N ตัวอักษร/หน้า (เช่น 50)
หรือ MIME เป็น image/* → ส่งเข้า sidecar; บันทึก `documents.ocr=true`
เพื่อให้ UI แสดงได้ว่าเอกสารนี้ผ่าน OCR (ความแม่นยำต่างจาก text layer)

## สัญญา API ของ sidecar (ร่าง)

```
GET  /health  → { status, model, target_px, min_px }   worker เช็คก่อนส่งงาน

POST /ocr     { "image_b64": "..." } | { "pdf_b64": "..." }
           → { model, total_ms, pages: [ { page, markdown, ms } ] }
           422 ถ้าภาพด้านยาว < OCR_MIN_PX (ให้ต้นทางถ่ายใหม่ ไม่รับผลเพี้ยน)

POST /extract { "markdown": "...", "expected": <PO|null> }
           → { doc_type, doc_no, date, site, receiver, line_items[], totals,
               exceptions[], reconciliation, checks{}, needs_review, review_reasons[] }
           สเปกเช็ค: docs/delivery-note-checks.md
```

ลำดับใน worker: `/ocr` → (ต่อ markdown ทุกหน้า) → `/extract` → ถ้า `needs_review`
เข้าคิวคนตรวจพร้อม `review_reasons`; ไม่งั้นบันทึกเข้าระบบต้นทุนอัตโนมัติ

## สิ่งที่ benchmark รอบแรกบอกแล้ว (2026-09-13, ยังไม่ครบ 8 ภาพ)

- ตาราง + จำนวนเงิน: แม่นมาก (ใบเสนอราคา clean = 10/10)
- จุดอ่อน: ชื่อเฉพาะ/เลขผู้เสียภาษีในส่วนหัวที่ตัวอักษรเล็ก — เพี้ยนหรือตกหลัก
- แนวแก้ที่ต้องทดลองต่อ:
  1. render/สแกนที่ 200dpi ขึ้นไป (ตอนนี้ 150dpi)
  2. ส่ง dimension จริงใน prompt ตาม format ของ typhoon-ocr package
  3. ถ้ายังไม่พอ → ลอง typhoon-ocr-7b (ต้องการ VRAM มากขึ้น; เครื่อง production
     ของลูกค้าค่อยกำหนดสเปกตามนี้)

## ผลกระทบต่อการขาย (one-pager)

ห้ามเคลม "อ่านถูก 100%" — เคลมที่พิสูจน์ได้: "ตารางและตัวเลขแม่นยำสูง,
ทุกคำตอบอ้างอิงหน้าเอกสารต้นทางให้ตรวจได้                          "
จุดตรวจมนุษย์ (human review) เป็นฟีเจอร์ ไม่ใช่ข้อแก้ตัว
