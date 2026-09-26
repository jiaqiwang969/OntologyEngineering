#!/usr/bin/env python3
"""把整机 STEP 的唯一零件定义三角化导出为 STL（XCAF 变体，供 S7/FCL 与 Blender 消费）。

沿用 X1 export_part_stl.py 的显示挠度规则：
    linear_deflection = clamp(bbox_diag / 400, 0.02, 0.6) mm，angular 0.4 rad
**只用于显示与网格碰撞代理**：不作任何尺寸结论；无实体件照样导出并在清单标出。

用法：python3 export_definition_stl.py --step X --occ S1清单 --outdir DIR
输出：DIR/*.stl + DIR/manifest.json
"""
import argparse
import hashlib
import json
import math
import re
import sys
import time
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
from OCP.TopAbs import TopAbs_SOLID
from OCP.BRepMesh import BRepMesh_IncrementalMesh
from OCP.StlAPI import StlAPI_Writer
from OCP.Bnd import Bnd_Box
from OCP.BRepBndLib import BRepBndLib

DEFL_MIN, DEFL_MAX, DEFL_RATIO = 0.02, 0.6, 1 / 400.0
ANG_DEFL = 0.4


def get_name(label):
    n = TDataStd_Name()
    if label.FindAttribute(TDataStd_Name.GetID_s(), n):
        es = n.Get()
        return "".join(es.Value(i) for i in range(1, es.Length() + 1))
    return None


def safe_stem(nm):
    return re.sub(r"[^0-9A-Za-zÀ-ÿ_.-]+", "_", nm)[:80]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--step", required=True, type=Path)
    ap.add_argument("--label-map", type=Path, default=None, help="OLSK 增量：S1 定义键→XCAF 标签 entry 映射；给了就按 entry 取形，不按标签名")
    ap.add_argument("--occ", required=True, type=Path)
    ap.add_argument("--outdir", required=True, type=Path)
    ap.add_argument("--only", default="", help="逗号分隔定义名子串；只重导出命中的定义")
    ap.add_argument("--defl-ratio", type=float, default=DEFL_RATIO)
    ap.add_argument("--defl-min", type=float, default=DEFL_MIN)
    ap.add_argument("--defl-max", type=float, default=DEFL_MAX)
    args = ap.parse_args()
    only = [x for x in args.only.split(",") if x]
    t0 = time.time()

    doc = TDocStd_Document(TCollection_ExtendedString("XmlXCAF"))
    r = STEPCAFControl_Reader()
    r.SetNameMode(True)
    if r.ReadFile(str(args.step)) != IFSelect_RetDone:
        print("READ_FAILED", file=sys.stderr)
        return 2
    r.Transfer(doc)
    st = XCAFDoc_DocumentTool.ShapeTool_s(doc.Main())
    labels = TDF_LabelSequence()
    st.GetShapes(labels)
    defs = {}
    for i in range(1, labels.Length() + 1):
        lab = labels.Value(i)
        if st.IsAssembly_s(lab) or st.IsComponent_s(lab) or not st.IsSimpleShape_s(lab):
            continue
        nm = get_name(lab)
        if nm and nm not in defs:
            defs[nm] = st.GetShape_s(lab)

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
            defs[pid] = shape
        label_map_report = {"entries": len(lm["id_to_entry"]), "resolved": len(defs), "not_found": not_found, "name_mismatch": name_mismatch,
                            "occt_version_in_map": lm.get("occt_version"), "occt_version_now": __import__("OCP").__version__}
        print(f"      标签映射：{len(defs)}/{len(lm['id_to_entry'])} 解析；缺 {len(not_found)}；名不符 {len(name_mismatch)}", flush=True)
        if not_found or name_mismatch:
            print("LABEL_MAP_INCONSISTENT", not_found[:5], name_mismatch[:5], file=sys.stderr)
            return 3
    args.outdir.mkdir(parents=True, exist_ok=True)

    def ascii_key(s):
        return "".join(c for c in s if " " <= c <= "~")

    norm_defs = {}
    for nm0 in sorted(defs):
        norm_defs.setdefault(ascii_key(nm0), nm0)

    man, missing = {}, []
    # --only 模式：读入既有 manifest，只更新命中的条目
    if only and (args.outdir / "manifest.json").exists():
        man = json.loads((args.outdir / "manifest.json").read_text(encoding="utf-8"))["parts"]
    for nm in need:
        if only and not any(s in nm for s in only):
            continue
        src = nm if nm in defs else norm_defs.get(ascii_key(nm))
        if src is None:
            missing.append(nm)
            continue
        shape = defs[src]
        # 词干冲突消解：中文名净化后可能撞车（如两种"数据替代模型"都→FSA60-43E_）；
        # 已被"别的名字"占用时加名字哈希后缀，绝不静默顶掉
        stem = safe_stem(nm)
        if stem in man and man[stem].get("name") not in (None, nm):
            stem = f"{stem}-{hashlib.sha256(nm.encode('utf-8')).hexdigest()[:8]}"
        box = Bnd_Box()
        BRepBndLib.Add_s(shape, box)
        if box.IsVoid():
            man[stem] = {"name": nm, "error": "EMPTY_BBOX"}
            continue
        x0, y0, z0, x1, y1, z1 = box.Get()
        diag = math.dist((x0, y0, z0), (x1, y1, z1))
        defl = min(args.defl_max, max(args.defl_min, diag * args.defl_ratio))
        BRepMesh_IncrementalMesh(shape, defl, False, ANG_DEFL, True)
        out = args.outdir / f"{stem}.stl"
        w = StlAPI_Writer()
        w.ASCIIMode = False
        ok = w.Write(shape, str(out))
        has_solid = TopExp_Explorer(shape, TopAbs_SOLID).More()
        rec = {"name": nm, "deflection_mm": round(defl, 4), "angular_rad": ANG_DEFL,
               "bbox_diag_mm": round(diag, 3), "has_solid": has_solid}
        if ok and out.exists():
            rec["bytes"] = out.stat().st_size
            rec["sha256"] = hashlib.sha256(out.read_bytes()).hexdigest()
        else:
            rec["error"] = "WRITE_FAILED"
        man[stem] = rec

    doc_out = {
        "$schema": "assembly-ontology.definition-stl-manifest/v1",
        "source_step": str(args.step),
        "deflection_rule": "默认 clamp(bbox_diag/400, 0.02, 0.6) mm; angular 0.4 rad；"
                           "逐件覆盖见各条目 deflection_mm（--only 定点细分）",
        "claim_boundary": ["显示与网格碰撞代理用途；不得用于尺寸、间隙或精确干涉结论。",
                          "无实体（开壳）定义照样导出，has_solid=false 标出。"],
        "missing_definitions": missing,
        "parts": man,
    }
    (args.outdir / "manifest.json").write_text(json.dumps(doc_out, ensure_ascii=False), encoding="utf-8")
    ok_n = sum(1 for v in man.values() if "sha256" in v)
    print(f"导出 {ok_n}/{len(need)} 定义 STL，缺失 {len(missing)}，{time.time()-t0:.0f}s")
    if missing:
        print("缺失:", missing[:10])
    return 0


if __name__ == "__main__":
    sys.exit(main())
