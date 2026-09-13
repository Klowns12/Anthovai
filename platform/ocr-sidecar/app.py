# -*- coding: utf-8 -*-
"""Anthovai OCR Sidecar v0.1

แปลงเอกสารสแกน/รูปภาพภาษาไทยเป็น markdown ด้วย Typhoon OCR ผ่าน Ollama
ออกแบบให้ anthovai-worker (Rust) เรียกผ่าน HTTP ภายในเครื่อง — ไม่มีข้อมูลออกนอกองค์กร

    uvicorn app:app --host 127.0.0.1 --port 9090

ENV:
    OCR_OLLAMA_URL   default http://localhost:11434
    OCR_MODEL        default scb10x/typhoon-ocr-3b
    OCR_RENDER_DPI   default 240   (ความละเอียดตอนแปลงหน้า PDF เป็นภาพ)
    OCR_MAX_PAGES    default 200
"""
import base64
import binascii
import io
import json
import os
import re
import time
import urllib.error
import urllib.request

import pypdfium2 as pdfium  # BSD/Apache — ตั้งใจเลี่ยง PyMuPDF (AGPL)
from fastapi import FastAPI, HTTPException
from fastapi.responses import FileResponse
from PIL import Image
from pydantic import BaseModel

from extract import extract

OLLAMA = os.environ.get("OCR_OLLAMA_URL", "http://localhost:11434")
# สูตรที่ชนะ benchmark 2026-09-13 (docs/benchmark-2026-09-13.md): 1.5-3B + prompt ทางการ + ด้านยาว 1800px
MODEL = os.environ.get("OCR_MODEL", "scb10x/typhoon-ocr1.5-3b")
TARGET_PX = int(os.environ.get("OCR_TARGET_PX", "1800"))  # model card: trained at fixed 1800 px
MAX_PAGES = int(os.environ.get("OCR_MAX_PAGES", "200"))
# ภาพเล็กกว่านี้ (ด้านยาว) โมเดลเริ่มอ่านเลขเพี้ยน → ปฏิเสธตั้งแต่ทางเข้าดีกว่าให้ผลผิดเงียบ ๆ
MIN_PX = int(os.environ.get("OCR_MIN_PX", "1200"))

# prompt ทางการของ typhoon-ocr 1.5 (model card) — ตอบ markdown ตรง ๆ ตารางเป็น HTML
PROMPT = (
    "Extract all text from the image. Instructions: Only return the clean Markdown. "
    "Do not include any explanation or extra text. You must include all information "
    "on the page. Formatting Rules: Tables using HTML, Equations using LaTeX, "
    "Images/Charts/Diagrams wrapped in <figure> tags, Page Numbers in <page_number> "
    "tags, Checkboxes using ☐/☑."
)

app = FastAPI(title="Anthovai OCR Sidecar", version="0.1.0")


class OcrRequest(BaseModel):
    # อย่างใดอย่างหนึ่ง: ภาพเดี่ยว (PNG/JPEG) หรือ PDF ทั้งไฟล์ — base64 ทั้งคู่
    image_b64: str | None = None
    pdf_b64: str | None = None


class PageResult(BaseModel):
    page: int
    markdown: str
    ms: int


class OcrResponse(BaseModel):
    model: str
    pages: list[PageResult]
    total_ms: int


class ExtractRequest(BaseModel):
    markdown: str                 # ผลจาก /ocr (หน้าเดียว หรือต่อหลายหน้าแล้ว)
    expected: dict | None = None  # ใบสั่งซื้อ {po_no, items:[{description, qty, unit}]} ถ้ามี


def _ollama_generate(image_b64: str, timeout: int = 1800) -> str:
    req = urllib.request.Request(
        OLLAMA + "/api/generate",
        data=json.dumps({
            "model": MODEL,
            "prompt": PROMPT,
            "images": [image_b64],
            "stream": False,
            "options": {"temperature": 0, "num_predict": 4096},
        }).encode(),
        headers={"Content-Type": "application/json"},
    )
    try:
        with urllib.request.urlopen(req, timeout=timeout) as r:
            resp = json.loads(r.read())
    except urllib.error.URLError as e:
        raise HTTPException(502, f"inference server unreachable: {e}") from e
    text = resp.get("response", "")
    # โมเดลตอบ {"natural_text": ...} — แกะไม่ได้ก็คืนดิบ ดีกว่าคืนว่าง
    try:
        return json.loads(text)["natural_text"]
    except (json.JSONDecodeError, KeyError, TypeError):
        return text


def _fit_longest_side(im: Image.Image, target: int) -> Image.Image:
    """สูตร resize_if_needed จาก model card: ด้านยาว = target, รักษาสัดส่วน"""
    w, h = im.size
    if w >= h:
        new = (target, max(1, int(h * target / float(w))))
    else:
        new = (max(1, int(w * target / float(h))), target)
    return im.resize(new, Image.Resampling.LANCZOS)


def _to_png_b64(im: Image.Image) -> str:
    buf = io.BytesIO()
    im.convert("RGB").save(buf, format="PNG")
    return base64.b64encode(buf.getvalue()).decode()


def _prepare_image_b64(image_b64: str) -> str:
    """ภาพจากผู้ใช้ → ตรวจขนาดขั้นต่ำ → resize เป็น TARGET_PX → PNG"""
    try:
        raw = base64.b64decode(image_b64, validate=True)
        im = Image.open(io.BytesIO(raw))
        im.load()
    except (binascii.Error, OSError) as e:
        raise HTTPException(400, f"unreadable image: {e}") from e
    if max(im.size) < MIN_PX:
        # ต่ำกว่านี้โมเดลอ่านเลขเพี้ยนโดยไม่เตือน (benchmark photo-stress) — ให้ต้นทางถ่ายใหม่
        raise HTTPException(
            422, f"image too small ({im.size[0]}x{im.size[1]}); longest side must be >= {MIN_PX}px")
    return _to_png_b64(_fit_longest_side(im, TARGET_PX))


def _pdf_pages_to_png_b64(pdf_bytes: bytes) -> list[str]:
    try:
        doc = pdfium.PdfDocument(pdf_bytes)
    except Exception as e:
        raise HTTPException(400, f"unreadable PDF: {e}") from e
    n = len(doc)
    if n > MAX_PAGES:
        raise HTTPException(413, f"PDF has {n} pages; limit is {MAX_PAGES}")
    out = []
    for i in range(n):
        page = doc[i]
        w_pt, h_pt = page.get_size()
        # render ให้ด้านยาวตรง TARGET_PX เลย ไม่ต้อง render ใหญ่แล้วย่อ
        scale = TARGET_PX / float(max(w_pt, h_pt))
        out.append(_to_png_b64(page.render(scale=scale).to_pil()))
    return out


HERE = os.path.dirname(os.path.abspath(__file__))


@app.get("/", include_in_schema=False)
def demo():
    """A page to drop a Thai document on and watch it be read.

    Served from the sidecar itself so trying it is one command rather than a
    second server. The sidecar is never published outside the machine — compose
    only `expose`s it, and locally it binds loopback — so this adds no surface
    that was not already there.

    It is also the demo to put in front of a customer: it shows the reading, the
    fields pulled out of it, and the arithmetic that decides whether a document
    can be trusted without a person looking at it.
    """
    return FileResponse(os.path.join(HERE, "demo.html"), media_type="text/html")


SAMPLE_LABELS = {
    "03_delivery_note.png": "ใบส่งของ",
    "01_tax_invoice.png": "ใบกำกับภาษี",
    "02_quotation.png": "ใบเสนอราคา",
}


@app.get("/samples", include_in_schema=False)
def samples():
    """Which sample documents this checkout actually has.

    The page asks rather than probing each file: a HEAD against a GET route
    answers 405, so probing reported "missing" for files that were there.
    """
    here = os.path.join(HERE, "samples")
    return [
        {"name": name, "label": label}
        for name, label in SAMPLE_LABELS.items()
        if os.path.isfile(os.path.join(here, name))
    ]


@app.get("/samples/{name}", include_in_schema=False)
def sample(name: str):
    """One of the synthetic Thai documents, if they were put next to this file."""
    if not re.fullmatch(r"[0-9A-Za-z_.-]{1,64}\.(png|jpg|jpeg|webp|pdf)", name):
        raise HTTPException(404, "no such sample")
    path = os.path.join(HERE, "samples", name)
    if not os.path.isfile(path):
        raise HTTPException(404, "no such sample")
    return FileResponse(path)


@app.get("/health")
def health():
    """โหลดโมเดลพร้อมหรือยัง — worker ใช้เช็คก่อนส่งงาน"""
    try:
        with urllib.request.urlopen(OLLAMA + "/api/tags", timeout=5) as r:
            tags = json.loads(r.read())
    except urllib.error.URLError as e:
        raise HTTPException(503, f"ollama unreachable: {e}") from e
    names = [m.get("name", "") for m in tags.get("models", [])]
    if not any(n.startswith(MODEL) for n in names):
        raise HTTPException(503, f"model {MODEL} not pulled; have: {names}")
    return {"status": "ok", "model": MODEL, "target_px": TARGET_PX, "min_px": MIN_PX}


@app.post("/extract")
def extract_endpoint(req: ExtractRequest):
    """markdown จาก OCR → ข้อมูลโครงสร้าง + checks + needs_review/review_reasons
    ใบส่งของควรส่ง `expected` (PO) มาด้วย ไม่งั้นยืนยันจำนวนไม่ได้และจะถูกส่งเข้าคิว review เสมอ"""
    return extract(req.markdown, req.expected)


@app.post("/ocr", response_model=OcrResponse)
def ocr(req: OcrRequest):
    if bool(req.image_b64) == bool(req.pdf_b64):
        raise HTTPException(400, "send exactly one of image_b64 or pdf_b64")
    t0 = time.time()
    if req.image_b64:
        images = [_prepare_image_b64(req.image_b64)]
    else:
        try:
            pdf_bytes = base64.b64decode(req.pdf_b64, validate=True)
        except binascii.Error as e:
            raise HTTPException(400, f"invalid base64: {e}") from e
        images = _pdf_pages_to_png_b64(pdf_bytes)
    pages = []
    for i, img in enumerate(images, 1):
        p0 = time.time()
        md = _ollama_generate(img)
        pages.append(PageResult(page=i, markdown=md, ms=int((time.time() - p0) * 1000)))
    return OcrResponse(model=MODEL, pages=pages, total_ms=int((time.time() - t0) * 1000))
