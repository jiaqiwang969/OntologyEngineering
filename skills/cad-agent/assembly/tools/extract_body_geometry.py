#!/usr/bin/env python3
"""S2 补充（OLSK 新增，2026-09-02）：对拿到体级绑定的多体定义，逐 SOLID 提取解析面（局部坐标），
键 = body_id（`<PRODUCT.id>#b<k>`），字段与 extract_definition_geometry 完全一致（复用其 extract_faces）。
输出合并进 S2 零件解析面文件的 parts（新增键，不改既有键），并记录来源。
用法：python extract_body_geometry.py --step X --label-map L --bodies 本体/S1-定义体清单 --select-from-bridge 本体/S3-GLB桥接
      --geo 本体/S2-零件解析面.v1.json --out 本体/S2-零件解析面.v1.json
"""
import argparse, json, sys, time, importlib.util
from pathlib import Path
from OCP.STEPCAFControl import STEPCAFControl_Reader
from OCP.IFSelect import IFSelect_RetDone
from OCP.TDocStd import TDocStd_Document
from OCP.TCollection import TCollection_ExtendedString, TCollection_AsciiString
from OCP.XCAFDoc import XCAFDoc_DocumentTool
from OCP.TDF import TDF_Label, TDF_Tool
from OCP.TopExp import TopExp_Explorer
from OCP.TopAbs import TopAbs_SOLID
from OCP.TopoDS import TopoDS
spec = importlib.util.spec_from_file_location("edg", Path(__file__).with_name("extract_definition_geometry.py"))
edg = importlib.util.module_from_spec(spec); spec.loader.exec_module(edg)

ap = argparse.ArgumentParser()
for k in ["step", "label_map", "bodies", "select_from_bridge", "geo", "out"]: ap.add_argument("--" + k.replace("_", "-"), required=True, type=Path)
ap.add_argument("--all-multibody", action="store_true", help="不按桥选，多体定义全部提")
a = ap.parse_args()
t0 = time.time()
lm = json.loads(a.label_map.read_text(encoding="utf-8"))["id_to_entry"]
BD = json.loads(a.bodies.read_text(encoding="utf-8"))["definitions"]
if a.all_multibody:
    select = {pid for pid, d in BD.items() if len(d.get("bodies", [])) >= 2}
else:
    BR = json.loads(a.select_from_bridge.read_text(encoding="utf-8"))
    select = {m["best"]["product"] for m in BR["matches"] if m["class"] != "UNMATCHED" and m["best"] and m["best"].get("kind") == "body"}
    for m in BR["matches"]:
        for alt in m.get("alternatives", []) or []:
            if alt.get("kind") == "body": select.add(alt["product"])
print(f"选中多体定义 {len(select)}：{sorted(select)[:10]}", flush=True)
doc = TDocStd_Document(TCollection_ExtendedString("XmlXCAF")); r = STEPCAFControl_Reader(); r.SetNameMode(True)
if r.ReadFile(str(a.step)) != IFSelect_RetDone: sys.exit("READ_FAILED")
r.Transfer(doc); st = XCAFDoc_DocumentTool.ShapeTool_s(doc.Main()); print(f"[xcaf] {time.time()-t0:.0f}s", flush=True)
geo = json.loads(a.geo.read_text(encoding="utf-8"))
stat = {"平面": 0, "锥面": 0, "平面_面积过小": 0, "锥面_面积过小": 0, "平面_无三角化": 0, "平面_三角形超上限": 0,
        "uv面内残差最大mm": 0.0, "三角结点离面最大mm": 0.0, "离面超0.01mm的面": 0, "结点数": 0}
added, tc, tp, tk = 0, 0, 0, 0
for pid in sorted(select):
    lab = TDF_Label(); TDF_Tool.Label_s(doc.GetData(), TCollection_AsciiString(lm[pid]["entry"]), lab, False)
    shape = st.GetShape_s(lab); k = 0
    ex = TopExp_Explorer(shape, TopAbs_SOLID)
    while ex.More():
        sol = TopoDS.Solid_s(ex.Current()); bid = f"{pid}#b{k:03d}"
        cyls, planes, cones = edg.extract_faces(sol, stat)
        geo["parts"][bid] = {"name": bid, "xcaf_name": lm[pid]["xcaf_name"], "body_of": pid, "has_solid": True, "cylinders": cyls, "planes": planes, "cones": cones}
        tc += len(cyls); tp += len(planes); tk += len(cones); added += 1; k += 1; ex.Next()
geo.setdefault("body_extraction", {}).update({"record": "OLSK extract_body_geometry 2026-09-02", "definitions": sorted(select), "bodies_added": added,
                                              "圆柱面": tc, "平面_落盘": tp, "圆锥面_落盘": tk, "selfcheck": {k: (round(v, 6) if isinstance(v, float) else v) for k, v in stat.items()},
                                              "note": "体级 parts 与其父定义 parts 并存；界面图按占用清单里出现的 product 取用。"})
a.out.write_text(json.dumps(geo, ensure_ascii=False), encoding="utf-8")
print(json.dumps(geo["body_extraction"], ensure_ascii=False)[:600]); print("写出", a.out, f"{time.time()-t0:.0f}s")
