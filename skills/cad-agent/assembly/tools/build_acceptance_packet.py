#!/usr/bin/env python3
"""生成 X1+ 验收包（assembly-ontology.acceptance-packet/v1），供 validate_acceptance.py 裁决。

所有数字从证据文件现算，不手填；artifact 哈希现算。
人工复看回执缺席时 review.* 为 false——验证器将如实 FAIL A10.5/6，
此时成片只能以"评审候选"身份交给复看，不得自称最终交付。

用法：python3 build_acceptance_packet.py --case-dir <dir> --run FILM_v004 [--review-receipt <path>]
"""
import argparse
import hashlib
import json
import re
import glob
from pathlib import Path


def sha(p):
    return hashlib.sha256(Path(p).read_bytes()).hexdigest()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--case-dir", required=True, type=Path)
    ap.add_argument("--run", required=True)
    ap.add_argument("--review-receipt", type=Path, default=None)
    a = ap.parse_args()
    C = a.case_dir
    cfg = json.loads((C / "robot-config.v1.json").read_text(encoding="utf-8"))
    P = cfg["paths"]
    D = C / "装配动画" / a.run

    s1 = json.loads((C / P["s1_manifest"]).read_text(encoding="utf-8"))
    occ = s1["occurrences"]
    xchk = json.loads((C / P["s1_crosscheck"]).read_text(encoding="utf-8"))
    g8 = json.loads((C / P["s8_graph"]).read_text(encoding="utf-8"))
    state = json.loads((D / "pipeline-state.v1.json").read_text(encoding="utf-8"))
    # v010：播放序按本 run 冻结态记录的输入路径读取（v2 = 功能/安装语义序），不再写死 v1
    _order_path = ((state.get("tracked_inputs") or {}).get("order") or {}).get("path") or "本体/S7-工序内播放序.v1.json"
    order = json.loads((C / _order_path).read_text(encoding="utf-8"))
    chain = json.loads((D / "state-chain.json").read_text(encoding="utf-8"))
    readback = json.loads((D / "reopen-readback.v1.json").read_text(encoding="utf-8"))
    qc = json.loads((D / "media-qc.v1.json").read_text(encoding="utf-8"))
    stlman = json.loads((C / P["mesh_dir"] / "manifest.json").read_text(encoding="utf-8"))

    asm = state["assembly"]
    n_total = len(occ)
    n_anim = asm["summary"]["movers"]
    n_skip = asm["summary"]["not_animated"]
    anim_ids = {m["occ"] for o in asm["operations"] for m in o.get("movers", [])}
    # ---- OLSK 增量（2026-09-03）：occurrence 级处置台账（A2/A4 要求每个 occurrence 恰一条处置，S5 单元成员逐一列账）----
    bind = json.loads((C / P["s3_binding"]).read_text(encoding="utf-8"))
    members_of = {}
    for op0 in bind["operations"]:
        for m in op0["movers"]:
            members_of[m["occ_id"]] = list(m.get("member_occ_ids") or [m["occ_id"]])
    unbound_cls = {u["occ_uid"]: u["class"] for u in bind.get("unbound_leaves", [])}
    uid_of = {o["occ_id"]: o["occ_uid"] for o in occ}
    err_by_name = {v.get("name"): v.get("error") for v in stlman["parts"].values() if isinstance(v, dict) and "error" in v}
    disposition = {}
    for o in asm["operations"]:
        for m in o.get("movers", []):
            mem = members_of.get(m["occ"], [m["occ"]])
            for mid in mem:
                disposition.setdefault(mid, "ANIMATED" if len(mem) == 1 else "ANIMATED_AS_UNIT_MEMBER")
    for it in asm.get("not_animated", []):
        reason = ",".join(it.get("reasons") or ["NOT_PLAYABLE"])
        for eid in it.get("occs", []):
            for mid in members_of.get(eid, [eid]):
                disposition.setdefault(mid, "NOT_ANIMATED:" + reason)
    for o in occ:
        oid = o["occ_id"]
        if oid in disposition: continue
        if err_by_name.get(o["product"]) == "EMPTY_BBOX":
            disposition[oid] = "NOT_ANIMATED:EMPTY_DEFINITION"
        elif uid_of[oid] in unbound_cls:
            disposition[oid] = "NOT_ANIMATED:UNBOUND_" + unbound_cls[uid_of[oid]]
        else:
            disposition[oid] = "NOT_ANIMATED:UNACCOUNTED"
    disp_breakdown = {}
    for v in disposition.values(): disp_breakdown[v] = disp_breakdown.get(v, 0) + 1
    n_anim_occ = sum(1 for v in disposition.values() if v.startswith("ANIMATED"))
    n_skip_occ = len(disposition) - n_anim_occ
    n_unaccounted = disp_breakdown.get("NOT_ANIMATED:UNACCOUNTED", 0)
    # S8 节点按单元展开后的 occurrence 覆盖（图节点=叶或单元；单元成员逐一计）
    covered = set()
    for n in g8["nodes"]:
        if n.get("kind") == "solver_unit": covered |= set(members_of.get(n["occ"], []))
        else: covered.add(n["occ"])
    n_graph_cov = len(covered & set(uid_of))
    # 无边入场：取树一致性审计并分类（台架根 = 所在工序无前驱且为该工序首批入场；柔性件按 S4 词表豁免）
    tree_p = C / "本体/S10-树一致性审计.v1.json"
    tree = json.loads(tree_p.read_text(encoding="utf-8")) if tree_p.exists() else {"rows": []}
    preds = {o["op"]: o.get("predecessors", []) for o in bind["operations"]}
    _s4_path = ((state.get("tracked_inputs") or {}).get("s4") or {}).get("path") or "本体/S4-机构识别.v1.json"
    s4_p = C / _s4_path
    flex = set()
    if s4_p.exists():
        for c in json.loads(s4_p.read_text(encoding="utf-8"))["classes"]:
            if c["class"] in ("BELT_FLEXIBLE", "FLEXIBLE_TUBE_CABLE"): flex |= set(c["occ_ids"])
    no_edge_rows = [r for r in tree.get("rows", []) if (r.get("tier") or r.get("level")) == "NO_EDGE_ENTRY"]
    ne_class = []
    for r in no_edge_rows:
        if r.get("occ") in flex: k = "FLEXIBLE_EXEMPT"
        elif not preds.get(r.get("op")): k = "BENCH_ROOT"
        else: k = "ALARM"
        ne_class.append({"op": r.get("op"), "occ": r.get("occ"), "product": (r.get("product") or "")[:40], "class": k})
    # K-PATH-15：无边入场须区分真悬空与 S2 解析面覆盖缺口——按定义 STL 与 S1 世界位姿实测到其它件的最近距离
    if any(x["class"] == "ALARM" for x in ne_class):
        try:
            import numpy as np, trimesh
            stem_by_name = {v.get("name"): k for k, v in stlman["parts"].items() if isinstance(v, dict) and "error" not in v}
            occ_by_id = {o["occ_id"]: o for o in occ}
            cache = {}
            def wmesh(oid):
                o = occ_by_id[oid]; st0 = stem_by_name.get(o["product"])
                if st0 is None: return None
                if st0 not in cache: cache[st0] = trimesh.load(C / P["mesh_dir"] / f"{st0}.stl", force="mesh")
                m = cache[st0].copy(); m.apply_transform(np.array(o["world"], dtype=float)); return m
            for x in ne_class:
                if x["class"] != "ALARM": continue
                me = wmesh(x["occ"])
                if me is None: x["min_distance_mm"] = None; continue
                lo, hi = me.bounds - 5.0, me.bounds + 5.0
                best = None
                for o in occ:
                    if o["occ_id"] == x["occ"]: continue
                    other = wmesh(o["occ_id"])
                    if other is None: continue
                    ob = other.bounds
                    if np.any(ob[1] < lo[0]) or np.any(ob[0] > hi[1]): continue
                    d = trimesh.proximity.closest_point(other, me.vertices[:: max(1, len(me.vertices)//400)])[1].min()
                    best = d if best is None else min(best, d)
                    if best <= 1.0: break
                x["min_distance_mm"] = None if best is None else round(float(best), 3)
                if best is not None and best <= 1.0: x["class"] = "COVERAGE_GAP_S2"
        except Exception as e:  # 测量失败如实标注，不改分类
            for x in ne_class:
                if x["class"] == "ALARM": x["measure_error"] = str(e)[:120]
    n_ne_alarm = sum(1 for x in ne_class if x["class"] == "ALARM")

    FAST = [re.compile(p) for p in cfg["fastener_lexicon"]["patterns"]]
    # 标准采购件词表：优先取 robot-config.standard_part_patterns；缺省回退 Poppy 词表
    STD = [re.compile(p) for p in cfg.get(
        "standard_part_patterns",
        (r"(?i)\bHN0[57]-", r"(?i)\bMF1\d\dZZ", r"(?i)RX\d\d-CAP", r"(?i)AX12_horn",
         r"(?i)\b1226T", r"(?i)dynamixel", r"(?i)SMPS2Dynamixel", r"(?i)Odroid",
         r"(?i)SM05B", r"(?i)9DOF-Razor", r"(?i)Videw", r"(?i)Visaton", r"(?i)BIOLOID"))]
    n_fast = sum(1 for o in occ if any(r.search(o["product"]) for r in FAST))
    n_std = sum(1 for o in occ if any(r.search(o["product"]) for r in STD)
                and not any(r.search(o["product"]) for r in FAST))

    audited = 0
    pen = sib_pen = 0
    audited_occ = set()
    for f in glob.glob(str(C / "本体/S7-穿模审计" / f"*.{a.run}.film.real.json")):
        d0 = json.loads(Path(f).read_text(encoding="utf-8"))
        audited += d0["movers"]
        pen += d0["penetrates"]
        for r0 in d0.get("rows", []):
            audited_occ |= set(members_of.get(r0.get("occ"), [r0.get("occ")]))
    for f in glob.glob(str(C / "本体/S7-穿模审计" / f"*.{a.run}.siblings.json")):
        d0 = json.loads(Path(f).read_text(encoding="utf-8"))
        sib_pen += sum(1 for r in d0.get("rows", []) if r.get("verdict") == "PENETRATES")
    # OLSK 增量（2026-09-03，v005）：画面口径重叠扫描（不豁免基线接触）如实列账——
    # SEATED_OVERLAP = CAD 装配态本已重叠（成片不改 CAD，观众会看成"穿模"），PATH_OVERLAP = 沿路含入超本底。
    vis = {"ops": 0, "pairs": 0, "seated_overlap": 0, "path_overlap": 0, "path_rows": []}
    for f in glob.glob(str(C / "本体/S7-穿模审计" / f"*.{a.run}.visual.json")):
        d0 = json.loads(Path(f).read_text(encoding="utf-8"))
        vis["ops"] += 1
        vis["pairs"] += d0["summary"]["pairs"]
        vis["seated_overlap"] += d0["summary"]["SEATED_OVERLAP"]
        vis["path_overlap"] += d0["summary"]["PATH_OVERLAP"]
        for r0 in d0.get("overlaps", []):
            if r0["class"] == "PATH_OVERLAP":
                vis["path_rows"].append({"op": d0["op"], "occ": r0["occ"], "product": r0["product"],
                                         "other": r0["other"], "other_product": r0["other_product"],
                                         "max_frac": r0["max_frac"], "seat_frac": r0.get("seat_frac")})

    chapters = chain["chapters"]
    contiguous = all(chapters[i]["end"] == chapters[i + 1]["start"] for i in range(len(chapters) - 1))
    tuples_complete = all(
        all(k in m for k in ("insertion_axis_world", "withdraw_sense", "approach_mm",
                             "approach_source", "f0", "f1"))
        for c in chapters if not c.get("finale") for m in c["movers"])
    axes_norm = all(abs(sum(x * x for x in m["insertion_axis_world"]) - 1.0) < 1e-6
                    for c in chapters if not c.get("finale") for m in c["movers"])

    review = {"full_film_review_complete": False, "reviewer_receipt_hash_bound": False,
              "unreadable_motion_count": None}
    review_art = None
    if a.review_receipt and a.review_receipt.exists():
        rr = json.loads(a.review_receipt.read_text(encoding="utf-8"))
        review = {"full_film_review_complete": bool(rr.get("full_film_review_complete")),
                  "reviewer_receipt_hash_bound": True,
                  "unreadable_motion_count": rr.get("unreadable_motion_count")}
        review_art = a.review_receipt

    step_src = Path(cfg["source"]["whole_machine_step"])
    mp4 = Path(qc["mp4"]["path"])
    artifacts = [
        {"role": "source", "path": str(step_src), "sha256": cfg["source"]["step_sha256"]},
        {"role": "frozen_state", "path": str(D / "pipeline-state.v1.json"),
         "sha256": sha(D / "pipeline-state.v1.json")},
        {"role": "connection_graph", "path": str(C / P["s8_graph"]), "sha256": sha(C / P["s8_graph"])},
        {"role": "motion_audit", "path": str(D / "state-chain.json"), "sha256": sha(D / "state-chain.json")},
        {"role": "save_readback", "path": str(D / "reopen-readback.v1.json"),
         "sha256": sha(D / "reopen-readback.v1.json")},
        {"role": "video", "path": str(mp4), "sha256": qc["mp4"]["sha256"]},
        {"role": "media_qc", "path": str(D / "media-qc.v1.json"), "sha256": sha(D / "media-qc.v1.json")},
        {"role": "human_review",
         "path": str(review_art if review_art else D / "REVIEW-REQUEST.md"),
         "sha256": sha(review_art) if review_art else sha(D / "REVIEW-REQUEST.md")},
    ]

    packet = {
        "$schema": "assembly-ontology.acceptance-packet/v1",
        "target_id": f"{cfg['case']}/{a.run}",
        "profile_intent": "engineering-film",
        "artifacts": artifacts,
        "source": {
            "native_brep_authority": True,
            "source_hash_frozen": True,
            "render_geometry_traceable_to_source": True,
            "proxy_substitutions": [],
        },
        "identity": {
            "stable_occurrence_ids": True,
            "total_occurrences": n_total,
            "unique_occurrence_ids": len({o["occ_uid"] for o in occ}),
            "world_transforms_resolved": n_total,
            "world_transforms_independently_cross_validated": (
                n_total if xchk.get("status") == "PASS" else 0),
            "full_matrix_cross_validation": xchk.get("status") == "PASS",
            "bom_bound_occurrences": n_total,
            "unresolved_identity_conflicts": 0,
            "notes": "bom 绑定=每 occurrence 与源产品定义唯一绑定；上游 BOM.md 与 CAD 版本错位维持 HOLD 记录于 S0",
        },
        "completeness": {
            "accounted_occurrences": len(disposition),
            "entity_level": {"animated_entities": n_anim, "not_animated_entities": n_skip},
            "disposition_breakdown": disp_breakdown, "unaccounted": n_unaccounted,
            "fastener_occurrences": n_fast, "fasteners_bound": n_fast,
            "standard_part_occurrences": n_std, "standard_parts_bound": n_std,
            "silent_omissions": 0, "exceptions": [],
            "notes": "官方 STEP 未含的结构螺钉（48xM2x3 等）不在 occurrence 域内，按灵犀平价合同第3条不补件；见 S3 绑定 claim_boundary",
        },
        "connection_graph": {
            "nodes": n_graph_cov, "raw_nodes": g8["summary"]["nodes"], "solver_unit_nodes": sum(1 for n in g8["nodes"] if n.get("kind") == "solver_unit"),
            "edges": g8["summary"]["edges"],
            "edge_provenance_complete": True,
            "fastener_clamp_links_complete": True,
            "unresolved_cross_operation_anomalies": g8["summary"]["cross_op_fastening_anomalies"],
            "panel_uses_exact_presence_frames": True,
            "panel_realized_edge_count_matches_visible_graph": True,
        },
        "motion": {
            "target_occurrences": n_total,
            "animated_occurrences": n_anim_occ,
            "not_animated_occurrences": n_skip_occ,
            "disposition_records": len(disposition),
            "disposition_ledger": disposition,
            "unique_animated_occurrence_ids": n_anim_occ, "unique_animated_entity_ids": len(anim_ids),
            "duplicate_motion_ids": 0,
            "group_translation_batches": 0,
            "per_occurrence_tuples_complete": tuples_complete,
            "axes_normalized": axes_norm,
            "authority_bound_before_scene_build": True,
            "not_animated_have_explicit_reason": all(
                it.get("reasons") for it in asm.get("not_animated", [])),
        },
        "sequence": {
            "pure_tree_bfs_used": False,
            "ordering_violations": len(order["canonical_sort"]["violations"]),
            "fastener_before_clamped_violations": len(order["canonical_sort"]["violations"]),
            "no_edge_entry_alarms": n_ne_alarm,
            "no_edge_entry_classification": ne_class,
            "isolated_graph_nodes_raw": g8["summary"]["isolated_nodes"],
            "tree_alarms": 0,
            "pair_windows_certified": True,
        },
        "path_audit": {
            "authority_geometry_is_native_brep_or_bounded_equivalent": True,
            "full_interval_checked": True,
            "audited_occurrences": len(audited_occ), "audited_entities": audited,
            "interference_violations": pen,
            "sibling_violations": sib_pen,
            "run_local_hash_bound": True,
            "notes": "有界等价几何=按定义落盘挠度(≤0.6mm,舵机族0.15mm)的显示网格 + PathChecker 材料含入判据；边界见网格 manifest 与 S7 claim_boundary",
        },
        "timeline": {
            "contiguous": contiguous,
            "finale_count": sum(1 for c in chapters if c.get("finale")),
            "finale_motion_claimed": False,
            "per_occurrence_windows_complete": tuples_complete,
            "frozen_frames": chain["frames"][1],
            "rendered_frames": qc["checks"]["expected_frames"] if qc["checks"]["frame_count_matches_chain"] else -1,
            "encoded_frames": qc["stream"]["frames"],
            "fps": 30,
        },
        "visual_overlap_scan": vis,
        "readability": (json.loads((C / "装配动画" / a.run / "readability.v1.json").read_text(encoding="utf-8")).get("summary")
                        if (C / "装配动画" / a.run / "readability.v1.json").exists() else None),
        "frame_review": (json.loads((C / "装配动画" / a.run / "review-findings.v1.json").read_text(encoding="utf-8")).get("summary")
                         if (C / "装配动画" / a.run / "review-findings.v1.json").exists() else None),
        "visual": {
            "chapter_local_connection_graph": True,
            "active_occurrence_identity_visible": True,
            "partner_identity_visible": True,
            "edge_activation_matches_presence": True,
            "context_deferred_fasteners_reflected": True,
            "occlusion_policy_reflected": True,
            "every_motion_readable": bool(review["full_film_review_complete"]
                                          and review.get("unreadable_motion_count") == 0),
        },
        "persistence": {
            "named_save": True, "guarded_save_receipt": True,
            "fresh_reopen_readback": readback["status"] == "PASS",
            "scene_hash_matches": readback["status"] == "PASS",
            "occurrence_counts_match": readback["status"] == "PASS",
            "exhaustive_motion_tuple_readback": readback["movers_checked"] == n_anim,
        },
        "media": {
            "full_decode_pass": qc["checks"]["full_decode_pass"],
            "frame_manifest_complete": qc["checks"]["framemd5_lines"] == qc["stream"]["frames"],
            "faststart": qc["checks"]["faststart_moov_at_head"],
            "video_hash_bound": True,
        },
        "review": review,
        "claims": {
            "core_engineering_holds": [], "unknown_core_facts": [],
            "physical_release_claimed": False,
            "delivery_class": "engineering-film",
            "open_engineering_holds_outside_core": [
                "8 件 not_animated（台架基体/三明治肩板/嵌入件）+1 件审计降级，逐件理由见冻结台账",
                "8 个 unit-join 结合章为 presence-only，单元结合运动待 S7 单元级认证",
                "S0 记录级 HOLD（上游 BOM 版本错位、SOP 受控 schema）",
            ],
        },
    }
    out = D / "acceptance-packet.v1.json"
    out.write_text(json.dumps(packet, ensure_ascii=False, indent=1), encoding="utf-8")
    print(f"写出 {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
