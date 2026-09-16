# -*- coding: utf-8 -*-
"""สร้างเอกสารธุรกิจไทยสังเคราะห์สำหรับทดสอบ OCR — ข้อมูลสมมติทั้งหมด

render ผ่าน headless Chrome เพราะ Pillow ไม่มี libraqm จึงวางสระ/วรรณยุกต์ไทยผิด
"""
import os, sys, math, random, subprocess, shutil
from PIL import Image, ImageFilter

ROOT = os.path.dirname(os.path.abspath(__file__))
HTML = os.path.join(ROOT, "samples", "html")
CLEAN = os.path.join(ROOT, "samples", "clean")
DEG = os.path.join(ROOT, "samples", "degraded")
for p in (HTML, CLEAN, DEG):
    os.makedirs(p, exist_ok=True)

CHROME_CANDIDATES = [
    r"C:\Program Files\Google\Chrome\Application\chrome.exe",
    r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
    r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
]
W, H = 1240, 1754  # A4 @150dpi

CSS = """
@page { size: 210mm 297mm; margin: 0; }
* { box-sizing: border-box; }
body {
  width: 1240px; height: 1754px; margin: 0; padding: 90px;
  background: #fff; color: #000;
  font-family: "Leelawadee UI", "Leelawadee", Tahoma, sans-serif;
  font-size: 20px; line-height: 1.45;
}
.row { display: flex; justify-content: space-between; align-items: flex-start; }
.big { font-size: 30px; font-weight: 700; }
.title { font-size: 34px; font-weight: 700; text-align: center; }
.small { font-size: 19px; }
.muted { color: #888; }
b, .b { font-weight: 700; }
hr.thick { border: 0; border-top: 3px solid #000; margin: 14px 0; }
hr.thin  { border: 0; border-top: 2px solid #000; margin: 12px 0; }
table { width: 100%; border-collapse: collapse; margin-top: 6px; }
th, td { border: 2px solid #000; padding: 9px 12px; font-size: 20px; }
td { border-width: 1px 2px; border-color: #999 #000; }
th { font-weight: 700; text-align: left; }
.r { text-align: right; }
.c { text-align: center; }
.kv td { border: 0; padding: 4px 0; vertical-align: top; }
.kv td.k { font-weight: 700; width: 150px; }
.totals { width: 100%; margin-top: 22px; }
.totals td { border: 0; padding: 5px 0; font-size: 21px; }
.sign { display: flex; justify-content: space-between; margin-top: 110px; text-align: center; }
.sign div { width: 240px; }
.sign .line { border-top: 1px solid #000; padding-top: 10px; }
.footer { position: absolute; bottom: 60px; left: 0; width: 1240px;
          text-align: center; font-size: 17px; color: #888; }
"""

FOOTER = '<div class="footer">เอกสารตัวอย่างสำหรับทดสอบระบบ — ข้อมูลสมมติ</div>'


def money(n):
    return "{:,.2f}".format(n)


def page(body):
    return ("<!doctype html><html lang=\"th\"><head><meta charset=\"utf-8\">"
            "<style>" + CSS + "</style></head><body>" + body + FOOTER +
            "</body></html>")


# ------------------------------------------------------------ 1. ใบกำกับภาษี
def tax_invoice():
    items = [
        ("เหล็กเส้นกลม SR24 ขนาด 9 มม. ยาว 10 ม.", 200, 148.00),
        ("ปูนซีเมนต์ปอร์ตแลนด์ ประเภท 1 ขนาด 50 กก.", 150, 168.50),
        ("ทรายหยาบล้าง (คิวละ)", 24, 520.00),
        ("ค่าแรงติดตั้งโครงหลังคาเหล็ก งวดที่ 2", 1, 185000.00),
        ("ค่าขนส่งวัสดุเข้าหน่วยงาน", 3, 2800.00),
    ]
    rows, sub = "", 0.0
    for i, (name, qty, price) in enumerate(items, 1):
        amt = qty * price
        sub += amt
        rows += ("<tr><td class=\"c\">{}</td><td>{}</td><td class=\"r\">{:,}</td>"
                 "<td class=\"r\">{}</td><td class=\"r\">{}</td></tr>").format(
            i, name, qty, money(price), money(amt))
    vat = sub * 0.07
    body = """
<div class="row">
  <div>
    <div class="big">บริษัท ศรีบูรพาวิศวกรรม จำกัด (สำนักงานใหญ่)</div>
    <div class="small">128/45 ถนนพระราม 9 แขวงห้วยขวาง เขตห้วยขวาง กรุงเทพมหานคร 10310</div>
    <div class="small">เลขประจำตัวผู้เสียภาษีอากร 0105558012345 &nbsp; โทร. 02-245-7788 &nbsp; โทรสาร 02-245-7789</div>
  </div>
  <div style="text-align:right">
    <div class="big">ใบกำกับภาษี / ใบเสร็จรับเงิน</div>
    <div>ต้นฉบับ</div>
  </div>
</div>
<hr class="thick">
<div class="row">
  <table class="kv" style="width:58%">
    <tr><td class="k">ชื่อลูกค้า</td><td>บริษัท ทวีทรัพย์ก่อสร้าง จำกัด</td></tr>
    <tr><td class="k">ที่อยู่</td><td>99/7 หมู่ 4 ตำบลบางพลีใหญ่ อำเภอบางพลี จังหวัดสมุทรปราการ 10540</td></tr>
    <tr><td class="k">เลขผู้เสียภาษี</td><td>0115549008876</td></tr>
  </table>
  <table class="kv" style="width:36%">
    <tr><td class="k" style="width:90px">เลขที่</td><td>IV-2569/0412</td></tr>
    <tr><td class="k">วันที่</td><td>12 กันยายน 2569</td></tr>
    <tr><td class="k">เงื่อนไข</td><td>เครดิต 30 วัน</td></tr>
  </table>
</div>
<hr class="thin">
<table>
  <tr><th class="c" style="width:80px">ลำดับ</th><th>รายการ</th>
      <th class="r" style="width:120px">จำนวน</th>
      <th class="r" style="width:160px">ราคา/หน่วย</th>
      <th class="r" style="width:180px">จำนวนเงิน</th></tr>
  {rows}
</table>
<table class="totals">
  <tr><td class="r">รวมเป็นเงิน</td><td class="r" style="width:200px">{sub}</td></tr>
  <tr><td class="r">ภาษีมูลค่าเพิ่ม 7%</td><td class="r">{vat}</td></tr>
  <tr><td class="r b">จำนวนเงินรวมทั้งสิ้น</td><td class="r b">{tot}</td></tr>
</table>
<hr class="thin">
<div class="b">จำนวนเงินเป็นตัวอักษร&nbsp; (สองแสนเจ็ดหมื่นเก้าพันเจ็ดบาทแปดสิบห้าสตางค์)</div>
<div class="sign">
  <div><div class="line">ผู้รับเงิน</div><div class="small">วันที่ ......../......../........</div></div>
  <div><div class="line">ผู้ตรวจสอบ</div><div class="small">วันที่ ......../......../........</div></div>
  <div><div class="line">ผู้มีอำนาจลงนาม</div><div class="small">วันที่ ......../......../........</div></div>
</div>
""".format(rows=rows, sub=money(sub), vat=money(vat), tot=money(sub + vat))
    return body, "01_tax_invoice"


# ------------------------------------------------------------ 2. ใบเสนอราคา
def quotation():
    items = [
        ("เครื่องปรับอากาศแบบแขวน ขนาด 36,000 BTU อินเวอร์เตอร์", "4 ชุด", 132000.00),
        ("ท่อทองแดงหุ้มฉนวน พร้อมอุปกรณ์ยึด", "60 เมตร", 27000.00),
        ("งานเดินท่อน้ำทิ้งและฉนวนกันความร้อน", "1 งาน", 18500.00),
        ("ค่าแรงติดตั้งและทดสอบระบบ", "1 งาน", 45000.00),
        ("รื้อถอนเครื่องเดิมและขนย้ายออกนอกพื้นที่", "4 ชุด", 6000.00),
    ]
    rows, sub = "", 0.0
    for i, (name, qty, amt) in enumerate(items, 1):
        sub += amt
        rows += ("<tr><td class=\"c\">{}</td><td>{}</td><td class=\"r\">{}</td>"
                 "<td class=\"r\">{}</td></tr>").format(i, name, qty, money(amt))
    vat = sub * 0.07
    body = """
<div class="big">หจก. ไทยรุ่งเรืองแอร์เซอร์วิส</div>
<div class="small">45/12 ซอยลาดพร้าว 71 แขวงสะพานสอง เขตวังทองหลาง กรุงเทพฯ 10310</div>
<div class="small">โทร. 081-234-5678 &nbsp; LINE: @thairoongair &nbsp; อีเมล sale@example.co.th</div>
<div class="title" style="margin-top:34px">ใบเสนอราคา</div>
<hr class="thick">
<div class="row">
  <table class="kv" style="width:62%">
    <tr><td class="k" style="width:80px">เรียน</td><td>คุณสมชาย ใจดี &nbsp; บริษัท เอ็นเทคโปรดักส์ จำกัด</td></tr>
    <tr><td class="k">เรื่อง</td><td>เสนอราคางานติดตั้งเครื่องปรับอากาศ อาคารสำนักงาน ชั้น 3</td></tr>
  </table>
  <table class="kv" style="width:32%">
    <tr><td>เลขที่ QT-6909-0087</td></tr>
    <tr><td>วันที่ 12 ก.ย. 2569</td></tr>
  </table>
</div>
<div style="margin-top:14px">ตามที่ท่านได้ให้ความสนใจ ทางบริษัทฯ ขอเสนอราคาดังรายละเอียดต่อไปนี้</div>
<table>
  <tr><th class="c" style="width:80px">ลำดับ</th><th>รายละเอียด</th>
      <th class="r" style="width:160px">จำนวน</th>
      <th class="r" style="width:200px">รวมเงิน</th></tr>
  {rows}
</table>
<table class="totals">
  <tr><td class="r">รวมเป็นเงิน</td><td class="r" style="width:200px">{sub}</td></tr>
  <tr><td class="r">ภาษีมูลค่าเพิ่ม 7%</td><td class="r">{vat}</td></tr>
  <tr><td class="r b">รวมทั้งสิ้น</td><td class="r b">{tot}</td></tr>
</table>
<div class="b" style="margin-top:30px;font-size:22px">เงื่อนไข</div>
<div style="margin-left:20px">
  <div>1. ราคานี้ยืนราคา 30 วัน นับจากวันที่ในเอกสาร</div>
  <div>2. ชำระเงินมัดจำ 50% ก่อนเริ่มงาน ส่วนที่เหลือชำระเมื่องานแล้วเสร็จ</div>
  <div>3. รับประกันงานติดตั้ง 1 ปี และรับประกันตัวเครื่องตามเงื่อนไขผู้ผลิต</div>
  <div>4. ระยะเวลาดำเนินการ 7 วันทำการ นับจากวันที่ได้รับใบสั่งซื้อ</div>
</div>
<div class="sign" style="justify-content:flex-end">
  <div><div class="line">ผู้เสนอราคา</div></div>
</div>
""".format(rows=rows, sub=money(sub), vat=money(vat), tot=money(sub + vat))
    return body, "02_quotation"


# ------------------------------------------------------------ 3. ใบส่งของหน้างาน
def delivery_note():
    items = [("อิฐมอญ ขนาด 6x14x3 ซม.", "4,000", "ก้อน", "-"),
             ("ปูนก่อสำเร็จรูป 50 กก.", "80", "ถุง", "-"),
             ("ทรายละเอียด", "6", "คิว", "เทหน้างาน"),
             ("เหล็กข้ออ้อย DB12 ยาว 10 ม.", "60", "เส้น", "-"),
             ("ลวดผูกเหล็ก เบอร์ 18", "25", "กก.", "-"),
             ("แผ่นสมาร์ทบอร์ด 8 มม.", "40", "แผ่น", "ชำรุด 2 แผ่น")]
    rows = ""
    for i, (name, qty, unit, note) in enumerate(items, 1):
        rows += ("<tr><td class=\"c\">{}</td><td>{}</td><td class=\"r\">{}</td>"
                 "<td>{}</td><td>{}</td></tr>").format(i, name, qty, unit, note)
    body = """
<div class="row">
  <div>
    <div class="big">ร้านวัสดุก่อสร้าง พี.เอส. ค้าส่ง</div>
    <div class="small">212 หมู่ 8 ต.คลองหนึ่ง อ.คลองหลวง จ.ปทุมธานี 12120 &nbsp; โทร 02-529-1188</div>
  </div>
  <div style="text-align:right" class="b" >ใบส่งของ / ใบกำกับภาษีอย่างย่อ</div>
</div>
<hr class="thick">
<div class="row">
  <table class="kv" style="width:62%">
    <tr><td class="k" style="width:170px">ส่งที่หน่วยงาน</td><td>โครงการบ้านพักอาศัย 2 ชั้น ซ.รามอินทรา 19</td></tr>
    <tr><td class="k">ผู้รับของ</td><td>นายประยูร แสงทอง (หัวหน้าช่าง)</td></tr>
    <tr><td class="k">ทะเบียนรถ</td><td>70-8842 ปทุมธานี</td></tr>
  </table>
  <table class="kv" style="width:32%">
    <tr><td>เลขที่ DN-690912-03</td></tr>
    <tr><td>วันที่ 12/09/2569</td></tr>
    <tr><td>เวลา 09:40 น.</td></tr>
  </table>
</div>
<hr class="thin">
<table>
  <tr><th class="c" style="width:70px">ที่</th><th>รายการวัสดุ</th>
      <th class="r" style="width:130px">จำนวน</th>
      <th style="width:120px">หน่วย</th><th style="width:200px">หมายเหตุ</th></tr>
  {rows}
</table>
<div style="margin-top:28px">หมายเหตุ: แผ่นสมาร์ทบอร์ดชำรุด 2 แผ่น แจ้งเปลี่ยนแล้ว รอส่งรอบถัดไป</div>
<div class="sign">
  <div><div class="line">ผู้ส่งของ</div></div>
  <div><div class="line">ผู้รับของ</div></div>
  <div><div class="line">ผู้ควบคุมงาน</div></div>
</div>
""".format(rows=rows)
    return body, "03_delivery_note"


# ------------------------------------------------------------ 4. บันทึกข้อความ
def official_memo():
    body = """
<div class="title">บันทึกข้อความ</div>
<table class="kv" style="margin-top:26px">
  <tr><td class="k" style="width:180px">ส่วนราชการ</td>
      <td colspan="3">ฝ่ายวิศวกรรมและซ่อมบำรุง &nbsp; โทร. ภายใน 2214</td></tr>
  <tr><td class="k">ที่</td><td style="width:460px">วศ 0410/ว 233</td>
      <td class="k" style="width:100px">วันที่</td><td>10 กันยายน 2569</td></tr>
  <tr><td class="k">เรื่อง</td><td colspan="3">ขออนุมัติจัดซื้ออะไหล่เครื่องอัดอากาศ สายการผลิตที่ 3</td></tr>
</table>
<hr class="thin">
<div class="b">เรียน &nbsp; ผู้จัดการฝ่ายผลิต</div>
<p style="text-indent:56px;margin-top:26px">
ตามที่ฝ่ายวิศวกรรมและซ่อมบำรุงได้ตรวจพบความผิดปกติของเครื่องอัดอากาศ หมายเลขครุภัณฑ์
AC-03-1189 ประจำสายการผลิตที่ 3 โดยมีอาการแรงดันตกและมีเสียงดังผิดปกติขณะทำงานต่อเนื่องเกิน
4 ชั่วโมง ซึ่งส่งผลให้กำลังการผลิตลดลงประมาณร้อยละ 12 เมื่อเทียบกับค่ามาตรฐาน นั้น</p>
<p style="text-indent:56px">
ฝ่ายวิศวกรรมฯ ได้ตรวจสอบเบื้องต้นแล้วพบว่าสาเหตุเกิดจากชุดวาล์วระบายและไส้กรองอากาศ
เสื่อมสภาพตามอายุการใช้งาน จึงมีความจำเป็นต้องจัดซื้ออะไหล่เพื่อเปลี่ยนทดแทน รวมเป็นเงินทั้งสิ้น
68,450.00 บาท (หกหมื่นแปดพันสี่ร้อยห้าสิบบาทถ้วน) รายละเอียดตามเอกสารแนบ</p>
<p style="text-indent:56px">
จึงเรียนมาเพื่อโปรดพิจารณาอนุมัติ และหากได้รับอนุมัติแล้ว ฝ่ายวิศวกรรมฯ จะดำเนินการ
จัดซื้อและเข้าดำเนินการเปลี่ยนอะไหล่ในวันหยุดประจำสัปดาห์ เพื่อมิให้กระทบต่อแผนการผลิต</p>
<div class="sign" style="justify-content:flex-end">
  <div><div class="line">(นายอนุชา ธนวัฒน์)</div>
       <div class="small">วิศวกรซ่อมบำรุงอาวุโส</div></div>
</div>
"""
    return body, "04_official_memo"


# ------------------------------------------------------------ render / degrade
def find_chrome():
    for p in CHROME_CANDIDATES:
        if os.path.isfile(p):
            return p
    return None


def shoot(chrome, html_path, png_path):
    tmp = os.path.join(os.environ.get("TEMP", "."), "chrome-ocrlab")
    subprocess.run([chrome, "--headless=new", "--disable-gpu", "--hide-scrollbars",
                    "--force-device-scale-factor=1",
                    "--user-data-dir=" + tmp,
                    "--window-size={},{}".format(W, H),
                    "--default-background-color=FFFFFFFF",
                    "--screenshot=" + png_path,
                    "file:///" + html_path.replace("\\", "/")],
                   check=True, capture_output=True, timeout=90)


def degrade(img, seed=0):
    """จำลองรูปถ่ายมือถือหน้างาน: เอียง ความละเอียดต่ำ เบลอ แสงไม่สม่ำเสมอ noise"""
    rnd = random.Random(seed)
    im = img.convert("RGB").rotate(rnd.uniform(-2.2, 2.2), expand=False,
                                   fillcolor="white", resample=Image.BICUBIC)
    im = im.resize((int(W * 0.62), int(H * 0.62)), Image.LANCZOS)
    im = im.filter(ImageFilter.GaussianBlur(rnd.uniform(0.6, 1.1)))
    px = im.load()
    w, h = im.size
    cx, cy = rnd.uniform(0.3, 0.7) * w, rnd.uniform(0.2, 0.5) * h
    maxd = math.hypot(w, h)
    for yy in range(0, h, 2):
        for xx in range(0, w, 2):
            r, g, b = px[xx, yy]
            shade = 1.0 - 0.28 * (math.hypot(xx - cx, yy - cy) / maxd)
            n = rnd.randint(-12, 12)
            nr = max(0, min(255, int(r * shade) + n))
            ng = max(0, min(255, int(g * shade) + n))
            nb = max(0, min(255, int(b * shade) + n))
            for dy in (0, 1):
                for dx in (0, 1):
                    if xx + dx < w and yy + dy < h:
                        px[xx + dx, yy + dy] = (nr, ng, nb)
    return im


if __name__ == "__main__":
    chrome = find_chrome()
    if not chrome:
        sys.exit("หา Chrome/Edge ไม่เจอ — ต้องใช้ render ภาษาไทยให้ถูกต้อง")
    print("renderer:", chrome)
    for fn in (tax_invoice, quotation, delivery_note, official_memo):
        body, name = fn()
        hp = os.path.join(HTML, name + ".html")
        with open(hp, "w", encoding="utf-8") as f:
            f.write(page(body))
        cp = os.path.join(CLEAN, name + ".png")
        shoot(chrome, hp, cp)
        dp = os.path.join(DEG, name + "_photo.jpg")
        degrade(Image.open(cp), seed=sum(ord(c) for c in name)).save(dp, "JPEG", quality=58)
        print("{}: clean={}KB  photo={}KB".format(
            name, os.path.getsize(cp) // 1024, os.path.getsize(dp) // 1024))
