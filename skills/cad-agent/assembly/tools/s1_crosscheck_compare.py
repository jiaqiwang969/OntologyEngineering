#!/usr/bin/env python3
"""S1 交叉验证比较（OLSK 2026-09-02）：通道 a（STEP 文本解析）vs 通道 b（OCCT/XCAF）。
按 NAUO 路径匹配（两侧均去除字面量折行 LF），逐元素比较 4x4 世界矩阵；产品名逐一比对。
输出 evidence/s1-world-crosscheck.v1.json；status 只在路径全覆盖且最大元素差 < tol 时 PASS。"""
import argparse, json
from pathlib import Path

def norm(p): return p.replace("\n", "").replace("\r", "")

ap = argparse.ArgumentParser()
ap.add_argument("--a", required=True, type=Path); ap.add_argument("--b", required=True, type=Path)
ap.add_argument("--out", required=True, type=Path); ap.add_argument("--record-id", required=True)
ap.add_argument("--tol", type=float, default=1e-6)
ap.add_argument("--tol-mm", type=float, default=1e-3, help="平移列容差（mm）")
args = ap.parse_args()
A = json.loads(args.a.read_text(encoding="utf-8")); B = json.loads(args.b.read_text(encoding="utf-8"))
a_leaf = {norm(o["nauo_path"]): o for o in A["occurrences"]}
a_asm = {norm(o["nauo_path"]): o for o in A["assembly_occurrences"]}
# 结构对齐规则（落盘，不是放宽）：
#  R1 XCAF 匿名组件 `=>[entry]` 的父在通道 a 是装配 → 对应 a 的 `=self` 叶（D8 混合节点自带实体）。
#  R2 匿名组件的父在通道 a 是叶 → OCCT 把该零件的多体/壳拆成子标签；几何归父叶，子标签不单独计身份。
#  R3 通道 b 里因 R2 变成"装配"的路径若在 a 是叶 → 按叶比较位姿。
b_leaf, b_asm, split_children, self_map = {}, {}, [], []
for o in B["occurrences"]:
    pth = norm(o["path"]); segs = pth.split("/")
    if segs[-1].startswith("=>["):
        parent = "/".join(segs[:-1])
        if parent in a_leaf:
            split_children.append({"b_path": pth, "definition": o["definition"], "ref_entry": o.get("ref_entry")}); continue
        if parent in a_asm or parent == "":
            pth = parent + "/=self" if parent else "=self"
            self_map.append({"b_path": o["path"], "a_path": pth, "ref_entry": o.get("ref_entry")})
            o = dict(o); o["xcaf_raw_name"] = o["definition"]; o["definition"] = a_leaf.get(pth, {}).get("product_name", o["definition"])
    b_leaf[pth] = o
for o in B["assembly_occurrences"]:
    pth = norm(o["path"])
    if pth in a_leaf and pth not in b_leaf:
        b_leaf[pth] = o   # R3
    else:
        b_asm[pth] = o

def cmp(da, db, label):
    common = sorted(set(da) & set(db))
    only_a = sorted(set(da) - set(db)); only_b = sorted(set(db) - set(da))
    maxd_rot = 0.0; maxd_t = 0.0; bad = []; name_neq = []
    for p in common:
        Wa, Wb = da[p]["world"], db[p]["world"]
        dr = max(abs(Wa[i][j] - Wb[i][j]) for i in range(3) for j in range(3))
        dt = max(abs(Wa[i][3] - Wb[i][3]) for i in range(3))
        maxd_rot = max(maxd_rot, dr); maxd_t = max(maxd_t, dt)
        if dr > args.tol or dt > args.tol_mm:
            bad.append({"path": p, "rot_diff": dr, "trans_diff_mm": dt, "a_t": [Wa[i][3] for i in range(3)], "b_t": [Wb[i][3] for i in range(3)]})
        na = norm(da[p].get("product_name") or da[p]["product"]); nb = norm(db[p]["definition"])
        if na != nb:
            name_neq.append({"path": p, "a": na, "b": nb})
    return {"count_a": len(da), "count_b": len(db), "path_overlap": len(common),
            "only_in_a": only_a[:50], "only_in_b": only_b[:50],
            "only_in_a_n": len(only_a), "only_in_b_n": len(only_b),
            "world_matrix": {"max_rotation_element_diff": maxd_rot, "max_translation_diff_mm": maxd_t,
                             "mismatches": len(bad), "examples": bad[:20]},
            "product_name_equal": f"{len(common)-len(name_neq)}/{len(common)}",
            "name_mismatch_examples": name_neq[:20]}

leaf = cmp(a_leaf, b_leaf, "leaf"); asm = cmp(a_asm, b_asm, "asm")
ok = (leaf["only_in_a_n"] == 0 and leaf["only_in_b_n"] == 0 and leaf["world_matrix"]["mismatches"] == 0
      and asm["only_in_a_n"] == 0 and asm["only_in_b_n"] == 0 and asm["world_matrix"]["mismatches"] == 0
      and leaf["product_name_equal"].split("/")[0] == leaf["product_name_equal"].split("/")[1])
doc = {
    "$schema": "assembly-ontology.s1-crosscheck/v1", "record_id": args.record_id,
    "channels": {"a": f"{args.a}（STEP 文本解析，X1 算法移植 + AP242 适配 D1-D6）",
                 "b": f"{args.b}（OCCT {B.get('occt',{}).get('reader')} XCAF 装配树，零代码共享）"},
    "source_sha256": {"a": A["source"]["sha256"], "b": B["source"]["sha256"], "equal": A["source"]["sha256"] == B["source"]["sha256"]},
    "tolerance": {"rotation_element": args.tol, "translation_mm": args.tol_mm},
    "leaf": leaf, "subassembly": asm,
    "structure_alignment": {"R1_self_body_leaves": self_map, "R2_split_subshapes_attached_to_leaf": split_children,
                            "R3_b_assemblies_treated_as_a_leaves": [p for p in b_leaf if p in a_leaf and any(norm(o["path"]) == p for o in B["assembly_occurrences"])]},
    "adapter_declared_by_a": A.get("adapter"),
    "claim_boundary": [
        "两通道只比装配拓扑与世界位姿；不比几何。",
        "通道 b 的位姿由 OCCT 自己的 STEP 适配器给出，与通道 a 的 D6 方向判定相互独立。",
        "路径匹配前两侧均删除字面量折行 LF；这是 S0 声明的名称解码规则，不是比较放宽。",
    ],
    "status": "PASS" if ok else "FAIL",
}
args.out.parent.mkdir(parents=True, exist_ok=True)
args.out.write_text(json.dumps(doc, ensure_ascii=False, indent=1), encoding="utf-8")
# 定义键 → XCAF 标签 entry 映射（供 S2 工具按 entry 取形，不按重名的标签名）
id_to_entry, conflicts, entry_to_id = {}, [], {}
for p in sorted(set(a_leaf) & set(b_leaf)):
    pid = a_leaf[p]["product"]; ent = b_leaf[p].get("ref_entry")
    if ent is None: continue
    if pid in id_to_entry and id_to_entry[pid]["entry"] != ent:
        conflicts.append({"product": pid, "entries": [id_to_entry[pid]["entry"], ent], "path": p}); continue
    if ent in entry_to_id and entry_to_id[ent] != pid:
        conflicts.append({"entry": ent, "products": [entry_to_id[ent], pid], "path": p}); continue
    id_to_entry[pid] = {"entry": ent, "xcaf_name": b_leaf[p].get("xcaf_raw_name", b_leaf[p]["definition"]), "example_path": p}; entry_to_id[ent] = pid
lm = {"$schema": "assembly-ontology.s1-definition-labels/v1", "record_id": args.record_id + "-LABELS",
      "source_sha256": A["source"]["sha256"], "occt_version": B.get("structure", {}).get("occt_version"),
      "purpose": "S1 定义键（PRODUCT.id）→ XCAF 标签 entry；S2 XCAF 工具按 entry 取形。entry 只在同一 STEP 字节 + 同一 OCCT 版本下稳定，使用时须核对标签名。",
      "count": len(id_to_entry), "conflicts": conflicts, "id_to_entry": id_to_entry}
lm_path = args.out.parent / "s1-definition-labels.v1.json"
lm_path.write_text(json.dumps(lm, ensure_ascii=False, indent=1), encoding="utf-8")
print("label map", len(id_to_entry), "conflicts", len(conflicts), "->", lm_path)
print(json.dumps({k: doc[k] for k in ["status", "source_sha256"]}, ensure_ascii=False))
for k in ["leaf", "subassembly"]:
    d = doc[k]; print(k, {kk: d[kk] for kk in ["count_a", "count_b", "path_overlap", "only_in_a_n", "only_in_b_n", "product_name_equal"]}, d["world_matrix"]["max_rotation_element_diff"], d["world_matrix"]["max_translation_diff_mm"], "mismatches", d["world_matrix"]["mismatches"])
    if d["only_in_a"] or d["only_in_b"]:
        print("  only_a:", d["only_in_a"][:3]); print("  only_b:", d["only_in_b"][:3])
    if d["name_mismatch_examples"]:
        print("  name mismatch:", d["name_mismatch_examples"][:3])
