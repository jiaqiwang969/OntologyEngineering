#!/usr/bin/env python3
"""合并各工序 S7 结果，并按 K-DIR-02 对粗端判据做分域验证后重判方向（移植版）。

与 X1 merge_s7.py 同构：
  ① 工序顺序取 S3 绑定序（不是文件名序）。
  ② 在方向已由几何唯一确定（GEOMETRY_FORCED）的动件上分域验证 THICK_END；
     只有可判吻合率 >= 0.90 的类才启用，其余 THICK_END 判定降级为
     UNDETERMINED_THICK_END_NOT_VALID_FOR_CLASS（方向改取脱离距离短者并如实标注任意）。
  ③ 与第二实现（Blender BVH）的交叉验证：缺席即 status=MISSING，构成 HOLD。

分域词表来自 robot-config 的 part_class_lexicon（英/法文）。

用法：python3 merge_s7_generic.py --case-dir <robot dir>
"""
import argparse
import collections
import glob
import json
import re
from pathlib import Path


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--case-dir", required=True, type=Path)
    ap.add_argument("--out-name", default="S7-扫掠认证.v2.json")
    a = ap.parse_args()
    cfg = json.loads((a.case_dir / "robot-config.v1.json").read_text(encoding="utf-8"))
    P = cfg["paths"]
    RUNS = a.case_dir / P["s7_runs"]
    OUT = a.case_dir / "本体" / a.out_name
    BVH = a.case_dir / "本体/S7-扫掠认证-bvh.v1.json"
    THICK_MIN_DIFF = 0.15

    lex = cfg["part_class_lexicon"]
    FAST = [re.compile(p) for p in lex["fastener_pin_patterns"]]
    SHELL = [re.compile(p) for p in lex["shell_cover_patterns"]]

    def cls(p):
        if any(r.search(p) for r in FAST):
            return "紧固件/销"
        if any(r.search(p) for r in SHELL):
            return "壳/盖"
        return "结构/其他"

    bind = json.loads((a.case_dir / P["s3_binding"]).read_text(encoding="utf-8"))
    sop_order = {o["op"]: i for i, o in enumerate(bind["operations"])}
    ops = [json.loads(Path(f).read_text(encoding="utf-8"))
           for f in glob.glob(str(RUNS / "*.json"))]
    missing = [o["op"] for o in ops if o["op"] not in sop_order]
    if missing:
        raise SystemExit(f"这些工序不在绑定顺序里，拒绝出结果：{missing}")
    ops.sort(key=lambda o: sop_order[o["op"]])
    rows = [(o["op"], m) for o in ops for m in o.get("per_mover", [])]

    table = {}
    for k in ("紧固件/销", "壳/盖", "结构/其他"):
        agree = dis = tie = 0
        for _, m in rows:
            if m.get("direction_rule") != "GEOMETRY_FORCED" or cls(m["product"]) != k:
                continue
            rlo, rhi = m["end_radius_lo_mm"], m["end_radius_hi_mm"]
            if abs(rhi - rlo) < THICK_MIN_DIFF:
                tie += 1
            elif (1 if rhi > rlo else -1) == m["withdraw_sense"]:
                agree += 1
            else:
                dis += 1
        table[k] = {"符合": agree, "不符": dis, "两端等粗": tie,
                    "可判吻合率": (round(agree / (agree + dis), 4) if agree + dis else None)}
    ENABLED = [k for k, v in table.items()
               if v["可判吻合率"] is not None and v["可判吻合率"] >= 0.90]

    changed = 0
    for _, m in rows:
        if m.get("direction_rule") != "THICK_END":
            continue
        if cls(m["product"]) in ENABLED:
            continue
        a1 = m["release_mm_by_sense"]["+1"]
        a2 = m["release_mm_by_sense"]["-1"]
        m["direction_rule"] = "UNDETERMINED_THICK_END_NOT_VALID_FOR_CLASS"
        m["withdraw_sense"] = 1 if (a1 is not None and (a2 is None or a1 <= a2)) else -1
        changed += 1

    for _, m in rows:
        m["part_class"] = cls(m["product"])

    xv = {"status": "MISSING",
          "hold": "K-GOV-02 双实现交叉验证未执行；第二实现（Blender BVHTree）结果落到 "
                  "本体/S7-扫掠认证-bvh.v1.json 后重跑本合并"}
    if BVH.exists():
        bl = json.loads(BVH.read_text(encoding="utf-8"))
        B = {m["occ"]: m for op in bl["operations"] for m in op["per_mover"]}
        F = {m["occ"]: m for _, m in rows}
        common = sorted(set(B) & set(F))
        dv = [o for o in common if F[o].get("verdict") != B[o].get("verdict")]
        dk = [o for o in common if F[o].get("klass") != B[o].get("klass")]
        da = [{"occ": o, "fcl": F[o].get("certified_approach_mm"),
               "bvh": B[o].get("certified_approach_mm")}
              for o in common
              if F[o].get("certified_approach_mm") is not None
              and B[o].get("certified_approach_mm") is not None
              and abs(F[o]["certified_approach_mm"] - B[o]["certified_approach_mm"]) > 0.5]
        xv = {"status": "DONE", "common_movers": len(common),
              "verdict_agree": len(common) - len(dv), "class_agree": len(common) - len(dk),
              "approach_diff_gt_0p5mm": len(da), "diff_samples": da[:8]}

    allm = [m for _, m in rows]
    doc = {
        "$schema": "assembly-ontology.s7-sweep-certification/v2",
        "record_id": f"{cfg['case']}-S7-SWEEP-MERGED-V001",
        "purpose": "S7 扫掠认证的权威合并版：分域验证粗端判据后重判方向；交叉验证状态如实落盘。",
        "operation_order": "按 S3 绑定工序顺序（不是文件名顺序）",
        "thick_end_domain_validation": {
            "method": "在 GEOMETRY_FORCED 动件上看粗端判据是否给出同一方向",
            "per_class": table, "enabled_classes": ENABLED,
            "threshold_to_enable": 0.90, "downgraded_movers": changed,
        },
        "cross_validation": xv,
        "summary": {
            "工序": len(ops), "动件": len(allm),
            "脱离": sum(1 for m in allm if m.get("verdict") == "RELEASES"),
            "未脱离": sum(1 for m in allm if m.get("verdict") == "NO_RELEASE"),
            "无共轴界面": sum(1 for m in allm if m.get("klass") == "E_无共轴界面"),
            "分类": dict(collections.Counter(m.get("klass") for m in allm)),
            "零件类别": dict(collections.Counter(m.get("part_class") for m in allm)),
            "方向判据": dict(collections.Counter(m.get("direction_rule") for m in allm)),
        },
        "claim_boundary": [
            "静止集取保守口径：通过即真通过；报阻挡不等于真阻挡（K-PATH-05）。",
            "运动模型只有沿轴单自由度平移；拧入、边转边落、相位对准都不覆盖。",
            "碰撞用显示级三角网格，不是精确 B-Rep；间隙数值不得由此得出。",
            "被降级 UNDETERMINED 的动件方向是任意选的，不得声称已确定。",
            "交叉验证 MISSING 时本文件不满足 K-GOV-02，处于 HOLD。",
        ],
        "operations": [{"op": o["op"], "movers": o.get("movers", 0),
                        "released": sum(1 for m in o.get("per_mover", [])
                                        if m.get("verdict") == "RELEASES"),
                        "per_mover": o.get("per_mover", [])} for o in ops],
    }
    OUT.write_text(json.dumps(doc, ensure_ascii=False), encoding="utf-8")
    print(f"写出 {OUT}")
    print("  分域验证:", json.dumps(table, ensure_ascii=False))
    print("  启用类别:", ENABLED, " 降级动件:", changed)
    print("  交叉验证:", xv.get("status"))
    for k, v in doc["summary"].items():
        print(f"  {k}: {v}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
