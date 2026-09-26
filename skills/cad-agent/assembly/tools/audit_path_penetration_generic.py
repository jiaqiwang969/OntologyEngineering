#!/usr/bin/env python3
"""穿模审计：动画真正播放的那一段 [0, 起始偏移] 上，有没有**非基线**接触。

Orbita 的 b_class_insertion_pipeline 每个动件都落盘一个 `non_baseline_contacts_on_path`，
X1 的 S7 漏了这一项——S7 只算了"从哪里开始完全脱离"（release）和"从脱离点起还能清多远"
（certified_approach），**没查过 [0, release) 这一段里都碰到了谁**。

而动画播的恰恰是 [0, 起始偏移] 整段。这一段里：
  · 与**装配态基线**里的件接触 = 件正在滑进自己的配合孔，合法（K-IF-04）
  · 与基线之外的件接触 = **穿模**

判据就是 K-IF-04：CAD 装配态按定义无干涉，t=0 时接触到的件即许可界面；
此后碰到基线之外的任何件即判失败。

用法：/opt/orbita/venv/bin/python tools/audit_path_penetration.py --op <工序> [--step 0.1]
输出：本体/S7-穿模审计/<op>.json
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

# 参数化移植（重建-20260829）：--case-dir 注入路径；判据与 X1 audit_path_penetration.py 一致。
import json as _json
_case = Path(os.environ["REBUILD_CASE_DIR"]).resolve()
_cfg = _json.loads((_case / "robot-config.v1.json").read_text(encoding="utf-8"))
ROOT = _case
MESH = _case / _cfg["paths"]["mesh_dir"]
OCC = _case / _cfg["paths"]["s1_manifest"]
S7 = _case / "本体/S7-扫掠认证.v2.json"
CHAIN_DEFAULT = "FILM_v001"
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


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--op", required=True)
    ap.add_argument("--step", type=float, default=0.1)
    # 基线豁免不能是零件级的：螺钉翻向后从**自己拧入的那个件**实体里穿出去，
    # 名字集合差一辈子看不见（v007 章1 的 7 颗 M3×25 就是这么全绿的）。
    # 许可区 = t=0 接触到的基线件三角形 + 膨胀半径；碰到许可区外的基线件三角形=啃新材料。
    ap.add_argument("--dilate", type=float, default=2.0,
                    help="基线接触区膨胀半径 mm；配合面滑动允许的邻域")
    # 接触斑会随动件沿孔道/座面/球面移动，离开 t=0 邻域本身不是穿模；
    # 真正的问法是**采样点有没有进入基线件材料内部**（射线含入，见 baseline_region.py）。
    ap.add_argument("--run", default=CHAIN_DEFAULT, help="消费哪个 FILM_* 的 state-chain")
    # s7 = 工艺口径（除本工序动件外全部在场，顺序无关、偏严+兄弟盲区）；
    # film = 成片口径（按链序逆向行走：后续工序动件与未在播件退场，先装兄弟在场）。
    # 序列感知认证之后成片按 film 口径验收；s7 口径保留作对照。
    ap.add_argument("--scene", choices=("s7", "film"), default="s7")
    ap.add_argument("--geom", choices=("real", "proxy"), default="real",
                    help="real=真几何（判画面里有没有穿模，默认）；proxy=凸包代理（S7 的保守口径）")
    a = ap.parse_args()
    t0 = time.time()
    OUTD.mkdir(parents=True, exist_ok=True)

    man = json.loads((MESH / "manifest.json").read_text(encoding="utf-8"))["parts"]
    occ = json.loads(OCC.read_text(encoding="utf-8"))["occurrences"]
    # v010：S7 按本 run 冻结态记录的路径读（v3 = 含单元结合行），否则回退 v2
    _S7 = S7
    try:
        _st0 = json.loads((ROOT / "装配动画" / a.run / "pipeline-state.v1.json").read_text(encoding="utf-8"))
        _p7 = ((_st0.get("tracked_inputs") or {}).get("s7") or {}).get("path")
        if _p7 and (ROOT / _p7).exists():
            _S7 = ROOT / _p7
    except Exception:
        pass
    s7 = json.loads(_S7.read_text(encoding="utf-8"))
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
    if ch is None:
        doc = {
            "$schema": "x1.path-penetration-audit/v2", "op": a.op, "run": a.run,
            "geometry": a.geom, "scene": a.scene, "step_mm": a.step,
            "dilate_mm": a.dilate, "movers": 0, "penetrates": 0,
            "seconds": round(time.time() - t0, 1), "rows": [],
        }
        (OUTD / (f"{a.op}.{a.run}." + ("film." if a.scene == "film" else "")
                 + f"{a.geom}.json")).write_text(
            json.dumps(doc, ensure_ascii=False), encoding="utf-8")
        return 0
    s7ops = {o["op"]: o for o in s7["operations"]}
    s7m = {m["occ"]: m for m in s7ops.get(a.op, {}).get("per_mover", [])}

    id2i = {o["occ_id"]: i for i, o in enumerate(occ)}
    # S5 折叠单元：审计实体 = 成员烘焙合并网格整体行走（与 S7/构建器同口径）
    bindp = json.loads((ROOT / _cfg["paths"]["s3_binding"]).read_text(encoding="utf-8"))
    ent_members = {}
    for _op0 in bindp["operations"]:
        for _m0 in _op0["movers"]:
            ent_members[_m0["occ_id"]] = _m0.get("member_occ_ids") or [_m0["occ_id"]]

    def members_of(oid):
        return ent_members.get(oid, [oid])

    movers = [m for m in ch["movers"]]
    if a.scene == "film":
        # 成片口径（台架可见性语义，2026-08-29 与构建器同步）：
        # 本章可见集 = 前驱闭包成员 + 本章成员（movers + static_withdrawn + context）；
        # 其余一律不在场。本工序动件初始在场，循环里按链序逆向行走逐件移出——
        # 先装兄弟保留在场（F-SCN-12 修复面）。闭包从冻结状态 predecessors 读取。
        state_p = ROOT / "装配动画" / a.run / "pipeline-state.v1.json"
        frozen = json.loads(state_p.read_text(encoding="utf-8"))
        asm = frozen["assembly"]
        preds_map = asm.get("predecessors", {})
        op_members = {o2["op"]: {m2["occ"] for m2 in o2.get("movers", [])}
                      for o2 in asm.get("operations", [])}
        for it in asm.get("not_animated", []):
            op_members.setdefault(it["op"], set()).update(it.get("occs", []))
        acc, stack = set(), [a.op]
        while stack:
            cur = stack.pop()
            for p2 in preds_map.get(cur, []):
                if p2 not in acc:
                    acc.add(p2)
                    stack.append(p2)
        visible = set()
        for p2 in acc:
            visible |= op_members.get(p2, set())
        visible |= op_members.get(a.op, set())
        visible |= set(ch.get("context", []))
        # 本章 not_animated 件在全部动件就位后才出现（构建器 static_withdrawn_appear_frame
        # 语义）——动件路径期间画面上没有它们，不作为障碍
        own_static = set()
        for it in asm.get("not_animated", []):
            if it["op"] == a.op:
                own_static.update(it.get("occs", []))
        visible -= own_static
        visible_m = {mid for oid in visible for mid in members_of(oid)}
        exclude = {id2i[o2["occ_id"]] for o2 in occ if o2["occ_id"] not in visible_m}
    else:
        # 排除集必须与 S7 一致：**本工序的全部动件**，而不是"这一版里可播的那些"。
        # 踩过：v003 剔掉 45 个不可播动件后，它们不再属于 movers，于是变成了场景里的障碍物，
        # 同一批件的审计结论在两版之间漂了——口径必须钉在工艺（本工序装什么），不是钉在这一版播什么。
        exclude = {id2i[mid] for m in s7ops.get(a.op, {}).get("per_mover", [])
                   for mid in members_of(m["occ"]) if mid in id2i}
        exclude |= {id2i[mid] for m in movers for mid in members_of(m["occ"]) if mid in id2i}

    cache, world = {}, {}
    for i, o in enumerate(occ):
        st = stem_for(o["product"])
        if st is None:
            continue
        use = st
        # 判"画面里有没有穿模"必须用**真几何**：画面渲的就是真几何。
        # 用凸包代理会把"代理填平了真件的让位孔"当成穿模——
        # 这正是 S5 自己写下的边界："凸包会填平凹腔，真件能让开的地方代理可能报阻挡"。
        if a.geom == "proxy" and (not man[st].get("has_solid", True)) and (MESH / f"proxy_{st}.stl").exists():
            use = f"proxy_{st}"
        if use not in cache:
            cache[use] = load_stl(MESH / f"{use}.stl")
        W = np.array(o["world"], dtype=np.float64)
        T = np.eye(4)
        T[:3, :3] = W[:3, :3]
        T[:3, 3] = W[:3, 3]
        world[i] = (use, T)

    mgr = trimesh.collision.CollisionManager()
    name2occ = {}
    for i, (use, T) in world.items():
        if i in exclude:
            continue
        nm = f"o{i}"
        name2occ[nm] = i
        mgr.add_object(nm, cache[use], T)
    checker = PathChecker(mgr,
                          {nm: cache[world[i][0]] for nm, i in name2occ.items()},
                          {nm: world[i][1] for nm, i in name2occ.items()},
                          dilate=a.dilate)

    # 未绑定紧固件的被夹集(S8):夹某动件者,其在场随该动件的就位走(图拓扑时序)
    clampers_of = {}
    g8p = ROOT / "本体/S8-连接关系图.v1.json"
    if a.scene == "film" and g8p.exists():
        _g8 = json.loads(g8p.read_text(encoding="utf-8"))
        _unb = {n["occ"] for n in _g8["nodes"] if not n.get("op")}
        for _f, _cl in _g8.get("fastens", {}).items():
            if _f in _unb:
                for _c in _cl:
                    clampers_of.setdefault(_c, set()).add(_f)
    rows = []
    iter_movers = list(reversed(movers)) if a.scene == "film" else movers
    pair_of = {}
    for m2 in movers:
        if m2.get("pair_id"):
            pair_of.setdefault(m2["pair_id"], []).append(m2["occ"])
    for m in iter_movers:
        midx = [id2i[mid] for mid in members_of(m["occ"]) if mid in id2i and id2i[mid] in world]
        if a.scene == "film":
            # 耦合对是共动件：互检由 pair_closure 认证承担；本审计把整对同时移出场景
            drop = {m["occ"]}
            if m.get("pair_id"):
                drop |= set(pair_of.get(m["pair_id"], ()))
            # 夹本动件的未绑定紧固件随之退场(它们在成片中也是此刻才消失/尚未出现)
            drop |= clampers_of.get(m["occ"], set())
            for oid2 in drop:
                for mid2 in members_of(oid2):
                    if mid2 not in id2i:
                        continue
                    nm2 = f"o{id2i[mid2]}"
                    if nm2 in name2occ:
                        mgr.remove_object(nm2)
                        checker.meshes.pop(nm2, None)
                        checker.world_T.pop(nm2, None)
                        del name2occ[nm2]
        if not midx:
            continue
        if len(midx) == 1:
            use, T = world[midx[0]]
            mesh = cache[use]
        else:
            mesh = trimesh.util.concatenate(
                [cache[world[j0][0]].copy().apply_transform(world[j0][1]) for j0 in midx])
            T = np.eye(4)
        info = s7m.get(m["occ"])
        if info is None:
            continue
        # **轴与方向一律取状态链里的那一份**（动画实际播的就是它）。
        # 踩过：从 S7 取轴、从状态链取偏移，换轴的动件被拼成"旧轴 + 新偏移"，
        # 审计因此报 19 个假穿模。同一个动件的字段必须整组来自同一份记录。
        src_axis = m.get("insertion_axis_world") or info["insertion_axis_world"]
        axis = np.array(src_axis, dtype=np.float64)
        axis /= np.linalg.norm(axis)
        sense = m.get("withdraw_sense") if m.get("withdraw_sense") is not None else info.get("withdraw_sense")
        axis_from = "STATE_CHAIN" if m.get("insertion_axis_world") else "S7_FALLBACK"
        approach = float(m["approach_mm"])

        checker.prepare(mesh, T)
        base = checker._base
        first_bad, first_kind, worst, seen = None, None, set(), 0
        t = 0.0
        while t <= approach + 1e-9:
            is_bad, kind, bad_names = checker.bad_at(axis, sense, t)
            if is_bad:
                seen += 1
                worst |= bad_names
                if first_bad is None:
                    first_bad = round(t, 3)
                    first_kind = kind
            t += a.step
        rows.append({
            "occ": m["occ"], "product": m["product"],
            "klass": info.get("klass"), "axis_candidates": info.get("axis_candidates"),
            "axis_from": axis_from,
            "direction_rule": info.get("direction_rule"),
            "approach_mm": approach,
            "release_mm": (info.get("release_mm_by_sense") or {}).get(str(sense) if sense == -1 else "+1"),
            "baseline_occ": sorted(occ[name2occ[n]]["occ_id"] for n in base),
            "baseline_parts": sorted(occ[name2occ[n]]["product"][:40] for n in base),
            "first_non_baseline_at_mm": first_bad,
            "first_bad_kind": first_kind,
            "steps_with_non_baseline": seen,
            "non_baseline_occ": sorted(occ[name2occ[n]]["occ_id"] for n in worst),
            "non_baseline_parts": sorted({occ[name2occ[n]]["product"][:40] for n in worst}),
            "verdict": "CLEAR" if first_bad is None else "PENETRATES",
        })

    if a.scene == "film":
        rows.reverse()
    bad = [r for r in rows if r["verdict"] == "PENETRATES"]
    doc = {"$schema": "x1.path-penetration-audit/v2", "op": a.op, "run": a.run, "geometry": a.geom,
           "scene": a.scene,
           "criterion": "K-IF-04 装配态基线（区域+体积级）：非基线件接触即穿模；基线件上 t=0 接触区"
                        "邻域外的新区接触为嫌疑，由射线含入仲裁——动件采样点进入基线件材料内部"
                        "超过 t=0 本底即穿模，否则为接触斑沿配合面移动（合法）",
           "dilate_mm": a.dilate,
           "step_mm": a.step, "movers": len(rows), "penetrates": len(bad),
           "seconds": round(time.time() - t0, 1), "rows": rows}
    (OUTD / (f"{a.op}.{a.run}." + ("film." if a.scene == "film" else "") + f"{a.geom}.json")).write_text(json.dumps(doc, ensure_ascii=False), encoding="utf-8")
    print(f"{a.op}: 动件 {len(rows)} 穿模 {len(bad)}  {doc['seconds']}s")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
