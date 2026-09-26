#!/usr/bin/env python3
"""画面口径穿模扫描（OLSK 新增，2026-09-03）：不豁免基线接触，只回答“观众会不会看到两件互穿”。
对冻结状态每个动件，沿其冻结轴取 t∈{0,¼,½,¾,1}·approach 的位姿，用广义绕数双向数“对方材料内的采样点占比”
（K-IF-08 双向），可见集=成片口径（前驱闭包 + 本章成员 + context，本章 not_animated 件不在场）。
输出：本体/S7-穿模审计/<op>.<run>.visual.json：每个动件与每个可见件的最大内含占比、发生的 t、分类：
  SEATED_OVERLAP（t=0 已重叠：CAD 装配态/绑定级重叠）；PATH_OVERLAP（t>0 才重叠：路径穿模）。
用法：REBUILD_CASE_DIR=<case> python visual_overlap_scan.py --op <op> --run FILM_v004 [--frac 0.02]
"""
import argparse, json, os, sys, importlib.util
from pathlib import Path
import numpy as np, trimesh
ROOT = Path(os.environ["REBUILD_CASE_DIR"]).resolve()
spec = importlib.util.spec_from_file_location("br", ROOT / "tools/baseline_region.py"); br = importlib.util.module_from_spec(spec); spec.loader.exec_module(br)
ap = argparse.ArgumentParser(); ap.add_argument("--op", required=True); ap.add_argument("--run", required=True); ap.add_argument("--frac", type=float, default=0.02); ap.add_argument("--samples", type=int, default=600)
ap.add_argument("--path_margin", type=float, default=0.05, help="路径含入超出就位本底多少算路径穿模（采样点占比）")
a = ap.parse_args()
cfg = json.loads((ROOT / "robot-config.v1.json").read_text(encoding="utf-8")); P = cfg["paths"]
MESH = ROOT / P["mesh_dir"]; man = json.loads((MESH / "manifest.json").read_text(encoding="utf-8"))["parts"]
occ = json.loads((ROOT / P["s1_manifest"]).read_text(encoding="utf-8"))["occurrences"]; id2o = {o["occ_id"]: o for o in occ}
bind = json.loads((ROOT / P["s3_binding"]).read_text(encoding="utf-8"))
members = {}
for o2 in bind["operations"]:
    for m in o2["movers"]: members[m["occ_id"]] = list(m.get("member_occ_ids") or [m["occ_id"]])
def members_of(e): return members.get(e, [e])
D = ROOT / "装配动画" / a.run; frozen = json.loads((D / "pipeline-state.v1.json").read_text(encoding="utf-8")); asm = frozen["assembly"]
chain = json.loads((D / "state-chain.json").read_text(encoding="utf-8")); ch = next((c for c in chain["chapters"] if c["op"] == a.op), None)
if ch is None or not ch.get("movers"): print(a.op, "no movers"); sys.exit(0)
preds_map = asm.get("predecessors", {}); op_members = {o2["op"]: {m2["occ"] for m2 in o2.get("movers", [])} for o2 in asm.get("operations", [])}
for it in asm.get("not_animated", []): op_members.setdefault(it["op"], set()).update(it.get("occs", []))
acc, stack = set(), [a.op]
while stack:
    cur = stack.pop()
    for p2 in preds_map.get(cur, []):
        if p2 not in acc: acc.add(p2); stack.append(p2)
visible = set()
for p2 in acc: visible |= op_members.get(p2, set())
visible |= op_members.get(a.op, set()); visible |= set(ch.get("context", []))
own_static = set()
for it in asm.get("not_animated", []):
    if it["op"] == a.op: own_static.update(it.get("occs", []))
visible -= own_static
stem_by = {v.get("name"): k for k, v in man.items() if isinstance(v, dict) and "error" not in v}
cache = {}
def wmesh(oid):
    o = id2o.get(oid); st = stem_by.get(o["product"]) if o else None
    if st is None: return None
    if st not in cache: cache[st] = trimesh.load(MESH / f"{st}.stl", force="mesh")
    m = cache[st].copy(); m.apply_transform(np.array(o["world"], dtype=float)); return m
def sample_pts(m, n):
    if len(m.faces) == 0: return np.zeros((0, 3))
    pts, _ = trimesh.sample.sample_surface(m, n); return pts
vis_m = {mid for e in visible for mid in members_of(e)}
scene = {}
for mid in vis_m:
    mm = wmesh(mid)
    if mm is not None: scene[mid] = mm
rows = []
order = [m["occ"] for m in ch["movers"]]
for m in ch["movers"]:
    mem = members_of(m["occ"]); ax = np.array(m["insertion_axis_world"], float); sense = float(m["withdraw_sense"]); app = float(m["approach_mm"])
    parts = [wmesh(x) for x in mem]; parts = [p for p in parts if p is not None]
    if not parts: continue
    mover = trimesh.util.concatenate(parts) if len(parts) > 1 else parts[0]
    mpts0 = sample_pts(mover, a.samples)
    # 同工序先装兄弟在场，后装兄弟不在场（成片口序）
    later = set(order[order.index(m["occ"]) + 1:]); later_m = {mid for e in later for mid in members_of(e)}
    own = set(mem); worst = {}; seatf = {}; bad_at = {}
    for t in (0.0, 0.25, 0.5, 0.75, 1.0):
        off = ax * sense * app * t; mp = mpts0 + off; mb = mover.bounds + off
        for oid, other in scene.items():
            if oid in own or oid in later_m: continue
            ob = other.bounds
            if np.any(ob[1] < mb[0] - 1) or np.any(ob[0] > mb[1] + 1): continue
            w1 = br._winding(other, mp); f1 = float(np.mean(np.abs(w1) > 0.5))
            opts = sample_pts(other, a.samples); mv2 = mover.copy(); mv2.apply_translation(off)
            w2 = br._winding(mv2, opts); f2 = float(np.mean(np.abs(w2) > 0.5)) if len(opts) else 0.0
            f = max(f1, f2)
            if t == 0.0:
                seatf[oid] = round(f, 4)          # 就位态含入本底（CAD 态/绑定态重叠）
            if f >= a.frac:
                cur = worst.get(oid)
                if cur is None or f > cur["frac"]: worst[oid] = {"frac": round(f, 4), "t": t, "mover_in_other": round(f1, 4), "other_in_mover": round(f2, 4)}
            # 干净前缀（K-PATH-18 回灌用）：沿路含入首次超出就位本底+裕度之前的最后一个采样距离
            sf0 = seatf.get(oid, 0.0)
            if t > 0.0 and oid not in bad_at and (f - sf0) >= a.path_margin:
                bad_at[oid] = t
    for oid, w in worst.items():
        sf = seatf.get(oid, 0.0)
        # 路径穿模 = 沿路含入量**超出就位本底**（就位态本已重叠的 CAD/绑定问题归 SEATED，
        # 否则 t=1 处仅比就位多 1–2% 的接触噪声会被当成路径穿模——v004 首轮扫描 71 条 PATH 里多数即此）
        klass = "PATH_OVERLAP" if (w["frac"] - sf) >= a.path_margin else "SEATED_OVERLAP"
        clean_prefix_mm = None
        if klass == "PATH_OVERLAP":
            tb = bad_at.get(oid, 0.25)
            clean_prefix_mm = round(max(0.0, (tb - 0.25)) * app, 3)   # 采样 t 步长 0.25：首个违规采样之前的采样点
        rows.append({"occ": m["occ"], "product": m.get("product", "")[:40], "other": oid, "other_product": id2o[oid]["product"][:40],
                     "max_frac": w["frac"], "seat_frac": sf, "t_at_max": w["t"], "mover_in_other": w["mover_in_other"], "other_in_mover": w["other_in_mover"],
                     "class": klass, "clean_prefix_mm": clean_prefix_mm, "approach_mm": app})
out = {"op": a.op, "run": a.run, "frac_threshold": a.frac, "visible_parts": len(scene), "movers": len(ch["movers"]),
       "overlaps": rows, "summary": {"pairs": len(rows), "SEATED_OVERLAP": sum(1 for r in rows if r["class"] == "SEATED_OVERLAP"), "PATH_OVERLAP": sum(1 for r in rows if r["class"] == "PATH_OVERLAP")}}
OUTD = ROOT / "本体/S7-穿模审计"; OUTD.mkdir(exist_ok=True)
(OUTD / f"{a.op}.{a.run}.visual.json").write_text(json.dumps(out, ensure_ascii=False, indent=1), encoding="utf-8")
print(a.op, out["summary"])
