#!/usr/bin/env python3
"""工序内播放序（参数化移植版）。

初始序 = S7 v2 每工序 per_mover 的几何认证序（先结构件后紧固件的安装序）；
再施加 X1 `assembly_film.graph_constrained_order` 的稳定约束（逐字保留）：
结构件前、紧固件后、耦合对末尾；纯树 BFS 禁用。
同时输出 fastener-before-clamped 违规与不可见被夹件台账。

用法：python3 build_play_order_generic.py --case-dir <robot dir>
"""
import argparse
import copy
import json
from collections import Counter
from pathlib import Path


def graph_constrained_order(order_doc, graph):
    """与 X1 assembly_film.graph_constrained_order 逐行一致。"""
    result = copy.deepcopy(order_doc)
    nodes = {node["occ"]: node for node in graph.get("nodes", [])}
    fastens = {key: set(value) for key, value in graph.get("fastens", {}).items()}
    moved_slots = 0
    changed_ops = 0
    invisible = []
    violations = []

    for op_name, spec in result.get("ops", {}).items():
        order = list(spec.get("order", []))
        if len({entry.get("occ") for entry in order}) != len(order):
            raise ValueError(f"{op_name}: ORDER 含重复 occ")
        pair_tail = [entry for entry in order if entry.get("pair_id")]
        core = [entry for entry in order if not entry.get("pair_id")]
        structures = [entry for entry in core if not nodes.get(entry["occ"], {}).get("is_fastener")]
        fasteners = [entry for entry in core if nodes.get(entry["occ"], {}).get("is_fastener")]
        new_order = structures + fasteners + pair_tail
        moved = sum(
            1 for index, entry in enumerate(new_order)
            if index >= len(order) or entry["occ"] != order[index]["occ"]
        )
        if moved:
            changed_ops += 1
            moved_slots += moved
        spec["order"] = new_order
        spec["canonical_sort"] = {
            "primary": "GEOMETRY_CERTIFIED_ORDER",
            "graph_constraint": "STABLE_STRUCTURE_THEN_FASTENER_PAIR_TAIL",
            "moved_slots": moved,
        }

        positions = {entry["occ"]: index for index, entry in enumerate(new_order)}
        present = set(positions)
        for fastener in fasteners:
            fastener_id = fastener["occ"]
            clamped_here = [
                item for item in fastens.get(fastener_id, set())
                if item in present and nodes.get(item, {}).get("op") == op_name
            ]
            late = [item for item in clamped_here if positions[item] > positions[fastener_id]]
            if late:
                violations.append({
                    "op": op_name,
                    "fastener": fastener_id,
                    "clamped_after_fastener": sorted(late),
                })
            for item in fastens.get(fastener_id, set()):
                if item not in present and nodes.get(item, {}).get("op") == op_name:
                    invisible.append({
                        "op": op_name,
                        "fastener": fastener_id,
                        "clamped_unplayed": item,
                    })

    result["canonical_sort"] = {
        "rule": "几何认证序为主键；连接图只施加结构件前/紧固件后与耦合对末尾约束",
        "tree_bfs_production_use": False,
        "changed_ops": changed_ops,
        "moved_slots": moved_slots,
        "fasteners_clamping_unplayed": invisible,
        "violations": violations,
    }
    return result, result["canonical_sort"]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--case-dir", required=True, type=Path)
    a = ap.parse_args()
    cfg = json.loads((a.case_dir / "robot-config.v1.json").read_text(encoding="utf-8"))
    P = cfg["paths"]
    s7 = json.loads((a.case_dir / "本体/S7-扫掠认证.v2.json").read_text(encoding="utf-8"))
    graph = json.loads((a.case_dir / P["s8_graph"]).read_text(encoding="utf-8"))
    OUT = a.case_dir / "本体/S7-工序内播放序.v1.json"

    order_doc = {"$schema": "assembly-ontology.intra-op-play-order/v2",
                 "source": "S7 v2 per_mover 几何认证序",
                 "ops": {}}
    for op in s7["operations"]:
        entries = [{"occ": m["occ"]} for m in op.get("per_mover", [])]
        if entries:
            order_doc["ops"][op["op"]] = {"order": entries}

    result, report = graph_constrained_order(order_doc, graph)
    OUT.write_text(json.dumps(result, ensure_ascii=False, indent=1), encoding="utf-8")
    print(f"写出 {OUT}")
    print(f"  重排工序 {report['changed_ops']} · 移动席位 {report['moved_slots']} · "
          f"顺序违规 {len(report['violations'])} · 紧固不可见被夹 {len(report['fasteners_clamping_unplayed'])}")
    if report["violations"]:
        for v in report["violations"][:5]:
            print("  违规:", v)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
