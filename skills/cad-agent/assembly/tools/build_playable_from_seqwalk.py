#!/usr/bin/env python3
"""把序列感知逆向行走结果转成可播接近距离表（重建流版）。

X1 的可播表由『穿模审计 + 轴选取验证』合成，且审计依赖既有 state-chain；
重建流里权威运动来源是逆向拆卸行走本身——每件在真实剩余集里、用与穿模审计
同一把量具（PathChecker）量出的最长只有基线接触连续段。因此：

    可播接近距离 = min(cap, 行走 best.clear_run_mm)
    轴/方向     = 行走 best（同一份记录，不跨文件拼字段）
    可播        = 距离 >= min 且 verdict != STUCK

STUCK 与 < 下限的件如实进不可播行，等 freeze 时归入 not_animated 台账。
成片冻结后仍须按链跑穿模审计（film 口径）回验——本表不是终局验收。

用法：python3 build_playable_from_seqwalk.py --case-dir <robot dir> [--min 1.0] [--cap 20]
"""
import argparse
import glob
import json
from pathlib import Path


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--case-dir", required=True, type=Path)
    ap.add_argument("--min", type=float, default=1.0)
    ap.add_argument("--cap", type=float, default=20.0)
    # OLSK 增量（2026-09-03，v005）：--seq-dir 指向延长行程的行走记录目录；--out 另存可播表（不覆盖 v004 的 v1 表）。
    ap.add_argument("--seq-dir", default=None)
    ap.add_argument("--out", default=None)
    # K-PATH-18（2026-09-03）：画面口径 PATH_OVERLAP 回灌为干净前缀；可播行程低于 --min-visible 的不作动画（静态出场），
    # 2 mm 的"弹入"既不可读也掩盖穿模。默认 0 = 关闭（v004 行为不变）。
    ap.add_argument("--min-visible", type=float, default=0.0)
    ap.add_argument("--s4", default=None, help="机构识别记录（v2 = 修正柔性类词表）")
    a = ap.parse_args()
    cfg = json.loads((a.case_dir / "robot-config.v1.json").read_text(encoding="utf-8"))
    P = cfg["paths"]
    SEQ = (a.case_dir / a.seq_dir) if a.seq_dir else (a.case_dir / P["s7_seq_cert"])
    OUT = (a.case_dir / a.out) if a.out else (a.case_dir / "本体/S7-可播接近距离.v1.json")
    s7 = json.loads((a.case_dir / "本体/S7-扫掠认证.v2.json").read_text(encoding="utf-8"))
    kls = {m["occ"]: m for op in s7["operations"] for m in op["per_mover"]}
    bind = json.loads((a.case_dir / P["s3_binding"]).read_text(encoding="utf-8"))
    sop_rank = {o["op"]: i for i, o in enumerate(bind["operations"])}

    # film 审计回灌（X1 TRIMMED_TO_FIRST_NON_BASELINE 规则）：
    # 审计报 PENETRATES 的动件，行程收短到首个非基线接触前一步；收短后 < 下限则降级不可播。
    audit_trim = {}
    ad = a.case_dir / "本体/S7-穿模审计"
    if ad.exists():
        runs = sorted({Path(f).name.split(".")[1] for f in glob.glob(str(ad / "*.film.real.json"))})
        # OLSK 增量（2026-09-03）：审计回灌取**全部历史 run** 的并集（同一动件取最短收短），不只取最新 run。
        # 只取最新 run 会振荡：v002 按 v001 审计收短了 4 件，v003 按 v002 审计收短了另外 2 件，却把 v001 的 4 件放回去。
        # 收短是保守方向（K-PATH-02 可播=只有基线接触的连续段，更短仍是认证区间），并集不引入未认证运动。
        for latest in runs:
            state_p = a.case_dir / "装配动画" / latest / "pipeline-state.v1.json"
            stuck_occ = set()
            if state_p.exists():
                asm0 = json.loads(state_p.read_text(encoding="utf-8"))["assembly"]
                for it in asm0.get("not_animated", []):
                    stuck_occ.update(it.get("occs", []))
            for f in glob.glob(str(ad / f"*.{latest}.film.real.json")):
                d0 = json.loads(Path(f).read_text(encoding="utf-8"))
                step0 = float(d0.get("step_mm", 0.1))
                for r0 in d0.get("rows", []):
                    if r0.get("verdict") != "PENETRATES" or r0.get("first_non_baseline_at_mm") is None:
                        continue
                    blockers = set(r0.get("non_baseline_occ", []))
                    # 只被"卡死件"挡住的不收短：卡死件已改为本章动件全部就位后才出现，
                    # 动件路径期间它不在画面上（构建器 static_withdrawn_appear_frame 语义）。
                    if blockers and blockers <= stuck_occ:
                        continue
                    v_trim = max(0.0, float(r0["first_non_baseline_at_mm"]) - step0)
                    audit_trim[r0["occ"]] = min(audit_trim.get(r0["occ"], v_trim), v_trim)

    # OLSK 增量：S4 柔性类（同步带/管线/拖链）不作刚体滑入动画（K-ID-04 / K-DEL-03），如实进不可播行
    flex = set()
    s4p = a.case_dir / (a.s4 or "本体/S4-机构识别.v1.json")
    if s4p.exists():
        for c in json.loads(s4p.read_text(encoding="utf-8"))["classes"]:
            if c["class"] in ("BELT_FLEXIBLE", "FLEXIBLE_TUBE_CABLE"):
                flex |= set(c["occ_ids"])
    # 画面口径扫描回灌（K-GOV-15 独立口径 → K-PATH-18）：全部历史 run 的 PATH_OVERLAP 取最短干净前缀
    visual_trim = {}
    if ad.exists():
        for f in glob.glob(str(ad / "*.visual.json")):
            dv = json.loads(Path(f).read_text(encoding="utf-8"))
            for r0 in dv.get("overlaps", []):
                if r0.get("class") != "PATH_OVERLAP" or r0.get("clean_prefix_mm") is None:
                    continue
                v = float(r0["clean_prefix_mm"])
                visual_trim[r0["occ"]] = min(visual_trim.get(r0["occ"], v), v)
    rows = []
    files = sorted(glob.glob(str(SEQ / "*.json")),
                   key=lambda f: sop_rank.get(Path(f).stem, 9e9))
    for f in files:
        d = json.loads(Path(f).read_text(encoding="utf-8"))
        for r in d.get("rows", []):
            best = r.get("best") or {}
            run = float(best.get("clear_run_mm") or 0.0)
            playable = min(a.cap, run)
            trimmed = False
            if r["occ"] in audit_trim and audit_trim[r["occ"]] < playable:
                playable = audit_trim[r["occ"]]
                trimmed = True
            vtrimmed = False
            if r["occ"] in visual_trim and visual_trim[r["occ"]] < playable:
                playable = visual_trim[r["occ"]]
                trimmed = True
                vtrimmed = True
            is_flex = r["occ"] in flex
            below_visible = a.min_visible > 0 and playable < a.min_visible
            ok = r.get("verdict") != "STUCK" and playable >= a.min and not is_flex and not below_visible
            info = kls.get(r["occ"], {})
            rows.append({
                "op": d["op"], "occ": r["occ"], "product": r["product"],
                "klass": info.get("klass"), "direction_rule": info.get("direction_rule"),
                "insertion_axis_world": best.get("dir"),
                "withdraw_sense": best.get("sense"),
                "walk_clear_run_mm": run,
                "playable_approach_mm": round(playable, 3),
                "source": ("SEQUENCE_AWARE_REVERSE_WALK+TRIMMED_TO_VISUAL_CLEAN_PREFIX" if vtrimmed else
                           "SEQUENCE_AWARE_REVERSE_WALK+TRIMMED_TO_FIRST_NON_BASELINE"
                           if trimmed else "SEQUENCE_AWARE_REVERSE_WALK"),
                "playable": bool(ok and best.get("dir")),
                "why_not": None if ok else (
                    "FLEXIBLE_CLASS_NOT_RIGID_ANIMATED" if is_flex else
                    "STUCK_IN_TRUE_REMAINING_SET" if r.get("verdict") == "STUCK"
                    else f"BELOW_MIN_VISIBLE_MM(K-PATH-18): 可播 {playable:.2f}mm < {a.min_visible}mm" if below_visible
                    else f"可播 {playable:.2f}mm < 下限 {a.min}mm"
                         + ("（审计收短）" if trimmed else "")),
            })

    play = [r for r in rows if r["playable"]]
    doc = {
        "$schema": "assembly-ontology.playable-approach/v2",
        "record_id": f"{cfg['case']}-PLAYABLE-FROM-SEQWALK-V001",
        "purpose": "逆向拆卸行走导出的可播接近距离；轴/方向/行程整组来自同一份行走记录。",
        "rule": "可播 = min(cap, 行走最长只有基线接触连续段)；STUCK 与 <下限 如实不可播",
        "params": {"s4": str(a.s4 or "本体/S4-机构识别.v1.json"), "min_playable_mm": a.min, "cap_mm": a.cap, "min_visible_mm": a.min_visible,
                   "walk_source": str(SEQ), "visual_trims": len(visual_trim)},
        "summary": {
            "行走动件": len(rows),
            "可播": len(play),
            "不可播": len(rows) - len(play),
            "STUCK": sum(1 for r in rows if r["why_not"] == "STUCK_IN_TRUE_REMAINING_SET"),
        },
        "claim_boundary": [
            "行走场景 = 真实剩余集（后续工序动件与已移出同工序动件退场）；这是成片口径。",
            "碰撞用显示级三角网格；间隙数值不得由此得出。",
            "运动模型只有沿轴单自由度平移；拧入、边转边落不覆盖。",
            "冻结成链后仍须按链跑 film 口径穿模审计回验，本表不是终局验收。",
        ],
        "rows": rows,
    }
    OUT.write_text(json.dumps(doc, ensure_ascii=False), encoding="utf-8")
    print(f"写出 {OUT}")
    for k, v in doc["summary"].items():
        print(f"  {k}: {v}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
