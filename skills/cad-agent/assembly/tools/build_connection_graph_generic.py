#!/usr/bin/env python3
"""S8：连接关系图（参数化移植版，边判据与 X1 build_connection_graph.py 一致）。

节点=occurrence，边=真配合（只消费 S2 已量出的数）：
  COAX  共轴且 max_overlap_mm >= 0.5 且 min_radial_gap_mm <= 1.0
  CONE  锥面配对    PLANE  平面贴合
紧固件（robot-config 词表）：其 COAX 伙伴即被夹件 → FASTENS。
跨工序异常台账：紧固件工序早于被夹件工序（只报告不改动）。

用法：python3 build_connection_graph_generic.py --case-dir <robot dir>
"""
import argparse
import json
import re
import time
from pathlib import Path


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--case-dir", required=True, type=Path)
    a = ap.parse_args()
    t0 = time.time()
    cfg = json.loads((a.case_dir / "robot-config.v1.json").read_text(encoding="utf-8"))
    P = cfg["paths"]
    occ = json.loads((a.case_dir / P["s1_manifest"]).read_text(encoding="utf-8"))["occurrences"]
    coax = json.loads((a.case_dir / P["s2_coax_graph"]).read_text(encoding="utf-8"))
    plane = json.loads((a.case_dir / P["s2_plane_graph"]).read_text(encoding="utf-8"))
    s7 = json.loads((a.case_dir / "本体/S7-扫掠认证.v2.json").read_text(encoding="utf-8"))
    OUT = a.case_dir / P["s8_graph"]
    FAST = [re.compile(p) for p in cfg["fastener_lexicon"]["patterns"]]

    op_of = {}
    for op in s7["operations"]:
        for m in op["per_mover"]:
            op_of[m["occ"]] = op["op"]

    # 实体收缩：S5 折叠单元是一个节点，成员的对外界面并到单元上，内部界面不出图。
    # 全叶绑定（Poppy/X1）时 ent_of 是恒等映射，行为与原版逐字一致。
    bind = json.loads((a.case_dir / P["s3_binding"]).read_text(encoding="utf-8"))
    ent_of, ent_info = {}, {}
    for op0 in bind["operations"]:
        for m in op0["movers"]:
            ent_info[m["occ_id"]] = m
            for mid in (m.get("member_occ_ids") or [m["occ_id"]]):
                ent_of[mid] = m["occ_id"]

    idx2id = {i: o["occ_id"] for i, o in enumerate(occ)}
    by_id = {o["occ_id"]: o for o in occ}
    nodes = {}
    for eid, m in ent_info.items():
        if m.get("kind") == "solver_unit":
            nodes[eid] = {"occ": eid, "uid": m.get("occ_uid", eid),
                          "product": m.get("product", "")[:60], "op": op_of.get(eid),
                          "is_fastener": False, "kind": "solver_unit",
                          "member_count": len(m.get("member_occ_ids") or [])}
        else:
            o = by_id[eid]
            nodes[eid] = {"occ": eid, "uid": o["occ_uid"], "product": o["product"][:60],
                          "op": op_of.get(eid),
                          "is_fastener": any(p.search(o["product"]) for p in FAST)}
    for o in occ:  # 不在任何绑定动件里的 occurrence（理论上无，防御性保留）
        if o["occ_id"] not in ent_of:
            ent_of[o["occ_id"]] = o["occ_id"]
            nodes.setdefault(o["occ_id"], {"occ": o["occ_id"], "uid": o["occ_uid"],
                                           "product": o["product"][:60], "op": op_of.get(o["occ_id"]),
                                           "is_fastener": any(p.search(o["product"]) for p in FAST)})

    edges, seen = [], set()

    def add_edge(a_i, b_i, kind, extra):
        x, y = idx2id.get(a_i), idx2id.get(b_i)
        if x is None or y is None:
            return
        x, y = ent_of.get(x, x), ent_of.get(y, y)
        if x == y:
            return
        key = (min(x, y), max(x, y), kind)
        if key in seen:
            return
        seen.add(key)
        edges.append({"a": x, "b": y, "kind": kind, **extra})

    for p in coax["occurrence_pairs"]:
        ov = p.get("max_overlap_mm") or 0.0
        gap = p.get("min_radial_gap_mm")
        if ov >= 0.5 and gap is not None and gap <= 1.0:
            add_edge(p["a_occ"], p["b_occ"], "COAX",
                     {"overlap_mm": round(float(ov), 2), "radial_gap_mm": round(float(gap), 3)})
    for p in plane.get("cone_pairs", []):
        add_edge(p["a_occ"], p["b_occ"], "CONE", {"faces": p.get("faces")})
    for p in plane.get("occurrence_pairs", []):
        add_edge(p["a_occ"], p["b_occ"], "PLANE",
                 {"faces": p.get("faces"), "max_overlap_mm2": p.get("max_overlap_mm2"),
                  "min_gap_mm": p.get("min_gap_mm")})
    # 宽松间隙贴合（0.05<gap<=0.5mm）：单独分级为 PLANE_LOOSE，证据是 S2 gap05 测量。
    # 它覆盖"放入开口卡槽/滑轨"类连接（舵机入支架、屏入导槽、Odroid 落位），
    # 这些件的插入轴与行走认证正来自该测量；不加则它们成"无入边却在动"的告警。
    loose_p = a.case_dir / "本体/S2-平面界面图-gap05.v1.json"
    if loose_p.exists():
        loose = json.loads(loose_p.read_text(encoding="utf-8"))
        for p in loose.get("occurrence_pairs", []):
            add_edge(p["a_occ"], p["b_occ"], "PLANE_LOOSE",
                     {"faces": p.get("faces"), "max_overlap_mm2": p.get("max_overlap_mm2"),
                      "min_gap_mm": p.get("min_gap_mm")})

    adj = {}
    for e in edges:
        if e["kind"] == "COAX":
            adj.setdefault(e["a"], set()).add(e["b"])
            adj.setdefault(e["b"], set()).add(e["a"])
    # 隔离台架（源 CAD 停放垃圾位姿）不参与 FASTENS 推断：停放副本与真件在空间上
    # 假重叠，几何推出的"夹持"不是真关系。边照记（证据保留），只从语义层剔除。
    QUARANTINE_OPS = {"OUTLIER-PARKED"}
    quarantined = {oid for oid, n in nodes.items() if n.get("op") in QUARANTINE_OPS}
    fastens = {}
    for oid, n in nodes.items():
        if n["is_fastener"] and oid not in quarantined:
            clamped = sorted(x for x in adj.get(oid, ())
                             if not nodes[x]["is_fastener"] and x not in quarantined)
            if clamped:
                fastens[oid] = clamped

    ops_order = [op["op"] for op in s7["operations"]]
    rank = {o: i for i, o in enumerate(ops_order)}
    cross = []
    for f, cl in fastens.items():
        fo = nodes[f]["op"]
        for c in cl:
            co = nodes[c]["op"]
            if fo and co and rank.get(fo, 9e9) < rank.get(co, 9e9):
                cross.append({"fastener": f, "fastener_op": fo, "clamped": c, "clamped_op": co})

    from collections import Counter
    deg = Counter()
    for e in edges:
        deg[e["a"]] += 1
        deg[e["b"]] += 1
    isolated = [oid for oid in nodes if deg[oid] == 0]

    doc = {
        "$schema": "assembly-ontology.s8-connection-graph/v2",
        "record_id": f"{cfg['case']}-S8-CONNECTION-GRAPH-V001",
        "purpose": "连接关系图：顺序约束、context 选取、右栏局部子图与完整性台账的规范层。",
        "edge_criteria": {"COAX": "max_overlap_mm>=0.5 且 min_radial_gap_mm<=1.0",
                          "CONE": "S2 平面界面图 cone_pairs 全收",
                          "PLANE": "S2 平面界面图 occurrence_pairs 全收",
                          "FASTENS": "紧固件（config 词表）的 COAX 非紧固件邻居即被夹件"},
        "summary": {"nodes": len(nodes), "edges": len(edges),
                    "by_kind": dict(Counter(e["kind"] for e in edges)),
                    "fastener_nodes": sum(1 for n in nodes.values() if n["is_fastener"]),
                    "fasteners_with_clamped": len(fastens),
                    "fasten_links": sum(len(v) for v in fastens.values()),
                    "cross_op_fastening_anomalies": len(cross),
                    "isolated_nodes": len(isolated),
                    "seconds": round(time.time() - t0, 1)},
        "isolated_nodes": [{"occ": o, "product": nodes[o]["product"]} for o in isolated],
        "cross_op_fastening": cross,
        "claim_boundary": [
            "FASTENS 是『词表判紧固件 + COAX 邻接』的推断，不是读取螺纹特征的证明。",
            "隔离台架（OUTLIER-PARKED 停放垃圾位姿）不参与 FASTENS/跨工序异常推断；其边保留为证据。",
            "S2 只覆盖解析面；自由曲面配合不在边集内。",
            "边按最终装配位姿计算，可能混入更晚工步才形成的界面（K-IF-06）。",
            "unit-join 集成工序的动件无 S7 工序记录时 op=null，须在顺序阶段另行闭合。",
        ],
        "nodes": list(nodes.values()),
        "edges": edges,
        "fastens": fastens,
    }
    OUT.write_text(json.dumps(doc, ensure_ascii=False), encoding="utf-8")
    print(f"写出 {OUT}")
    for k, v in doc["summary"].items():
        print(f"  {k}: {v}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
