"""Set up one account to sign in with, using nothing but the platform's own API.

No direct database writes: the address is confirmed by following the link the
platform issues, exactly as a customer would, so what this produces is an
ordinary account and not a special one.

    python seed_account.py <path to the api log>
"""
import json
import os
import re
import sys
import time
import urllib.error
import urllib.request

D = os.environ.get("ANTHOVAI_API", "http://127.0.0.1:8080") + "/dashboard/v1"
LOG = sys.argv[1]

# Not defaulted, and not written down here: this repository is public, and a
# demo password committed into it is a password somebody will eventually try
# against a real deployment.
#
#   SEED_EMAIL=you@example.com SEED_PASSWORD=... python seed_account.py <log>
EMAIL = os.environ.get("SEED_EMAIL")
PASSWORD = os.environ.get("SEED_PASSWORD")
if not EMAIL or not PASSWORD:
    print("Set SEED_EMAIL and SEED_PASSWORD. The password needs 10 characters.")
    sys.exit(2)

state = {"cookie": None, "org": None}


def call(method, path, body=None, form=None, expect=(200, 201, 202, 204)):
    headers = {}
    data = None
    if form is not None:
        boundary = "----seed"
        crlf = chr(13) + chr(10)
        chunks = []
        for name, value in form:
            chunks.append("--" + boundary + crlf)
            chunks.append('Content-Disposition: form-data; name="' + name + '"' + crlf + crlf)
            chunks.append(value + crlf)
        chunks.append("--" + boundary + "--" + crlf)
        data = "".join(chunks).encode()
        headers["content-type"] = "multipart/form-data; boundary=" + boundary
    elif body is not None:
        data = json.dumps(body).encode()
        headers["content-type"] = "application/json"

    if state["cookie"]:
        headers["cookie"] = "__Host-av_session=" + state["cookie"]
    if state["org"]:
        headers["x-org-id"] = state["org"]

    req = urllib.request.Request(D + path, data=data, method=method, headers=headers)
    try:
        with urllib.request.urlopen(req) as r:
            raw, status, got = r.read(), r.status, r.headers
    except urllib.error.HTTPError as e:
        raw, status, got = e.read(), e.code, e.headers

    for value in got.get_all("set-cookie") or []:
        if value.startswith("__Host-av_session="):
            token = value.split("=", 1)[1].split(";")[0]
            if token:
                state["cookie"] = token

    parsed = json.loads(raw) if raw else None
    if status not in expect:
        print("  !! %s %s -> %s %s" % (method, path, status, parsed))
        sys.exit(1)
    return status, parsed


# --- the account ---------------------------------------------------------
status, _ = call("POST", "/auth/signup",
                 {"email": EMAIL, "password": PASSWORD, "name": "Anthovai Demo"},
                 expect=(201, 409, 422))
if status == 201:
    print("account created")
else:
    print("account already exists, signing in")

call("POST", "/auth/login", {"email": EMAIL, "password": PASSWORD})
_, me = call("GET", "/me")
user_id = me["user"]["id"]

# --- confirm the address, by following the platform's own link ------------
if not me["user"]["email_verified"]:
    before = len(open(LOG, encoding="utf-8", errors="replace").read())
    call("POST", "/auth/verify/request")
    time.sleep(1)
    tail = open(LOG, encoding="utf-8", errors="replace").read()[before:]
    token = re.search(r"/verify\?token=([0-9a-f]{64})", tail)
    if not token:
        print("  !! the confirmation link was not in the log")
        sys.exit(1)
    call("POST", "/auth/verify", {"token": token.group(1)}, expect=(204,))
    _, me = call("GET", "/me")
print("email confirmed:", me["user"]["email_verified"])

# --- an organization, if there is not one already ------------------------
if me["organizations"]:
    state["org"] = me["organizations"][0]["id"]
    print("organization already there:", state["org"])
else:
    _, org = call("POST", "/organizations",
                  {"name": "คลินิกทันตกรรม เดนทัลแคร์", "slug": "dentalcare-demo"})
    state["org"] = org["organization"]["id"]
    print("organization created:", state["org"])

_, workspaces = call("GET", "/workspaces")
workspace = workspaces["data"][0]["id"]

# --- something to look at ------------------------------------------------
_, bases = call("GET", "/knowledge_bases")
if bases["data"]:
    kb_id = bases["data"][0]["id"]
    print("knowledge base already there:", kb_id)
else:
    _, kb = call("POST", "/knowledge_bases",
                 {"workspace_id": workspace, "name": "ข้อมูลคลินิกสำหรับคนไข้"})
    kb_id = kb["id"]

    rates = "\n".join([
        "# อัตราค่าบริการ คลินิกทันตกรรมเดนทัลแคร์",
        "",
        "## ทันตกรรมทั่วไป",
        "- ตรวจสุขภาพช่องปาก พร้อมเอกซเรย์ 1 ฟิล์ม: 500 บาท",
        "- ขูดหินปูนและขัดฟัน: 900 บาทต่อครั้ง",
        "- อุดฟันด้วยวัสดุสีเหมือนฟัน: 1,200 ถึง 1,800 บาทต่อซี่",
        "- ถอนฟันธรรมดา: 1,000 บาทต่อซี่",
        "- ผ่าฟันคุด: 4,500 บาทต่อซี่",
        "",
        "## รักษารากฟัน",
        "- ฟันหน้า 8,000 บาท ฟันกรามน้อย 11,000 บาท ฟันกราม 15,000 บาท",
        "- ราคานี้ยังไม่รวมครอบฟัน ซึ่งจำเป็นต้องทำหลังรักษารากฟันเสร็จ",
        "",
        "## จัดฟัน",
        "- จัดฟันโลหะ 45,000 บาท ผ่อน 0% ได้ 10 เดือน",
        "- จัดฟันแบบใส เริ่มต้น 120,000 บาท",
    ])
    hours = "\n".join([
        "# เวลาทำการและเงื่อนไขการนัด",
        "",
        "## เวลาทำการ",
        "- จันทร์ถึงศุกร์ 10:00 ถึง 20:00 น.",
        "- เสาร์และอาทิตย์ 09:00 ถึง 18:00 น.",
        "- หยุดวันนักขัตฤกษ์",
        "",
        "## การนัดหมาย",
        "- นัดล่วงหน้าทางโทรศัพท์ 02-259-8800 หรือ LINE @dentalcare-th",
        "- เลื่อนหรือยกเลิกนัดน้อยกว่า 6 ชั่วโมงก่อนเวลานัด มีค่าธรรมเนียม 500 บาท",
        "- มาสายเกิน 20 นาที ทางคลินิกขอสงวนสิทธิ์เลื่อนนัด",
        "",
        "## สิทธิและการชำระเงิน",
        "- รับเงินสด บัตรเครดิต และพร้อมเพย์",
        "- ใช้สิทธิประกันสังคมได้เฉพาะถอนฟัน อุดฟัน ขูดหินปูน และผ่าฟันคุด วงเงิน 900 บาทต่อปี",
        "- สิทธิบัตรทองใช้ที่คลินิกไม่ได้",
    ])

    docs = []
    for title, text in [("อัตราค่าบริการ", rates), ("เวลาทำการและการนัด", hours)]:
        _, doc = call("POST", "/documents", form=[
            ("knowledge_base_id", kb_id), ("title", title), ("text", text),
        ])
        docs.append(doc["id"])

    deadline = time.time() + 120
    while time.time() < deadline:
        _, listing = call("GET", "/knowledge_bases/%s/documents" % kb_id)
        states = {d["id"]: d["status"] for d in listing["data"]}
        if all(states.get(i) in ("ready", "failed") for i in docs):
            break
        time.sleep(1)
    print("documents:", ", ".join("%s" % s for s in states.values()))

# --- a published agent ---------------------------------------------------
_, agents = call("GET", "/agents")
if agents["data"]:
    agent_id = agents["data"][0]["id"]
    print("agent already there:", agent_id)
else:
    _, agent = call("POST", "/agents",
                    {"workspace_id": workspace, "name": "ผู้ช่วยตอบคำถามคนไข้"})
    agent_id = agent["id"]
    call("PUT", "/agents/%s/knowledge_bases" % agent_id, {"knowledge_base_ids": [kb_id]})
    call("PATCH", "/agents/%s" % agent_id, {
        "config": {
            "instructions": (
                "คุณคือผู้ช่วยประจำคลินิกทันตกรรมเดนทัลแคร์ "
                "ตอบคำถามคนไข้ด้วยภาษาที่สุภาพ สั้น ตรงประเด็น "
                "ระบุราคาและเวลาเป็นตัวเลขเสมอเมื่อเอกสารระบุไว้ "
                "ห้ามวินิจฉัยหรือให้คำแนะนำการรักษา "
                "หากคนไข้ถามเรื่องอาการ ให้แนะนำให้นัดเข้ามาพบทันตแพทย์"
            ),
            "behavior": {"strict_knowledge": True},
        },
    })
    _, published = call("POST", "/agents/%s/publish" % agent_id)
    print("agent published, version", published["published_version"])

# --- a live key, since the address is confirmed --------------------------
_, keys = call("GET", "/api_keys")
secret = None
if not keys["data"]:
    _, issued = call("POST", "/api_keys",
                     {"workspace_id": workspace, "name": "เว็บคลินิก",
                      "scopes": ["chat"], "all_agents": True, "environment": "live"})
    secret = issued["secret"]

print()
print("=" * 64)
print("  sign in at   http://localhost:3000/signin")
print("  email        " + EMAIL)
print("  password     " + PASSWORD)
print()
print("  organization " + state["org"])
print("  agent        " + agent_id)
if secret:
    print("  live key     " + secret)
    print("               (shown once — the platform keeps only a hash)")
print("=" * 64)
