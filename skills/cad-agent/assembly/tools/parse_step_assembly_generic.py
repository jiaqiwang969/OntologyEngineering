#!/usr/bin/env python3
"""S1 身份绑定（第一步，参数化移植版）：从 STEP 装配结构解出 occurrence 树与世界位姿。

源自灵犀 X1 `tools/parse_step_assembly.py`（算法逐行保留），改动仅限：

1. CLI 参数化：--step/--out/--encoding/--record-id/--case，不再硬绑 X1 路径。
2. 稳定身份：新增 `occ_uid = "nauo:" + nauo_path`（装配路径派生，重跑不变），
   回应 X1 审计指出的 `occ_id 解析顺序编号不稳定` 缺口；`occ_id` 顺序编号仅
   保留作可读别名。
3. 同时输出非叶（子装配）occurrence 及其世界位姿，供 S5 solver-unit 使用。
4. schema 改为 assembly-ontology.s1-occurrence-manifest/v2。

解析链（SolidWorks STEP 惯例，AP203/AP214 同构）：
    NEXT_ASSEMBLY_USAGE_OCCURRENCE(parent_pd, child_pd)
      ← PRODUCT_DEFINITION_SHAPE(nauo)
      ← CONTEXT_DEPENDENT_SHAPE_REPRESENTATION(rep_rel, pds)
    子件世界变换 = 父件世界变换 · M(transform_item_1)
（X1 实测 item_2 恒为单位阵、位姿在 item_1；保留 M2⁻¹ 项防别的导出器不满足该前提。）
"""
import argparse
import hashlib
import json
import re
import sys
from collections import defaultdict
from pathlib import Path

REF = re.compile(rb"#(\d+)")


def unescape_step(s):
    """ISO-10303-21 字符串转义：\\X2\\..\\X0\\ 为 UTF-16BE 十六进制段，\\X4\\ 为 UTF-32BE，
    \\S\\c 为 Latin 位移，\\X\\hh 为单字节。SolidWorks 导出的非 ASCII 名称走 \\X2\\。"""
    out, i, n = [], 0, len(s)
    while i < n:
        if s.startswith("\\X2\\", i):
            j = s.find("\\X0\\", i + 4)
            if j < 0:
                out.append(s[i:]); break
            hexs = s[i + 4:j]
            try:
                out.append(bytes.fromhex(hexs).decode("utf-16-be"))
            except Exception:
                out.append(hexs)
            i = j + 4
        elif s.startswith("\\X4\\", i):
            j = s.find("\\X0\\", i + 4)
            if j < 0:
                out.append(s[i:]); break
            hexs = s[i + 4:j]
            try:
                out.append(bytes.fromhex(hexs).decode("utf-32-be"))
            except Exception:
                out.append(hexs)
            i = j + 4
        elif s.startswith("\\X\\", i) and i + 5 <= n:
            try:
                out.append(bytes([int(s[i + 3:i + 5], 16)]).decode("latin-1"))
            except Exception:
                out.append(s[i:i + 5])
            i += 5
        elif s.startswith("\\S\\", i) and i + 4 <= n:
            out.append(chr(ord(s[i + 3]) + 128))
            i += 4
        elif s.startswith("\\\\", i):
            out.append("\\"); i += 2
        else:
            out.append(s[i]); i += 1
    return "".join(out)


def make_dec(name_encoding):
    def dec(b):
        try:
            raw = b.decode(name_encoding)
        except UnicodeDecodeError:
            return b.decode("latin-1") + "  <解码失败>"
        return unescape_step(raw)
    return dec


def entities(path, wanted):
    """流式扫一遍，只留下类型在 wanted 里的实体。返回 {id: (type, argstring)}。"""
    out = {}
    buf = b""
    head = re.compile(rb"^#(\d+)\s*=\s*\(?\s*([A-Z_0-9]+)?")
    with open(path, "rb") as f:
        for line in f:
            buf += line
            if b";" not in line:
                continue
            for stmt in buf.split(b";"):
                stmt = stmt.strip()
                if not stmt.startswith(b"#"):
                    continue
                m = head.match(stmt)
                if not m:
                    continue
                typ = m.group(2)
                if typ is None or typ not in wanted:
                    continue
                out[int(m.group(1))] = (typ.decode(), stmt)
            buf = b""
    return out


def refs(stmt):
    body = stmt.split(b"=", 1)[1] if b"=" in stmt else stmt
    return [int(x) for x in REF.findall(body)]


def mat_from_a2p(a2p, pts, dirs):
    r = refs(a2p)
    loc = pts.get(r[0]) if r else None
    ax = dirs.get(r[1]) if len(r) > 1 else None
    rd = dirs.get(r[2]) if len(r) > 2 else None
    o = loc or (0.0, 0.0, 0.0)
    z = ax or (0.0, 0.0, 1.0)
    x = rd or (1.0, 0.0, 0.0)

    def sub(a, b): return (a[0] - b[0], a[1] - b[1], a[2] - b[2])
    def dot(a, b): return a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
    def scale(a, k): return (a[0] * k, a[1] * k, a[2] * k)
    def norm(a):
        n = dot(a, a) ** 0.5
        return scale(a, 1.0 / n) if n > 1e-12 else (0.0, 0.0, 1.0)
    def cross(a, b):
        return (a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0])

    z = norm(z)
    x = norm(sub(x, scale(z, dot(x, z))))
    y = cross(z, x)
    return [[x[0], y[0], z[0], o[0]],
            [x[1], y[1], z[1], o[1]],
            [x[2], y[2], z[2], o[2]],
            [0.0, 0.0, 0.0, 1.0]]


def mul(A, B):
    return [[sum(A[i][k] * B[k][j] for k in range(4)) for j in range(4)] for i in range(4)]


def inv_rigid(M):
    R = [[M[i][j] for j in range(3)] for i in range(3)]
    t = [M[i][3] for i in range(3)]
    Rt = [[R[j][i] for j in range(3)] for i in range(3)]
    ti = [-sum(Rt[i][k] * t[k] for k in range(3)) for i in range(3)]
    return [Rt[0] + [ti[0]], Rt[1] + [ti[1]], Rt[2] + [ti[2]], [0, 0, 0, 1]]


def det3(M):
    return (M[0][0] * (M[1][1] * M[2][2] - M[1][2] * M[2][1])
            - M[0][1] * (M[1][0] * M[2][2] - M[1][2] * M[2][0])
            + M[0][2] * (M[1][0] * M[2][1] - M[1][1] * M[2][0]))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--step", required=True, type=Path)
    ap.add_argument("--out", required=True, type=Path)
    ap.add_argument("--encoding", default="utf-8",
                    help="源名称编码（X1 用 gb18030，Poppy/西文导出常为 utf-8/latin-1）")
    ap.add_argument("--record-id", required=True)
    ap.add_argument("--case", required=True, help="案例名，写入 manifest")
    args = ap.parse_args()
    STEP, OUT = args.step, args.out
    dec = make_dec(args.encoding)

    if not STEP.exists():
        print(f"缺 STEP：{STEP}", file=sys.stderr)
        return 2
    print("[1/3] 扫装配结构…", flush=True)
    W1 = {b"PRODUCT", b"PRODUCT_DEFINITION", b"PRODUCT_DEFINITION_FORMATION",
          b"PRODUCT_DEFINITION_FORMATION_WITH_SPECIFIED_SOURCE",
          b"PRODUCT_DEFINITION_SHAPE", b"NEXT_ASSEMBLY_USAGE_OCCURRENCE",
          b"CONTEXT_DEPENDENT_SHAPE_REPRESENTATION", b"ITEM_DEFINED_TRANSFORMATION",
          b"REPRESENTATION_RELATIONSHIP", None}
    ents = entities(STEP, W1)
    rr = {}
    with open(STEP, "rb") as f:
        for line in f:
            if b"REPRESENTATION_RELATIONSHIP_WITH_TRANSFORMATION" not in line:
                continue
            m = re.match(rb"^#(\d+)\s*=", line)
            if m:
                rr[int(m.group(1))] = line.strip()
    print(f"      实体 {len(ents)} · 带变换的表示关系 {len(rr)}")

    prod, pdf, pd, pds, nauo, cdsr, idt = {}, {}, {}, {}, {}, {}, {}
    for i, (t, s) in ents.items():
        if t == "PRODUCT":
            m = re.search(rb"PRODUCT \( '([^']*)'", s)
            prod[i] = dec(m.group(1)) if m else "?"
        elif t.startswith("PRODUCT_DEFINITION_FORMATION"):
            pdf[i] = refs(s)[0] if refs(s) else None
        elif t == "PRODUCT_DEFINITION":
            pd[i] = refs(s)
        elif t == "PRODUCT_DEFINITION_SHAPE":
            pds[i] = refs(s)[0] if refs(s) else None
        elif t == "NEXT_ASSEMBLY_USAGE_OCCURRENCE":
            r = refs(s)
            nm = re.search(rb"NEXT_ASSEMBLY_USAGE_OCCURRENCE \( '([^']*)'", s)
            nauo[i] = {"parent_pd": r[0], "child_pd": r[1], "tag": dec(nm.group(1)) if nm else ""}
        elif t == "CONTEXT_DEPENDENT_SHAPE_REPRESENTATION":
            r = refs(s)
            cdsr[i] = {"rr": r[0], "pds": r[1]}
        elif t == "ITEM_DEFINED_TRANSFORMATION":
            idt[i] = refs(s)

    pd_product = {}
    for i, r in pd.items():
        f = r[0] if r else None
        pd_product[i] = prod.get(pdf.get(f)) if f in pdf else None

    print("[2/3] 取变换用到的 AXIS2_PLACEMENT_3D…", flush=True)
    need_a2p = set()
    idt_by_rr = {}
    for i, line in rr.items():
        m = re.search(rb"REPRESENTATION_RELATIONSHIP_WITH_TRANSFORMATION\s*\(\s*#(\d+)", line)
        if m:
            idt_by_rr[i] = int(m.group(1))
    for k in idt_by_rr.values():
        for a in idt.get(k, [])[:2]:
            need_a2p.add(a)
    a2p = {}
    with open(STEP, "rb") as f:
        for line in f:
            if b"AXIS2_PLACEMENT_3D" not in line:
                continue
            m = re.match(rb"^#(\d+)\s*=", line)
            if m and int(m.group(1)) in need_a2p:
                a2p[int(m.group(1))] = line.strip()
    need_geo = set()
    for s in a2p.values():
        need_geo.update(refs(s)[:3])
    print(f"      A2P {len(a2p)} · 需要的点/方向 {len(need_geo)}")

    print("[3/3] 取点与方向…", flush=True)
    pts, dirs = {}, {}
    num = re.compile(rb"\(\s*([-\d.E+]+)\s*,\s*([-\d.E+]+)\s*,\s*([-\d.E+]+)\s*\)")
    with open(STEP, "rb") as f:
        for line in f:
            if b"CARTESIAN_POINT" not in line and b"DIRECTION" not in line:
                continue
            m = re.match(rb"^#(\d+)\s*=\s*([A-Z_]+)", line)
            if not m or int(m.group(1)) not in need_geo:
                continue
            v = num.search(line)
            if not v:
                continue
            xyz = tuple(float(x) for x in v.groups())
            (pts if m.group(2) == b"CARTESIAN_POINT" else dirs)[int(m.group(1))] = xyz

    cdsr_by_pds = {v["pds"]: v for v in cdsr.values()}
    edges = []
    for ni, n in nauo.items():
        holder = [p for p, target in pds.items() if target == ni]
        M = None
        if holder and holder[0] in cdsr_by_pds:
            c = cdsr_by_pds[holder[0]]
            k = idt_by_rr.get(c["rr"])
            if k and len(idt.get(k, [])) >= 2:
                a1, a2 = idt[k][0], idt[k][1]
                if a1 in a2p and a2 in a2p:
                    M = mul(mat_from_a2p(a2p[a1], pts, dirs),
                            inv_rigid(mat_from_a2p(a2p[a2], pts, dirs)))
        edges.append({"nauo": ni, "tag": n["tag"], "parent": n["parent_pd"],
                      "child": n["child_pd"], "M": M})

    children = defaultdict(list)
    for e in edges:
        children[e["parent"]].append(e)
    all_children = {e["child"] for e in edges}
    roots = [p for p in children if p not in all_children]

    occ, asm_occ, missing, depth_max = [], [], 0, 0
    IDENT = [[1.0 if i == j else 0.0 for j in range(4)] for i in range(4)]

    def walk(p, W, path, depth, parent_prod=None):
        nonlocal missing, depth_max
        depth_max = max(depth_max, depth)
        # 稳定遍历序：按 NAUO tag 再 nauo id 排序，杜绝解析顺序影响编号
        for e in sorted(children.get(p, []), key=lambda e: (e["tag"], e["nauo"])):
            if e["M"] is None:
                missing += 1
                Wc = W
            else:
                Wc = mul(W, e["M"])
            nm = pd_product.get(e["child"]) or "?"
            sub = path + [e["tag"]]
            uid = "nauo:" + "/".join(sub)
            rec = {"occ_id": None, "occ_uid": uid, "product": nm,
                   "parent_product": parent_prod,
                   "nauo_path": "/".join(sub), "depth": depth + 1,
                   "world": [[round(v, 9) for v in row] for row in Wc],
                   "det": round(det3(Wc), 9)}
            if e["child"] not in children:
                rec["occ_id"] = f"occ_{len(occ):04d}"
                occ.append(rec)
            else:
                rec["occ_id"] = f"asm_{len(asm_occ):04d}"
                asm_occ.append(rec)
                walk(e["child"], Wc, sub, depth + 1, nm)

    for r in sorted(roots):
        walk(r, IDENT, [], 0, pd_product.get(r))

    inst = defaultdict(int)
    for o in occ:
        inst[o["product"]] += 1

    doc = {
        "$schema": "assembly-ontology.s1-occurrence-manifest/v2",
        "record_id": args.record_id,
        "case": args.case,
        "purpose": "S1 身份绑定第一步：STEP 装配结构解出的 occurrence 树与世界位姿。不含几何求值。",
        "method_ref": "灵犀X1 tools/parse_step_assembly.py 参数化移植；算法一致，新增 occ_uid 稳定身份与非叶 occurrence 输出。",
        "source": {"path": str(STEP),
                   "sha256": hashlib.sha256(STEP.read_bytes()).hexdigest(),
                   "size_bytes": STEP.stat().st_size},
        "name_encoding": args.encoding,
        "structure": {
            "product_definitions": len(pd),
            "products": len(prod),
            "nauo_edges": len(edges),
            "edges_without_transform": missing,
            "roots": len(roots),
            "root_products": [pd_product.get(r) for r in sorted(roots)],
            "max_depth": depth_max,
            "leaf_occurrences": len(occ),
            "subassembly_occurrences": len(asm_occ),
            "nodes_including_root": len(occ) + len(asm_occ) + len(roots),
            "unique_leaf_products": len(inst),
            "subassembly_products": sorted({o["parent_product"] for o in occ
                                            if o["parent_product"] and o["depth"] > 1}),
        },
        "chirality": {
            "说明": "STEP 的 AXIS2_PLACEMENT_3D 只能表达右手系，装配变换必为纯旋转（det=+1）。镜像件必然是独立 PRODUCT，手性判据必须落在几何上。",
            "negative_determinant_occurrences": sum(1 for o in occ if o["det"] < 0),
        },
        "instances_per_product": dict(sorted(inst.items(), key=lambda x: -x[1])),
        "claim_boundary": [
            "只解装配拓扑与变换链，没有做任何几何求值：没有体积、包围盒、面或碰撞信息。",
            "occ_uid 由 NAUO 装配路径派生，重跑稳定；occ_id 仅是稳定遍历序下的可读别名。",
            "世界位姿尚未与 OCCT/XCAF 独立交叉验证；交叉验证是 S1 的独立后续门禁，本文件不自证。",
            "叶子判定按『该 product_definition 不再作为任何 NAUO 的父』，未核对它是否真的带实体几何。",
        ],
        "occurrences": occ,
        "assembly_occurrences": asm_occ,
    }
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps(doc, ensure_ascii=False, indent=1), encoding="utf-8")
    print(f"\n写出 {OUT}")
    for k, v in doc["structure"].items():
        print(f"  {k}: {v}")
    print(f"  负行列式 occurrence: {doc['chirality']['negative_determinant_occurrences']}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
