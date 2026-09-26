#!/usr/bin/env python3
"""S7 扫掠认证 —— Blender BVHTree 独立第二实现（K-GOV-02 交叉验证，参数化移植版）。

判据与 sweep_certify_fcl_generic.py 完全一致，代码路径全然不同：
零容差三角形重叠 + 全局 BVH 树 + exclude 集过滤，vs trimesh+python-fcl 增量管理器。
**场景口径与 FCL 版逐字对齐**：静止集 = 全部件 − 本工序中排在本件及其之后的动件
（工序内先非紧固件后紧固件、同类按 occ 序）。这是与 X1 原 BVH 版的唯一差别——
X1 v1 用旧口径（除本工序全部动件），与 FCL 版口径不一致，交叉验证有系统性盲区。

用法：blender -b --factory-startup -P sweep_certify_bvh_generic.py -- --case-dir <dir> [--ops a,b] [--step 0.25] [--max 60]
输出：<case>/本体/S7-扫掠认证-bvh.v1.json
"""
import json
import math
import re
import struct
import sys
import time
import unicodedata
from pathlib import Path

import mathutils
from mathutils.bvhtree import BVHTree

argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []


def arg(name, default):
    return argv[argv.index(name) + 1] if name in argv else default


CASE = Path(arg("--case-dir", "")).resolve()
cfg = json.loads((CASE / "robot-config.v1.json").read_text(encoding="utf-8"))
P = cfg["paths"]
MESH = CASE / P["mesh_dir"]
OCC = CASE / P["s1_manifest"]
GRAPH = CASE / P["s2_coax_graph"]
BIND = CASE / P["s3_binding"]
OUT = CASE / ("本体/S7-扫掠认证-bvh" + arg("--out-suffix", "") + ".v1.json")  # OLSK 增量：--out-suffix 供分片并行，merge_s7_bvh_parts 合并

STEP_MM = float(arg("--step", 0.25))
MAX_MM = float(arg("--max", 60.0))
ONLY = [x for x in arg("--ops", "").split(",") if x]
THICK_MIN_DIFF = 0.15
END_FRAC = 0.25


def log(m):
    print(f"[s7-bvh] {m}", flush=True)


def read_stl(p):
    with open(p, "rb") as f:
        f.read(80)
        n = struct.unpack("<I", f.read(4))[0]
        buf = f.read(n * 50)
    tris = []
    for i in range(n):
        o = i * 50 + 12
        t = struct.unpack_from("<9f", buf, o)
        tris.append((t[0:3], t[3:6], t[6:9]))
    return tris


def main():
    t0 = time.time()
    man = json.loads((MESH / "manifest.json").read_text(encoding="utf-8"))["parts"]
    occ = json.loads(OCC.read_text(encoding="utf-8"))["occurrences"]
    graph = json.loads(GRAPH.read_text(encoding="utf-8"))
    bind = json.loads(BIND.read_text(encoding="utf-8"))
    FAST = [re.compile(p) for p in cfg["fastener_lexicon"]["patterns"]]

    CODE = re.compile(r"^[0-9A-Z]{6}_")

    def nrm(x):
        return unicodedata.normalize("NFKC", str(x or "").strip()).replace("╱", "/")

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
            if re.match(r"^(?:M|ST|Ø|φ)?\s*\d+(?:\.\d+)?\s*(?:[×xX*]\s*\d+(?:\.\d+)?)?\s*$", suf):
                cands += [b, CODE.sub("", b)]
        for k in cands:
            if k in lut:
                return lut[k]
        return None

    order = [op["op"] for op in bind["operations"]]
    rank = {o: i for i, o in enumerate(order)}
    id2i0 = {o["occ_id"]: i for i, o in enumerate(occ)}
    # 实体表（与 FCL 版同构）：叶=单成员；solver_unit=成员并集，刚体整体扫掠
    ents_all = []
    for op0 in bind["operations"]:
        for m in op0["movers"]:
            mem = m.get("member_occ_ids") or [m["occ_id"]]
            ents_all.append({"eid": m["occ_id"], "op": op0["op"], "kind": m.get("kind", "leaf"),
                             "product": m.get("product", ""),
                             "mem_idx": sorted(id2i0[x] for x in mem if x in id2i0)})

    log("载入几何…")
    parts_cache = {}
    tri_all, tri_owner = [], []
    occ_tris = {}
    no_solid_used = []
    for i, o in enumerate(occ):
        st = stem_for(o["product"])
        if st is None:
            continue
        if not man[st].get("has_solid", True) and st not in no_solid_used:
            no_solid_used.append(st)
        if st not in parts_cache:
            parts_cache[st] = read_stl(MESH / f"{st}.stl")
        W = o["world"]
        R = [[W[a][b] for b in range(3)] for a in range(3)]
        T = [W[a][3] for a in range(3)]
        loc = []
        for tri in parts_cache[st]:
            wt = tuple(tuple(sum(R[a][k] * p[k] for k in range(3)) + T[a] for a in range(3))
                       for p in tri)
            loc.append(wt)
        occ_tris[i] = (len(tri_all), len(loc))
        tri_all.extend(loc)
        tri_owner.extend([i] * len(loc))
    log(f"三角形 {len(tri_all)}  无实体件 {len(no_solid_used)}  {time.time()-t0:.0f}s")

    verts, polys = [], []
    for a, b, c in tri_all:
        n = len(verts)
        verts += [a, b, c]
        polys.append((n, n + 1, n + 2))
    tree = BVHTree.FromPolygons(verts, polys, all_triangles=True, epsilon=0.0)
    log(f"全局 BVH 建好  {time.time()-t0:.0f}s")

    axes_by_pair, mates = {}, {}
    for p in graph["occurrence_pairs"]:
        axes_by_pair[(p["a_occ"], p["b_occ"])] = p["axes"]
        axes_by_pair[(p["b_occ"], p["a_occ"])] = p["axes"]
        mates.setdefault(p["a_occ"], set()).add(p["b_occ"])
        mates.setdefault(p["b_occ"], set()).add(p["a_occ"])

    by_op = {}
    for e in ents_all:
        if e["mem_idx"]:
            by_op.setdefault(e["op"], []).append(e)

    ops = [o for o in order if by_op.get(o)]
    if ONLY:
        ops = [o for o in ops if o in ONLY]
    log(f"待认证工序 {len(ops)}")

    def is_fast_ent(e):
        return e["kind"] == "leaf" and any(p.search(e["product"]) for p in FAST)

    def contacts(midx, off, exclude):
        vs, ps = [], []
        for mi in midx:
            s, n = occ_tris[mi]
            for k in range(n):
                a, b, c = tri_all[s + k]
                base = len(vs)
                vs += [(a[0] + off[0], a[1] + off[1], a[2] + off[2]),
                       (b[0] + off[0], b[1] + off[1], b[2] + off[2]),
                       (c[0] + off[0], c[1] + off[1], c[2] + off[2])]
                ps.append((base, base + 1, base + 2))
        mt = BVHTree.FromPolygons(vs, ps, all_triangles=True, epsilon=0.0)
        hits = {}
        for _, sj in mt.overlap(tree):
            ow = tri_owner[sj]
            if ow in exclude:
                continue
            hits[ow] = hits.get(ow, 0) + 1
        return hits

    def end_radii(midx, axis):
        ax = mathutils.Vector(axis).normalized()
        ts, pts = [], []
        for mi in midx:
            s, n = occ_tris[mi]
            for k in range(n):
                for p in tri_all[s + k]:
                    v = mathutils.Vector(p)
                    ts.append(v.dot(ax))
                    pts.append(v)
        lo, hi = min(ts), max(ts)
        span = hi - lo
        if span < 1e-6:
            return 0.0, 0.0
        c = sum(pts, mathutils.Vector()) / len(pts)

        def rmax(sel):
            r = 0.0
            for v, t in zip(pts, ts):
                if sel(t):
                    d = (v - c) - ax * ((v - c).dot(ax))
                    r = max(r, d.length)
            return r
        return rmax(lambda t: t <= lo + END_FRAC * span), rmax(lambda t: t >= hi - END_FRAC * span)

    results = []
    for opname in ops:
        movers = sorted(by_op[opname], key=lambda e: (is_fast_ent(e), e["mem_idx"][0]))
        later_idx, accx = {}, set()
        for e in reversed(movers):
            accx |= set(e["mem_idx"])
            later_idx[e["eid"]] = set(accx)
        fixed_earlier = set()
        for e0 in ents_all:
            if rank[e0["op"]] < rank[opname]:
                fixed_earlier.update(e0["mem_idx"])
        per = []
        for e in movers:
            exclude = later_idx[e["eid"]]      # 与 FCL 版同口径：先装兄弟在场
            midx = [i for i in e["mem_idx"] if i in occ_tris]
            memset = set(e["mem_idx"])
            base = {"occ": e["eid"], "product": e["product"],
                    "kind": e["kind"], "member_count": len(e["mem_idx"])}
            if not midx:
                per.append({**base, "klass": "NO_MESH", "axis_source": None})
                continue
            cand, src = [], "EARLIER_FIXED"
            for i0 in memset:
                for j in mates.get(i0, ()):
                    if j not in memset and j in fixed_earlier:
                        cand.append((j, axes_by_pair[(i0, j)]))
            if not cand:
                src = "ANY_NON_MOVER"
                for i0 in memset:
                    for j in mates.get(i0, ()):
                        if j not in memset and j not in exclude:
                            cand.append((j, axes_by_pair[(i0, j)]))
            axset = {}
            for j, axs in cand:
                for a in axs:
                    k = tuple(round(x, 3) for x in a["dir"]) + tuple(round(x, 1) for x in a["foot"])
                    axset.setdefault(k, a)
            if not axset:
                per.append({**base, "klass": "E_无共轴界面", "axis_source": src})
                continue
            klass = "B_轴向插装" if len(axset) == 1 else "D_多轴候选"
            axis = max(axset.values(), key=lambda a: a["faces"])["dir"]
            ax = mathutils.Vector(axis).normalized()

            rel = {}
            for sense in (1, -1):
                t, released_at, last_clear = 0.0, None, None
                while t <= MAX_MM + 1e-9:
                    off = ax * (sense * t)
                    h = contacts(midx, (off.x, off.y, off.z), exclude | memset)
                    clear = not h
                    if clear and released_at is None:
                        released_at = t
                    if released_at is not None:
                        if clear:
                            last_clear = t
                        else:
                            break
                    t += STEP_MM
                rel[sense] = {"release_mm": released_at, "certified_approach_mm": last_clear}
            rlo, rhi = end_radii(midx, axis)
            if rel[1]["release_mm"] is not None and rel[-1]["release_mm"] is None:
                sense, rule = 1, "GEOMETRY_FORCED"
            elif rel[-1]["release_mm"] is not None and rel[1]["release_mm"] is None:
                sense, rule = -1, "GEOMETRY_FORCED"
            elif rel[1]["release_mm"] is None and rel[-1]["release_mm"] is None:
                sense, rule = None, "NO_RELEASE"
            elif abs(rhi - rlo) >= THICK_MIN_DIFF:
                sense, rule = (1 if rhi > rlo else -1), "THICK_END"
            else:
                a1, a2 = rel[1]["release_mm"], rel[-1]["release_mm"]
                sense, rule = (1 if a1 <= a2 else -1), "UNDETERMINED_SHORTER_RELEASE"
            per.append({
                **base,
                "klass": klass, "axis_source": src, "axis_candidates": len(axset),
                "insertion_axis_world": [round(x, 9) for x in axis],
                "withdraw_sense": sense, "direction_rule": rule,
                "end_radius_lo_mm": round(rlo, 4), "end_radius_hi_mm": round(rhi, 4),
                "release_mm_by_sense": {"+1": rel[1]["release_mm"], "-1": rel[-1]["release_mm"]},
                "certified_approach_mm": (rel[sense]["certified_approach_mm"] if sense else None),
                "verdict": ("RELEASES" if sense else "NO_RELEASE"),
            })
        ok = sum(1 for x in per if x.get("verdict") == "RELEASES")
        results.append({"op": opname, "movers": len(movers), "released": ok, "per_mover": per})
        log(f"{opname:<22} 动件 {len(movers):>3}  脱离 {ok:>3}  {time.time()-t0:.0f}s")

    allm = [m for r in results for m in r["per_mover"]]
    import collections
    doc = {
        "$schema": "assembly-ontology.s7-sweep-certification-bvh/v2",
        "record_id": f"{cfg['case']}-S7-SWEEP-BVH-V001",
        "purpose": "第二独立实现（Blender BVHTree 零容差三角形重叠），供 K-GOV-02 交叉验证。",
        "params": {"step_mm": STEP_MM, "max_travel_mm": MAX_MM,
                   "thick_end_min_radius_diff_mm": THICK_MIN_DIFF, "end_segment_fraction": END_FRAC,
                   "collision": "Blender BVHTree 零容差三角形重叠（全局树 + exclude 集过滤）",
                   "scene": "静止集 = 全部件 − 本工序中排在本件及其之后的动件（与 FCL 版同口径）"},
        "summary": {
            "工序": len(results), "动件": len(allm),
            "脱离": sum(1 for m in allm if m.get("verdict") == "RELEASES"),
            "未脱离": sum(1 for m in allm if m.get("verdict") == "NO_RELEASE"),
            "无共轴界面": sum(1 for m in allm if m.get("klass") == "E_无共轴界面"),
            "分类": dict(collections.Counter(m.get("klass") for m in allm)),
            "方向判据": dict(collections.Counter(m.get("direction_rule") for m in allm)),
            "无实体件": no_solid_used,
            "耗时秒": round(time.time() - t0, 1),
        },
        "claim_boundary": [
            "与 FCL 版共享 STL 与世界位姿，但碰撞判据、加速结构、遍历方式全然不同（K-GOV-02）。",
            "零容差三角形重叠在『恰好接触』处与 FCL 语义不同——恰好贴合 FCL 判碰撞、BVH 不判；"
            "交叉验证的接近距离差异集中于此，属已知语义分歧而非实现错误。",
            "2 个无实体大腿壳用开壳真几何入树；其阻挡结论按保守代理口径另行裁决。",
        ],
        "operations": results,
    }
    OUT.write_text(json.dumps(doc, ensure_ascii=False), encoding="utf-8")
    log(f"写出 {OUT}")
    for k, v in doc["summary"].items():
        log(f"  {k}: {v}")
    return 0


main()
