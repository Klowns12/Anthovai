# Benchmark

How the OCR model, the prompt and the 1800 px rendering were chosen, and how to
check the choice again when a new model appears.

The answer it produced is in [`../docs/benchmark-2026-09-13.md`](../docs/benchmark-2026-09-13.md):
`scb10x/typhoon-ocr1.5-3b` with the model card's own prompt, images resized so
the longest side is 1800 px. That recipe read 97% of the fields on clean Thai
documents; the previous model with a naive rendering read 69%, and on poor
photographs it invented plausible text rather than failing.

## Run it

```bash
ollama pull scb10x/typhoon-ocr1.5-3b
python make_samples.py                       # documents + phone-photo versions
python make_hidpi.py                         # optional: 240dpi versions
PYTHONUTF8=1 python run_ocr.py clean degraded
```

`run_ocr.py` takes the sample sets to read as arguments and writes one Markdown
file per document under `out/<model>/`, then prints a field-accuracy score per
document and a total.

Both are opinionated about two things worth knowing before trusting a number:

- **`OCR_MODEL` and the prompt go together.** `OCR_PROMPT=v15` selects the
  prompt from the 1.5 model card; the older prompt asks for JSON and scores
  badly against 1.5. Changing one without the other measures the mismatch
  rather than the model.
- **`RESIZE_MAX=1800`.** The model was trained at a fixed 1800 px. Feeding it
  the file's own size is what produced the original 69%, and half of the gain
  between the two runs was ours to fix, not the model's.

## The documents

Four kinds, each chosen because it breaks something different, and all of it
invented — no real company, no real person, no real tax number.

| | | what it tests |
|---|---|---|
| `01_tax_invoice` | ใบกำกับภาษี | a priced table, VAT, tax numbers, an amount written in Thai words |
| `02_quotation` | ใบเสนอราคา | a table plus prose conditions underneath it |
| `03_delivery_note` | ใบส่งของหน้างาน | quantities and units with no money at all — the cost-control case |
| `04_official_memo` | บันทึกข้อความ | long Thai prose, an asset number, a Thai government document number |

`samples/html/` holds the source of each, and is the ground truth: it is what
the generator renders and what a score is measured against. It is committed.
The PNGs and JPEGs are not — one command rebuilds them, and they are 1.4 MB.

**Rendered through headless Chrome, not Pillow.** Pillow without libraqm places
Thai vowels and tone marks wrongly — `ที่อยู่` comes out as `ทีอยู่` — which
would have meant benchmarking OCR against text no Thai reader would accept. The
first version of these documents had exactly that bug.

`make_samples.py` also writes a phone-photo version of each: rotated a couple of
degrees, scaled down, blurred, unevenly lit, noisy, saved as JPEG at quality 58.
It is deliberately harsher than a real phone photo — a lower bound, not a
representative sample.

`fixtures/PO-6909-017.json` is a purchase order matching the delivery note, for
exercising the reconciliation in `../extract.py`:

```bash
python ../extract.py out/<model>/clean_03_delivery_note.md --po fixtures/PO-6909-017.json
```
