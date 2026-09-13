# -*- coding: utf-8 -*-
"""ยิงไฟล์ภาพหรือ PDF เข้า sidecar แล้วพิมพ์ผล — ใช้ทดสอบหลังติดตั้ง"""
import base64
import json
import sys
import urllib.request

sys.stdout.reconfigure(encoding="utf-8", errors="replace")

path = sys.argv[1] if len(sys.argv) > 1 else None
if not path:
    sys.exit("usage: python smoke_test.py <image.png|file.pdf>")

with open(path, "rb") as f:
    b64 = base64.b64encode(f.read()).decode()
key = "pdf_b64" if path.lower().endswith(".pdf") else "image_b64"

req = urllib.request.Request(
    "http://127.0.0.1:9090/ocr",
    data=json.dumps({key: b64}).encode(),
    headers={"Content-Type": "application/json"},
)
with urllib.request.urlopen(req, timeout=1800) as r:
    resp = json.loads(r.read())

print("model:", resp["model"], "| total:", resp["total_ms"], "ms")
for p in resp["pages"]:
    print("--- page {} ({} ms) ---".format(p["page"], p["ms"]))
    print(p["markdown"][:2000])
