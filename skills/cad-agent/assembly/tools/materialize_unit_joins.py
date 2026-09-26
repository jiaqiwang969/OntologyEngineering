#!/usr/bin/env python3
"""把单元结合认证落成可冻结的动件（K-SCN 台架语义）：

  S3 v1 → S3 v2：每个结合工序前插入一个 kind=unit 的动件 `unit:BENCH-xx`（member_occ_ids = 组成员）
  S7-扫掠认证 v2 → v3：对应 per_mover 行（axis/sense/certified_approach）
  可播表（build_playable 输出）→ 追加 unit 记录（playable, approach = min(clear_run, --cap)）
  工序内播放序 → 结合工序的 order 前插 unit 动件
只写新文件（v2/v3/后缀 -unit），不改原记录。verdict=STUCK 或 clear_run < --min 的单元不落成（保持在场汇合）。
用法：materialize_unit_joins.py --case-dir . --cert 本体/S7-单元结合认证.v1.json --playable 本体/S7-可播接近距离.v5.json \
        --play-order 本体/S7-工序内播放序.v2.json [--cap 300 --min 40]
"""
import argparse, json
from pathlib import Path


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--case-dir", required=True, type=Path)
    ap.add_argument("--cert", default="本体/S7-单元结合认证.v1.json")
    ap.add_argument("--playable", required=True)
    ap.add_argument("--play-order", required=True)
    ap.add_argument("--s3-out", default="本体/S3-工序实例绑定.v2.json")
    ap.add_argument("--s7-out", default="本体/S7-扫掠认证.v3.json")
    ap.add_argument("--cap", type=float, default=300.0)
    ap.add_argument("--min", type=float, default=40.0)
    a = ap.parse_args()
    C = a.case_dir
    cert = json.loads((C / a.cert).read_text(encoding="utf-8"))
    s3 = json.loads((C / "本体/S3-工序实例绑定.v1.json").read_text(encoding="utf-8"))
    s7 = json.loads((C / "本体/S7-扫掠认证.v2.json").read_text(encoding="utf-8"))
    play = json.loads((C / a.playable).read_text(encoding="utf-8"))
    po = json.loads((C / a.play_order).read_text(encoding="utf-8"))
    occs = json.loads((C / json.loads((C / "robot-config.v1.json").read_text(encoding="utf-8"))["paths"]["s1_manifest"]).read_text(encoding="utf-8"))["occurrences"]
    id2uid = {o["occ_id"]: o["occ_uid"] for o in occs}
    s3ops = {o["op"]: o for o in s3["operations"]}
    s7ops = {o["op"]: o for o in s7["operations"]}
    recs = play.setdefault("rows", [])          # build_playable_from_seqwalk 输出：rows 列表（freeze 按 r["occ"] 建索引）
    ledger = []
    PRIORITY = ["+Z", "+X", "-X", "+Y", "-Y", "-Z"]   # 并列时优先"向上退出=从上方落装"（吊装语义），最后才是从下方顶入
    DIRV = {"+X": [1, 0, 0], "-X": [-1, 0, 0], "+Y": [0, 1, 0], "-Y": [0, -1, 0], "+Z": [0, 0, 1], "-Z": [0, 0, -1]}
    for r in cert["rows"]:
        if r.get("per_axis_mm"):
            mx = max(r["per_axis_mm"].values())
            for name in PRIORITY:
                if r["per_axis_mm"].get(name, -1) >= mx - 1e-6:
                    r["best"] = {"dir": DIRV[name], "sense": 1, "clear_run_mm": r["per_axis_mm"][name], "axis_name": name}
                    break
        if r.get("verdict") in (None, "NO_GEOMETRY", "STUCK") or r["best"]["clear_run_mm"] < a.min:
            ledger.append({"op": r["op"], "unit": r["unit"], "materialized": False, "why": r.get("verdict")})
            continue
        uid = f"unit:{r['unit']}"
        op = r["op"]
        approach = round(min(r["best"]["clear_run_mm"], a.cap), 3)
        mover = {"occ_uid": uid, "kind": "unit", "product": f"单元 {r['unit']}（台架预装组整体上机，来自 {'/'.join(r['from_ops'])}）",
                 "product_name": f"Unit {r['unit']}", "depth": 0, "via_glb": False, "via_glb_names": [], "occ_id": uid,
                 "member_occ_ids": r["members"], "unit_join": {"from_ops": r["from_ops"], "cert": a.cert}}
        o3 = s3ops.get(op)
        if o3 is None:
            ledger.append({"op": op, "unit": r["unit"], "materialized": False, "why": "op not in S3"})
            continue
        n_units_before = sum(1 for m in o3["movers"] if str(m["occ_id"]).startswith("unit:"))
        if not any(m["occ_id"] == uid for m in o3["movers"]):
            o3["movers"].insert(n_units_before, mover)          # 多分量/多单元按认证顺序排在工序最前
        pm = {"occ": uid, "product": mover["product"], "kind": "unit", "member_count": len(r["members"]), "klass": "U_单元结合",
              "axis_source": "UNIT_JOIN_CERT", "axis_candidates": 6, "insertion_axis_world": [float(x) for x in r["best"]["dir"]],
              "withdraw_sense": int(r["best"]["sense"]), "direction_rule": "UNIT_JOIN", "certified_approach_mm": r["best"]["clear_run_mm"],
              "verdict": r["verdict"], "part_class": "单元"}
        o7 = s7ops.get(op)
        if o7 is None:
            o7 = {"op": op, "movers": [], "released": [], "per_mover": []}
            s7["operations"].append(o7)
            s7ops[op] = o7
        n7 = sum(1 for m in o7["per_mover"] if str(m["occ"]).startswith("unit:"))
        if not any(m["occ"] == uid for m in o7["per_mover"]):
            o7["per_mover"].insert(n7, pm)
            o7.setdefault("movers", []).insert(n7, uid)
        rec = {"occ": uid, "op": op, "playable": True, "insertion_axis_world": pm["insertion_axis_world"], "withdraw_sense": pm["withdraw_sense"],
               "playable_approach_mm": approach, "certified_approach_mm": r["best"]["clear_run_mm"], "source": "UNIT_JOIN_CERT", "pair_id": None,
               "member_count": len(r["members"])}
        recs[:] = [x for x in recs if x.get("occ") != uid] + [rec]
        play.setdefault("unit_joins", []).append({"occ": uid, "op": op, "cert": a.cert})
        spec = po["ops"].setdefault(op, {"order": []})
        spec["order"] = [e for e in spec["order"] if e.get("occ") != uid]
        npo = sum(1 for e in spec["order"] if str(e.get("occ")).startswith("unit:"))
        spec["order"].insert(npo, {"occ": uid})
        spec.setdefault("unit_joins_first", []).append(uid)
        ledger.append({"op": op, "unit": r["unit"], "materialized": True, "axis": r["best"]["dir"], "approach_mm": approach, "members": len(r["members"])})
    s3["record_id"] = s3.get("record_id", "S3") + "-v2-unit-joins"
    s3["unit_join_materialization"] = {"cert": a.cert, "rule": "结合工序前插 kind=unit 动件；成员=组成员；行程=min(净行程, cap)", "ledger": ledger}
    (C / a.s3_out).write_text(json.dumps(s3, ensure_ascii=False, indent=1), encoding="utf-8")
    s7["record_id"] = s7.get("record_id", "S7") + "-v3-unit-joins"
    (C / a.s7_out).write_text(json.dumps(s7, ensure_ascii=False, indent=1), encoding="utf-8")
    play_out = str(a.playable).replace(".json", "-unit.json")
    (C / play_out).write_text(json.dumps(play, ensure_ascii=False, indent=1), encoding="utf-8")
    po_out = str(a.play_order).replace(".json", "-unit.json")
    (C / po_out).write_text(json.dumps(po, ensure_ascii=False, indent=1), encoding="utf-8")
    print(json.dumps({"materialized": sum(1 for l in ledger if l["materialized"]), "skipped": [l for l in ledger if not l["materialized"]],
                      "s3": a.s3_out, "s7": a.s7_out, "playable": play_out, "play_order": po_out}, ensure_ascii=False))


if __name__ == "__main__":
    main()
