#!/usr/bin/env python3
"""验证 D/B 类动件的轴选取启发式（参数化移植版，判据与 X1 validate_axis_choice.py 一致）。

独立量：沿该轴能走出多长的「只有基线接触」的连续段（PathChecker 区域嫌疑+射线含入）。
步长必须与穿模审计一致（0.25，K-PATH-01：粗步长会跨过窄干涉带）。

用法：python3 validate_axis_choice_generic.py --case-dir <robot dir> --op <工序>
"""
import argparse
import json
import re
import struct
import time
import unicodedata
from pathlib import Path

import numpy as np
import trimesh

import sys
sys.path.insert(0, str(Path(__file__).resolve().parent))
from baseline_region import PathChecker

CODE = re.compile(r"^[0-9A-Z]{6}_")
CFG = re.compile(r"^(?:M|ST|Ø|φ)?\s*\d+(?:\.\d+)?\s*(?:[×xX*]\s*\d+(?:\.\d+)?)?\s*$")


def nrm(x):
    return unicodedata.normalize("NFKC", str(x or "").strip()).replace("╱", "/")


def load_stl(p):
    with open(p, "rb") as f:
        f.read(80)
        n = struct.unpack("<I", f.read(4))[0]
        raw = np.frombuffer(f.read(n * 50), dtype=np.uint8).reshape(n, 50)
    v = np.frombuffer(raw[:, 12:48].tobytes(), dtype="<f4").reshape(n * 3, 3).astype(np.float64)
    return trimesh.Trimesh(vertices=v, faces=np.arange(n * 3).reshape(n, 3), process=False)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--case-dir", required=True, type=Path)
    ap.add_argument("--op", required=True)
    ap.add_argument("--step", type=float, default=0.25)
    ap.add_argument("--max", type=float, default=25.0)
    ap.add_argument("--dilate", type=float, default=2.0)
    a = ap.parse_args()
    t0 = time.time()

    cfg = json.loads((a.case_dir / "robot-config.v1.json").read_text(encoding="utf-8"))
    P = cfg["paths"]
    MESH = a.case_dir / P["mesh_dir"]
    OUTD = a.case_dir / "本体/S7-轴选取验证"
    OUTD.mkdir(parents=True, exist_ok=True)
    man = json.loads((MESH / "manifest.json").read_text(encoding="utf-8"))["parts"]
    occ = json.loads((a.case_dir / P["s1_manifest"]).read_text(encoding="utf-8"))["occurrences"]
    graph = json.loads((a.case_dir / P["s2_coax_graph"]).read_text(encoding="utf-8"))
    s7 = json.loads((a.case_dir / "本体/S7-扫掠认证.v2.json").read_text(encoding="utf-8"))
    s7op = next((o for o in s7["operations"] if o["op"] == a.op), None)
    if s7op is None:
        return 0
    dmov = [m for m in s7op["per_mover"] if m.get("klass") in ("D_多轴候选", "B_轴向插装")]
    if not dmov:
        (OUTD / f"{a.op}.json").write_text(json.dumps({"op": a.op, "rows": []}, ensure_ascii=False),
                                           encoding="utf-8")
        print(f"{a.op}: 无 D/B 类动件")
        return 0

    lut = {}
    for stem, v in man.items():
        if "error" in v and "sha256" not in v:
            continue
        nm = nrm(v.get("name") or stem)
        for k in (nm, CODE.sub("", nm)):
            lut.setdefault(k, stem)

    def stem_for(prod):
        s0 = nrm(prod)
        cands = [s0, CODE.sub("", s0)]
        if "_" in s0:
            b, suf = s0.rsplit("_", 1)
            if CFG.match(suf):
                cands += [b, CODE.sub("", b)]
        for k in cands:
            if k in lut:
                return lut[k]
        return None

    id2i = {o["occ_id"]: i for i, o in enumerate(occ)}
    # 实体支持：折叠单元 = 成员烘焙合并网格；轴候选取成员对外界面（叶实体行为不变）
    bind = json.loads((a.case_dir / P["s3_binding"]).read_text(encoding="utf-8"))
    ent_members = {}
    for op0 in bind["operations"]:
        for m0 in op0["movers"]:
            ent_members[m0["occ_id"]] = m0.get("member_occ_ids") or [m0["occ_id"]]

    def mem_idx(eid):
        return [id2i[x] for x in ent_members.get(eid, [eid]) if x in id2i]

    exclude = {i for m in s7op["per_mover"] for i in mem_idx(m["occ"])}
    cache, world = {}, {}
    for i, o in enumerate(occ):
        st = stem_for(o["product"])
        if st is None:
            continue
        if st not in cache:
            cache[st] = load_stl(MESH / f"{st}.stl")
        W = np.array(o["world"], dtype=np.float64)
        T = np.eye(4); T[:3, :3] = W[:3, :3]; T[:3, 3] = W[:3, 3]
        world[i] = (st, T)
    mgr = trimesh.collision.CollisionManager()
    mgr_meshes, mgr_T = {}, {}
    for i, (st, T) in world.items():
        if i in exclude:
            continue
        mgr.add_object(f"o{i}", cache[st], T)
        mgr_meshes[f"o{i}"] = cache[st]
        mgr_T[f"o{i}"] = T
    checker = PathChecker(mgr, mgr_meshes, mgr_T, dilate=a.dilate)

    axes_by_pair, mates = {}, {}
    for p in graph["occurrence_pairs"]:
        axes_by_pair[(p["a_occ"], p["b_occ"])] = p["axes"]
        axes_by_pair[(p["b_occ"], p["a_occ"])] = p["axes"]
        mates.setdefault(p["a_occ"], set()).add(p["b_occ"])
        mates.setdefault(p["b_occ"], set()).add(p["a_occ"])

    rows = []
    for m in dmov:
        midx = [i for i in mem_idx(m["occ"]) if i in world]
        if not midx:
            continue
        memset = set(midx)
        if len(midx) == 1:
            st, T = world[midx[0]]
            mesh = cache[st]
        else:
            mesh = trimesh.util.concatenate(
                [cache[world[j0][0]].copy().apply_transform(world[j0][1]) for j0 in midx])
            T = np.eye(4)
        axset = {}
        for mi in memset:
            for j in mates.get(mi, ()):
                if j in exclude or j in memset:
                    continue
                for ax in axes_by_pair[(mi, j)]:
                    k = tuple(round(x, 3) for x in ax["dir"]) + tuple(round(x, 1) for x in ax["foot"])
                    cur = axset.setdefault(k, dict(ax))
                    cur["faces"] = max(cur.get("faces", 0), ax["faces"])

        checker.prepare(mesh, T)

        cands = []
        for k, ax in axset.items():
            axis = np.array(ax["dir"], dtype=np.float64)
            axis /= np.linalg.norm(axis)
            for sense in (1, -1):
                t, run = 0.0, 0.0
                while t <= a.max + 1e-9:
                    is_bad, _kind, _names = checker.bad_at(axis, sense, t)
                    if is_bad:
                        break
                    run = t
                    t += a.step
                cands.append({"faces": ax["faces"], "dir": [round(float(x), 6) for x in axis],
                              "sense": sense, "clear_run_mm": round(run, 3)})
        if not cands:
            rows.append({"occ": m["occ"], "product": m["product"], "axis_candidates": 0,
                         "heuristic_faces": None, "heuristic_clear_run_mm": None,
                         "best_clear_run_mm": None, "best_faces": None,
                         "heuristic_is_best": False, "gain_mm": None, "candidates": []})
            continue
        best = max(cands, key=lambda c: c["clear_run_mm"])
        chosen_dir = [round(float(x), 6) for x in
                      (np.array(m["insertion_axis_world"]) / np.linalg.norm(m["insertion_axis_world"]))]
        cur = next((c for c in cands
                    if c["sense"] == m["withdraw_sense"]
                    and max(abs(c["dir"][i] - chosen_dir[i]) for i in range(3)) < 1e-3), None)
        rows.append({
            "occ": m["occ"], "product": m["product"], "axis_candidates": len(axset),
            "heuristic_faces": (cur or {}).get("faces"),
            "heuristic_clear_run_mm": (cur or {}).get("clear_run_mm"),
            "best_clear_run_mm": best["clear_run_mm"], "best_faces": best["faces"],
            "heuristic_is_best": bool(cur and abs(cur["clear_run_mm"] - best["clear_run_mm"]) < 1e-6),
            "gain_mm": (round(best["clear_run_mm"] - cur["clear_run_mm"], 3) if cur else None),
            "candidates": sorted(cands, key=lambda c: -c["clear_run_mm"])[:6],
        })

    ok = sum(1 for r in rows if r["heuristic_is_best"])
    doc = {"$schema": "assembly-ontology.axis-choice-validation/v2", "op": a.op,
           "criterion": "K-DIR-02 / K-GOV-11：启发式必须验证；独立量取『沿该轴的只有基线接触连续段』",
           "heuristic": "取共享面最多的那条轴",
           "movers": len(rows), "heuristic_is_best": ok,
           "seconds": round(time.time() - t0, 1), "rows": rows}
    (OUTD / f"{a.op}.json").write_text(json.dumps(doc, ensure_ascii=False), encoding="utf-8")
    print(f"{a.op}: D/B 类 {len(rows)}，启发式最优 {ok}  {doc['seconds']}s")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
