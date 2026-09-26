#!/usr/bin/env python3
"""S7 扫掠认证（参数化移植版）—— trimesh + python-fcl。

算法与 X1 sweep_certify_fcl.py 一致：
  静止集 = 全部件 − 本工序中排在本件及其之后的动件（其他工序件一律在场，保守）；
  工序内先非紧固件后紧固件、同类按 occ 序；装第 k 件时前 k-1 件已在场。
  轴候选：优先与更早工序已装件（EARLIER_FIXED）的 S2 共轴界面，退 ANY_NON_MOVER。
  方向判定链：GEOMETRY_FORCED → NO_RELEASE → THICK_END → UNDETERMINED_SHORTER_RELEASE。

适配差异（只改数据接口，不改判据）：
  ① 绑定格式为 assembly-ontology.s3-op-binding/v2（op.movers[].occ_id），无 S3b 指派。
  ② 紧固件词表来自 robot-config 正则（英/法文），不再用中文关键词。
  ③ 网格 manifest 为 definition-stl-manifest/v1；无实体定义无代理网格时如实标注。

实体化扩展（Fourier S5 折叠单元）：
  动件实体 = 叶（单成员）或求解单元（member_occ_ids 多成员）。单元网格 = 成员世界位姿
  烘焙合并，刚体整体扫掠；轴候选 = 成员与实体外伙伴的 S2 界面轴并集。叶实体走同一条
  烘焙路径（T=I），与旧逐叶实现几何等价。
  --chunk s:e 只认证实体序 [s,e)（场景仍含 [0,s) 已装成员），供农场分片；输出带 chunk 后缀。

用法：python3 sweep_certify_fcl_generic.py --case-dir <robot dir> --op <工序> [--step 0.25] [--max 60] [--chunk s:e]
"""
import argparse
import json
import math
import re
import struct
import time
import unicodedata
from collections import Counter
from pathlib import Path

import numpy as np
import trimesh

CODE = re.compile(r"^[0-9A-Z]{6}_")
CFG = re.compile(r"^(?:M|ST|Ø|φ)?\s*\d+(?:\.\d+)?\s*(?:[×xX*]\s*\d+(?:\.\d+)?)?\s*$")
THICK_MIN_DIFF = 0.15
END_FRAC = 0.25


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
    ap.add_argument("--max", type=float, default=60.0)
    ap.add_argument("--chunk", default="", help="s:e 实体分片（含前缀场景）")
    a = ap.parse_args()
    t0 = time.time()

    cfg = json.loads((a.case_dir / "robot-config.v1.json").read_text(encoding="utf-8"))
    P = cfg["paths"]
    MESH = a.case_dir / P["mesh_dir"]
    OUTD = a.case_dir / P["s7_runs"]
    OUTD.mkdir(parents=True, exist_ok=True)

    man = json.loads((MESH / "manifest.json").read_text(encoding="utf-8"))["parts"]
    occ = json.loads((a.case_dir / P["s1_manifest"]).read_text(encoding="utf-8"))["occurrences"]
    graph = json.loads((a.case_dir / P["s2_coax_graph"]).read_text(encoding="utf-8"))
    bind = json.loads((a.case_dir / P["s3_binding"]).read_text(encoding="utf-8"))
    FAST = [re.compile(p) for p in cfg["fastener_lexicon"]["patterns"]]

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

    order = [o["op"] for o in bind["operations"]]
    rank = {o: i for i, o in enumerate(order)}
    # 实体表：叶=单成员，solver_unit=member_occ_ids 多成员
    id2i = {o["occ_id"]: i for i, o in enumerate(occ)}
    ents_all = []
    for op0 in bind["operations"]:
        for m in op0["movers"]:
            mem = m.get("member_occ_ids") or [m["occ_id"]]
            ents_all.append({"eid": m["occ_id"], "op": op0["op"], "kind": m.get("kind", "leaf"),
                             "product": m.get("product", ""),
                             "mem_idx": sorted(id2i[x] for x in mem if x in id2i)})

    if a.op not in rank:
        print(f"{a.op}: 不在绑定工序表")
        return 2

    movers = [e for e in ents_all if e["op"] == a.op and e["mem_idx"]]
    if not movers:
        (OUTD / f"{a.op}.json").write_text(json.dumps(
            {"op": a.op, "movers": 0, "per_mover": [], "note": "无叶动件（unit-join 集成工序）"},
            ensure_ascii=False), encoding="utf-8")
        print(f"{a.op}: 无叶动件（unit-join 工序，运动在 S5/S7 单元级另行认证）")
        return 0

    cache, world, no_solid_used = {}, {}, set()
    for i, o in enumerate(occ):
        st = stem_for(o["product"])
        if st is None:
            continue
        if not man[st].get("has_solid", True):
            no_solid_used.add(st)
        if st not in cache:
            cache[st] = load_stl(MESH / f"{st}.stl")
        W = np.array(o["world"], dtype=np.float64)
        T = np.eye(4)
        T[:3, :3] = W[:3, :3]
        T[:3, 3] = W[:3, 3]
        world[i] = (st, T)

    def is_fast_ent(e):
        return e["kind"] == "leaf" and any(p.search(e["product"]) for p in FAST)

    movers = sorted(movers, key=lambda e: (is_fast_ent(e), e["mem_idx"][0]))
    later_idx = {}
    acc = set()
    for e in reversed(movers):
        acc |= set(e["mem_idx"])
        later_idx[e["eid"]] = set(acc)
    op_member_idx = set(acc)

    c0, c1 = 0, len(movers)
    if a.chunk:
        c0, c1 = (int(x) for x in a.chunk.split(":"))

    mgr = trimesh.collision.CollisionManager()
    added = set()
    for i, (use, T) in world.items():
        if i in op_member_idx:
            continue
        mgr.add_object(f"o{i}", cache[use], T)
        added.add(i)
    print(f"静止集（不含本工序动件成员）{len(added)} 件  {time.time()-t0:.0f}s", flush=True)

    def ent_world_mesh(e):
        parts = []
        for i in e["mem_idx"]:
            if i not in world:
                continue
            st, T = world[i]
            parts.append(cache[st].copy().apply_transform(T))
        return trimesh.util.concatenate(parts) if parts else None

    axes_by_pair, mates = {}, {}
    for p in graph["occurrence_pairs"]:
        axes_by_pair[(p["a_occ"], p["b_occ"])] = p["axes"]
        axes_by_pair[(p["b_occ"], p["a_occ"])] = p["axes"]
        mates.setdefault(p["a_occ"], set()).add(p["b_occ"])
        mates.setdefault(p["b_occ"], set()).add(p["a_occ"])
    fixed_earlier = set()
    for e0 in ents_all:
        if rank[e0["op"]] < rank[a.op]:
            fixed_earlier.update(e0["mem_idx"])

    per = []
    for k, e in enumerate(movers):
        if k >= c1:
            break
        for e2 in movers[:k]:
            for j in e2["mem_idx"]:
                if j in world and j not in added:
                    u2, T2 = world[j]
                    mgr.add_object(f"o{j}", cache[u2], T2)
                    added.add(j)
        if k < c0:
            continue
        memset = set(e["mem_idx"])
        exclude = later_idx[e["eid"]]
        mesh = ent_world_mesh(e)
        base = {"occ": e["eid"], "product": e["product"],
                "kind": e["kind"], "member_count": len(e["mem_idx"])}
        if mesh is None:
            per.append({**base, "klass": "NO_MESH", "axis_source": None})
            continue
        T = np.eye(4)
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
            for ax in axs:
                kk = tuple(round(x, 3) for x in ax["dir"]) + tuple(round(x, 1) for x in ax["foot"])
                axset.setdefault(kk, ax)
        if not axset:
            per.append({**base, "klass": "E_无共轴界面", "axis_source": src})
            continue
        klass = "B_轴向插装" if len(axset) == 1 else "D_多轴候选"
        axis = np.array(max(axset.values(), key=lambda x: x["faces"])["dir"], dtype=np.float64)
        axis = axis / np.linalg.norm(axis)

        # 动件 BVH 建一次逐步复用（in_collision_single 每步重建，大合并网格是主开销）
        import fcl as _fcl
        geom = mgr._get_fcl_obj(mesh)
        _req = _fcl.CollisionRequest(num_max_contacts=1, enable_contact=False)

        def hit(t):
            M = T.copy()
            M[:3, 3] = T[:3, 3] + axis * t
            o1 = _fcl.CollisionObject(geom, _fcl.Transform(M[:3, :3], M[:3, 3]))
            cd = _fcl.CollisionData(request=_req)
            mgr._manager.collide(o1, cd, _fcl.defaultCollisionCallback)
            return cd.result.is_collision

        rel = {}
        for sense in (1, -1):
            t, rel_at, last = 0.0, None, None
            while t <= a.max + 1e-9:
                c = hit(sense * t)
                if not c and rel_at is None:
                    rel_at = t
                if rel_at is not None:
                    if not c:
                        last = t
                    else:
                        break
                t += a.step
            rel[sense] = {"release_mm": rel_at, "certified_approach_mm": last}

        V = (mesh.vertices @ T[:3, :3].T) + T[:3, 3]
        ts = V @ axis
        lo, hi = ts.min(), ts.max()
        span = hi - lo
        ctr = V.mean(axis=0)
        D = V - ctr
        rad = np.linalg.norm(D - np.outer(D @ axis, axis), axis=1)
        rlo = float(rad[ts <= lo + END_FRAC * span].max()) if span > 1e-6 else 0.0
        rhi = float(rad[ts >= hi - END_FRAC * span].max()) if span > 1e-6 else 0.0

        if rel[1]["release_mm"] is not None and rel[-1]["release_mm"] is None:
            sense, rule = 1, "GEOMETRY_FORCED"
        elif rel[-1]["release_mm"] is not None and rel[1]["release_mm"] is None:
            sense, rule = -1, "GEOMETRY_FORCED"
        elif rel[1]["release_mm"] is None and rel[-1]["release_mm"] is None:
            sense, rule = None, "NO_RELEASE"
        elif abs(rhi - rlo) >= THICK_MIN_DIFF:
            sense, rule = (1 if rhi > rlo else -1), "THICK_END"
        else:
            sense, rule = (1 if rel[1]["release_mm"] <= rel[-1]["release_mm"] else -1), \
                          "UNDETERMINED_SHORTER_RELEASE"
        per.append({**base,
                    "klass": klass, "axis_source": src, "axis_candidates": len(axset),
                    "insertion_axis_world": [round(float(x), 9) for x in axis],
                    "withdraw_sense": sense, "direction_rule": rule,
                    "end_radius_lo_mm": round(rlo, 4), "end_radius_hi_mm": round(rhi, 4),
                    "release_mm_by_sense": {"+1": rel[1]["release_mm"], "-1": rel[-1]["release_mm"]},
                    "certified_approach_mm": (rel[sense]["certified_approach_mm"] if sense else None),
                    "verdict": ("RELEASES" if sense else "NO_RELEASE")})

    doc = {"$schema": "assembly-ontology.s7-sweep-certification-op/v2", "op": a.op,
           "backend": "trimesh 5.0.0 + python-fcl 0.7.0.11",
           "params": {"step_mm": a.step, "max_travel_mm": a.max,
                      "thick_end_min_radius_diff_mm": THICK_MIN_DIFF,
                      "scene": "静止集 = 全部件 − 本工序中排在本件及其之后的动件；同工序按安装顺序"},
           "movers": len(movers), "entity_kinds": dict(Counter(e["kind"] for e in movers)),
           "chunk": [c0, c1] if a.chunk else None,
           "no_solid_defs_in_scene": sorted(no_solid_used),
           "intra_op_order": "先非紧固件后紧固件、同类按成员 occ 序（词表见 robot-config）；实体=叶或折叠单元",
           "released": sum(1 for x in per if x.get("verdict") == "RELEASES"),
           "seconds": round(time.time() - t0, 1), "per_mover": per}
    suffix = f".chunk{c0}-{c1}" if a.chunk else ""
    (OUTD / f"{a.op}{suffix}.json").write_text(json.dumps(doc, ensure_ascii=False), encoding="utf-8")
    print(f"{a.op}{suffix}: 实体 {len(per)}/{len(movers)} 脱离 {doc['released']}  {doc['seconds']}s")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
