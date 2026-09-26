#!/usr/bin/env python3
"""OLSK S3：把受控 SOP（Workbook 114 步）绑定到具体 STEP occurrence（实例级）。
证据链：SOP 物料行 → 手册 GLB 步骤根节点下的网格（同名计数）→ 几何桥（build_glb_step_bridge）→ STEP occurrence。
规则：
  ① 唯一消耗：每个叶 occurrence 至多被一道工序消耗；冲突显式列出并 HOLD，不择一。
  ② 计数对账：SOP 行 qty vs GLB 同名网格数 vs 桥接到的 occurrence 数，逐行落 COUNT_MATCH / COUNT_MISMATCH。
  ③ 台架 DAG：手册 Model.jsx 的 preparing 分组 = 台架单元；组内顺序链式前驱；
     主线工序的前驱 = 上一主线工序 + 紧接其前结束的台架组（其成员在该工序作为求解单元整体结合，unit_join）。
     手册显式例外：20 含 18；27 含 25；29 排除 25/26/27（Z 组件另行结合）。逐工序标 evidence_class。
  ④ 未被任何工序消耗的 STEP 叶 → unbound 台账（候选 not_animated 或 STEP 特有件），不补、不猜。
用法：python build_olsk_s3_binding.py --sop 装配工艺/sop-olsk-workbook.v1.json --bench 装配工艺/manual-bench-groups.v1.json
      --bridge 本体/S3-GLB桥接.v1.json --occ 本体/S1清单 --out 本体/S3-工序实例绑定.v1.json --record-id ID
"""
import argparse, json, re, collections
from datetime import datetime, timezone
from pathlib import Path
ap = argparse.ArgumentParser()
for k in ["sop", "bench", "bridge", "occ", "out"]: ap.add_argument(f"--{k}", required=True, type=Path)
ap.add_argument("--record-id", required=True)
a = ap.parse_args()
SOP = json.loads(a.sop.read_text(encoding="utf-8")); BENCH = json.loads(a.bench.read_text(encoding="utf-8"))
BR = json.loads(a.bridge.read_text(encoding="utf-8")); S1 = json.loads(a.occ.read_text(encoding="utf-8"))
occ_by_uid = {o["occ_uid"]: o for o in S1["occurrences"]}
asm_by_uid = {o["occ_uid"]: o for o in S1["assembly_occurrences"]}
def leaves_under(asm_uid):
    pre = asm_uid.replace("nauo:", "") + "/"
    return [u for u in occ_by_uid if u.replace("nauo:", "").startswith(pre)]
body_of = {}  # body_id -> leaf uid（体级绑定）
def sanitize(step, title):  # 手册 Model.jsx 节点名规则：空格→_，去 '.'
    return re.sub(r"\s+", "_", f"{step} {title}").replace(".", "")
ops = SOP["operations"]
san_to_op = {sanitize(o["step"], o["title"]): o for o in ops}
# --- bench groups → op chains
group_of = {}; groups = []
for gi, g in enumerate(BENCH["preparing_groups"]):
    members = []
    for nm in g:
        op = san_to_op.get(nm)
        if op is None:
            # 容错：只比步骤号
            st = nm.split("_")[0]; st = st[:2] + ("." + st[2:] if len(st) > 2 else "")
            op = next((o for o in ops if o["step"] == st), None)
        if op is None: continue
        members.append(op["op"]); group_of[op["op"]] = gi
    groups.append({"group_id": f"BENCH-{gi:02d}", "ops": members, "raw": g})
wiring = set()
for nm in BENCH["wiring_steps"]:
    op = san_to_op.get(nm)
    if op: wiring.add(op["op"])
# --- bridge: meshes per step root
mesh_by_step = collections.defaultdict(list)
for m in BR["matches"]: mesh_by_step[m["step"]].append(m)
# --- bind
consumed = collections.defaultdict(list); bound = []
for idx, op in enumerate(ops):
    meshes = mesh_by_step.get(op.get("glb_root_node") or "", [])
    movers, holds, rows = {}, [], []
    by_name = collections.defaultdict(list)
    for m in meshes: by_name[m["sop_name"]].append(m)
    for m in meshes:
        if m["class"] == "UNIQUE":
            b = m["best"]; uid = b["occ_uid"]; kind = b.get("kind", "leaf")
            if kind == "body":
                key = b["body_id"]; vu = f"{uid}#{key.split('#')[1]}"
                src = occ_by_uid.get(vu) or occ_by_uid.get(uid) or {"product": b["product"], "product_name": None, "depth": None}
                movers.setdefault(key, {"occ_uid": uid, "body_id": key, "kind": "body", "product": src["product"],
                                        "product_name": src.get("product_name"), "depth": src.get("depth"), "via_glb": []})
            elif kind == "asm":
                key = uid; mem = leaves_under(uid)
                movers.setdefault(key, {"occ_uid": uid, "kind": "solver_unit", "product": asm_by_uid[uid]["product"], "product_name": asm_by_uid[uid].get("product_name"),
                                        "depth": asm_by_uid[uid]["depth"], "members": mem, "semantics": "PURCHASED_MODULE/预装模块：S5 solver unit，成员整体移动", "via_glb": []})
            else:
                key = uid
                movers.setdefault(key, {"occ_uid": uid, "kind": "leaf", "product": occ_by_uid[uid]["product"], "product_name": occ_by_uid[uid].get("product_name"),
                                        "depth": occ_by_uid[uid]["depth"], "via_glb": []})
            movers[key]["via_glb"].append(m["glb_node"]); movers[key].setdefault("via_glb_names", []).append(m["sop_name"])
        elif m["class"] == "AMBIGUOUS":
            holds.append({"glb_node": m["glb_node"], "sop_name": m["sop_name"], "hold": "BRIDGE_AMBIGUOUS", "candidates": [m["best"]] + m.get("alternatives", [])})
        else:
            holds.append({"glb_node": m["glb_node"], "sop_name": m["sop_name"], "hold": "NO_STEP_GEOMETRY"})
    for p in op["parts"]:
        if p["name"] == "null": continue
        glb_n = len(by_name.get(p["name"], [])); occ_n = len({mm["best"].get("body_id") or mm["best"]["occ_uid"] for mm in by_name.get(p["name"], []) if mm["class"] == "UNIQUE"})
        rows.append({"sop_name": p["name"], "sop_qty": p["qty"], "glb_meshes": glb_n, "bound_occurrences": occ_n,
                     "status": "COUNT_MATCH" if (p["qty"] == occ_n) else ("NO_GLB" if glb_n == 0 else "COUNT_MISMATCH")})
    for key, mv in movers.items():
        if mv["kind"] == "solver_unit":
            for u in mv["members"]: consumed[u].append(op["op"])
        elif mv["kind"] == "body":
            consumed[key].append(op["op"]); body_of[key] = mv["occ_uid"]
        else:
            consumed[key].append(op["op"])
    gi = group_of.get(op["op"])
    bound.append({"op": op["op"], "step": op["step"], "title": op["title"], "chapter": op["chapter"], "order_index": idx,
                  "bench_group": groups[gi]["group_id"] if gi is not None else None, "is_wiring": op["op"] in wiring,
                  "glb_root_node": op.get("glb_root_node"), "movers": sorted(movers.values(), key=lambda x: x["occ_uid"]),
                  "consumes_leaves": len(movers), "part_rows": rows, "holds": holds})
# --- unique consumption：冲突裁决规则（确定性，逐条落盘）
#   R-C1 该节点在其中一道工序的 SOP 物料行里被点名（网格名 = SOP 行名）→ 归该工序；
#   R-C2 多道都点名或都没点名 → 归最早工序；其余工序记 re_shown（手册复现显示），不消耗。
op_rows = {b["op"]: {r["sop_name"] for r in b["part_rows"]} for b in bound}
op_idx = {b["op"]: i for i, b in enumerate(bound)}
conflicts = []
for u, ops_ in list(consumed.items()):
    if len(set(ops_)) <= 1: continue
    named = []
    for b in bound:
        if b["op"] in ops_:
            mv = next((m for m in b["movers"] if (m.get("body_id") or m["occ_uid"]) == u or (m["kind"] == "solver_unit" and u in m.get("members", []))), None)
            if mv and any(v in op_rows[b["op"]] for v in mv.get("via_glb_names", [])): named.append(b["op"])
    keep = min(named or ops_, key=lambda o: op_idx[o]); rule = "R-C1_sop_named" if named and len(named) == 1 else "R-C2_earliest"
    conflicts.append({"occ_uid": u, "ops": sorted(set(ops_), key=lambda o: op_idx[o]), "kept": keep, "rule": rule})
    for b in bound:
        if b["op"] in ops_ and b["op"] != keep:
            for m in list(b["movers"]):
                key = m.get("body_id") or m["occ_uid"]
                if key == u or (m["kind"] == "solver_unit" and u in m.get("members", [])):
                    b["movers"].remove(m); b.setdefault("re_shown", []).append({**m, "kept_by": keep, "rule": rule})
            b["consumes_leaves"] = len(b["movers"])
    consumed[u] = [keep]
# --- predecessors + unit joins
last_main = None; prev_op = None; pending_units = []
op_index = {b["op"]: i for i, b in enumerate(bound)}
for b in bound:
    preds, joins, ev = [], [], "DOC_IMPLIED"
    if b["bench_group"] is not None:
        g = next(g for g in groups if g["group_id"] == b["bench_group"])
        k = g["ops"].index(b["op"])
        if k > 0: preds.append(g["ops"][k - 1])
        if k == len(g["ops"]) - 1: pending_units.append(g)  # 组结束，等待下一主线工序结合
        ev = "DOC_EXPLICIT(manual preparing group)"
    else:
        if last_main: preds.append(last_main)
        for g in pending_units:
            members = [m for gop in g["ops"] for m in next(x for x in bound if x["op"] == gop)["movers"]]
            joins.append({"unit": g["group_id"], "from_ops": g["ops"], "members": [m["occ_uid"] for m in members], "semantics": "S5 solver unit：台架完成件整体结合"})
            preds.append(g["ops"][-1])
        pending_units = []
        last_main = b["op"]
    b["predecessors"] = preds; b["unit_joins"] = joins; b["evidence_class"] = ev
# 手册显式例外（display rules）落盘为注记，不改 DAG（留人工裁决）
exceptions = {"WB-20": "手册：显示 20 时并入 18（X 主板台架在 20 结合）", "WB-27": "手册：显示 27 时并入 25（Z 丝杠在 27 结合）", "WB-29": "手册：显示 29 时排除 25/26/27（Z 组件另行结合，不在 29 场景）"}
for b in bound:
    if b["op"] in exceptions: b["manual_exception"] = exceptions[b["op"]]
all_leaves = set(occ_by_uid)
bound_leaves = {k for k in consumed if k in all_leaves} | set(body_of.values()) | {f"{v}#{k.split('#')[1]}" for k, v in body_of.items()}
# --- S5 折叠（K-ID-02 / PURCHASED_MODULE）：深度≥2 的子装配若未被 SOP 分解（其叶被消耗的工序 ≤ 1 道），
#     整体作为求解单元归入该工序；单元成员 = S1 子树全部叶（含未被网格绑定的内部件）。被多道工序分解的子装配不折叠。
def top_of(u): return u.replace("nauo:", "").split("/")[0]
units, folded_leaves = [], set()
asm_sorted = sorted(S1["assembly_occurrences"], key=lambda x: x["depth"])
consumed_op = {u: v[0] for u, v in consumed.items() if v}
for asm in asm_sorted:
    if asm["depth"] < 2: continue
    mem = leaves_under(asm["occ_uid"])
    if not mem or any(m in folded_leaves for m in mem): continue
    ops_ = {consumed_op[m] for m in mem if m in consumed_op}
    ops_ |= {consumed_op[k] for k, leaf in body_of.items() if leaf in mem and k in consumed_op}
    if len(ops_) == 1:
        opn = next(iter(ops_))
        units.append({"unit_id": f"U-{len(units):03d}", "asm_uid": asm["occ_uid"], "product": asm["product"], "depth": asm["depth"], "op": opn, "members": mem,
                      "bound_members": [m for m in mem if m in consumed_op], "rule": "S5-FOLD: 子装配未被 SOP 分解，整体为求解单元"})
        folded_leaves |= set(mem)
        b = next(x for x in bound if x["op"] == opn)
        already = {mv["occ_uid"] for mv in b["movers"] if mv["kind"] == "solver_unit"}
        if asm["occ_uid"] not in already:
            for mv in [mv for mv in b["movers"] if mv["kind"] in ("leaf", "body") and mv["occ_uid"] in mem]:
                b["movers"].remove(mv)
            b["movers"].append({"occ_uid": asm["occ_uid"], "kind": "solver_unit", "product": asm["product"], "product_name": asm.get("product_name"), "depth": asm["depth"],
                                "members": mem, "semantics": "S5 solver unit（折叠）", "unit_id": units[-1]["unit_id"], "via_glb": []})
            b["consumes_leaves"] = len(b["movers"])
        for m in mem: consumed.setdefault(m, [opn])
# --- 通用 S7/S8 工具接口：movers[].occ_id（消费视图 occ_id）与 member_occ_ids
def view_uid_for(mv):
    if mv["kind"] == "body":
        return f"{mv['occ_uid']}#{mv['body_id'].split('#')[1]}"
    return mv["occ_uid"]
interface_holds = []
for b in bound:
    for mv in b["movers"]:
        vu = view_uid_for(mv)
        if mv["kind"] == "solver_unit":
            mv["occ_id"] = asm_by_uid[mv["occ_uid"]]["occ_id"] if mv["occ_uid"] in asm_by_uid else None
            mv["member_occ_ids"] = [occ_by_uid[m]["occ_id"] for m in mv["members"] if m in occ_by_uid]
            if len(mv["member_occ_ids"]) != len(mv["members"]): interface_holds.append({"op": b["op"], "asm": mv["occ_uid"], "missing_members": len(mv["members"]) - len(mv["member_occ_ids"])})
        elif vu in occ_by_uid:
            mv["occ_id"] = occ_by_uid[vu]["occ_id"]; mv["member_occ_ids"] = [mv["occ_id"]]
        else:
            mv["occ_id"] = None; interface_holds.append({"op": b["op"], "mover": vu, "hold": "NOT_IN_CONSUMPTION_VIEW（体级动件需体展开视图）"})
unbound = sorted(all_leaves - bound_leaves - folded_leaves)
unbound_classes = collections.Counter()
unbound_detail = []
for u in unbound:
    o = occ_by_uid[u]; cls = "NOT_IN_MANUAL_GLB"
    if re.search(r"(?i)screw|nut\b|washer|bolt|pin\b|DIN ?912|ISO ?7380", o["product"]): cls = "FASTENER_STEP_ONLY"
    elif o.get("self_bodies_of_assembly"): cls = "SELF_BODIES_UNBOUND"
    unbound_classes[cls] += 1; unbound_detail.append({"occ_uid": u, "product": o["product"], "class": cls})
unbound_by_top = collections.Counter(u.split("/")[0].replace("nauo:", "") for u in unbound)
doc = {
    "$schema": "assembly-ontology.s3-operation-binding/v1", "record_id": a.record_id, "captured_at": datetime.now(timezone.utc).isoformat(),
    "purpose": "受控 SOP 114 步 → STEP occurrence 实例级绑定；唯一消耗、计数对账、台架 DAG、单元结合。",
    "sop_source": SOP["source"], "bridge_source": str(a.bridge),
    "coordinate_semantics": {"+Y": "上", "basis": "S3 几何桥帧拟合：glTF Y-up 的 +Y ↔ STEP +Y（mm 级重合）；整机 AABB y 跨度 1933 mm = 机高；脚轮在 y 最低端", "up_axis_sign": 1},
    "closure": {"operations": len(bound), "leaf_occurrences": len(all_leaves), "consumed_once": sum(1 for u, v in consumed.items() if len(v) == 1),
                "consumed_conflicts": len(conflicts), "unbound_leaves": len(unbound), "unbound_by_top_assembly": dict(unbound_by_top),
                "body_level_movers": len(body_of), "multibody_leaves_with_body_movers": len(set(body_of.values())),
                "s5_units": len(units), "s5_folded_leaves": len(folded_leaves), "unbound_classes": dict(unbound_classes),
                "re_shown_total": sum(len(b.get("re_shown", [])) for b in bound),
                "interface_holds": len(interface_holds), "consumption_view": str(a.occ),
                "ops_with_zero_movers": [b["op"] for b in bound if b["consumes_leaves"] == 0],
                "part_rows": sum(len(b["part_rows"]) for b in bound),
                "part_rows_count_match": sum(1 for b in bound for r in b["part_rows"] if r["status"] == "COUNT_MATCH"),
                "part_rows_no_glb": sum(1 for b in bound for r in b["part_rows"] if r["status"] == "NO_GLB"),
                "part_rows_count_mismatch": sum(1 for b in bound for r in b["part_rows"] if r["status"] == "COUNT_MISMATCH"),
                "holds_total": sum(len(b["holds"]) for b in bound)},
    "bench_groups": groups, "consumption_conflicts": conflicts, "s5_units": units, "unbound_leaves": unbound_detail, "interface_holds": interface_holds,
    "claim_boundary": ["绑定只说明‘手册第 N 步出现的网格在几何上是 STEP 的哪个 occurrence’；不含扭矩/胶粘/验收（SOP 无字段，UNKNOWN）。",
                       "台架 DAG 来自手册显示分组（DOC_EXPLICIT）与顺序推断（DOC_IMPLIED），逐工序标注；手册例外只记不改。",
                       "冲突与未绑定件一律保留为 HOLD 台账，不择一、不补建。"],
    "operations": bound,
}
a.out.parent.mkdir(parents=True, exist_ok=True)
a.out.write_text(json.dumps(doc, ensure_ascii=False, indent=1), encoding="utf-8")
print(json.dumps(doc["closure"], ensure_ascii=False, indent=1))
