#!/usr/bin/env python3
"""S2 第一步（Poppy 变体）：从整机 STEP 经 XCAF 按**零件定义**提取解析面（局部坐标）。

X1 有 187 个单件 STEP 可逐件读；Poppy 只有整机 STEP，因此改为 XCAF 单次读入、
按唯一定义提取一次几何，再由 S1 世界变换实例化——与 X1 方法的核心结构
（K-ID-02：按定义提一次，比按实例读便宜一个数量级）一致。

每个圆柱面记：局部轴线上一点 p、轴向单位向量 d、半径 r、v 轴向参数区间、
u 圆周张角、面积——字段与 X1 `extract_part_cylinders.py` 完全一致。
平面记：p/n/rev/面积（本版仅落盘供后续平面贴合图使用，不做三角化轮廓）。
圆锥面记：apex 近似（location）/轴/半角/参考半径/v 区间/面积。

输出按定义名建 parts 字典（与 X1 schema 同构），并与 S1 manifest 的唯一叶
产品集合做闭合对账：缺失/多余定义都显式列出，不静默。

用法：python3 extract_definition_geometry.py --step X --occ S1清单 --out OUT
"""
import argparse
import json
import math
import sys
import time
from collections import defaultdict
from pathlib import Path

from OCP.STEPCAFControl import STEPCAFControl_Reader
from OCP.IFSelect import IFSelect_RetDone
from OCP.TDocStd import TDocStd_Document
from OCP.TCollection import TCollection_ExtendedString
from OCP.XCAFDoc import XCAFDoc_DocumentTool
from OCP.TDF import TDF_LabelSequence, TDF_Label, TDF_Tool
from OCP.TCollection import TCollection_AsciiString
from OCP.TDataStd import TDataStd_Name
from OCP.TopExp import TopExp_Explorer
from OCP.TopAbs import TopAbs_FACE, TopAbs_SOLID, TopAbs_SHELL, TopAbs_REVERSED
from OCP.TopoDS import TopoDS
from OCP.BRep import BRep_Tool
from OCP.BRepAdaptor import BRepAdaptor_Surface
from OCP.GeomAbs import GeomAbs_Cylinder, GeomAbs_Plane, GeomAbs_Cone
from OCP.BRepTools import BRepTools
from OCP.BRepMesh import BRepMesh_IncrementalMesh
from OCP.TopLoc import TopLoc_Location
from OCP.GProp import GProp_GProps
from OCP.BRepGProp import BRepGProp

AREA_FLOOR = 0.5      # mm²，平面/锥面抽取地板（X1 同值；配对阈值另设且不得低于此）
MESH_DEFL = 0.5       # mm，平面轮廓三角化线性挠度
MESH_ANG = 0.5        # rad
MAX_TRI = 600         # 单面三角形上限，超了只留 AABB 并计数


def get_name(label):
    n = TDataStd_Name()
    if label.FindAttribute(TDataStd_Name.GetID_s(), n):
        es = n.Get()
        # OCP 的 Value(i) 直接返回单字符 str；逐字符拼接保住非 ASCII（é 等）
        return "".join(es.Value(i) for i in range(1, es.Length() + 1))
    return None


def faces_of(shape):
    ex = TopExp_Explorer(shape, TopAbs_FACE)
    while ex.More():
        yield TopoDS.Face_s(ex.Current())
        ex.Next()


def extract_faces(shape, stat):
    """字段与 X1 extract_part_cylinders.py / extract_part_planes.py 完全一致。
    平面带 (u,v)=平面自身 2D 坐标 的三角化真实轮廓（tri），并做逐结点自检。"""
    # 三角化一次，供平面轮廓 2D 化。只影响有曲边（孔、圆角）的平面。
    BRepMesh_IncrementalMesh(shape, MESH_DEFL, False, MESH_ANG, True)
    cyls, planes, cones = [], [], []
    for face in faces_of(shape):
        ad = BRepAdaptor_Surface(face)
        t = ad.GetType()
        if t not in (GeomAbs_Cylinder, GeomAbs_Plane, GeomAbs_Cone):
            continue
        g = GProp_GProps()
        BRepGProp.SurfaceProperties_s(face, g)
        area = g.Mass()
        rev = 1 if face.Orientation() == TopAbs_REVERSED else 0
        umin, umax, vmin, vmax = BRepTools.UVBounds_s(face)
        if t == GeomAbs_Cylinder:
            cy = ad.Cylinder()
            ax = cy.Axis()
            loc, dr = ax.Location(), ax.Direction()
            cyls.append({
                "p": [round(loc.X(), 6), round(loc.Y(), 6), round(loc.Z(), 6)],
                "d": [round(dr.X(), 9), round(dr.Y(), 9), round(dr.Z(), 9)],
                "r": round(cy.Radius(), 6),
                "v": [round(vmin, 6), round(vmax, 6)],
                "u_span_deg": round(math.degrees(umax - umin), 3),
                "area": round(area, 4),
            })
        elif t == GeomAbs_Plane:
            stat["平面"] += 1
            if area < AREA_FLOOR:
                stat["平面_面积过小"] += 1
                continue
            pl = ad.Plane()
            pos = pl.Position()
            loc, nd, xd = pos.Location(), pos.Direction(), pos.XDirection()
            yd = pos.YDirection()
            rec = {
                "p": [round(loc.X(), 6), round(loc.Y(), 6), round(loc.Z(), 6)],
                "n": [round(nd.X(), 9), round(nd.Y(), 9), round(nd.Z(), 9)],
                "xd": [round(xd.X(), 9), round(xd.Y(), 9), round(xd.Z(), 9)],
                "yd": [round(yd.X(), 9), round(yd.Y(), 9), round(yd.Z(), 9)],
                "rev": rev,
                "uv": [round(umin, 5), round(umax, 5), round(vmin, 5), round(vmax, 5)],
                "area": round(area, 4),
            }
            tl = TopLoc_Location()
            tri = BRep_Tool.Triangulation_s(face, tl)
            tris = None
            if tri is not None and tri.HasUVNodes():
                nt = tri.NbTriangles()
                if nt > MAX_TRI:
                    stat["平面_三角形超上限"] += 1
                else:
                    trf = tl.Transformation()
                    uvs = []
                    f_off = 0.0
                    for i in range(1, tri.NbNodes() + 1):
                        q = tri.UVNode(i)
                        uvs.append((q.X(), q.Y()))
                        n3 = tri.Node(i).Transformed(trf)
                        w = (n3.X() - loc.X(), n3.Y() - loc.Y(), n3.Z() - loc.Z())
                        pu = w[0] * xd.X() + w[1] * xd.Y() + w[2] * xd.Z()
                        pv = w[0] * yd.X() + w[1] * yd.Y() + w[2] * yd.Z()
                        pn = w[0] * nd.X() + w[1] * nd.Y() + w[2] * nd.Z()
                        stat["uv面内残差最大mm"] = max(
                            stat["uv面内残差最大mm"], math.hypot(pu - q.X(), pv - q.Y()))
                        f_off = max(f_off, abs(pn))
                    stat["结点数"] += tri.NbNodes()
                    stat["三角结点离面最大mm"] = max(stat["三角结点离面最大mm"], f_off)
                    if f_off > 0.01:
                        stat["离面超0.01mm的面"] += 1
                    tris = []
                    for i in range(1, nt + 1):
                        a_, b_, c_ = tri.Triangle(i).Get()
                        (x1, y1), (x2, y2), (x3, y3) = uvs[a_ - 1], uvs[b_ - 1], uvs[c_ - 1]
                        tris.append([round(x1, 4), round(y1, 4), round(x2, 4),
                                     round(y2, 4), round(x3, 4), round(y3, 4)])
            else:
                stat["平面_无三角化"] += 1
            if tris:
                rec["tri"] = tris
            planes.append(rec)
        else:
            stat["锥面"] += 1
            if area < AREA_FLOOR:
                stat["锥面_面积过小"] += 1
                continue
            cn = ad.Cone()
            pos = cn.Position()
            loc, dd = pos.Location(), pos.Direction()
            apx = cn.Apex()
            cones.append({
                "p": [round(loc.X(), 6), round(loc.Y(), 6), round(loc.Z(), 6)],
                "d": [round(dd.X(), 9), round(dd.Y(), 9), round(dd.Z(), 9)],
                "apex": [round(apx.X(), 6), round(apx.Y(), 6), round(apx.Z(), 6)],
                "半角rad": round(cn.SemiAngle(), 9),
                "rref": round(cn.RefRadius(), 6),
                "v": [round(vmin, 6), round(vmax, 6)],
                "u_span_deg": round(math.degrees(umax - umin), 3),
                "rev": rev,
                "area": round(area, 4),
            })
    return cyls, planes, cones


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--step", required=True, type=Path)
    ap.add_argument("--label-map", type=Path, default=None, help="OLSK 增量：S1 定义键→XCAF 标签 entry 映射；给了就按 entry 取形，不按标签名")
    ap.add_argument("--occ", required=True, type=Path, help="S1 occurrence 清单（v2）")
    ap.add_argument("--out", required=True, type=Path)
    ap.add_argument("--record-id", required=True)
    args = ap.parse_args()

    t0 = time.time()
    print("[1/3] STEPCAF 读入整机 STEP…", flush=True)
    doc = TDocStd_Document(TCollection_ExtendedString("XmlXCAF"))
    r = STEPCAFControl_Reader()
    r.SetNameMode(True)
    if r.ReadFile(str(args.step)) != IFSelect_RetDone:
        print("READ_FAILED", file=sys.stderr)
        return 2
    if not r.Transfer(doc):
        print("TRANSFER_FAILED", file=sys.stderr)
        return 2
    st = XCAFDoc_DocumentTool.ShapeTool_s(doc.Main())

    # 收集全部 simple-shape（定义级）标签：非装配、有几何
    labels = TDF_LabelSequence()
    st.GetShapes(labels)
    defs = {}
    dup_names = defaultdict(int)
    for i in range(1, labels.Length() + 1):
        lab = labels.Value(i)
        if st.IsAssembly_s(lab) or st.IsComponent_s(lab):
            continue
        if not st.IsSimpleShape_s(lab):
            continue
        nm = get_name(lab)
        if not nm:
            continue
        shape = st.GetShape_s(lab)
        has_solid = TopExp_Explorer(shape, TopAbs_SOLID).More()
        has_shell = TopExp_Explorer(shape, TopAbs_SHELL).More()
        if nm in defs:
            dup_names[nm] += 1
            continue
        defs[nm] = {"shape": shape, "has_solid": has_solid, "has_shell": has_shell}
    print(f"      simple-shape 定义 {len(defs)}（重名丢弃 {sum(dup_names.values())}） {time.time()-t0:.0f}s", flush=True)

    occd = json.loads(args.occ.read_text(encoding="utf-8"))
    need = sorted({o["product"] for o in occd["occurrences"]})
    # ---- OLSK 增量 L1（2026-09-02）：按 XCAF 标签 entry 取定义形，键=S1 product（PRODUCT.id）----
    # 原因：OCCT 标签名=PRODUCT.name，本案 55 个名被多个定义共用、407 个 id≠name，按名取形会错配/丢失。
    label_map_report = None
    if args.label_map is not None:
        lm = json.loads(args.label_map.read_text(encoding="utf-8"))
        defs = {}
        name_mismatch, not_found = [], []
        for pid, info in lm["id_to_entry"].items():
            lab = TDF_Label()
            TDF_Tool.Label_s(doc.GetData(), TCollection_AsciiString(info["entry"]), lab, False)
            if lab.IsNull():
                not_found.append(pid); continue
            nm_lab = get_name(lab)
            if (nm_lab or "") != (info.get("xcaf_name") or ""):
                name_mismatch.append({"product": pid, "entry": info["entry"], "expected": info.get("xcaf_name"), "found": nm_lab}); continue
            shape = st.GetShape_s(lab)
            defs[pid] = {"shape": shape, "has_solid": TopExp_Explorer(shape, TopAbs_SOLID).More(),
                         "has_shell": TopExp_Explorer(shape, TopAbs_SHELL).More()}
        label_map_report = {"entries": len(lm["id_to_entry"]), "resolved": len(defs), "not_found": not_found, "name_mismatch": name_mismatch,
                            "occt_version_in_map": lm.get("occt_version"), "occt_version_now": __import__("OCP").__version__}
        print(f"      标签映射：{len(defs)}/{len(lm['id_to_entry'])} 解析；缺 {len(not_found)}；名不符 {len(name_mismatch)}", flush=True)
        if not_found or name_mismatch:
            print("LABEL_MAP_INCONSISTENT", not_found[:5], name_mismatch[:5], file=sys.stderr)
            return 3

    # XCAF/OCCT 会丢非 ASCII 字节（'Défaut'→'Dfaut'）。做确定性 ASCII 归一匹配：
    # S1 名为正本键；归一冲突显式报告，不静默择一。
    def ascii_key(s):
        return "".join(c for c in s if " " <= c <= "~")

    norm_defs, norm_collisions = {}, []
    for nm in sorted(defs):
        k = ascii_key(nm)
        if k in norm_defs:
            norm_collisions.append({"key": k, "kept": norm_defs[k], "dropped": nm})
        else:
            norm_defs[k] = nm
    resolved = {}
    for nm in need:
        if nm in defs:
            resolved[nm] = nm
        elif ascii_key(nm) in norm_defs:
            resolved[nm] = norm_defs[ascii_key(nm)]
    missing = [n for n in need if n not in resolved]
    extra = sorted(set(defs) - set(resolved.values()))

    print("[2/3] 按定义提取解析面…", flush=True)
    parts, tot_c, tot_p, tot_k = {}, 0, 0, 0
    stat = {"平面": 0, "锥面": 0, "平面_面积过小": 0, "锥面_面积过小": 0,
            "平面_无三角化": 0, "平面_三角形超上限": 0,
            "uv面内残差最大mm": 0.0, "三角结点离面最大mm": 0.0,
            "离面超0.01mm的面": 0, "结点数": 0}
    for nm in need:
        if nm not in resolved:
            parts[nm] = {"error": "DEFINITION_NOT_FOUND_IN_XCAF"}
            continue
        src = defs[resolved[nm]]
        cyls, planes, cones = extract_faces(src["shape"], stat)
        parts[nm] = {"name": nm, "xcaf_name": resolved[nm], "has_solid": src["has_solid"],
                     "cylinders": cyls, "planes": planes, "cones": cones}
        tot_c += len(cyls); tot_p += len(planes); tot_k += len(cones)
    print(f"      圆柱 {tot_c} · 平面 {tot_p} · 圆锥 {tot_k}  {time.time()-t0:.0f}s", flush=True)

    print("[3/3] 落盘…", flush=True)
    out = {
        "$schema": "assembly-ontology.s2-definition-geometry/v1",
        "record_id": args.record_id,
        "purpose": "按唯一零件定义提取解析面（局部坐标），供 S1 世界变换实例化为全局界面图。",
        "method_ref": "X1 extract_part_cylinders.py 的 XCAF 整机变体；字段与判据保持一致（K-IF-02：只取解析面，不用距离启发式）。",
        "source_step": str(args.step),
        "summary": {"S1唯一叶定义": len(need), "XCAF定义命中": len(need) - len(missing),
                    "缺失定义": len(missing), "XCAF多余simple-shape": len(extra),
                    "圆柱面": tot_c, "平面_落盘": tot_p, "圆锥面_落盘": tot_k,
                    "耗时秒": round(time.time() - t0, 1)},
        "thresholds": {"area_floor_mm2": AREA_FLOOR, "mesh_linear_deflection_mm": MESH_DEFL,
                       "mesh_angular_deflection_rad": MESH_ANG, "max_triangles_per_face": MAX_TRI,
                       "note": "area_floor 是抽取地板不是配对阈值；配对阈值在界面图工具里且不得低于此。"},
        "extraction_selfcheck": {k: (round(v, 6) if isinstance(v, float) else v) for k, v in stat.items()},
        "missing_definitions": missing,
        "extra_simple_shapes_not_in_s1_leaves": extra,
        "ascii_normalization_collisions": norm_collisions,
        "label_map_report": label_map_report,
        "duplicate_names_dropped": dict(dup_names),
        "claim_boundary": [
            "几何在定义局部坐标系；world 实例化由界面图阶段用 S1 变换完成。",
            "只提解析圆柱/平面/圆锥面；自由曲面配合不覆盖。",
            "缺失定义与重名定义都已显式列出；重名定义按首个标签取形，存在同名异形风险时必须人工裁决。",
        ],
        "parts": parts,
    }
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(out, ensure_ascii=False), encoding="utf-8")
    print(f"写出 {args.out}")
    for k, v in out["summary"].items():
        print(f"  {k}: {v}")
    if missing:
        print(f"  !! 缺失定义: {missing[:10]}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
