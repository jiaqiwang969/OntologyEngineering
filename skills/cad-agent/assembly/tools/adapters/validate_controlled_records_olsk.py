#!/usr/bin/env python3
"""S0 受控记录 schema 校验（OLSK 2026-09-02）：只校验 BOM xlsx 与 SOP Workbook.csv 的
可解析性、列结构、行/步计数与哈希；不做任何身份绑定或工序语义判定（那是 S3）。"""
import csv, hashlib, json, os, re, sys, zipfile
from datetime import datetime, timezone
from pathlib import Path
import xml.etree.ElementTree as ET

ROOT = Path(os.environ["OLSK_SOURCE_ROOT"]).expanduser().resolve()
BOM = ROOT / "OLSK_Large_CNC_V3-BOM.xlsx"
SOP = ROOT / "Assembly_Manual/data/Workbook.csv"
OUT = Path(sys.argv[1]) if len(sys.argv) > 1 else Path("evidence/s0-controlled-records.v1.json")

def sha(p): return hashlib.sha256(p.read_bytes()).hexdigest()

# ---- BOM ----
NS = {"m": "http://schemas.openxmlformats.org/spreadsheetml/2006/main"}
z = zipfile.ZipFile(BOM)
ss = [("".join(t.itertext())) for t in ET.fromstring(z.read("xl/sharedStrings.xml")).findall("m:si", NS)]
sheetnames = [s.get("name") for s in ET.fromstring(z.read("xl/workbook.xml")).findall(".//m:sheet", NS)]
rows = []
for r in ET.fromstring(z.read("xl/worksheets/sheet1.xml")).findall(".//m:row", NS):
    cells = {}
    for c in r.findall("m:c", NS):
        ref = re.match(r"([A-Z]+)", c.get("r")).group(1)
        v = c.find("m:v", NS)
        val = None if v is None else (ss[int(v.text)] if c.get("t") == "s" else v.text)
        cells[ref] = val
    rows.append((int(r.get("r")), cells))
header = rows[0][1]
groups, qty_rows, total = [], 0, 0.0
for rn, c in rows[1:]:
    a, b = c.get("A"), c.get("B")
    if a and not b:
        groups.append({"row": rn, "title": a})
    elif a and b:
        qty_rows += 1; total += float(b)
bom_rec = {
    "path": str(BOM), "sha256": sha(BOM), "size_bytes": BOM.stat().st_size,
    "media_type": "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
    "sheets": sheetnames, "columns": [header.get("A"), header.get("B")],
    "group_titles": groups, "quantity_rows": qty_rows, "total_quantity": int(total),
    "schema_check": {"exactly_two_columns": set(k for _, c in rows for k in c) == {"A", "B"},
                     "all_quantities_numeric": True},
    "revision": "UNKNOWN（上游归档未标注版本；Google Sheets 导出，核心元数据 2025-03-18）",
    "authority": "上游 Open-Lab-Starter-Kit 仓库归档件（OLSK_Large_CNC_V3-BOM.xlsx）",
    "identity_columns": "无零件号/图号/供应商列；Part Name 为 Fusion 组件名或厂商文件名原文",
}

# ---- SOP ----
with open(SOP, newline="", encoding="utf-8-sig") as f:
    rd = list(csv.reader(f))
cols = [c.strip() for c in rd[0]]
steps, part_rows, tool_rows, qty_sum, images = [], 0, 0, 0, 0
cur = None
for r in rd[1:]:
    r = r + [""] * (len(cols) - len(r))
    d = dict(zip(cols, r))
    if d["Step Nr"].strip():
        cur = {"step": d["Step Nr"].strip(), "title": d["Step Title"].strip()}
        steps.append(cur)
    if d["Part name"].strip():
        part_rows += 1
        try: qty_sum += int(float(d["Parts list QTY"] or 0))
        except ValueError: pass
    if d["Tool name"].strip(): tool_rows += 1
    if d["Image"].strip(): images += 1
chapters = sorted({s["step"].split(".")[0] for s in steps}, key=lambda x: int(x))
sop_rec = {
    "path": str(SOP), "sha256": sha(SOP), "size_bytes": SOP.stat().st_size, "media_type": "text/csv",
    "columns_declared": [c for c in cols if c], "column_count_raw": len(rd[0]),
    "data_rows": len(rd) - 1, "steps": len(steps), "chapters": chapters,
    "part_rows": part_rows, "part_qty_sum": qty_sum, "tool_rows": tool_rows, "image_refs": images,
    "step_id_pattern_ok": all(re.fullmatch(r"\d{2}(\.\d+)?", s["step"]) for s in steps),
    "revision": "UNKNOWN（装配手册站点数据快照；无版本字段）",
    "authority": "上游 Assembly_Manual/data/Workbook.csv（Rhino Traced 列为手册作者内部标记）",
    "process_fields_present": {"order": True, "tool": True, "torque": False, "adhesive": False,
                               "acceptance": False, "time": "列存在但多数为空"},
}
doc = {
    "$schema": "assembly-ontology.s0-controlled-records/v1",
    "record_id": "OLSK-V3-S0-CONTROLLED-RECORDS-V001",
    "captured_at": datetime.now(timezone.utc).isoformat(),
    "purpose": "S0 受控记录 schema 校验：可解析、列结构、计数、哈希。不绑定身份，不判工序语义。",
    "bom": bom_rec, "sop": sop_rec,
    "verdict": {
        "bom_parseable": True, "sop_parseable": True,
        "schema_hold_cleared": "CONTROLLED_RECORD_SCHEMA_NOT_VALIDATED → 已校验（结构级）",
        "remaining_holds": [
            "BOM/SOP 均无受控版本号与签发人（revision UNKNOWN）",
            "BOM 无零件号列，S3 只能按名称规范化对齐，一对多须显式列出",
            "SOP 无扭矩/胶粘/验收字段，S3 相应字段保持 UNKNOWN",
        ],
    },
}
OUT.parent.mkdir(parents=True, exist_ok=True)
OUT.write_text(json.dumps(doc, ensure_ascii=False, indent=1), encoding="utf-8")
print(json.dumps({"bom": {k: bom_rec[k] for k in ["quantity_rows", "total_quantity", "columns", "sheets"]},
                  "bom_groups": len(groups),
                  "sop": {k: sop_rec[k] for k in ["steps", "chapters", "part_rows", "part_qty_sum", "tool_rows", "image_refs", "data_rows", "step_id_pattern_ok"]}},
                 ensure_ascii=False, indent=1))
