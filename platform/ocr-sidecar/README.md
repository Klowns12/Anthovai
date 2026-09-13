# OCR sidecar

Reads scanned PDFs and photographs for `anthovai-worker`. The worker's own PDF
parser handles files that carry text; when one does not — a scan, a set of
page images — the pages come here, Typhoon OCR (SCB 10X, Apache 2.0, trained
on Thai documents) reads them, and Markdown goes back, one string per page. A
`.png`/`.jpg`/`.webp` uploaded as a document takes the same path through
`ImageParser`, as one page.

It is a separate process on the same machine because the model is a Python
runtime on a GPU, and because a crash in the model should cost one document,
not the queue. Nothing leaves the machine: the worker reaches this over the
loopback or the compose network, and this reaches Ollama the same way.

## Run it

```bash
ollama pull scb10x/typhoon-ocr1.5-3b          # once
uvicorn app:app --host 127.0.0.1 --port 9090  # for cargo run next to it
```

Then in the worker's environment: `ANTHOVAI__OCR__ENABLED=true`
(`config/default.toml` has the rest). Or as a container, with the stack:

```bash
ANTHOVAI__OCR__ENABLED=true docker compose --profile ocr \
  -f docker/docker-compose.yml -f docker/docker-compose.stack.yml up --build
```

## Try it

With the service running, open <http://127.0.0.1:9090> and drop a Thai document
on the page — a photo of a delivery note, a scanned invoice, a PDF. It shows the
text that came back, the fields pulled out of it, and the arithmetic that
decides whether the document can be trusted without a person reading it. Three
sample documents are included for when you have nothing to hand.

It is also the demo to put in front of a customer: dropping a photograph of a
delivery note and watching `40 → 38` appear against the damaged line makes the
point faster than any description of it does.

## Endpoints

| | |
|---|---|
| `GET /` | The demo page. |
| `GET /health` | 200 when Ollama answers and the model is pulled; 503 otherwise. The worker checks this at startup and reports, but does not refuse to start. |
| `POST /ocr` | `{"pdf_b64": ...}` or `{"image_b64": ...}` → `{model, total_ms, pages: [{page, markdown, ms}]}`. 422 when a page is too small to read reliably (`OCR_MIN_PX`) — better a refusal than numbers that are quietly wrong. |
| `POST /extract` | Markdown → structured fields, arithmetic and reconciliation checks, `needs_review` with reasons in Thai ([`docs/delivery-note-checks.md`](docs/delivery-note-checks.md)). For the cost-control product; the RAG path does not call it. |

## Settings

| Variable | Default | |
|---|---|---|
| `OCR_OLLAMA_URL` | `http://localhost:11434` | In the container: `http://host.docker.internal:11434` |
| `OCR_MODEL` | `scb10x/typhoon-ocr1.5-3b` | The build the model's authors publish; they warn that third-party GGUFs lose accuracy |
| `OCR_TARGET_PX` | `1800` | The longest side the model was trained at. Pages are rendered to this |
| `OCR_MIN_PX` | `1200` | Below this a page is refused rather than misread |
| `OCR_MAX_PAGES` | `200` | |

## How it was chosen

[`docs/benchmark-2026-09-13.md`](docs/benchmark-2026-09-13.md) — the model, the prompt and
the 1800 px rendering are the recipe that scored 97% field accuracy on Thai
invoices, quotations, delivery notes and an official memo. The earlier model
and a naive rendering scored 69%, and on poor scans it invented plausible text
rather than failing; the current one misreads a digit instead, which the
`/extract` checks catch.
