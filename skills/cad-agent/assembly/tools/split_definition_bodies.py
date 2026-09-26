#!/usr/bin/env python3
"""S1/S2 补充（OLSK 新增，2026-09-02）：按定义枚举 SOLID 体，多体定义逐体导出 STL 并落盘体级身份。
动机：STEP 把机架型材建成 Frame 组件自带的一个复合体（Frame [self] = 多根型材实体），
SOP 01–06 却逐根装配；不拆体，型材在装配序列里没有身份。体序按 OCCT TopExp 遍历序（同字节同版本稳定），
体 id = `<定义键>#b<序号>`；每体记局部 AABB、体积、质心，供几何桥与 S7 消费。
用法：python split_definition_bodies.py --step X --label-map evidence/s1-definition-labels.v1.json --outdir 网格导出/bodies --out 本体/S1-定义体清单.v1.json
"""
import argparse, json, hashlib, math, time, re, sys
from pathlib import Path
from OCP.STEPCAFControl import STEPCAFControl_Reader
from OCP.IFSelect import IFSelect_RetDone
from OCP.TDocStd import TDocStd_Document
from OCP.TCollection import TCollection_ExtendedString, TCollection_AsciiString
from OCP.XCAFDoc import XCAFDoc_DocumentTool
from OCP.TDF import TDF_Label, TDF_Tool
from OCP.TopExp import TopExp_Explorer
from OCP.TopAbs import TopAbs_SOLID, TopAbs_SHELL
from OCP.TopoDS import TopoDS
from OCP.BRepMesh import BRepMesh_IncrementalMesh
from OCP.StlAPI import StlAPI_Writer
from OCP.Bnd import Bnd_Box
from OCP.BRepBndLib import BRepBndLib
from OCP.GProp import GProp_GProps
from OCP.BRepGProp import BRepGProp

ap = argparse.ArgumentParser()
ap.add_argument("--step", required=True, type=Path); ap.add_argument("--label-map", required=True, type=Path)
ap.add_argument("--outdir", required=True, type=Path); ap.add_argument("--out", required=True, type=Path)
ap.add_argument("--min-bodies", type=int, default=2)
a = ap.parse_args()
t0 = time.time()
lm = json.loads(a.label_map.read_text(encoding="utf-8"))
doc = TDocStd_Document(TCollection_ExtendedString("XmlXCAF")); r = STEPCAFControl_Reader(); r.SetNameMode(True)
if r.ReadFile(str(a.step)) != IFSelect_RetDone: sys.exit("READ_FAILED")
r.Transfer(doc); st = XCAFDoc_DocumentTool.ShapeTool_s(doc.Main())
print(f"[xcaf] {time.time()-t0:.0f}s", flush=True)
def safe_stem(nm): return re.sub(r"[^0-9A-Za-zÀ-ÿ_.-]+", "_", nm)[:70]
a.outdir.mkdir(parents=True, exist_ok=True)
defs, multi = {}, 0
for pid, info in lm["id_to_entry"].items():
    lab = TDF_Label(); TDF_Tool.Label_s(doc.GetData(), TCollection_AsciiString(info["entry"]), lab, False)
    if lab.IsNull(): defs[pid] = {"error": "LABEL_NOT_FOUND"}; continue
    shape = st.GetShape_s(lab)
    solids = []
    ex = TopExp_Explorer(shape, TopAbs_SOLID)
    while ex.More():
        solids.append(TopoDS.Solid_s(ex.Current())); ex.Next()
    nshell = 0
    ex = TopExp_Explorer(shape, TopAbs_SHELL)
    while ex.More(): nshell += 1; ex.Next()
    rec = {"solids": len(solids), "shells": nshell, "bodies": []}
    if len(solids) >= a.min_bodies:
        multi += 1
        stem = safe_stem(pid) + "-" + hashlib.sha256(pid.encode()).hexdigest()[:6]
        for k, sol in enumerate(solids):
            box = Bnd_Box(); BRepBndLib.Add_s(sol, box)
            if box.IsVoid(): rec["bodies"].append({"body_id": f"{pid}#b{k:03d}", "error": "EMPTY_BBOX"}); continue
            x0, y0, z0, x1, y1, z1 = box.Get(); diag = math.dist((x0, y0, z0), (x1, y1, z1))
            defl = min(0.6, max(0.02, diag / 400.0))
            BRepMesh_IncrementalMesh(sol, defl, False, 0.4, True)
            g = GProp_GProps(); BRepGProp.VolumeProperties_s(sol, g); c = g.CentreOfMass()
            out = a.outdir / f"{stem}_b{k:03d}.stl"; w = StlAPI_Writer(); w.ASCIIMode = False; ok = w.Write(sol, str(out))
            rec["bodies"].append({"body_id": f"{pid}#b{k:03d}", "stl": str(out.relative_to(a.outdir.parent)) if ok else None,
                                  "bbox_min": [round(x0, 4), round(y0, 4), round(z0, 4)], "bbox_max": [round(x1, 4), round(y1, 4), round(z1, 4)],
                                  "volume_mm3": round(g.Mass(), 3), "centroid": [round(c.X(), 4), round(c.Y(), 4), round(c.Z(), 4)], "deflection_mm": round(defl, 4)})
    defs[pid] = rec
out = {"$schema": "assembly-ontology.s1-definition-bodies/v1", "record_id": "OLSK-V3-S1-DEFINITION-BODIES-V001",
       "source_step": str(a.step), "label_map": str(a.label_map), "occt_version": __import__("OCP").__version__,
       "summary": {"definitions": len(defs), "multi_body_definitions": multi, "bodies_exported": sum(len(v.get("bodies", [])) for v in defs.values()),
                   "elapsed_s": round(time.time() - t0, 1)},
       "body_id_rule": "`<PRODUCT.id>#b<k>`，k 为 OCCT TopExp SOLID 遍历序（同 STEP 字节 + 同 OCCT 版本下稳定）；体在定义局部坐标系，世界位姿 = 所属 occurrence 的 S1 世界矩阵。",
       "claim_boundary": ["体级身份只在多体定义内成立；单体定义不发体 id。", "体 STL 是显示/碰撞代理，不作尺寸结论。"],
       "definitions": defs}
a.out.parent.mkdir(parents=True, exist_ok=True); a.out.write_text(json.dumps(out, ensure_ascii=False, indent=1), encoding="utf-8")
print(json.dumps(out["summary"], ensure_ascii=False)); print("写出", a.out)
