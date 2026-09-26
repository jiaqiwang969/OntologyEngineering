#!/usr/bin/env python3
"""单元结合认证（K-SCN 台架语义 / O-25）：台架预装组作为一个刚体整体上机的可行轴与净行程。

手册的 preparing 分组（BENCH-xx）在台架上装成一体，再在结合工序（S3 unit_joins）整体装到机器上。
v009 只把结合章做成"在场汇合"（presence-only），审片/功能线判为台架语义丢失（SAME_BENCH 违反）。
本工具对每个结合工序：在场集 = 结合工序前驱闭包的全部动件成员 − 该单元成员；单元 = 成员网格并集；
沿世界 ±X/±Y/±Z 六个方向做与 sweep_sequence_reverse_generic 相同的逆向行走（PathChecker 含入采样），
记录净行程最长的方向。输出 本体/S7-单元结合认证.v1.json。
用法：walk_unit_joins.py --case-dir . [--cap 400] [--step 0.25 --fine-until 20 --coarse-step 1.0] [--only WB-06]
"""
import argparse, json, re, sys, time
from pathlib import Path
import numpy as np
import trimesh
sys.path.insert(0, str(Path(__file__).resolve().parent))
from baseline_region import PathChecker  # noqa: E402


def load_stl(p):
    m = trimesh.load(str(p), force="mesh", process=False)
    return m


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--case-dir", required=True, type=Path)
    ap.add_argument("--cap", type=float, default=400.0)
    ap.add_argument("--step", type=float, default=0.25)
    ap.add_argument("--fine-until", type=float, default=20.0)
    ap.add_argument("--coarse-step", type=float, default=1.0)
    ap.add_argument("--dilate", type=float, default=2.0)
    ap.add_argument("--only", default=None)
    ap.add_argument("--out", default="本体/S7-单元结合认证.v1.json")
    a = ap.parse_args()
    t0 = time.time()
    C = a.case_dir
    cfg = json.loads((C / "robot-config.v1.json").read_text(encoding="utf-8"))
    P = cfg["paths"]
    MESH = C / P["mesh_dir"]
    man = json.loads((MESH / "manifest.json").read_text(encoding="utf-8"))["parts"]
    occ = json.loads((C / P["s1_manifest"]).read_text(encoding="utf-8"))["occurrences"]
    bind = json.loads((C / "本体/S3-工序实例绑定.v1.json").read_text(encoding="utf-8"))
    uid2id = {o["occ_uid"]: o["occ_id"] for o in occ}
    s8 = json.loads((C / P["s8_graph"]).read_text(encoding="utf-8"))
    adj = {}
    for e in s8["edges"]:
        if e["kind"] in ("COAX", "PLANE", "CONE"):
            adj.setdefault(e["a"], set()).add(e["b"]); adj.setdefault(e["b"], set()).add(e["a"])

    def components(members):
        """按 S8 强接触把单元成员分成连通分量（WB-06 的前/后端框其实是两个互不相连的子单元，合成一体六轴皆卡死）。"""
        left = set(members); comps = []
        while left:
            seed = sorted(left)[0]; comp = {seed}; stack = [seed]
            while stack:
                x = stack.pop()
                for y in adj.get(x, ()):
                    if y in left and y not in comp:
                        comp.add(y); stack.append(y)
            left -= comp; comps.append(sorted(comp))
        comps.sort(key=lambda c: -len(c))
        return comps
    id2i = {o["occ_id"]: i for i, o in enumerate(occ)}

    CODE = re.compile(r"^[0-9A-Z]{6}_")
    CFG = re.compile(r"^[0-9]+$")
    lut = {k: k for k in man}
    lut.update({man[k]["name"].strip(): k for k in man})

    def stem_for(prod):
        s0 = prod.strip()
        cands = [s0, CODE.sub("", s0), s0.replace(" ", "_"), "_" + s0.replace(" ", "_")]
        if "_" in s0:
            b, suf = s0.rsplit("_", 1)
            if CFG.match(suf):
                cands += [b, CODE.sub("", b)]
        for k in cands:
            if k in lut:
                return lut[k]
        return None

    ent_map = {}
    movers_of = {}
    preds = {}
    for o in bind["operations"]:
        preds[o["op"]] = list(o.get("predecessors") or [])
        movers_of[o["op"]] = [m["occ_id"] for m in o["movers"]]
        for m in o["movers"]:
            ent_map[m["occ_id"]] = m.get("member_occ_ids") or [m["occ_id"]]

    def closure(opn, acc):
        for p in preds.get(opn, []):
            if p not in acc:
                acc.add(p)
                closure(p, acc)
        return acc

    cache, world = {}, {}
    for i, o in enumerate(occ):
        st = stem_for(o["product"])
        if st is None or not man.get(st, {}).get("has_solid", True):
            continue
        if st not in cache:
            try:
                cache[st] = load_stl(MESH / f"{st}.stl")
            except Exception:
                continue
        W = np.array(o["world"], dtype=np.float64)
        T = np.eye(4)
        T[:3, :3] = W[:3, :3]
        T[:3, 3] = W[:3, 3]
        world[i] = (st, T)

    rows = []
    AXES = [((1, 0, 0), "+X"), ((-1, 0, 0), "-X"), ((0, 1, 0), "+Y"), ((0, -1, 0), "-Y"), ((0, 0, 1), "+Z"), ((0, 0, -1), "-Z")]
    for o in bind["operations"]:
        if not o.get("unit_joins"):
            continue
        if a.only and o["op"] != a.only:
            continue
        scene_ops = closure(o["op"], set())
        scene_occ = set()
        for p in scene_ops:
            for eid in movers_of.get(p, []):
                scene_occ.update(ent_map.get(eid, [eid]))
        # 同一结合工序里的多个台架组（如 WB-24 的 BENCH-04+05 = 整个 X 轴）视为一个整体单元：
        # 分别认证时另一组已在场会把本组卡死（WB-24 实测 BENCH-04 六轴 < 6 mm → STUCK）
        _units = o["unit_joins"]
        if len(_units) > 1:
            _units = [{"unit": "+".join(u["unit"] for u in _units), "from_ops": [p_ for u in _units for p_ in u.get("from_ops", [])], "members": [m for u in _units for m in u.get("members", [])], "combined_from": [u["unit"] for u in _units]}]
        for u in _units:
            # 成员按台架工序的动件展开（S3 unit_joins.members 里多体定义只给 "…/=self"，会把 36 个体折成 1 个 occ）
            members = set()
            for p_ in u.get("from_ops", []):
                for eid in movers_of.get(p_, []):
                    members.update(ent_map.get(eid, [eid]))
            members = sorted(members) or sorted({uid2id[m] for m in u["members"] if m in uid2id})
            all_members = set(members)
            # 策略：先整体认证；整体 STUCK（净行程 < 40 mm）才按连通分量顺序拆开认证（WB-06 前/后端框）；
            # 分量拆开可能更差（WB-32 两件无强接触却应同走），所以整体可行就不拆。
            comps_whole = [sorted(all_members)]
            comps_split = components(members)
            strategies = [("whole", comps_whole)] + ([("split", comps_split)] if len(comps_split) > 1 else [])
            rows_unit = []
            for strategy, comps in strategies:
              rows_unit = []
              placed_prev = set()
              for ci, comp in enumerate(comps):
                members = comp
                unit_name = u["unit"] if len(comps) == 1 else f"{u['unit']}#{ci+1}"
                member_idx = [id2i[m] for m in members if m in id2i and id2i[m] in world]
                if not member_idx:
                  rows_unit.append({"op": o["op"], "unit": unit_name, "members": members, "verdict": "NO_GEOMETRY"})
                  continue
                memset = set(members)
                mgr = trimesh.collision.CollisionManager()
                meshes, Ts = {}, {}
                for i, (st, T) in world.items():
                  oid = occ[i]["occ_id"]
                  # 在场 = 闭包成员 − 本单元全部成员 + 已认证的前序分量
                  if oid in memset or (oid not in scene_occ and oid not in placed_prev) or (oid in all_members and oid not in placed_prev):
                      continue
                  nm = f"o{i}"
                  mgr.add_object(nm, cache[st], T)
                  meshes[nm], Ts[nm] = cache[st], T
                mesh = trimesh.util.concatenate([cache[world[j][0]].copy().apply_transform(world[j][1]) for j in member_idx])
                checker = PathChecker(mgr, meshes, Ts, dilate=a.dilate)
                checker.prepare(mesh, np.eye(4))

                def run_axis(dirv, sense):
                    axv = np.array(dirv, dtype=np.float64)
                    axv /= np.linalg.norm(axv)
                    t, run = 0.0, 0.0
                    while t <= a.cap + 1e-9:
                        bad, _k2, _n2 = checker.bad_at(axv, sense, t)
                        if bad:
                            break
                        run = t
                        t += a.coarse_step if t >= a.fine_until - 1e-9 else a.step
                    return run
                best = None
                per_axis = {}
                for dirv, name in AXES:
                    # withdraw_sense 语义与 sweep 工具一致：沿 dir 的 sense 方向退出
                    r = run_axis(dirv, 1)
                    per_axis[name] = round(r, 3)
                    if best is None or r > best[2] + 1e-9:
                        best = (dirv, 1, r)
                verdict = "FREE" if best[2] >= a.cap - 1e-9 else "LIMITED" if best[2] >= 20.0 else "STUCK"
                rows_unit.append({"op": o["op"], "unit": unit_name, "from_ops": u["from_ops"], "members": members, "combined_from": u.get("combined_from"), "component": [ci + 1, len(comps)], "strategy": strategy,
                             "member_count": len(members), "scene_parts": len(meshes),
                             "best": {"dir": list(best[0]), "sense": best[1], "clear_run_mm": round(best[2], 3)},
                             "per_axis_mm": per_axis, "verdict": verdict})
                print(f"{o['op']} {unit_name}: members={len(members)} scene={len(meshes)} best={per_axis} -> {verdict} {time.time()-t0:.0f}s", flush=True)
                placed_prev |= memset
              ok = rows_unit and all(r.get("verdict") in ("FREE", "LIMITED") and r.get("best", {}).get("clear_run_mm", 0) >= 40 for r in rows_unit)
              if ok or strategy == strategies[-1][0]:
                  rows.extend(rows_unit)
                  break
              print(f"{o['op']} {u['unit']}: 整体不可行，改按 {len(comps_split)} 个连通分量拆开认证", flush=True)
    out = {"$schema": "assembly-ontology.unit-join-cert/v1", "purpose": "台架预装组整体上机的刚体行走认证（六个世界轴，含入采样 PathChecker，与序列感知行走同判据）",
           "params": {"cap_mm": a.cap, "step_mm": a.step, "fine_until_mm": a.fine_until, "coarse_step_mm": a.coarse_step, "dilate_mm": a.dilate},
           "basis": {"s3": "本体/S3-工序实例绑定.v1.json", "s1": P["s1_manifest"], "mesh_dir": P["mesh_dir"]},
           "rows": rows, "summary": {"units": len(rows), "free": sum(1 for r in rows if r.get("verdict") == "FREE"), "limited": sum(1 for r in rows if r.get("verdict") == "LIMITED"), "stuck": sum(1 for r in rows if r.get("verdict") == "STUCK")},
           "claim_boundary": ["只认证整体平移可行性（六个世界轴），不认证吊装姿态/工装；单元成员取 S3 unit_joins.members。"]}
    (C / a.out).write_text(json.dumps(out, ensure_ascii=False, indent=1), encoding="utf-8")
    print(json.dumps(out["summary"], ensure_ascii=False), f"{time.time()-t0:.0f}s")


if __name__ == "__main__":
    main()
