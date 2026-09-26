#!/usr/bin/env python3
"""S1 独立交叉通道 b（OLSK 新增，2026-09-02）：用 OCCT/XCAF 读整机 STEP，递归组件树，
输出每个 occurrence（叶与子装配）的实例名路径、定义名、局部矩阵与世界矩阵。
与通道 a（parse_step_assembly_ap242.py，STEP 文本解析）完全独立：不共享任何解析代码，
变换由 OCCT 自己的 STEP 适配器给出。比较由 s1_crosscheck_compare.py 完成，本文件不自证。
用法：python s1_xcaf_channel_b.py --step X --out OUT --record-id ID --case CASE
"""
import argparse, hashlib, json, time
from pathlib import Path
from OCP.STEPCAFControl import STEPCAFControl_Reader
from OCP.TDocStd import TDocStd_Document
from OCP.TCollection import TCollection_ExtendedString
from OCP.XCAFDoc import XCAFDoc_DocumentTool, XCAFDoc_ShapeTool
from OCP.TDF import TDF_LabelSequence, TDF_Label, TDF_Tool
from OCP.TCollection import TCollection_AsciiString
from OCP.TDataStd import TDataStd_Name
from OCP.IFSelect import IFSelect_RetDone
from OCP.Interface import Interface_Static


def ext_to_str(es):
    out = []
    for i in range(1, es.Length() + 1):
        v = es.Value(i)
        out.append(v if isinstance(v, str) else chr(v))
    return "".join(out)


def label_name(label):
    n = TDataStd_Name()
    if label.FindAttribute(TDataStd_Name.GetID_s(), n):
        return ext_to_str(n.Get())
    return ""


def entry(label):
    s = TCollection_AsciiString(); TDF_Tool.Entry_s(label, s); return s.ToCString()


def trsf_to_mat(t):
    m = [[t.Value(r, c) for c in range(1, 5)] for r in range(1, 4)]
    m.append([0.0, 0.0, 0.0, 1.0])
    return m


def mul(A, B):
    return [[sum(A[i][k] * B[k][j] for k in range(4)) for j in range(4)] for i in range(4)]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--step", required=True, type=Path)
    ap.add_argument("--out", required=True, type=Path)
    ap.add_argument("--record-id", required=True)
    ap.add_argument("--case", required=True)
    a = ap.parse_args()
    t0 = time.time()
    sha = hashlib.sha256(a.step.read_bytes()).hexdigest()
    print(f"sha256 {sha}", flush=True)
    Interface_Static.SetIVal_s("read.stepcaf.subshapes.name", 0)
    unit = Interface_Static.CVal_s("xstep.cascade.unit")
    doc = TDocStd_Document(TCollection_ExtendedString("MDTV-XCAF"))
    rd = STEPCAFControl_Reader()
    rd.SetNameMode(True); rd.SetColorMode(False); rd.SetLayerMode(False); rd.SetMatMode(False)
    st = rd.ReadFile(str(a.step))
    if st != IFSelect_RetDone:
        raise SystemExit(f"ReadFile failed: {st}")
    print(f"[read] {time.time()-t0:.0f}s", flush=True)
    rd.Transfer(doc)
    print(f"[transfer] {time.time()-t0:.0f}s", flush=True)
    stool = XCAFDoc_DocumentTool.ShapeTool_s(doc.Main())
    free = TDF_LabelSequence(); stool.GetFreeShapes(free)
    IDENT = [[1.0 if i == j else 0.0 for j in range(4)] for i in range(4)]
    leaves, asms, roots = [], [], []
    depth_max = 0

    def walk(def_label, W, path, depth, parent_def):
        nonlocal depth_max
        depth_max = max(depth_max, depth)
        comps = TDF_LabelSequence(); XCAFDoc_ShapeTool.GetComponents_s(def_label, comps)
        items = []
        for i in range(1, comps.Length() + 1):
            c = comps.Value(i)
            items.append((label_name(c), c.Tag(), c))
        # 稳定序：按实例名再标签 tag（与通道 a 的 tag/nauo 排序无关，比较按路径匹配）
        for nm, tag, c in sorted(items, key=lambda x: (x[0], x[1])):
            ref = TDF_Label()
            if not XCAFDoc_ShapeTool.GetReferredShape_s(c, ref):
                continue
            L = trsf_to_mat(XCAFDoc_ShapeTool.GetLocation_s(c).Transformation())
            Wc = mul(W, L)
            dname = label_name(ref)
            sub = path + [nm]
            rec = {"instance": nm, "definition": dname, "parent_definition": parent_def,
                   "ref_entry": entry(ref), "component_entry": entry(c),
                   "path": "/".join(sub), "depth": depth + 1,
                   "local": [[round(v, 9) for v in row] for row in L],
                   "world": [[round(v, 9) for v in row] for row in Wc]}
            if XCAFDoc_ShapeTool.IsAssembly_s(ref):
                asms.append(rec)
                walk(ref, Wc, sub, depth + 1, dname)
            else:
                rec["is_simple_shape"] = bool(XCAFDoc_ShapeTool.IsSimpleShape_s(ref))
                leaves.append(rec)

    for i in range(1, free.Length() + 1):
        r = free.Value(i)
        roots.append({"name": label_name(r), "entry": entry(r)})
        walk(r, IDENT, [], 0, label_name(r))
    print(f"[walk] leaves {len(leaves)} asms {len(asms)} roots {roots} depth {depth_max} {time.time()-t0:.0f}s", flush=True)
    doc_out = {
        "$schema": "assembly-ontology.s1-xcaf-channel/v1",
        "record_id": a.record_id, "case": a.case,
        "purpose": "S1 独立交叉通道 b：OCCT/XCAF 装配树与世界位姿。与文本解析通道 a 零代码共享。",
        "source": {"path": str(a.step), "sha256": sha, "size_bytes": a.step.stat().st_size},
        "occt": {"reader": "STEPCAFControl_Reader", "xstep.cascade.unit": unit,
                 "subshapes_name": 0, "name_mode": True},
        "structure": {"roots": roots, "occt_version": __import__("OCP").__version__, "leaf_occurrences": len(leaves),
                      "subassembly_occurrences": len(asms), "max_depth": depth_max,
                      "unique_leaf_definitions": len({l["definition"] for l in leaves})},
        "claim_boundary": [
            "只给装配拓扑与位姿；不做几何求值。",
            "实例名取自 XCAF 组件标签名（OCCT 由 NAUO 赋名）；路径匹配由比较脚本负责。",
            "本文件不自证；与通道 a 的差异由 s1_crosscheck_compare.py 落盘。",
        ],
        "occurrences": leaves, "assembly_occurrences": asms,
        "elapsed_s": round(time.time() - t0, 1),
    }
    a.out.parent.mkdir(parents=True, exist_ok=True)
    a.out.write_text(json.dumps(doc_out, ensure_ascii=False, indent=1), encoding="utf-8")
    print(f"写出 {a.out}")


if __name__ == "__main__":
    main()
