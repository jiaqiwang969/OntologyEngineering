#!/usr/bin/env python3
"""冻结装配片运动状态（参数化移植版，语义对应 X1 assembly_film.py 的 compile+freeze）。

把 S7 v2、可播表（逆向行走导出）、工序内播放序、S8 图编译成一份 create-only
的冻结状态：每个动件一条完整五元组（axis/sense/approach/source/rule），
`animated + not_animated = 全部 target` 单表闭合（X1 审计要求的 canonical ledger）。

拒绝条件：任何可播动件缺轴/方向；重复动件；闭合不等式不成立。

用法：python3 freeze_film_state_generic.py --case-dir <robot dir> --run FILM_v001
"""
import argparse
import datetime
import hashlib
import json
import sys
from pathlib import Path


def sha256_file(p):
    return hashlib.sha256(Path(p).read_bytes()).hexdigest()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--case-dir", required=True, type=Path)
    ap.add_argument("--run", required=True)
    # OLSK 增量（2026-09-03，v005）：--playable 指向延长行程的可播表（默认 v1，v004 行为不变）
    ap.add_argument("--playable", default="本体/S7-可播接近距离.v1.json")
    ap.add_argument("--play-order", default=None, help="工序内播放序文件（默认 v1）")
    ap.add_argument("--s4", default=None, help="机构识别记录路径（v2 = 修正柔性类词表），记入 tracked_inputs.s4")
    ap.add_argument("--s7", default=None, help="扫掠认证记录（v3 = 含单元结合 per_mover 行）")
    a = ap.parse_args()
    cfg = json.loads((a.case_dir / "robot-config.v1.json").read_text(encoding="utf-8"))
    P = cfg["paths"]

    S7F = a.case_dir / (a.s7 or "本体/S7-扫掠认证.v2.json")
    PLAYF = a.case_dir / a.playable
    ORDERF = a.case_dir / (a.play_order or "本体/S7-工序内播放序.v1.json")
    G8F = a.case_dir / P["s8_graph"]
    BINDF = a.case_dir / P["s3_binding"]
    OUTD = a.case_dir / "装配动画" / a.run
    OUT = OUTD / "pipeline-state.v1.json"
    if OUT.exists():
        print(f"拒绝覆盖已有冻结状态: {OUT}", file=sys.stderr)
        return 2

    s7 = json.loads(S7F.read_text(encoding="utf-8"))
    play_doc = json.loads(PLAYF.read_text(encoding="utf-8"))
    order_doc = json.loads(ORDERF.read_text(encoding="utf-8"))
    graph = json.loads(G8F.read_text(encoding="utf-8"))
    bind = json.loads(BINDF.read_text(encoding="utf-8"))

    play = {}
    for r in play_doc["rows"]:
        if r["occ"] in play:
            print(f"PLAY 重复 occ: {r['occ']}", file=sys.stderr)
            return 2
        play[r["occ"]] = r

    within = order_doc.get("ops", {})
    nodes = {n["occ"]: n for n in graph["nodes"]}

    operations, skipped, seen = [], [], set()
    for op in s7["operations"]:
        op_name = op["op"]
        spec = within.get(op_name, {"order": [{"occ": m["occ"]} for m in op["per_mover"]]})
        good = []
        for entry in spec["order"]:
            occ_id = entry["occ"]
            m = next((x for x in op["per_mover"] if x["occ"] == occ_id), None)
            rec = play.get(occ_id)
            if m is None or rec is None or not rec.get("playable"):
                continue
            axis = rec.get("insertion_axis_world")
            sense = rec.get("withdraw_sense")
            if axis is None or sense is None:
                print(f"{op_name}/{occ_id}: 可播但缺轴或方向", file=sys.stderr)
                return 2
            if occ_id in seen:
                print(f"跨工序重复动件: {occ_id}", file=sys.stderr)
                return 2
            seen.add(occ_id)
            good.append({
                "occ": occ_id,
                "product": m["product"],
                "insertion_axis_world": [round(float(v), 9) for v in axis],
                "withdraw_sense": int(sense),
                "approach_mm": round(float(rec["playable_approach_mm"]), 3),
                "certified_approach_mm": m.get("certified_approach_mm"),
                "approach_source": rec["source"],
                "pair_id": rec.get("pair_id"),
                "direction_rule": m.get("direction_rule"),
            })
        gset = {g["occ"] for g in good}
        bad = [m for m in op["per_mover"] if m["occ"] not in gset]
        if bad:
            reasons = set()
            for m in bad:
                rec = play.get(m["occ"])
                if rec is None:
                    reasons.add("NOT_IN_WALK")
                elif not rec.get("playable"):
                    reasons.add(rec.get("why_not") or "NOT_PLAYABLE")
                else:
                    reasons.add("MISSING_MOTION_FIELDS")
            skipped.append({"op": op_name, "n": len(bad), "reasons": sorted(reasons),
                            "occs": [m["occ"] for m in bad]})
        if good:
            operations.append({"op": op_name, "movers": good})

    # unit-join 结合章：presence-only（v1 不主张单元运动，只做诚实的在场汇合）。
    # 台架章的件在本章结束后隐藏（"装完放一边"），到结合章按前驱闭包重现——
    # 否则其他台架的件在最终位姿常显，动件飞入会穿过画面上可见的它们（film 审计实测 39 例）。
    bind_ops = {o["op"]: o for o in bind["operations"]}
    rank = {o["op"]: i for i, o in enumerate(bind["operations"])}
    anim_ops = {o["op"] for o in operations}
    join_ops = [o for o in bind["operations"] if not o["movers"] and o.get("unit_joins")]
    merged = sorted(
        operations + [{"op": o["op"], "presence_only": True, "movers": [],
                       "unit_joins": o["unit_joins"],
                       "note": "单元结合章：前驱台架产物汇合呈现；单元运动未认证（HOLD），不主张结合动作"}
                      for o in join_ops],
        key=lambda x: rank[x["op"]])
    operations = merged

    n_anim = sum(len(o["movers"]) for o in operations)
    n_skip = sum(s["n"] for s in skipped)
    n_target = sum(len(o["per_mover"]) for o in s7["operations"])
    if n_anim + n_skip != n_target:
        print(f"闭合失败: animated {n_anim} + not_animated {n_skip} != target {n_target}",
              file=sys.stderr)
        return 2

    predecessors = {o["op"]: o.get("predecessors", []) for o in bind["operations"]}
    assembly = {
        "$schema": "assembly-ontology.frozen-assembly-motion-state/v2",
        "operation_order": [o["op"] for o in operations],
        "predecessors": predecessors,
        "bench_visibility": "台架语义：每章可见集 = 前驱闭包动件 + 本章已出场动件；"
                            "台架章结束后其件隐藏，至含它的结合章/终章重现",
        "ordering": order_doc.get("canonical_sort"),
        "operations": operations,
        "not_animated": skipped,
        "summary": {"operations": sum(1 for o in operations if not o.get("presence_only")),
                    "presence_chapters": sum(1 for o in operations if o.get("presence_only")),
                    "movers": n_anim, "not_animated": n_skip},
        "closure": {"targets": n_target, "animated": n_anim, "not_animated": n_skip,
                    "status": "PASS"},
        "claim_boundary": [
            "每次只表达一个件沿已记录轴的直线平移；不表达拧紧、扭矩、多自由度斜插或完整工艺。",
            "顺序、轴、方向与行程整组冻结；渲染层不得回退到其他 JSON 拼接字段。",
            "轴/方向/行程来自逆向拆卸行走（真实剩余集）；纯拓扑 BFS 禁用（X1 FILM_v014 负验证）。",
            "unit-join 集成工序（S3 中 consumes_leaves=0 的 8 章）本版未编入运动，"
            "在 not_animated 之外单列 pending_unit_chapters，须后续版本闭合。",
        ],
        "pending_unit_chapters": [o["op"] for o in bind["operations"] if not o["movers"]],
    }

    tracked = {}
    S4F = a.case_dir / (a.s4 or "本体/S4-机构识别.v1.json")
    for role, p in (("s7", S7F), ("play", PLAYF), ("order", ORDERF), ("graph", G8F), ("s4", S4F),
                    ("bind", BINDF), ("s1", a.case_dir / P["s1_manifest"])):
        tracked[role] = {"path": str(p), "sha256": sha256_file(p), "bytes": p.stat().st_size}

    doc = {
        "$schema": "assembly-ontology.film-pipeline-state/v2",
        "state_version": 1,
        "run_id": a.run,
        "case": cfg["case"],
        "status": "FROZEN_FOR_BUILD",
        "created_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "policy": {"create_only_run": True, "single_writer": True,
                   "legacy_tree_sorter_enabled": False, "physical_release_claimed": False},
        "tracked_inputs": tracked,
        "assembly": assembly,
    }
    OUTD.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps(doc, ensure_ascii=False, indent=1), encoding="utf-8")
    print(f"写出 {OUT}")
    print(f"  工序 {len(operations)}  动件 {n_anim}  not_animated {n_skip}  "
          f"闭合 {n_anim}+{n_skip}={n_target}  待闭合 unit 章 {len(assembly['pending_unit_chapters'])}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
