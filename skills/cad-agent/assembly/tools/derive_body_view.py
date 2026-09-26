#!/usr/bin/env python3
"""S1/S2 体展开视图（OLSK 新增，2026-09-02）：为 S7/S8 通用工具生成"体级 occurrence"消费视图。
权威不变：本体/S1-occurrence清单.v2.json 仍是 S1 权威；本视图只把桥接里拿到体级绑定的多体定义
（如 Frame [self] 的各根型材）展开成伪 occurrence：
   occ_uid = <父 occ_uid>#b<k>，product = <PRODUCT.id>#b<k>，world = 父世界矩阵，body_of = 父 occ_id。
同时生成体展开网格目录（其余定义 STL 以符号链接复用）与 manifest，并把 robot-config 的消费路径切到视图，
原权威路径保留在 paths_authority。
用法：python derive_body_view.py --occ 本体/S1-occurrence清单.v2.json --bodies 本体/S1-定义体清单.v1.json --bridge 本体/S3-GLB桥接.v1.json
      --mesh-dir 网格导出 --out-occ 本体/S1-occurrence清单-体展开.v2.json --out-mesh-dir 网格导出-体展开 --config robot-config.v1.json
"""
import argparse, json, os, re, hashlib
from pathlib import Path
ap = argparse.ArgumentParser()
for k in ["occ", "bodies", "bridge", "mesh_dir", "out_occ", "out_mesh_dir", "config"]: ap.add_argument("--" + k.replace("_", "-"), required=True, type=Path)
a = ap.parse_args()
S1 = json.loads(a.occ.read_text(encoding="utf-8")); BD = json.loads(a.bodies.read_text(encoding="utf-8"))["definitions"]
BR = json.loads(a.bridge.read_text(encoding="utf-8")); man = json.loads((a.mesh_dir / "manifest.json").read_text(encoding="utf-8"))
select = {m["best"]["product"] for m in BR["matches"] if m["best"] and m["best"].get("kind") == "body"}
for m in BR["matches"]:
    for alt in m.get("alternatives", []) or []:
        if alt.get("kind") == "body": select.add(alt["product"])
occs, n, expanded = [], 0, []
for o in S1["occurrences"]:
    if o["product"] in select and len(BD.get(o["product"], {}).get("bodies", [])) >= 2:
        for b in BD[o["product"]]["bodies"]:
            if "bbox_min" not in b: continue
            k = b["body_id"].split("#")[1]
            occs.append({**o, "occ_id": f"occ_{n:04d}", "occ_uid": f"{o['occ_uid']}#{k}", "product": b["body_id"], "product_name": f"{o.get('product_name')}#{k}",
                         "nauo_path": f"{o['nauo_path']}#{k}", "body_of": o["occ_id"], "body_of_uid": o["occ_uid"]}); n += 1
        expanded.append(o["occ_uid"])
    else:
        occs.append({**o, "occ_id": f"occ_{n:04d}"}); n += 1
view = {**S1, "$schema": "assembly-ontology.s1-occurrence-manifest/v2+bodies", "record_id": S1["record_id"] + "-BODYVIEW",
        "derived_from": {"authority": str(a.occ), "sha256": hashlib.sha256(a.occ.read_bytes()).hexdigest(), "bodies": str(a.bodies), "bridge": str(a.bridge)},
        "body_view": {"expanded_definitions": sorted(select), "expanded_occurrences": expanded, "leaf_occurrences": len(occs)},
        "occurrences": occs}
view["structure"] = {**S1["structure"], "leaf_occurrences": len(occs), "note": "体展开视图；occ_id 重新编号（稳定遍历序），occ_uid 带 #b 后缀"}
a.out_occ.write_text(json.dumps(view, ensure_ascii=False, indent=1), encoding="utf-8")
# mesh dir
a.out_mesh_dir.mkdir(parents=True, exist_ok=True)
parts = {}
for stem, v in man["parts"].items():
    if v.get("name") in select: continue
    src = (a.mesh_dir / f"{stem}.stl").resolve(); dst = a.out_mesh_dir / f"{stem}.stl"
    if src.exists() and not dst.exists(): os.symlink(src, dst)
    parts[stem] = v
for pid in sorted(select):
    for b in BD[pid]["bodies"]:
        if not b.get("stl"): continue
        src = (a.mesh_dir.parent / b["stl"]).resolve() if not Path(b["stl"]).is_absolute() else Path(b["stl"])
        if not src.exists(): src = (a.mesh_dir / Path(b["stl"]).name).resolve()
        if not src.exists(): src = (a.mesh_dir / "bodies" / Path(b["stl"]).name).resolve()
        stem = Path(b["stl"]).stem; dst = a.out_mesh_dir / f"{stem}.stl"
        if src.exists() and not dst.exists(): os.symlink(src, dst)
        parts[stem] = {"name": b["body_id"], "deflection_mm": b.get("deflection_mm"), "angular_rad": 0.4, "has_solid": True, "body_of": pid,
                       "bytes": src.stat().st_size if src.exists() else None, "sha256": hashlib.sha256(src.read_bytes()).hexdigest() if src.exists() else None}
mv = {**man, "$schema": "assembly-ontology.definition-stl-manifest/v1+bodies", "derived_from": str(a.mesh_dir / "manifest.json"), "parts": parts}
(a.out_mesh_dir / "manifest.json").write_text(json.dumps(mv, ensure_ascii=False, indent=1), encoding="utf-8")
cfg = json.loads(a.config.read_text(encoding="utf-8"))
cfg.setdefault("paths_authority", {}).update({"s1_manifest": cfg["paths"]["s1_manifest"], "mesh_dir": cfg["paths"]["mesh_dir"]})
cfg["paths"]["s1_manifest"] = str(a.out_occ); cfg["paths"]["mesh_dir"] = str(a.out_mesh_dir)
cfg["paths"]["s1_body_view_note"] = "S7/S8 消费体展开视图；权威在 paths_authority"
a.config.write_text(json.dumps(cfg, ensure_ascii=False, indent=1), encoding="utf-8")
print(json.dumps({"expanded_definitions": sorted(select), "expanded_occurrences": len(expanded), "view_leaves": len(occs), "mesh_parts": len(parts)}, ensure_ascii=False))
