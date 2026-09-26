#!/usr/bin/env python3
"""S4 v2：修正柔性类词表误判（K-KIN，2026-09-05 功能线 B/A 发现）。

v1 的 BELT_FLEXIBLE 用 `HTD|MGT|GT3` 把带轮（Pulley HTD …）当皮带；FLEXIBLE_TUBE_CABLE 用 `cable|wire|duct|pipe|chain`
把铝线槽、拖链、管夹/接头、线夹、感应开关都当柔性件，下游 build_playable 对柔性类一律“不作动画”，于是 WB-17 整章消失、
WB-60/64.4/65.4 的气弹簧/磁铁缺席、带轮从未出场。v2 只把真正的柔性件留在柔性类，其余移入 RIGID_RECLASSIFIED。
用法：reclassify_s4_flexible.py --case-dir . [--in 本体/S4-机构识别.v1.json] [--out 本体/S4-机构识别.v2.json]
"""
import argparse, json, re
from pathlib import Path

RIGID_HINT = re.compile(r"(?i)pulley|duct|attacher|fixer|holder|sensor|gland|clamp|grommet|guide|cover|plate|bracket|support|spacer|"
                        r"cable chain|chain\b|fitting|connector|nut|screw|ring|cap\b|block|mount|channel|box|panel|switch|button|magnet|piston|gas spring|wire way|wire housing|wire cover|way\b")
TRUE_FLEX = re.compile(r"(?i)\bbelt\b|hose|flex|\bpipe\b|\btube\b|tubing|\bwire\b|cable\b(?!\s*(duct|chain|gland|tie|holder|clamp|channel))")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--case-dir", required=True, type=Path)
    ap.add_argument("--in", dest="inp", default="本体/S4-机构识别.v1.json")
    ap.add_argument("--out", default="本体/S4-机构识别.v2.json")
    a = ap.parse_args()
    d = json.loads((a.case_dir / a.inp).read_text(encoding="utf-8"))
    moved = []
    rigid = {"class": "RIGID_RECLASSIFIED", "pattern": "v2：从柔性类移出的刚体件（词表误判）", "count": 0, "with_coax_interface": 0, "products": [], "occ_ids": []}
    for c in d["classes"]:
        if c["class"] not in ("BELT_FLEXIBLE", "FLEXIBLE_TUBE_CABLE"):
            continue
        keep_p, keep_ids = [], []
        # occ_ids 与 products 的对应：按 product 名逐一判定（occ_ids 顺序与 products 展开顺序一致时可分；否则用名字查 S1）
        name_of = {}
        occs = json.loads((a.case_dir / json.loads((a.case_dir / "robot-config.v1.json").read_text(encoding="utf-8"))["paths"]["s1_manifest"]).read_text(encoding="utf-8"))["occurrences"]
        for o in occs:
            name_of[o["occ_id"]] = o["product"]
        for oid in c.get("occ_ids", []):
            nm = name_of.get(oid, "")
            is_rigid = bool(RIGID_HINT.search(nm)) and not (re.search(r"(?i)\bbelt\b|hose|flex", nm))
            if is_rigid or not TRUE_FLEX.search(nm):
                rigid["occ_ids"].append(oid); moved.append((c["class"], oid, nm))
            else:
                keep_ids.append(oid)
        prods = {}
        for oid in keep_ids:
            prods[name_of.get(oid, "")] = prods.get(name_of.get(oid, ""), 0) + 1
        c["occ_ids"] = keep_ids
        c["products"] = sorted(prods.items(), key=lambda t: -t[1])
        c["count"] = len(keep_ids)
    prods = {}
    for _, oid, nm in moved:
        prods[nm] = prods.get(nm, 0) + 1
    rigid["products"] = sorted(prods.items(), key=lambda t: -t[1]); rigid["count"] = len(rigid["occ_ids"])
    d["classes"].append(rigid)
    d["summary"] = {c["class"]: c["count"] for c in d["classes"]}
    d["record_id"] = d.get("record_id", "S4") + "-v2"
    d["v2_reclassification"] = {"rule": "柔性类只保留 belt/hose/flex/pipe/tube/wire/cable(非 duct/chain/gland/tie/holder/clamp/channel)；含 pulley/duct/attacher/fixer/holder/sensor/… 的移入 RIGID_RECLASSIFIED",
                                "moved": len(moved), "from": {k: sum(1 for m in moved if m[0] == k) for k in ("BELT_FLEXIBLE", "FLEXIBLE_TUBE_CABLE")}, "basis": a.inp}
    (a.case_dir / a.out).write_text(json.dumps(d, ensure_ascii=False, indent=1), encoding="utf-8")
    print(json.dumps(d["v2_reclassification"], ensure_ascii=False)); print("summary", d["summary"])
    for k, v in rigid["products"][:40]:
        print("  moved:", k[:60], v)
    for c in d["classes"]:
        if c["class"] in ("BELT_FLEXIBLE", "FLEXIBLE_TUBE_CABLE"):
            print(c["class"], "kept:", [p[0][:40] for p in c["products"][:20]])


if __name__ == "__main__":
    main()
