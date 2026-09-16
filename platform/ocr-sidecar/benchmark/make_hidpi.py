# -*- coding: utf-8 -*-
"""ยิงเอกสารชุดเดิมใหม่ที่ ~240dpi (scale 1.6) เพื่อทดสอบว่าความละเอียดแก้ปัญหาชื่อเฉพาะได้ไหม"""
import os, subprocess
from make_samples import HTML, ROOT, find_chrome, W, H

OUT = os.path.join(ROOT, "samples", "clean_hidpi")
os.makedirs(OUT, exist_ok=True)
chrome = find_chrome()
tmp = os.path.join(os.environ.get("TEMP", "."), "chrome-ocrlab2")
for fn in sorted(os.listdir(HTML)):
    if not fn.endswith(".html"):
        continue
    src = os.path.join(HTML, fn)
    dst = os.path.join(OUT, fn.replace(".html", ".png"))
    subprocess.run([chrome, "--headless=new", "--disable-gpu", "--hide-scrollbars",
                    "--force-device-scale-factor=1.6",
                    "--user-data-dir=" + tmp,
                    "--window-size={},{}".format(W, H),
                    "--screenshot=" + dst,
                    "file:///" + src.replace("\\", "/")],
                   check=True, capture_output=True, timeout=90)
    print(fn, "->", os.path.getsize(dst) // 1024, "KB")
