#!/usr/bin/env python3
"""序列感知认证（参数化移植版）：沿装配序做逆向拆卸行走。

判据与 X1 sweep_sequence_reverse.py 一致：整机起步，按装配序倒序逐件移出，
每件在"移出前的真实剩余集"里认证——这正是成片播放时它飞入的那个场景。
量具：baseline_region.PathChecker（区域嫌疑 + 射线含入）。

重建流适配（无既有 state-chain）：
  工序间顺序 = S3 绑定序；工序内 = S7-工序内播放序（图约束后）。
  主轴 = S7-轴选取验证 的直接量最优轴（若有），否则 S7 v2 的轴/方向。
  再试 S2 界面图全部候选轴（含同工序先装兄弟的配对轴）。

实体化扩展（Fourier S5 折叠单元）：行走实体 = 叶或求解单元（成员世界位姿烘焙合并，
刚体整体移出）；轴候选 = 成员与实体外伙伴的界面轴并集。--chunk s:e 只行走实体序
[s,e)（先把 [e,N) 的实体按"已移出"从场景剔除），供农场分片，输出带 chunk 后缀。

用法：python3 sweep_sequence_reverse_generic.py --case-dir <robot dir> --op <工序> [--chunk s:e]
"""
import argparse
import json
import re
import struct
import sys
import time
import unicodedata
from pathlib import Path

import numpy as np
import trimesh

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


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--case-dir", required=True, type=Path)
    ap.add_argument("--op", required=True)
    ap.add_argument("--step", type=float, default=0.25)
    ap.add_argument("--cap", type=float, default=20.0)
    ap.add_argument("--dilate", type=float, default=2.0)
    ap.add_argument("--chunk", default="", help="s:e 实体分片")
    # OLSK 增量（2026-09-03，v005）：行程延长。20 mm 的认证段在 2–3 m 的机床画面里不可辨（动件"弹入"），
    # 用户实看即报"穿模"。延长到 --cap 150 但细步只保留到 --fine-until（默认 20 mm，与 v004 同精度），
    # 其后用 --coarse-step（默认 1 mm；薄板件最薄 3 mm，1 mm 步不会跨过）。--out-dir 另存，不覆盖 v004 记录。
    ap.add_argument("--fine-until", type=float, default=None, help="细步（--step）覆盖到此距离，之后改用 --coarse-step")
    ap.add_argument("--coarse-step", type=float, default=None)
    ap.add_argument("--out-dir", default=None, help="输出目录（默认 robot-config paths.s7_seq_cert）")
    ap.add_argument("--play-order", default="本体/S7-工序内播放序.v1.json", help="工序内播放序（v2=功能/安装语义序）")
    a = ap.parse_args()
    t0 = time.time()

    cfg = json.loads((a.case_dir / "robot-config.v1.json").read_text(encoding="utf-8"))
    P = cfg["paths"]
    MESH = a.case_dir / P["mesh_dir"]
    OUTD = (a.case_dir / a.out_dir) if a.out_dir else (a.case_dir / P["s7_seq_cert"])
    OUTD.mkdir(parents=True, exist_ok=True)

    man = json.loads((MESH / "manifest.json").read_text(encoding="utf-8"))["parts"]
    occ = json.loads((a.case_dir / P["s1_manifest"]).read_text(encoding="utf-8"))["occurrences"]
    s7 = json.loads((a.case_dir / "本体/S7-扫掠认证.v2.json").read_text(encoding="utf-8"))
    graph = json.loads((a.case_dir / P["s2_coax_graph"]).read_text(encoding="utf-8"))
    plane_graph = None
    # 轴候选源优先取宽松 gap 版（覆盖侧向放入开口卡槽等 0.05-0.5mm 间隙贴合）：
    # 方向候选不要求认证配合，行走本身就是测量裁决；S8 边仍只用默认阈值图。
    for pg_name in ("本体/S2-平面界面图-gap05.v1.json", P["s2_plane_graph"]):
        pg_path = a.case_dir / pg_name
        if pg_path.exists():
            plane_graph = json.loads(pg_path.read_text(encoding="utf-8"))
            break
    order_doc = json.loads((a.case_dir / a.play_order).read_text(encoding="utf-8"))

    axbest = {}
    axd = a.case_dir / "本体/S7-轴选取验证"
    if axd.exists():
        import glob
        for f in glob.glob(str(axd / "*.json")):
            for r in json.loads(Path(f).read_text(encoding="utf-8")).get("rows", []):
                if r.get("candidates"):
                    b = r["candidates"][0]
                    axbest[r["occ"]] = b

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

    played = {opn: [e["occ"] for e in spec["order"]]
              for opn, spec in order_doc.get("ops", {}).items()}
    seq = []
    for op in s7["operations"]:
        po = played.get(op["op"], [])
        rest = [m["occ"] for m in op["per_mover"] if m["occ"] not in set(po)]
        seq.append((op["op"], po + rest, set(po)))

    ops_order = [x[0] for x in seq]
    if a.op not in ops_order:
        print(f"{a.op}: 不在 S7 工序表")
        return 0
    oi = ops_order.index(a.op)
    this_op = seq[oi]
    op_occs, played_set = this_op[1], this_op[2]

    # 台架场景（模块化装配语义）：在场集 = 本工序动件 + **前驱工序闭包**的动件。
    # 官方指南分组并行制作——非前驱单元此刻不在台架上，按最终位姿摆进场景
    # 会造成假卡死（v2 行走实测：脊柱被电机单元卡死等 35 例）。
    bind = json.loads((a.case_dir / P["s3_binding"]).read_text(encoding="utf-8"))
    preds = {o["op"]: o.get("predecessors", []) for o in bind["operations"]}
    movers_of = {o["op"]: [m["occ_id"] for m in o["movers"]] for o in bind["operations"]}
    ent_map = {}
    for op0 in bind["operations"]:
        for m in op0["movers"]:
            ent_map[m["occ_id"]] = {"members": m.get("member_occ_ids") or [m["occ_id"]],
                                    "product": m.get("product", ""), "kind": m.get("kind", "leaf")}

    def mem_ids(eid):
        return ent_map[eid]["members"] if eid in ent_map else [eid]

    def closure(opn, acc):
        for p in preds.get(opn, []):
            if p not in acc:
                acc.add(p)
                closure(p, acc)
        return acc

    scene_ops = closure(a.op, set())
    scene_occ = {mid for p in scene_ops for eid in movers_of.get(p, []) for mid in mem_ids(eid)}
    scene_occ |= {mid for eid in op_occs for mid in mem_ids(eid)}
    later_occ = {o["occ_id"] for o in
                 json.loads((a.case_dir / P["s1_manifest"]).read_text(encoding="utf-8"))["occurrences"]
                 } - scene_occ

    id2i = {o["occ_id"]: i for i, o in enumerate(occ)}
    cache, world = {}, {}
    for i, o in enumerate(occ):
        st = stem_for(o["product"])
        if st is None:
            continue
        if st not in cache:
            cache[st] = load_stl(MESH / f"{st}.stl")
        W = np.array(o["world"], dtype=np.float64)
        T = np.eye(4)
        T[:3, :3] = W[:3, :3]
        T[:3, 3] = W[:3, 3]
        world[i] = (st, T)

    mgr = trimesh.collision.CollisionManager()
    meshes, Ts = {}, {}
    for i, (st, T) in world.items():
        oid = occ[i]["occ_id"]
        if oid in later_occ:
            continue
        nm = f"o{i}"
        mgr.add_object(nm, cache[st], T)
        meshes[nm], Ts[nm] = cache[st], T

    axes_by_pair, mates = {}, {}
    for p in graph["occurrence_pairs"]:
        axes_by_pair[(p["a_occ"], p["b_occ"])] = list(p["axes"])
        axes_by_pair[(p["b_occ"], p["a_occ"])] = list(p["axes"])
        mates.setdefault(p["a_occ"], set()).add(p["b_occ"])
        mates.setdefault(p["b_occ"], set()).add(p["a_occ"])
    # 平面法向 / 锥轴也是 S2 认证的界面轴（K-IF-01 三类面；E 类补轴同源）。
    # 平面只给直线不给正负——行走本来就双向都试，正负由行走裁定。
    if plane_graph is not None:
        for p in plane_graph.get("occurrence_pairs", []):
            axs = [{"dir": n["dir"], "foot": [0.0, 0.0, 0.0], "faces": n["faces"]}
                   for n in p.get("normals", [])]
            for key in ((p["a_occ"], p["b_occ"]), (p["b_occ"], p["a_occ"])):
                axes_by_pair.setdefault(key, [])
                axes_by_pair[key] = list(axes_by_pair[key]) + axs
            mates.setdefault(p["a_occ"], set()).add(p["b_occ"])
            mates.setdefault(p["b_occ"], set()).add(p["a_occ"])
        for p in plane_graph.get("cone_pairs", []):
            axs = [{"dir": n["dir"], "foot": n["foot"], "faces": n["faces"]}
                   for n in p.get("axes", [])]
            for key in ((p["a_occ"], p["b_occ"]), (p["b_occ"], p["a_occ"])):
                axes_by_pair.setdefault(key, [])
                axes_by_pair[key] = list(axes_by_pair[key]) + axs
            mates.setdefault(p["a_occ"], set()).add(p["b_occ"])
            mates.setdefault(p["b_occ"], set()).add(p["a_occ"])

    s7m = {m["occ"]: m for m in next(o for o in s7["operations"] if o["op"] == a.op)["per_mover"]}

    checker = PathChecker(mgr, meshes, Ts, dilate=a.dilate)
    c0, c1 = 0, len(op_occs)
    if a.chunk:
        c0, c1 = (int(x) for x in a.chunk.split(":"))
    rows = []
    for k in range(len(op_occs) - 1, -1, -1):
        oid = op_occs[k]
        midx = [id2i[m] for m in mem_ids(oid) if m in id2i and id2i[m] in world]
        for i in midx:
            nm_self = f"o{i}"
            if nm_self in meshes:
                mgr.remove_object(nm_self)
                del meshes[nm_self], Ts[nm_self]
        if k >= c1:
            continue
        if k < c0:
            break
        if not midx:
            continue
        i = midx[0]
        memset = set(midx)
        mesh = trimesh.util.concatenate(
            [cache[world[j0][0]].copy().apply_transform(world[j0][1]) for j0 in midx])
        T = np.eye(4)
        checker.prepare(mesh, T)

        def run_axis(dirv, sense):
            axv = np.array(dirv, dtype=np.float64)
            n = np.linalg.norm(axv)
            if n < 1e-12:
                return 0.0
            axv /= n
            t, run = 0.0, 0.0
            while t <= a.cap + 1e-9:
                bad, _k2, _n2 = checker.bad_at(axv, sense, t)
                if bad:
                    break
                run = t
                # 细步段之后改粗步（延长行程时用；默认参数下与 v004 行为完全一致）
                if a.fine_until is not None and a.coarse_step and t >= a.fine_until - 1e-9:
                    t += a.coarse_step
                else:
                    t += a.step
            return run

        prim = None
        ab = axbest.get(oid)
        info = s7m.get(oid, {})
        if ab:
            prim = (ab["dir"], int(ab["sense"]), None)
        elif info.get("insertion_axis_world") and info.get("withdraw_sense") is not None:
            prim = (info["insertion_axis_world"], int(info["withdraw_sense"]), None)
        prim_run = round(run_axis(prim[0], prim[1]), 3) if prim else None

        axset = {}
        for i0 in memset:
            for j_occ in mates.get(i0, ()):
                if j_occ in memset:
                    continue
                for ax in axes_by_pair[(i0, j_occ)]:
                    key = tuple(round(x, 3) for x in ax["dir"]) + tuple(round(x, 1) for x in ax["foot"])
                    axset.setdefault(key, ax["dir"])
        best = {"dir": prim[0] if prim else None,
                "sense": prim[1] if prim else None,
                "clear_run_mm": prim_run or 0.0}
        for dirv in axset.values():
            for sense in (1, -1):
                r = run_axis(dirv, sense)
                if r > best["clear_run_mm"] + 1e-9:
                    best = {"dir": [round(float(x), 6) for x in
                                    (np.array(dirv) / np.linalg.norm(dirv))],
                            "sense": sense, "clear_run_mm": round(r, 3)}
        verdict = ("FREE" if best["clear_run_mm"] >= a.cap - 1e-9 else
                   "LIMITED" if best["clear_run_mm"] >= 1.0 else "STUCK")
        was = oid in played_set
        rows.append({
            "occ": oid,
            "product": (ent_map.get(oid, {}).get("product") or occ[i]["product"])[:40],
            "kind": ent_map.get(oid, {}).get("kind", "leaf"),
            "member_count": len(midx),
            "walk_index": k, "was_in_play_order": was,
            "primary_axis": prim[0] if prim else None,
            "primary_sense": prim[1] if prim else None,
            "primary_clear_run_mm": prim_run,
            "best": best, "verdict": verdict,
            "recovered": (not was) and best["clear_run_mm"] >= 1.0,
        })

    nrec = sum(1 for r in rows if r["recovered"])
    nstuck = sum(1 for r in rows if r["verdict"] == "STUCK")
    doc = {"$schema": "assembly-ontology.sequence-aware-certification/v2", "op": a.op,
           "criterion": "逆向拆卸行走：每件在移出前的真实剩余集里认证（S3 工序序 + 图约束工序内播放序）；"
                        "区域嫌疑+射线含入，与穿模审计同量具",
           "scene": "台架语义：本工序动件 + 前驱工序闭包动件 − 已移出的同工序动件（模块化装配，非前驱单元不在场）",
           "scene_ops": sorted(scene_ops),
           "step_mm": a.step, "cap_mm": a.cap, "chunk": [c0, c1] if a.chunk else None,
           "total_entities": len(op_occs),
           "movers": len(rows), "recovered": nrec, "stuck": nstuck,
           "seconds": round(time.time() - t0, 1), "rows": rows}
    suffix = f".chunk{c0}-{c1}" if a.chunk else ""
    (OUTD / f"{a.op}{suffix}.json").write_text(json.dumps(doc, ensure_ascii=False), encoding="utf-8")
    print(f"{a.op}{suffix}: 实体 {len(rows)} 救回 {nrec} 真卡死 {nstuck}  {doc['seconds']}s")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
