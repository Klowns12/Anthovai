# -*- coding: utf-8 -*-
"""แกะข้อมูลโครงสร้างจาก markdown/HTML ที่ OCR ให้มา — สำหรับ use case คุมต้นทุน

รองรับ 2 ตระกูลเอกสาร:
  invoice_like   — ใบกำกับภาษี/ใบเสนอราคา: มียอดเงิน → ตรวจเลขคณิต
  delivery_note  — ใบส่งของ: ไม่มียอดเงิน → ตรวจโครงสร้าง + หน่วย + ข้อยกเว้น + เทียบ PO

หลักการเดียวกันทั้งคู่: ระบบต้องบอกเองได้ว่าใบไหน "เชื่อได้" ใบไหน "ต้องให้คนดู"
(`needs_review` + `review_reasons` เป็นภาษาคน) ไม่ว่าจะแกะด้วย rule หรือ LLM
"""
import difflib
import html
import json
import re
import sys
from datetime import date
from html.parser import HTMLParser

MONEY = re.compile(r"\d{1,3}(?:,\d{3})*\.\d{2}")
# จำนวน + หน่วยที่อาจติดมาใน cell เดียว เช่น "4 ชุด", "4,000", "60 เมตร"
QTY = re.compile(r"^(\d{1,3}(?:,\d{3})+|\d+)(?:\s*([ก-๙A-Za-z.]{1,12}))?$")
# "4.000" — OCR อ่านจุลภาคเป็นจุด? ไม่เดา ให้คนดู
AMBIG_QTY = re.compile(r"^\d{1,3}\.\d{3}$")

DOC_NO = re.compile(r"(?:เลขที่|No\.?)[\s:：]*([A-Z]{2,4}[-/]?\s?\d[\d/\-]+)")
# เลขที่หนังสือราชการ เช่น "ที่ วศ 0410/ว 233"
DOC_NO_TH = re.compile(r"(?:เลขที่|ที่)[\s:：]*([ก-๙]{1,5}\s?\d{3,5}/[ก-๙]?\s?\d{1,6})")
DOC_DATE = re.compile(r"วันที่[\s:：]*(\d{1,2}[\s/][ก-๙\d\s.]{2,20}\d{4}|\d{2}/\d{2}/\d{4})")
TAX_ID = re.compile(r"\b(\d{13})\b")
YEAR = re.compile(r"(\d{4})")

# คำในหมายเหตุที่แปลว่า "ของที่รับจริงไม่ตรงกับที่เขียน" → ต้องปรับจำนวน/ตามเรื่อง
EXCEPTION_WORDS = re.compile(r"(ชำรุด|เสียหาย|แตก|หัก|ขาด|ไม่ครบ|คืน|เปลี่ยน|ค้างส่ง|รอส่ง)")
EXCEPTION_QTY = re.compile(r"(?:ชำรุด|เสียหาย|แตก|หัก|ขาด|คืน|ค้าง)\s*(\d+)")

# หน่วยวัสดุก่อสร้างที่พบบ่อย — หน่วยนอกลิสต์ = OCR อาจอ่านผิด (เคยเจอ "ก้อน"→"ค้อน")
UNITS = {
    "ก้อน", "ถุง", "คิว", "ลบ.ม.", "ลบ.ม", "ตร.ม.", "ตร.ม", "ตร.ว.", "เส้น", "กก.", "กก", "กิโลกรัม",
    "ตัน", "แผ่น", "ม.", "เมตร", "ม้วน", "ชุด", "ตัว", "อัน", "ชิ้น", "กล่อง", "ลัง", "แท่ง", "ท่อน",
    "ถัง", "ลิตร", "แกลลอน", "กระป๋อง", "หลอด", "มัด", "คู่", "ใบ", "แพ็ค", "ลูก", "คัน", "เที่ยว",
    "งาน", "หน่วย", "จุด", "ชั้น", "ตร.ฟุต", "ฟุต", "นิ้ว", "หลา", "ซม.", "มม.",
}
UNIT_ALIASES = {"กก": "กก.", "กิโลกรัม": "กก.", "ลบ.ม": "ลบ.ม.", "ตร.ม": "ตร.ม.", "เมตร": "ม."}


# ---------------------------------------------------------------- parsing
class _HtmlTables(HTMLParser):
    """ดึง <table> → rows → cells ทนต่อ HTML เพี้ยน ๆ จาก OCR (td ซ้อน th, colspan มั่ว, <br/>)"""

    def __init__(self):
        super().__init__()
        self.tables, self._rows, self._row, self._cell = [], None, None, None

    def _close_cell(self):
        if self._cell is not None and self._row is not None:
            self._row.append("".join(self._cell).strip())
        self._cell = None

    def handle_starttag(self, tag, attrs):
        if tag == "table":
            self._rows = []
        elif tag == "tr" and self._rows is not None:
            self._close_cell()
            self._row = []
        elif tag in ("td", "th") and self._row is not None:
            self._close_cell()  # td เปิดซ้อนใน th ที่ยังไม่ปิด → ปิดของเดิมก่อน ไม่ทิ้งข้อความ
            self._cell = []
        elif tag == "br" and self._cell is not None:
            self._cell.append("\n")

    def handle_startendtag(self, tag, attrs):
        if tag == "br" and self._cell is not None:
            self._cell.append("\n")

    def handle_endtag(self, tag):
        if tag in ("td", "th"):
            self._close_cell()
        elif tag == "tr" and self._row is not None:
            self._close_cell()
            if any(c for c in self._row):
                self._rows.append(self._row)
            self._row = None
        elif tag == "table" and self._rows is not None:
            self._close_cell()
            if self._rows:
                self.tables.append(self._rows)
            self._rows = None

    def handle_data(self, data):
        if self._cell is not None:
            self._cell.append(data)


def parse_tables(md):
    """คืน list ของตาราง (rows → cells) — รับทั้ง markdown pipe table และ HTML"""
    tables = []
    if "<table" in md:
        p = _HtmlTables()
        p.feed(md)
        tables.extend(p.tables)
    cur = []
    for line in md.splitlines():
        line = line.strip()
        if line.startswith("|") and line.endswith("|"):
            cells = [c.strip() for c in line.strip("|").split("|")]
            if all(re.fullmatch(r":?-{2,}:?", c) for c in cells):
                continue
            cur.append(cells)
        elif cur:
            tables.append(cur)
            cur = []
    if cur:
        tables.append(cur)
    return tables


def strip_html(md):
    """HTML/markdown → ข้อความบรรทัด ๆ สำหรับ regex หาเลขที่/วันที่/หมายเหตุ"""
    t = re.sub(r"<br\s*/?>|</t[dhr]>|</p>|</li>", "\n", md)
    t = re.sub(r"<[^>]+>", " ", t)
    t = html.unescape(t)
    return "\n".join(l.strip() for l in t.splitlines() if l.strip())


def _col_map(header):
    """map ชื่อหัวตาราง → บทบาทคอลัมน์ (ลำดับการเช็คสำคัญ: จำนวนเงิน ก่อน จำนวน, ราคา/หน่วย ก่อน หน่วย)"""
    roles = {}
    for i, h in enumerate(header):
        hn = h.replace(" ", "")
        if not hn:
            continue
        if re.search(r"จำนวนเงิน|รวมเงิน|มูลค่า|^รวม", hn):
            roles.setdefault("amount", i)
        elif re.search(r"ราคา|หน่วยละ", hn):
            roles.setdefault("unit_price", i)
        elif re.search(r"จำนวน|ปริมาณ", hn):
            roles.setdefault("qty", i)
        elif re.search(r"หน่วย", hn):
            roles.setdefault("unit", i)
        elif re.search(r"หมายเหตุ", hn):
            roles.setdefault("note", i)
        elif re.search(r"รายการ|รายละเอียด|วัสดุ|สินค้า", hn):
            roles.setdefault("desc", i)
        elif re.fullmatch(r"ลำดับ|ที่|No\.?|#", hn, re.I):
            roles.setdefault("idx", i)
    return roles


def _num(s):
    return float(s.replace(",", ""))


def _parse_qty(cell, item):
    """เติม qty / unit / qty_raw ให้ item จาก cell จำนวน"""
    c = cell.strip()
    if not c or c == "-":
        return
    m = QTY.match(c)
    if m:
        item["qty"] = _num(m.group(1))
        if m.group(2) and "unit" not in item:
            item["unit"] = m.group(2)
        return
    if AMBIG_QTY.match(c):
        item["qty_raw"] = c  # ไม่แปลง — ให้คนตัดสิน
        return
    if re.search(r"\d", c):
        item["qty_raw"] = c


def line_items(md):
    """เลือกตารางที่เป็นรายการสินค้า/วัสดุ แล้วแกะเป็น item dict ตามหัวคอลัมน์"""
    best = []
    for tb in parse_tables(md):
        if len(tb) < 2:
            continue
        # OCR บางรุ่นรวมบล็อกหัวเอกสารกับตารางรายการเป็น <table> เดียว → หาแถวหัวตารางจากเนื้อหา
        hi = next((i for i, r in enumerate(tb)
                   if re.search(r"รายการ|รายละเอียด|วัสดุ|สินค้า", " ".join(r))), None)
        if hi is None or hi == len(tb) - 1:
            continue
        roles = _col_map(tb[hi])
        if "desc" not in roles:
            continue
        rows = []
        for r in tb[hi + 1:]:
            if len(r) <= roles["desc"] or not r[roles["desc"]].strip():
                continue
            item = {"description": r[roles["desc"]].strip()}
            if "idx" in roles and len(r) > roles["idx"] and re.fullmatch(r"\d{1,3}", r[roles["idx"]].strip()):
                item["idx"] = int(r[roles["idx"]])
            if "unit" in roles and len(r) > roles["unit"] and r[roles["unit"]].strip() not in ("", "-"):
                item["unit"] = r[roles["unit"]].strip()
            if "qty" in roles and len(r) > roles["qty"]:
                _parse_qty(r[roles["qty"]], item)
            for role, key in (("unit_price", "unit_price"), ("amount", "amount")):
                if role in roles and len(r) > roles[role]:
                    m = MONEY.search(r[roles[role]].replace(" ", ""))
                    if m:
                        item[key] = _num(m.group())
            if "amount" not in roles:
                # หัวคอลัมน์เงินอ่านไม่ออก → ใช้ตำแหน่ง: เงินตัวท้ายสุด = จำนวนเงิน, ก่อนหน้า = ราคา/หน่วย
                nums = [c for c in r if MONEY.fullmatch(c.replace(" ", ""))]
                if nums:
                    item.setdefault("amount", _num(nums[-1]))
                    if len(nums) >= 2:
                        item.setdefault("unit_price", _num(nums[-2]))
            if "note" in roles and len(r) > roles["note"] and r[roles["note"]].strip() not in ("", "-"):
                item["note"] = r[roles["note"]].strip()
            if "unit" in item:
                item["unit"] = UNIT_ALIASES.get(item["unit"], item["unit"])
            rows.append(item)
        if len(rows) > len(best):
            best = rows
    return best


def kv_pairs(md):
    """คู่ key/value จากบล็อกหัวเอกสาร — ทั้งแถวตาราง 2 cell และบรรทัด 'key: value'"""
    pairs = []
    for tb in parse_tables(md):
        for r in tb:
            cells = [c for c in r if c]
            if len(cells) >= 2 and len(cells[0]) <= 20:
                pairs.append((cells[0], cells[1]))
    for line in strip_html(md).splitlines():
        m = re.match(r"^([ก-๙A-Za-z./ ]{2,20})[\s:：]+(.{2,})$", line)
        if m:
            pairs.append((m.group(1).strip(), m.group(2).strip()))
    return pairs


def _lookup(pairs, key_re):
    for k, v in pairs:
        if re.search(key_re, k.replace(" ", "")):
            return v.split("\n")[0].strip()
    return None


def totals(md):
    out = {}
    for line in strip_html(md).splitlines():
        for key, words in (("subtotal", ["รวมเป็นเงิน"]),
                           ("vat", ["ภาษีมูลค่าเพิ่ม", "VAT"]),
                           ("grand_total", ["รวมทั้งสิ้น", "จำนวนเงินรวมทั้งสิ้น", "ยอดสุทธิ"])):
            if key not in out and any(w in line for w in words):
                m = MONEY.search(line)
                if m:
                    out[key] = _num(m.group())
    return out


def year_ce(date_str):
    """ปีจากสตริงวันที่ไทย: พ.ศ. (24xx–26xx) → ค.ศ.; ค.ศ. ใช้ตรง ๆ"""
    if not date_str:
        return None
    ys = [int(y) for y in YEAR.findall(date_str)]
    for y in ys:
        if 2400 <= y <= 2700:
            return y - 543
        if 1990 <= y <= 2100:
            return y
    return None


# ---------------------------------------------------------------- checks
def find_exceptions(items, text):
    """หมายเหตุที่บอกว่าของรับจริง ≠ ที่เขียน: ระดับแถว (ปรับ net_qty ได้) และระดับเอกสาร"""
    ex = []
    for it in items:
        blob = " ".join(filter(None, [it.get("note"), it.get("description")]))
        m = EXCEPTION_WORDS.search(blob)
        if not m:
            continue
        q = EXCEPTION_QTY.search(blob)
        e = {"row": it.get("idx"), "kind": m.group(1), "text": it.get("note") or blob}
        if q:
            e["qty"] = float(q.group(1))
            if "qty" in it and m.group(1) in ("ชำรุด", "เสียหาย", "แตก", "หัก", "ขาด", "คืน"):
                it["net_qty"] = it["qty"] - e["qty"]
        ex.append(e)
    for line in text.splitlines():
        if line.startswith("หมายเหตุ") and EXCEPTION_WORDS.search(line):
            ex.append({"row": None, "kind": EXCEPTION_WORDS.search(line).group(1), "text": line})
    return ex


def _sim(a, b):
    na, nb = (re.sub(r"[\s\-_.()]", "", x.lower()) for x in (a, b))
    ratio = difflib.SequenceMatcher(None, na, nb).ratio()
    ta, tb = set(a.lower().split()), set(b.lower().split())
    jac = len(ta & tb) / len(ta | tb) if ta | tb else 0.0
    return max(ratio, jac)


def reconcile(items, expected, threshold=0.45):
    """เทียบของที่ส่งจริงกับใบสั่งซื้อ — จับคู่รายการด้วยความคล้ายชื่อ แล้วเทียบจำนวนสุทธิ"""
    used, lines = set(), []
    for po in expected.get("items", []):
        best, best_s = None, 0.0
        for j, it in enumerate(items):
            if j in used:
                continue
            s = _sim(po["description"], it["description"])
            if s > best_s:
                best, best_s = j, s
        if best is None or best_s < threshold:
            lines.append({"po_item": po["description"], "ordered": po["qty"], "unit": po.get("unit"),
                          "status": "missing", "match_score": round(best_s, 2)})
            continue
        used.add(best)
        it = items[best]
        net = it.get("net_qty", it.get("qty"))
        if net is None:
            status = "qty_unreadable"
        elif abs(net - po["qty"]) < 1e-9:
            status = "ok"
        else:
            status = "over" if net > po["qty"] else "under"
        lines.append({"po_item": po["description"], "ordered": po["qty"], "unit": po.get("unit"),
                      "delivered_item": it["description"], "delivered": it.get("qty"),
                      "delivered_net": net, "status": status, "match_score": round(best_s, 2)})
    unexpected = [items[j]["description"] for j in range(len(items)) if j not in used]
    return {"po_no": expected.get("po_no"), "lines": lines, "unexpected": unexpected,
            "all_ok": all(l["status"] == "ok" for l in lines) and not unexpected}


def structural_checks(doc, today=None):
    today = today or date.today()
    items = doc["line_items"]
    c, why = {}, []

    c["has_doc_no"] = bool(doc["doc_no"])
    c["has_date"] = bool(doc["date"])
    y = doc["date_year_ce"]
    c["date_plausible"] = y is not None and today.year - 2 <= y <= today.year + 1
    c["has_items"] = len(items) > 0
    idxs = [it.get("idx") for it in items]
    c["rows_sequential"] = bool(items) and idxs == list(range(1, len(items) + 1))
    c["all_rows_have_qty"] = bool(items) and all("qty" in it for it in items)
    # หน่วยเป็นภาคบังคับเฉพาะใบส่งของ (ใบกำกับภาษี/ใบเสนอราคาหลายแบบไม่มีคอลัมน์หน่วย)
    unit_required = doc.get("doc_type") == "delivery_note"
    c["all_rows_have_unit"] = (not unit_required) or all(it.get("unit") for it in items)
    unknown = sorted({it["unit"] for it in items if it.get("unit") and it["unit"] not in UNITS})
    c["units_recognized"] = not unknown
    ambiguous = [it["qty_raw"] for it in items if "qty" not in it and it.get("qty_raw")]
    c["no_ambiguous_qty"] = not ambiguous

    if not c["has_doc_no"]:
        why.append("อ่านเลขที่เอกสารไม่ได้")
    if not c["has_date"]:
        why.append("อ่านวันที่ไม่ได้")
    elif not c["date_plausible"]:
        why.append("วันที่ผิดปกติ ({}) — ปีไม่อยู่ในช่วงที่คาด".format(doc["date"]))
    if not c["has_items"]:
        why.append("ไม่พบตารางรายการ")
    elif not c["rows_sequential"]:
        why.append("ลำดับแถวไม่ต่อเนื่อง ({}) — OCR อาจอ่านแถวหาย/ซ้ำ".format(
            ", ".join(str(i) for i in idxs)))
    if items and not c["all_rows_have_qty"]:
        why.append("บางแถวอ่านจำนวนไม่ได้")
    if ambiguous:
        why.append("จำนวนกำกวม: {} (4.000 = 4 หรือ 4,000?)".format(", ".join(ambiguous)))
    if unit_required and items and not c["all_rows_have_unit"]:
        why.append("บางแถวไม่มีหน่วย")
    if unknown:
        why.append("หน่วยไม่รู้จัก: {} — อาจเป็น OCR อ่านผิด".format(", ".join(unknown)))
    return c, why


def arithmetic_checks(doc):
    """ใบที่มียอดเงิน: รายการรวม = subtotal, VAT = 7%, subtotal+VAT = ยอดรวม"""
    t, c, why = doc["totals"], {}, []
    items_sum = round(sum(i.get("amount", 0) for i in doc["line_items"]), 2)
    if "subtotal" in t:
        c["items_match_subtotal"] = abs(items_sum - t["subtotal"]) < 0.01
        if not c["items_match_subtotal"]:
            why.append("รายการรวม {:,.2f} ≠ รวมเป็นเงิน {:,.2f}".format(items_sum, t["subtotal"]))
    if {"subtotal", "vat", "grand_total"} <= t.keys():
        c["vat_is_7pct"] = abs(t["subtotal"] * 0.07 - t["vat"]) < 1.0
        c["total_adds_up"] = abs(t["subtotal"] + t["vat"] - t["grand_total"]) < 0.02
        if not c["vat_is_7pct"]:
            why.append("VAT {:,.2f} ไม่ใช่ 7% ของ {:,.2f}".format(t["vat"], t["subtotal"]))
        if not c["total_adds_up"]:
            why.append("subtotal + VAT ≠ ยอดรวม {:,.2f}".format(t["grand_total"]))
    return c, why


# ---------------------------------------------------------------- entry
def extract(md, expected=None, today=None):
    text = strip_html(md)
    pairs = kv_pairs(md)
    date_str = (DOC_DATE.search(text) or [None, None])[1]
    m_no = DOC_NO.search(text) or DOC_NO_TH.search(text)
    doc = {
        "doc_no": m_no.group(1) if m_no else None,
        "date": date_str,
        "date_year_ce": year_ce(date_str),
        "tax_ids": TAX_ID.findall(text),
        "site": _lookup(pairs, r"หน่วยงาน|โครงการ|สถานที่ส่ง"),
        "receiver": _lookup(pairs, r"ผู้รับ"),
        "line_items": line_items(md),
        "totals": totals(md),
    }
    has_items = bool(doc["line_items"])
    # ยอดเงินในร้อยแก้ว (เช่น บันทึกข้อความขออนุมัติ) ไม่ทำให้เป็นใบแจ้งหนี้ — ต้องมีตารางรายการด้วย
    has_money = any("amount" in i for i in doc["line_items"]) or (bool(doc["totals"]) and has_items)
    if has_money:
        doc["doc_type"] = "invoice_like"
    elif has_items:
        doc["doc_type"] = "delivery_note"
    else:
        doc["doc_type"] = "text"

    if doc["doc_type"] == "text":
        # เอกสารร้อยแก้ว: ไม่มีรายการ/ยอดให้ตรวจ — ส่งต่อให้ RAG ตรงๆ ไม่เข้าคิว review ของงานคุมต้นทุน
        doc.update({"exceptions": [], "reconciliation": None, "checks": {},
                    "review_reasons": [], "needs_review": False,
                    "note": "text document — no line items or totals to check"})
        return doc

    checks, reasons = structural_checks(doc, today)
    if has_money:
        c2, w2 = arithmetic_checks(doc)
        checks.update(c2)
        reasons += w2
    doc["exceptions"] = find_exceptions(doc["line_items"], text)
    for e in doc["exceptions"]:
        reasons.append("ข้อยกเว้น{}: {}".format(
            " แถว {}".format(e["row"]) if e.get("row") else "", e["text"]))
    doc["reconciliation"] = None
    if expected:
        rec = reconcile(doc["line_items"], expected)
        doc["reconciliation"] = rec
        for l in rec["lines"]:
            if l["status"] == "missing":
                reasons.append("PO สั่ง '{}' {} {} แต่ไม่พบในใบส่งของ".format(
                    l["po_item"], l["ordered"], l["unit"] or ""))
            elif l["status"] in ("over", "under"):
                reasons.append("'{}' สั่ง {:g} รับสุทธิ {:g} ({})".format(
                    l["po_item"], l["ordered"], l["delivered_net"],
                    "เกิน" if l["status"] == "over" else "ขาด"))
            elif l["status"] == "qty_unreadable":
                reasons.append("'{}' อ่านจำนวนที่ส่งไม่ได้".format(l["po_item"]))
        for u in rec["unexpected"]:
            reasons.append("ส่งของที่ไม่อยู่ใน PO: '{}'".format(u))

    # ใบส่งของที่ไม่มี PO ให้เทียบ = ยังยืนยันไม่ได้ว่าของตรงกับที่สั่ง → ต้องมีคนดูเสมอ
    if doc["doc_type"] == "delivery_note" and expected is None:
        reasons.append("ไม่มีใบสั่งซื้อให้เทียบ — ยืนยันจำนวนไม่ได้")

    doc["checks"] = checks
    doc["review_reasons"] = reasons
    doc["needs_review"] = bool(reasons) or not all(checks.values())
    return doc


if __name__ == "__main__":
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    args = sys.argv[1:]
    po = None
    if "--po" in args:
        i = args.index("--po")
        with open(args[i + 1], encoding="utf-8") as f:
            po = json.load(f)
        del args[i:i + 2]
    for path in args:
        with open(path, encoding="utf-8") as f:
            md = f.read()
        print("=== {} ===".format(path))
        print(json.dumps(extract(md, po), ensure_ascii=False, indent=2))
