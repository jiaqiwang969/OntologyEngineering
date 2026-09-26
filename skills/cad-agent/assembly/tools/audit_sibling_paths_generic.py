#!/usr/bin/env python3
"""兄弟件路径审计：动件路径 vs 同工序**先装**的动件（成片视觉口径）。

S7 认证与穿模审计的排除集都是『本工序的全部动件』（F-GOV-07 钉在工艺口径）——
那回答的是"与机上其余一切相容吗"。但成片里同工序的件按链序**逐个**就位：
后飞入的件路径若扫过已就位的兄弟件，前两道审计都结构性看不见，画面却穿模。

本审计按 state-chain 的播放顺序，对每个动件 k 建"先装兄弟件"场景（j<k 就位姿态），
用与主审计同一把量具（baseline_region.PathChecker：t=0 接触=配合基线，
新区接触由射线含入仲裁）。同时报告 k 就位时在兄弟件材料内部的采样点数
（SEATED_INSIDE>本底 = 位姿/绑定级重叠，不是运动问题）。

用法：python3 tools/audit_sibling_paths.py --op <工序> --run FILM_v008 [--step 0.25]
输出：本体/S7-穿模审计/<op>.<run>.siblings.json
"""
import argparse
import json
import os
import re
import struct
import time
import unicodedata
from pathlib import Path

import numpy as np
import trimesh

import sys as _sys
_sys.path.insert(0, str(Path(__file__).resolve().parent))
from baseline_region import PathChecker

import json as _json
_case = Path(os.environ["REBUILD_CASE_DIR"]).resolve()
_cfg = _json.loads((_case / "robot-config.v1.json").read_text(encoding="utf-8"))
ROOT = _case
MESH = _case / _cfg["paths"]["mesh_dir"]
OCC = _case / _cfg["paths"]["s1_manifest"]
OUTD = _case / "本体/S7-穿模审计"
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


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--op", required=True)
    ap.add_argument("--step", type=float, default=0.25)
    ap.add_argument("--dilate", type=float, default=2.0)
    ap.add_argument("--run", default="FILM_v008")
    a = ap.parse_args()
    t0 = time.time()
    OUTD.mkdir(parents=True, exist_ok=True)

    man = json.loads((MESH / "manifest.json").read_text(encoding="utf-8"))["parts"]
    occ = json.loads(OCC.read_text(encoding="utf-8"))["occurrences"]
    chain = json.loads((ROOT / "装配动画" / a.run / "state-chain.json").read_text(encoding="utf-8"))

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

    ch = next((c for c in chain["chapters"] if c["op"] == a.op), None)
    rows = []
    if ch is None or len(ch["movers"]) < 2:
        doc = {"$schema": "x1.sibling-path-audit/v1", "op": a.op, "run": a.run,
               "criterion": "成片视觉口径：动件路径 vs 同工序先装兄弟件（就位姿态）。",
               "step_mm": a.step, "dilate_mm": a.dilate,
               "movers": 0 if ch is None else len(ch["movers"]), "violations": 0,
               "seconds": round(time.time() - t0, 1), "rows": []}
        (OUTD / f"{a.op}.{a.run}.siblings.json").write_text(json.dumps(doc, ensure_ascii=False),
                                                            encoding="utf-8")
        print(f"{a.op}: 动件不足 2，无兄弟关系")
        return 0

    byid = {o["occ_id"]: o for o in occ}
    cache = {}

    # S5 折叠单元：实体 = 成员烘焙合并网格（与 S7/构建器/穿模审计同口径）
    bindp = json.loads((_case / json.loads((_case / "robot-config.v1.json").read_text())
                        ["paths"]["s3_binding"]).read_text(encoding="utf-8"))
    ent_members = {}
    for _op0 in bindp["operations"]:
        for _m0 in _op0["movers"]:
            ent_members[_m0["occ_id"]] = _m0.get("member_occ_ids") or [_m0["occ_id"]]

    def _leaf_mesh_T(occ_id):
        o = byid[occ_id]
        st = stem_for(o["product"])
        if st is None:
            return None, None
        if st not in cache:
            cache[st] = load_stl(MESH / f"{st}.stl")
        W = np.array(o["world"], dtype=np.float64)
        T = np.eye(4)
        T[:3, :3] = W[:3, :3]
        T[:3, 3] = W[:3, 3]
        return cache[st], T

    def occ_mesh_T(occ_id):
        mem = ent_members.get(occ_id, [occ_id])
        if len(mem) == 1 and mem[0] in byid:
            return _leaf_mesh_T(mem[0])
        parts = []
        for mid in mem:
            if mid not in byid:
                continue
            m0, T0 = _leaf_mesh_T(mid)
            if m0 is not None:
                parts.append(m0.copy().apply_transform(T0))
        if not parts:
            return None, None
        return trimesh.util.concatenate(parts), np.eye(4)

    def prod_of(m):
        return (m.get("product") or byid.get(m["occ"], {}).get("product") or m["occ"])[:40]

    movers = ch["movers"]
    nviol = 0
    for k in range(1, len(movers)):
        m = movers[k]
        mesh, T = occ_mesh_T(m["occ"])
        if mesh is None:
            continue
        # 先装兄弟件场景（j<k，就位姿态）；耦合对共动，互检由 pair_closure 承担
        mgr = trimesh.collision.CollisionManager()
        meshes, Ts = {}, {}
        for j in range(k):
            if m.get("pair_id") and movers[j].get("pair_id") == m["pair_id"]:
                continue
            sm, sT = occ_mesh_T(movers[j]["occ"])
            if sm is None:
                continue
            nmj = f"s{j}"
            mgr.add_object(nmj, sm, sT)
            meshes[nmj], Ts[nmj] = sm, sT
        if not meshes:
            continue
        checker = PathChecker(mgr, meshes, Ts, dilate=a.dilate)
        checker.prepare(mesh, T)
        # 就位互穿要对**每个**先装兄弟查（全含时面接触为零，只查接触伙伴会漏）。
        # 阈值取 25% 采样点（与 order_within_op 同口径）：螺纹/压装不建模的
        # 本底含入约 10–15%（M4 在攻丝孔实测 32/260），不是位姿错误。
        zax = np.array([0.0, 0.0, 1.0])
        seated_inside = {}
        for nmj in meshes:
            v = checker._inside_count(nmj, 0.0, zax, 1)
            checker._inside0[nmj] = v
            if v > 0.25 * len(checker._local_pts):
                seated_inside[nmj] = v

        axis = np.array(m["insertion_axis_world"], dtype=np.float64)
        axis /= np.linalg.norm(axis)
        sense = int(m["withdraw_sense"])
        approach = float(m["approach_mm"])
        first_bad, first_kind, worst = None, None, set()
        t = 0.0
        while t <= approach + 1e-9:
            is_bad, kind, names_bad = checker.bad_at(axis, sense, t)
            if is_bad:
                worst |= names_bad
                if first_bad is None:
                    first_bad, first_kind = round(t, 3), kind
            t += a.step
        row = {
            "occ": m["occ"], "product": prod_of(m),
            "film_order": k, "approach_mm": approach,
            "first_bad_at_mm": first_bad, "first_bad_kind": first_kind,
            "bad_siblings": sorted(prod_of(movers[int(n[1:])])[:36] for n in worst),
            "bad_sibling_occ": sorted(movers[int(n[1:])]["occ"] for n in worst),
            # 就位含入是**数据质量报告**，不是成片缺陷：就位姿态来自 S1 CAD 装配态，
            # 静止无运动；小紧固件穿薄板通孔的合法配合占位可达自身采样的 25%+。
            # verdict 只判运动路径。
            "seated_inside_siblings": {movers[int(n[1:])]["occ"]: v for n, v in seated_inside.items()},
            "verdict": "CLEAR" if first_bad is None else "VIOLATES",
        }
        if row["verdict"] != "CLEAR":
            nviol += 1
        rows.append(row)

    doc = {"$schema": "x1.sibling-path-audit/v1", "op": a.op, "run": a.run,
           "criterion": "成片视觉口径：动件路径 vs 同工序先装兄弟件（就位姿态）。t=0 接触=配合基线；"
                        "新区接触由射线含入仲裁；就位态材料含入>本底=位姿/绑定级重叠",
           "step_mm": a.step, "dilate_mm": a.dilate,
           "movers": len(movers), "violations": nviol,
           "seconds": round(time.time() - t0, 1), "rows": rows}
    (OUTD / f"{a.op}.{a.run}.siblings.json").write_text(json.dumps(doc, ensure_ascii=False),
                                                        encoding="utf-8")
    print(f"{a.op}: 动件 {len(movers)} 兄弟违规 {nviol}  {doc['seconds']}s")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
