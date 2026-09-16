# -*- coding: utf-8 -*-
"""รัน Typhoon OCR ผ่าน Ollama กับเอกสารตัวอย่างทุกไฟล์ แล้วให้คะแนน field-level accuracy"""
import os, sys, json, time, base64, urllib.request

sys.stdout.reconfigure(encoding="utf-8", errors="replace")

ROOT = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(ROOT, "out")
os.makedirs(OUT, exist_ok=True)

MODEL = os.environ.get("OCR_MODEL", "scb10x/typhoon-ocr-3b")
# แยกโฟลเดอร์ผลลัพธ์ตามโมเดล จะได้เทียบกันได้โดยไม่ทับของเดิม
OUT = os.path.join(OUT, MODEL.split("/")[-1].replace(":", "_"))
os.makedirs(OUT, exist_ok=True)
OLLAMA = "http://localhost:11434/api/generate"

# prompt มาตรฐานของ typhoon-ocr v1 (task: default) — ตอบเป็น JSON natural_text
PROMPT_V1 = (
    "Below is an image of a document page along with its dimensions. "
    "Simply return the markdown representation of this document, "
    "presenting tables in markdown format as they naturally appear.\n"
    "If the document contains images, use a placeholder like dummy.png for each image.\n"
    "Your final output must be in JSON format with a single key `natural_text` "
    "containing the response."
)
# prompt ทางการของ typhoon-ocr 1.5 (จาก model card) — ตอบ markdown ตรง ๆ ตารางเป็น HTML
PROMPT_V15 = (
    "Extract all text from the image. Instructions: Only return the clean Markdown. "
    "Do not include any explanation or extra text. You must include all information "
    "on the page. Formatting Rules: Tables using HTML, Equations using LaTeX, "
    "Images/Charts/Diagrams wrapped in <figure> tags, Page Numbers in <page_number> "
    "tags, Checkboxes using ☐/☑."
)
PROMPT = PROMPT_V15 if os.environ.get("OCR_PROMPT") == "v15" else PROMPT_V1

# model card 1.5: "trained with a fixed image dimension of 1800 px" → ย่อ/ขยายด้านยาวให้เท่านี้
RESIZE_MAX = int(os.environ.get("RESIZE_MAX", "0"))
if RESIZE_MAX:
    OUT = OUT + "_r{}".format(RESIZE_MAX)
    os.makedirs(OUT, exist_ok=True)

# ฟิลด์สำคัญที่ต้องอ่านให้ได้ ต่อเอกสาร (ตัวเลขเงิน = หัวใจของ use case)
EXPECT = {
    "01_tax_invoice": [
        "ศรีบูรพาวิศวกรรม", "0105558012345", "IV-2569/0412", "ทวีทรัพย์ก่อสร้าง",
        "0115549008876", "เครดิต 30 วัน", "168.50", "185,000.00",
        "260,755.00", "18,252.85", "279,007.85",
    ],
    "02_quotation": [
        "ไทยรุ่งเรืองแอร์เซอร์วิส", "QT-6909-0087", "36,000 BTU",
        "132,000.00", "45,000.00", "228,500.00", "15,995.00", "244,495.00",
        "มัดจำ 50%", "รับประกันงานติดตั้ง 1 ปี",
    ],
    "03_delivery_note": [
        "พี.เอส. ค้าส่ง", "DN-690912-03", "รามอินทรา 19", "ประยูร แสงทอง",
        "อิฐมอญ", "4,000", "เหล็กข้ออ้อย DB12", "ลวดผูกเหล็ก",
        "ชำรุด 2 แผ่น", "09:40",
    ],
    "04_official_memo": [
        "บันทึกข้อความ", "วศ 0410/ว 233", "AC-03-1189", "สายการผลิตที่ 3",
        "ร้อยละ 12", "68,450.00", "หกหมื่นแปดพันสี่ร้อยห้าสิบบาทถ้วน",
        "อนุชา ธนวัฒน์",
    ],
}


def norm(s):
    """เทียบแบบทนต่อช่องว่าง/คอมมา เพื่อไม่ให้ตกเพราะ formatting"""
    return s.replace(",", "").replace(" ", "").replace(" ", "")


def load_image_b64(path):
    """อ่านภาพ; ถ้าตั้ง RESIZE_MAX ให้ย่อ/ขยายด้านยาวเท่านั้น (สูตร resize_if_needed ของ model card)"""
    if not RESIZE_MAX:
        with open(path, "rb") as f:
            return base64.b64encode(f.read()).decode()
    import io
    from PIL import Image
    with Image.open(path) as im:
        im = im.convert("RGB")
        w, h = im.size
        if w >= h:
            new = (RESIZE_MAX, int(h * RESIZE_MAX / float(w)))
        else:
            new = (int(w * RESIZE_MAX / float(h)), RESIZE_MAX)
        im = im.resize(new, Image.Resampling.LANCZOS)
        buf = io.BytesIO()
        im.save(buf, format="PNG")
    return base64.b64encode(buf.getvalue()).decode()


def ocr(path):
    b64 = load_image_b64(path)
    prompt = PROMPT
    # ทดลอง: แนบขนาดภาพแบบ anchor block ตาม preprocessing ของ typhoon-ocr package
    if os.environ.get("PROMPT_DIMS") == "1":
        from PIL import Image
        with Image.open(path) as im:
            w, h = im.size
        prompt = ("{}\nRAW_TEXT_START\nPage dimensions: {}.0x{}.0\n"
                  "RAW_TEXT_END").format(PROMPT, w, h)
    req = urllib.request.Request(
        OLLAMA,
        data=json.dumps({
            "model": MODEL,
            "prompt": prompt,
            "images": [b64],
            "stream": False,
            "options": {"temperature": 0, "num_predict": 4096},
        }).encode(),
        headers={"Content-Type": "application/json"},
    )
    t0 = time.time()
    with urllib.request.urlopen(req, timeout=1800) as r:
        resp = json.loads(r.read())
    text = resp.get("response", "")
    # โมเดลตอบเป็น JSON {"natural_text": ...} — พยายามแกะ, แกะไม่ได้ใช้ดิบ
    try:
        text = json.loads(text)["natural_text"]
    except Exception:
        pass
    return text, time.time() - t0


def main():
    subs = sys.argv[1:] or ["clean", "degraded"]
    results = []
    for sub in subs:
        d = os.path.join(ROOT, "samples", sub)
        for fn in sorted(os.listdir(d)):
            if not fn.lower().endswith((".png", ".jpg")):
                continue
            key = fn.split(".")[0].replace("_photo", "")
            path = os.path.join(d, fn)
            print("OCR [{}] {} ...".format(sub, fn), flush=True)
            try:
                text, dt = ocr(path)
            except Exception as e:
                print("  ERROR:", e, flush=True)
                results.append((sub, fn, None, None, str(e)))
                continue
            outp = os.path.join(OUT, "{}_{}.md".format(sub, key))
            with open(outp, "w", encoding="utf-8") as f:
                f.write(text)
            fields = EXPECT.get(key, [])
            hits = [x for x in fields if norm(x) in norm(text)]
            miss = [x for x in fields if norm(x) not in norm(text)]
            print("  {:.0f}s  fields {}/{}  miss: {}".format(
                dt, len(hits), len(fields), ", ".join(miss) if miss else "-"),
                flush=True)
            results.append((sub, fn, len(hits), len(fields), None))

    print("\n===== SUMMARY =====")
    tot_h = tot_f = 0
    for sub, fn, h, f, err in results:
        if err:
            print("{:9s} {:32s} ERROR {}".format(sub, fn, err))
        else:
            tot_h += h; tot_f += f
            print("{:9s} {:32s} {}/{} ({:.0f}%)".format(sub, fn, h, f, 100.0 * h / f))
    if tot_f:
        print("TOTAL: {}/{} = {:.1f}%".format(tot_h, tot_f, 100.0 * tot_h / tot_f))


if __name__ == "__main__":
    main()
