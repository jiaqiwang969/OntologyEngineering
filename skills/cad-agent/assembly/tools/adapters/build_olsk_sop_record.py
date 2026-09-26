#!/usr/bin/env python3
"""S3 第一步（OLSK）：把受控 SOP（Assembly_Manual/data/Workbook.csv，114 步）逐字搬运为工序记录，
并把手册 GLB 的 114 个步骤根节点（子节点=零件名计数）作为旁证并列落盘。
不做任何名称对齐、身份绑定或顺序推断——那是 S3 绑定脚本的事。"""
import csv, hashlib, json, os, re, sys
from datetime import datetime, timezone
from pathlib import Path
ROOT = Path(os.environ["OLSK_SOURCE_ROOT"]).expanduser().resolve()
WB = ROOT / "Assembly_Manual/data/Workbook.csv"
GLB = ROOT / "engineering_project/analysis/glb_steps.json"
OUT = Path(sys.argv[1])
rd = list(csv.reader(open(WB, newline="", encoding="utf-8-sig")))
cols = [c.strip() for c in rd[0]]
ops, cur = [], None
for i, r in enumerate(rd[1:], start=2):
    r = r + [""] * (len(cols) - len(r)); d = dict(zip(cols, r))
    if d["Step Nr"].strip():
        sid = d["Step Nr"].strip()
        cur = {"op": f"WB-{sid}", "step": sid, "chapter": sid.split(".")[0], "title": d["Step Title"].strip(),
               "csv_row": i, "time": d["Time"].strip() or None, "parts": [], "tools": [],
               "remarks": [], "notes": [], "problems": [], "details": [], "how_tos": [], "images": [],
               "rhino_traced": d["Rhino Traced"].strip() or None}
        ops.append(cur)
    if cur is None:
        continue
    if d["Part name"].strip():
        q = d["Parts list QTY"].strip()
        try: qv = int(float(q)) if q else None
        except ValueError: qv = q
        cur["parts"].append({"name": d["Part name"].strip(), "qty": qv, "csv_row": i})
    if d["Tool name"].strip():
        cur["tools"].append({"name": d["Tool name"].strip(), "qty": d["Tools list Quantity"].strip() or None})
    for key, dst in [("Remarks", "remarks"), ("Notes", "notes"), ("Problems & questions", "problems"), ("Details", "details"), ("How Tos", "how_tos"), ("Image", "images")]:
        if d[key].strip(): cur[dst].append(d[key].strip())
glb = json.loads(GLB.read_text(encoding="utf-8"))
glb_in = glb.get("IN", {})
def glb_key(op):
    return f"{op['step']} {op['title']}"
matched = 0
for op in ops:
    k = glb_key(op)
    node = glb_in.get(k)
    if node is None:
        # 手册根节点名 = "步骤号 空格 标题"，标题偶有空格/措辞差异：先去空格小写比对，再按步骤号唯一前缀匹配（落盘 match_rule）
        nk = re.sub(r"\s+", " ", k).lower()
        for kk in glb_in:
            if re.sub(r"\s+", " ", kk).lower() == nk: node = glb_in[kk]; k = kk; op["glb_match_rule"] = "title_normalized"; break
    if node is None:
        cands = [kk for kk in glb_in if kk.split(" ")[0] == op["step"]]
        if len(cands) == 1:
            k = cands[0]; node = glb_in[k]; op["glb_match_rule"] = "step_number_unique"; op["glb_title_differs"] = k
    if node is not None and "glb_match_rule" not in op: op["glb_match_rule"] = "exact"
    op["glb_root_node"] = k if node is not None else None
    # compose_assembly_film_generic 消费字段（与 Poppy SOP 记录同名；只搬运，不新增主张）
    fast_rows = [f"{p['qty']}×{p['name']}" for p in op["parts"] if re.search(r"(?i)screw|nut\b|washer|bolt|standoff", p["name"])]
    op["fasteners"] = "；".join(fast_rows) if fast_rows else None
    op["target"] = op["title"]
    op["notes"] = " / ".join(op["remarks"] + op["notes"]) if (op["remarks"] or op["notes"]) else None
    op["video"] = None
    op["tools_text"] = "；".join(f"{t['name']}" for t in op["tools"]) if op["tools"] else None
    op["glb_children_counts"] = node
    if node is not None: matched += 1
doc = {
    "$schema": "assembly-ontology.sop-record/v1",
    "record_id": "OLSK-V3-SOP-WORKBOOK-V001",
    "captured_at": datetime.now(timezone.utc).isoformat(),
    "purpose": "受控 SOP 逐字搬运（Workbook.csv）+ 手册 GLB 步骤根节点旁证；不含身份绑定。",
    "source": {"sop": {"path": str(WB), "sha256": hashlib.sha256(WB.read_bytes()).hexdigest()},
               "glb_index": {"path": str(GLB), "sha256": hashlib.sha256(GLB.read_bytes()).hexdigest(),
                             "origin": "Assembly_Manual/public/OLSK_Large_CNC_V2_IN.glb 114 根节点子节点名计数"}},
    "summary": {"operations": len(ops), "chapters": len({o["chapter"] for o in ops}),
                "part_rows": sum(len(o["parts"]) for o in ops), "part_qty_sum": sum(p["qty"] for o in ops for p in o["parts"] if isinstance(p["qty"], int)),
                "ops_with_tools": sum(1 for o in ops if o["tools"]), "ops_with_glb_node": matched,
                "ops_without_glb_node": [o["step"] for o in ops if o["glb_root_node"] is None]},
    "field_semantics": {"order": "Workbook 行序 = 步序（01.1 … 78）；同章小数子步按数字序", "torque": "UNKNOWN（无字段）", "adhesive": "UNKNOWN（无字段）",
                        "acceptance": "UNKNOWN（无字段）", "tools": "仅 01.1 填写（Allen Key 6 / Wrench 14）；其余 UNKNOWN",
                        "part_names": "手册命名（如 'B-screw M8-25'、'Base Inner Bottom Beam'），与 STEP 产品名/BOM 名不是同一命名空间，须 S3 显式别名表"},
    "claim_boundary": ["逐字搬运，未做别名归一；名称不一致不在此处消解。", "GLB 子节点计数只是手册作者的分组旁证，不是几何或身份证据。",
                       "SOP 无版本号与签发人（S0 hold 继承）。"],
    "operations": ops,
}
OUT.parent.mkdir(parents=True, exist_ok=True)
OUT.write_text(json.dumps(doc, ensure_ascii=False, indent=1), encoding="utf-8")
print(json.dumps(doc["summary"], ensure_ascii=False))
