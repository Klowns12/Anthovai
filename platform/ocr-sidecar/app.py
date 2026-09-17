# -*- coding: utf-8 -*-
"""Anthovai OCR Sidecar v0.1

แปลงเอกสารสแกน/รูปภาพภาษาไทยเป็น markdown ให้ anthovai-worker เรียกผ่าน HTTP
ภายในเครื่อง — ไม่มีไฟล์ต้นทางออกนอก process นี้ นอกจาก inference ที่ตั้งไว้

    uvicorn app:app --host 127.0.0.1 --port 9090

Backends:
    api     OpenAI-compatible Chat Completions + vision (default when an API key is set)
            OpenAI:     OCR_API_KEY / OPENAI_API_KEY  → https://api.openai.com/v1
            Typhoon:    TYPHOON_OCR_API_KEY           → https://api.opentyphoon.ai/v1
    ollama  local Typhoon OCR via Ollama (OCR_BACKEND=ollama)

ENV:
    OCR_BACKEND      auto | api | ollama     default auto
    OCR_API_BASE_URL OpenAI-compatible root  default depends on which key is set
    OCR_API_KEY      bearer token            falls back to OPENAI_API_KEY / TYPHOON_OCR_API_KEY
    OCR_MODEL        default gpt-5.4-mini (api) or scb10x/typhoon-ocr1.5-3b (ollama)
    OCR_OLLAMA_URL   default http://localhost:11434
    OCR_TARGET_PX    default 1800
    OCR_MAX_PAGES    default 200
    OCR_MIN_PX       default 1200
    OCR_DEMO_ORIGINS CORS allow-list for the public /demo page
    OCR_DEMO_LIMIT   browser OCR calls per IP per hour (default 20)
    OCR_DEMO_MAX_PAGES browser PDF page cap (default 3)
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
from collections import defaultdict, deque

import pypdfium2 as pdfium  # BSD/Apache — ตั้งใจเลี่ยง PyMuPDF (AGPL)
from fastapi import FastAPI, HTTPException, Request
from fastapi.middleware.cors import CORSMiddleware
from fastapi.responses import FileResponse
from PIL import Image
from pydantic import BaseModel

from extract import extract


def _load_dotenv() -> None:
    """Pick up platform/.env when uvicorn is started next to this file."""
    here = os.path.dirname(os.path.abspath(__file__))
    candidates = (
        os.path.join(here, "..", ".env"),
        os.path.join(here, ".env"),
    )
    for path in candidates:
        if not os.path.isfile(path):
            continue
        with open(path, encoding="utf-8") as fh:
            for raw in fh:
                line = raw.strip()
                if not line or line.startswith("#") or "=" not in line:
                    continue
                key, value = line.split("=", 1)
                key = key.strip()
                if key and key not in os.environ:
                    os.environ[key] = value.strip().strip('"').strip("'")
        break


_load_dotenv()

OLLAMA = os.environ.get("OCR_OLLAMA_URL", "http://localhost:11434")
TYPHOON_API_BASE = "https://api.opentyphoon.ai/v1"
OPENAI_API_BASE = "https://api.openai.com/v1"


def _first_env(*names: str) -> str:
    for name in names:
        value = os.environ.get(name, "").strip()
        if value:
            return value
    return ""


TYPHOON_KEY = _first_env("TYPHOON_OCR_API_KEY")
OPENAI_KEY = _first_env("OCR_API_KEY", "OPENAI_API_KEY")
API_KEY = TYPHOON_KEY or OPENAI_KEY


def _resolve_backend() -> str:
    chosen = os.environ.get("OCR_BACKEND", "auto").strip().lower()
    if chosen in ("api", "ollama"):
        return chosen
    return "api" if API_KEY else "ollama"


BACKEND = _resolve_backend()
if BACKEND == "api" and TYPHOON_KEY:
    API_BASE = os.environ.get("OCR_API_BASE_URL", TYPHOON_API_BASE).rstrip("/")
    DEFAULT_MODEL = "typhoon-ocr"
elif BACKEND == "api":
    API_BASE = os.environ.get("OCR_API_BASE_URL", OPENAI_API_BASE).rstrip("/")
    DEFAULT_MODEL = "gpt-5.4-mini"
else:
    API_BASE = ""
    DEFAULT_MODEL = "scb10x/typhoon-ocr1.5-3b"

MODEL = os.environ.get("OCR_MODEL", DEFAULT_MODEL)
TARGET_PX = int(os.environ.get("OCR_TARGET_PX", "1800"))
MAX_PAGES = int(os.environ.get("OCR_MAX_PAGES", "200"))
MIN_PX = int(os.environ.get("OCR_MIN_PX", "1200"))

PROMPT = (
    "Extract all text from the image. Instructions: Only return the clean Markdown. "
    "Do not include any explanation or extra text. You must include all information "
    "on the page. Formatting Rules: Tables using HTML, Equations using LaTeX, "
    "Images/Charts/Diagrams wrapped in <figure> tags, Page Numbers in <page_number> "
    "tags, Checkboxes using ☐/☑."
)

app = FastAPI(title="Anthovai OCR Sidecar", version="0.1.0")

_DEMO_ORIGINS = [
    origin.strip()
    for origin in os.environ.get("OCR_DEMO_ORIGINS", "").split(",")
    if origin.strip()
]
for _loopback in ("http://localhost:3000", "http://127.0.0.1:3000"):
    if _loopback not in _DEMO_ORIGINS:
        _DEMO_ORIGINS.append(_loopback)
if _DEMO_ORIGINS:
    app.add_middleware(
        CORSMiddleware,
        allow_origins=_DEMO_ORIGINS,
        allow_methods=["GET", "POST", "OPTIONS"],
        allow_headers=["Content-Type"],
        max_age=600,
    )

_OCR_HITS: dict[str, deque[float]] = defaultdict(deque)
_DEMO_LIMIT = int(os.environ.get("OCR_DEMO_LIMIT", "20"))
_DEMO_WINDOW = 3600
_DEMO_MAX_PAGES = int(os.environ.get("OCR_DEMO_MAX_PAGES", "3"))


def _client_ip(request: Request) -> str:
    forwarded = request.headers.get("x-forwarded-for", "")
    if forwarded:
        return forwarded.split(",")[0].strip()
    return request.client.host if request.client else "unknown"


def _enforce_browser_limits(request: Request, page_count: int) -> None:
    if not request.headers.get("origin"):
        return
    if page_count > _DEMO_MAX_PAGES:
        raise HTTPException(413, f"demo limit is {_DEMO_MAX_PAGES} pages per file")
    now = time.time()
    bucket = _OCR_HITS[_client_ip(request)]
    while bucket and bucket[0] < now - _DEMO_WINDOW:
        bucket.popleft()
    if len(bucket) >= _DEMO_LIMIT:
        raise HTTPException(
            429,
            "เดโมจำกัดจำนวนครั้งต่อชั่วโมง — ลองใหม่ภายหลังหรือนัดดูกับเอกสารของคุณ",
        )
    bucket.append(now)


class OcrRequest(BaseModel):
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
    markdown: str
    expected: dict | None = None


def _json_request(url: str, payload: dict, headers: dict, timeout: int) -> dict:
    req = urllib.request.Request(
        url,
        data=json.dumps(payload).encode(),
        headers=headers,
        method="POST",
    )
    try:
        with urllib.request.urlopen(req, timeout=timeout) as r:
            return json.loads(r.read())
    except urllib.error.HTTPError as e:
        detail = e.read().decode("utf-8", errors="replace")[:800]
        raise HTTPException(502, f"inference HTTP {e.code}: {detail}") from e
    except urllib.error.URLError as e:
        raise HTTPException(502, f"inference server unreachable: {e}") from e


def _unwrap_text(text: str) -> str:
    try:
        return json.loads(text)["natural_text"]
    except (json.JSONDecodeError, KeyError, TypeError):
        return text


def _ollama_generate(image_b64: str, timeout: int = 1800) -> str:
    resp = _json_request(
        OLLAMA + "/api/generate",
        {
            "model": MODEL,
            "prompt": PROMPT,
            "images": [image_b64],
            "stream": False,
            "options": {"temperature": 0, "num_predict": 4096},
        },
        {"Content-Type": "application/json"},
        timeout,
    )
    return _unwrap_text(resp.get("response", ""))


def _api_generate(image_b64: str, timeout: int = 1800) -> str:
    if not API_KEY:
        raise HTTPException(
            503,
            "OCR API backend needs OCR_API_KEY, OPENAI_API_KEY, or TYPHOON_OCR_API_KEY",
        )
    payload = {
        "model": MODEL,
        "temperature": 0,
        "messages": [
            {
                "role": "user",
                "content": [
                    {"type": "text", "text": PROMPT},
                    {
                        "type": "image_url",
                        "image_url": {"url": f"data:image/png;base64,{image_b64}"},
                    },
                ],
            }
        ],
    }
    if MODEL.startswith("gpt-5") or MODEL.startswith("o"):
        payload["max_completion_tokens"] = 4096
    else:
        payload["max_tokens"] = 4096
    resp = _json_request(
        API_BASE + "/chat/completions",
        payload,
        {
            "Content-Type": "application/json",
            "Authorization": f"Bearer {API_KEY}",
        },
        timeout,
    )
    try:
        content = resp["choices"][0]["message"]["content"]
    except (KeyError, IndexError, TypeError) as e:
        raise HTTPException(502, f"unexpected OCR API response: {e}") from e
    if isinstance(content, list):
        text = "".join(
            part.get("text", "") if isinstance(part, dict) else str(part)
            for part in content
        )
    else:
        text = content or ""
    return _unwrap_text(text)


def _generate(image_b64: str, timeout: int = 1800) -> str:
    if BACKEND == "api":
        return _api_generate(image_b64, timeout)
    return _ollama_generate(image_b64, timeout)


def _health_ollama() -> dict:
    try:
        with urllib.request.urlopen(OLLAMA + "/api/tags", timeout=5) as r:
            tags = json.loads(r.read())
    except urllib.error.URLError as e:
        raise HTTPException(503, f"ollama unreachable: {e}") from e
    names = [m.get("name", "") for m in tags.get("models", [])]
    if not any(n.startswith(MODEL) for n in names):
        raise HTTPException(503, f"model {MODEL} not pulled; have: {names}")
    return {
        "status": "ok",
        "backend": "ollama",
        "model": MODEL,
        "on_this_machine": True,
        "target_px": TARGET_PX,
        "min_px": MIN_PX,
    }


def _health_api() -> dict:
    if not API_KEY:
        raise HTTPException(503, "OCR API key is not set")
    return {
        "status": "ok",
        "backend": "api",
        "model": MODEL,
        "on_this_machine": False,
        "base_url": API_BASE,
        "target_px": TARGET_PX,
        "min_px": MIN_PX,
    }


def _fit_longest_side(im: Image.Image, target: int) -> Image.Image:
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
    try:
        raw = base64.b64decode(image_b64, validate=True)
        im = Image.open(io.BytesIO(raw))
        im.load()
    except (binascii.Error, OSError) as e:
        raise HTTPException(400, f"unreadable image: {e}") from e
    if max(im.size) < MIN_PX:
        raise HTTPException(
            422,
            f"image too small ({im.size[0]}x{im.size[1]}); longest side must be >= {MIN_PX}px",
        )
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
        scale = TARGET_PX / float(max(w_pt, h_pt))
        out.append(_to_png_b64(page.render(scale=scale).to_pil()))
    return out


HERE = os.path.dirname(os.path.abspath(__file__))


@app.get("/", include_in_schema=False)
def demo():
    """A page to drop a Thai document on and watch it be read."""
    return FileResponse(os.path.join(HERE, "demo.html"), media_type="text/html")


SAMPLE_LABELS = {
    "03_delivery_note.png": "ใบส่งของ",
    "01_tax_invoice.png": "ใบกำกับภาษี",
    "02_quotation.png": "ใบเสนอราคา",
}


@app.get("/samples", include_in_schema=False)
def samples():
    here = os.path.join(HERE, "samples")
    return [
        {"name": name, "label": label}
        for name, label in SAMPLE_LABELS.items()
        if os.path.isfile(os.path.join(here, name))
    ]


@app.get("/samples/{name}", include_in_schema=False)
def sample(name: str):
    if not re.fullmatch(r"[0-9A-Za-z_.-]{1,64}\.(png|jpg|jpeg|webp|pdf)", name):
        raise HTTPException(404, "no such sample")
    path = os.path.join(HERE, "samples", name)
    if not os.path.isfile(path):
        raise HTTPException(404, "no such sample")
    return FileResponse(path)


@app.get("/health")
def health():
    return _health_api() if BACKEND == "api" else _health_ollama()


@app.post("/extract")
def extract_endpoint(req: ExtractRequest):
    return extract(req.markdown, req.expected)


@app.post("/ocr", response_model=OcrResponse)
def ocr(req: OcrRequest, request: Request):
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
    _enforce_browser_limits(request, len(images))
    pages = []
    for i, img in enumerate(images, 1):
        p0 = time.time()
        md = _generate(img)
        pages.append(PageResult(page=i, markdown=md, ms=int((time.time() - p0) * 1000)))
    return OcrResponse(model=MODEL, pages=pages, total_ms=int((time.time() - t0) * 1000))
