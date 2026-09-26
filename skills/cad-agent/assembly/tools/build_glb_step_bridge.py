#!/usr/bin/env python3
"""S3 前置（OLSK 新增，2026-09-02，v2）：手册 GLB 网格 ↔ STEP occurrence 的几何桥接。
动机：SOP（Workbook.csv）零件名属手册命名空间，STEP 产品名属 Fusion/厂商命名空间，词面重合 <6%；
手册 GLB 的 114 个步骤根节点下的网格名 = SOP 零件名，而 GLB 与 STEP 同源于同一 Fusion 模型，
所以用几何（世界包围盒尺寸/质心）把 GLB 网格绑到 STEP 节点，是确定性、可复核的身份桥，不是猜名。
v2 相对 v1 的改动（v1 结论：432 叶绑定、质心中位差 47 mm、大件漏配）：
  a. 候选节点 = 叶 occurrence + 子装配 occurrence（包围盒=后代叶并集）。手册把外购模块画成一个网格，
     其尺寸等于 STEP 里整个子装配（S5 solver unit / PURCHASED_MODULE 语义），只允许叶候选会错配到某个内部体。
  b. 平移不再取整体 AABB 中心差（两模型极值件不同，偏 ~50 mm），改为"尺寸匹配对的质心偏移众数"（5 mm 格），
     旋转仍按 24 个正交正定置换里尺寸吻合 + 最近邻最小者。
  c. 评分（v3）= max(AABB IoU, 0.5×包含度×尺寸相似) × 质心接近；IoU 对称，容许手册网格把螺杆+螺母画成一件或把多体件拆开。
     v2 用尺寸相似主导时 Y 丝杠（STEP 32×32×3000 vs 手册含螺母 80×62×3000）被判 UNMATCHED，实测帧拟合本身到 mm 级正确（主轴框完全重合）。
类别：UNIQUE / AMBIGUOUS（次优/最优 ≥ --ambig，典型为同排等尺寸螺钉）/ UNMATCHED（STEP 无此几何，如缺失紧固件；不得补建）。
用法：python build_glb_step_bridge.py --glb-nodes ... --occ S1清单 --stl-dir 网格导出 --out OUT --record-id ID
"""
import argparse, json, re, itertools, math, collections
from pathlib import Path
import numpy as np, trimesh
from scipy.spatial import cKDTree

ap = argparse.ArgumentParser()
ap.add_argument("--glb-nodes", required=True, type=Path); ap.add_argument("--occ", required=True, type=Path)
ap.add_argument("--stl-dir", required=True, type=Path); ap.add_argument("--out", required=True, type=Path)
ap.add_argument("--record-id", required=True); ap.add_argument("--ambig", type=float, default=0.85)
ap.add_argument("--tol-m", type=float, default=0.004); ap.add_argument("--min-score", type=float, default=0.08)
ap.add_argument("--bodies", type=Path, default=None, help="S1 定义体清单：多体定义逐体作候选（体级身份）")
ap.add_argument("--asm-min-iou", type=float, default=0.3, help="子装配候选只在 IoU ≥ 此值时成立（外购模块网格应与整个子装配框大体重合）")
ap.add_argument("--lineage-ratio", type=float, default=0.9, help="同谱系候选（祖先/后代/同叶之体）分数比 ≥ 此值视为同一物：取祖先最高的子装配；体 vs 其整叶取整叶")
a = ap.parse_args()

G = json.loads(a.glb_nodes.read_text(encoding="utf-8"))["nodes"]
S1 = json.loads(a.occ.read_text(encoding="utf-8"))
man = json.loads((a.stl_dir / "manifest.json").read_text(encoding="utf-8"))["parts"]
name_to_stem = {v["name"]: k for k, v in man.items() if "error" not in v}

# ---- 1. 叶世界 AABB（STEP mm）
cache = {}
def local_verts(product):
    stem = name_to_stem.get(product)
    if stem is None: return None
    if stem not in cache:
        m = trimesh.load(a.stl_dir / f"{stem}.stl", force="mesh")
        v = np.asarray(m.vertices, dtype=float)
        if len(v) > 3000:
            try: v = np.asarray(m.convex_hull.vertices, dtype=float)
            except Exception: v = v[:: max(1, len(v) // 3000)]
        cache[stem] = v
    return cache[stem]
nodes, no_mesh = [], []
for o in S1["occurrences"]:
    v = local_verts(o["product"])
    if v is None or len(v) == 0:
        no_mesh.append(o["occ_uid"]); continue
    W = np.array(o["world"]); w = v @ W[:3, :3].T + W[:3, 3]
    nodes.append({"uid": o["occ_uid"], "product": o["product"], "kind": "leaf", "depth": o["depth"], "min": w.min(0), "max": w.max(0)})
# 子装配 = 后代叶并集（按路径前缀）
leaf_by_path = {n["uid"].replace("nauo:", ""): n for n in nodes}
asm_nodes = []
for asm in S1["assembly_occurrences"]:
    pre = asm["nauo_path"] + "/"
    mem = [n for p, n in leaf_by_path.items() if p.startswith(pre)]
    if not mem: continue
    lo = np.min([n["min"] for n in mem], axis=0); hi = np.max([n["max"] for n in mem], axis=0)
    asm_nodes.append({"uid": asm["occ_uid"], "product": asm["product"], "kind": "asm", "depth": asm["depth"], "min": lo, "max": hi, "members": len(mem)})
# v4：多体定义的体级候选（体在定义局部系 → 用 occurrence 世界矩阵实例化）
body_nodes = []
if a.bodies is not None:
    BD = json.loads(a.bodies.read_text(encoding="utf-8"))["definitions"]
    for o in S1["occurrences"]:
        d = BD.get(o["product"])
        if not d or len(d.get("bodies", [])) < 2: continue
        W = np.array(o["world"])
        for b in d["bodies"]:
            if "bbox_min" not in b: continue
            corners = np.array(list(itertools.product(*zip(b["bbox_min"], b["bbox_max"]))))
            w = corners @ W[:3, :3].T + W[:3, 3]
            body_nodes.append({"uid": o["occ_uid"], "body_id": b["body_id"], "product": o["product"], "kind": "body", "depth": o["depth"], "min": w.min(0), "max": w.max(0)})
allnodes = nodes + asm_nodes + body_nodes
NMIN = np.array([n["min"] for n in allnodes]); NMAX = np.array([n["max"] for n in allnodes])
NC = (NMIN + NMAX) / 2; NEXT = NMAX - NMIN
GM = np.array([g["bbox_min"] for g in G]); GX = np.array([g["bbox_max"] for g in G]); GC = (GM + GX) / 2; GEXT = GX - GM

# ---- 2. 帧拟合：旋转（尺寸+最近邻），平移（尺寸匹配对偏移众数）
def rot_candidates():
    for perm in itertools.permutations(range(3)):
        for signs in itertools.product([1, -1], repeat=3):
            R = np.zeros((3, 3))
            for i, (p, s) in enumerate(zip(perm, signs)): R[i, p] = s
            if round(np.linalg.det(R)) == 1: yield perm, signs, R
leafmask = np.array([n["kind"] == "leaf" for n in allnodes])
best = None
for perm, signs, R in rot_candidates():
    ext_r = np.abs(R @ (NMAX[leafmask].max(0) - NMIN[leafmask].min(0))) * 0.001
    ext_g = GX.max(0) - GM.min(0)
    ext_err = float(np.abs(ext_r - ext_g).sum())
    c = 0.001 * (R @ NC[leafmask].T).T
    t0 = (GM.min(0) + GX.max(0)) / 2 - 0.001 * (R @ ((NMAX[leafmask].max(0) + NMIN[leafmask].min(0)) / 2))
    d, _ = cKDTree(c + t0).query(GC, k=1)
    rec = {"perm": perm, "signs": signs, "R": R, "ext_err": ext_err, "nn": float(np.median(d)), "t0": t0}
    if best is None or (rec["nn"], rec["ext_err"]) < (best["nn"], best["ext_err"]): best = rec
R = best["R"]
# 平移精修：尺寸匹配对（排序边长对数比 < 0.03，且最大边 > 40 mm）的质心偏移众数
NEXT_r = np.sort(np.abs((R @ NEXT.T).T) * 0.001, axis=1); GEXT_s = np.sort(GEXT, axis=1)
NC_r = 0.001 * (R @ NC.T).T
offs = []
big = np.where(GEXT_s[:, 2] > 0.04)[0]
tree = cKDTree(np.log(NEXT_r + 1e-6))
for gi in big:
    idx = tree.query_ball_point(np.log(GEXT_s[gi] + 1e-6), r=0.03)
    for i in idx:
        offs.append(GC[gi] - NC_r[i])
offs = np.array(offs) if offs else np.zeros((1, 3))
# 众数：5 mm 网格投票
q = np.round(offs / 0.005).astype(int)
votes = collections.Counter(map(tuple, q))
mode, nvote = votes.most_common(1)[0]
sel = np.all(np.abs(q - np.array(mode)) <= 1, axis=1)
t = offs[sel].mean(0)
def to_glb(p): return 0.001 * (R @ np.asarray(p, dtype=float)) + t
BL = np.array([np.minimum(to_glb(n["min"]), to_glb(n["max"])) for n in allnodes]); BH = np.array([np.maximum(to_glb(n["min"]), to_glb(n["max"])) for n in allnodes])
BC = (BL + BH) / 2; BEXT_s = np.sort(BH - BL, axis=1)
frame = {"scale": 0.001, "R_step_to_glb": R.tolist(), "perm": best["perm"], "signs": best["signs"], "t_m": t.tolist(),
         "t_from_aabb_centers_m": best["t0"].tolist(), "translation_refit_pairs": int(len(offs)), "translation_mode_votes": int(nvote),
         "extent_error_m": best["ext_err"], "median_nn_before_refit_m": best["nn"]}

# ---- 3. 逐网格匹配
tol = a.tol_m
def vol(lo, hi): return float(np.prod(np.clip(hi - lo, 1e-6, None)))
results, per_node = [], collections.defaultdict(list)
for gi, g in enumerate(G):
    glo, ghi, gc, ge = GM[gi], GX[gi], GC[gi], GEXT_s[gi]
    ov_lo = np.maximum(BL - tol, glo); ov_hi = np.minimum(BH + tol, ghi)
    ok = np.all(ov_hi > ov_lo, axis=1); idx = np.nonzero(ok)[0]
    cands = []
    gsize = max(float(ge[2]), 0.01)
    for i in idx:
        # v3 评分：AABB IoU（对称，允许手册网格合并/拆分 STEP 件：如“Y Ball Screw With Nut”含螺母）× 质心接近
        inter = vol(np.maximum(BL[i], glo), np.minimum(BH[i], ghi)) if np.all(np.minimum(BH[i], ghi) > np.maximum(BL[i], glo)) else 0.0
        union = vol(glo, ghi) + vol(BL[i], BH[i]) - inter
        iou = inter / union if union > 0 else 0.0
        contain = min(1.0, vol(ov_lo[i], ov_hi[i]) / vol(glo, ghi))
        ext_sim = math.exp(-3.0 * float(np.mean(np.abs(np.log((BEXT_s[i] + 1e-4) / (ge + 1e-4))))))
        dc = float(np.linalg.norm(BC[i] - gc))
        prox = math.exp(-dc / (0.5 * gsize + 0.01))
        # v5：只用 IoU×质心接近（包含度通道会把螺钉配到包住它的垫块/主轴上）
        sc = iou * prox
        if allnodes[i]["kind"] == "asm" and iou < a.asm_min_iou:
            continue
        cands.append((sc, int(i), contain, ext_sim, dc))
    cands.sort(reverse=True)
    # v5 谱系折叠：分数在 lineage-ratio 内的候选若互为祖先/后代（路径前缀）或是同一叶的体，视为同一物；
    #   子装配之间取祖先最高者（单元），体 vs 整叶取整叶。折叠只发生在近等分候选之间，不改变异谱系歧义。
    if cands:
        top = cands[0][0]
        near = [c for c in cands if c[0] >= a.lineage_ratio * top]
        def path_of(i):
            n = allnodes[i]; return n["uid"].replace("nauo:", "")
        def same_lineage(i, j):
            pi, pj = path_of(i), path_of(j)
            return pi == pj or pi.startswith(pj + "/") or pj.startswith(pi + "/")
        if len(near) > 1 and all(same_lineage(near[0][1], c[1]) for c in near[1:]):
            asms = [c for c in near if allnodes[c[1]]["kind"] == "asm"]
            leafs = [c for c in near if allnodes[c[1]]["kind"] == "leaf"]
            if asms:
                pick = min(asms, key=lambda c: allnodes[c[1]]["depth"])   # 祖先最高
            elif leafs:
                pick = leafs[0]
            else:
                pick = near[0]
            rest = [c for c in cands if c[1] != pick[1] and not any(same_lineage(pick[1], c[1]) for _ in [0])]
            cands = [pick] + rest
            lineage_collapsed = [allnodes[c[1]]["uid"] for c in near if c[1] != pick[1]]
        else:
            lineage_collapsed = []
    else:
        lineage_collapsed = []
    name = re.sub(r"\.\d{3}$", "", g["node"])
    rec = {"glb_node": g["node"], "sop_name": name, "step": g["chain"][0], "extent_m": [round(float(x), 4) for x in GEXT[gi]]}
    if lineage_collapsed: rec["lineage_collapsed"] = lineage_collapsed
    if not cands or cands[0][0] < a.min_score:
        rec["class"] = "UNMATCHED"; rec["best"] = None
        if cands:
            s0, i0, c0, e0, d0 = cands[0]
            rec["nearest_rejected"] = {"uid": allnodes[i0]["uid"], "product": allnodes[i0]["product"], "kind": allnodes[i0]["kind"], "score": round(s0, 4), "ext_sim": round(e0, 3), "centroid_dist_mm": round(d0 * 1000, 1)}
    else:
        s0, i0, c0, e0, d0 = cands[0]; n0 = allnodes[i0]
        rec["best"] = {"occ_uid": n0["uid"], "body_id": n0.get("body_id"), "product": n0["product"], "kind": n0["kind"], "members": n0.get("members"), "score": round(s0, 4), "containment": round(c0, 4), "ext_sim": round(e0, 4), "centroid_dist_mm": round(d0 * 1000, 1)}
        if len(cands) > 1 and cands[1][0] / s0 >= a.ambig:
            rec["class"] = "AMBIGUOUS"
            rec["alternatives"] = [{"occ_uid": allnodes[i]["uid"], "body_id": allnodes[i].get("body_id"), "product": allnodes[i]["product"], "kind": allnodes[i]["kind"], "score": round(s, 4)} for s, i, *_ in cands[1:5]]
        else:
            rec["class"] = "UNIQUE"
        per_node[n0.get("body_id") or n0["uid"]].append(name)
    results.append(rec)
cls = collections.Counter(r["class"] for r in results)
bound = set(per_node) | {bn["uid"] for bn in body_nodes if bn["body_id"] in per_node}; leaf_uids = [n["uid"] for n in nodes]
# 叶被绑定 = 直接绑定或其祖先子装配被绑定
asm_bound_prefixes = [u.replace("nauo:", "") + "/" for u in bound if any(n["uid"] == u and n["kind"] == "asm" for n in asm_nodes)]
def covered(uid):
    if uid in bound: return True
    p = uid.replace("nauo:", "")
    return any(p.startswith(pre) for pre in asm_bound_prefixes)
unbound = [u for u in leaf_uids if not covered(u)]
doc = {
    "$schema": "assembly-ontology.s3-glb-step-bridge/v5", "record_id": a.record_id,
    "purpose": "手册 GLB 网格（SOP 命名空间）↔ STEP 节点（叶或子装配，Fusion 命名空间）的几何身份桥；供 S3 绑定与 S5 单元划分消费。",
    "inputs": {"glb_nodes": str(a.glb_nodes), "s1": str(a.occ), "stl_manifest": str(a.stl_dir / "manifest.json")},
    "frame_fit": frame | {"up_axis_evidence": "glTF 为 Y-up；R 第二行 = STEP 里对应 GLB +Y 的轴"},
    "summary": {"glb_meshes": len(G), "step_leaf_occurrences": len(S1["occurrences"]), "leaves_with_mesh": len(nodes), "leaves_without_mesh": len(no_mesh),
                "subassembly_candidates": len(asm_nodes), "body_candidates": len(body_nodes), "classes": dict(cls),
                "nodes_bound_body": sum(1 for k in per_node if "#b" in k),
                "nodes_bound": len(bound), "nodes_bound_leaf": sum(1 for u in bound if any(n["uid"] == u for n in nodes)),
                "nodes_bound_asm": sum(1 for u in bound if any(n["uid"] == u for n in asm_nodes)),
                "leaves_covered": len(leaf_uids) - len(unbound), "leaves_unbound": len(unbound),
                "nodes_bound_by_multiple_meshes": sum(1 for v in per_node.values() if len(v) > 1)},
    "thresholds": {"aabb_tol_m": tol, "ambiguity_ratio": a.ambig, "min_score": a.min_score, "asm_min_iou": a.asm_min_iou, "lineage_ratio": a.lineage_ratio, "score": "IoU(AABB)×exp(-质心距/(0.5×网格最大边+10mm))"},
    "claim_boundary": ["几何桥只说明‘同一位置、同一尺寸’；同排等尺寸重复件会 AMBIGUOUS，须由 S3 用步骤内计数与共配合消解。",
                       "绑定到子装配节点 = 该网格代表外购/预装模块整体（S5 solver unit 候选），成员按 S1 子树列出，不是任意成组。",
                       "UNMATCHED 表示 STEP 中无对应实体（如缺失的紧固件）或 GLB 特有辅助几何；不得补建。",
                       "帧拟合只用于身份桥，不改变 S1 世界位姿权威。"],
    "leaves_without_mesh": no_mesh, "leaves_unbound": unbound, "matches": results,
}
a.out.parent.mkdir(parents=True, exist_ok=True)
a.out.write_text(json.dumps(doc, ensure_ascii=False, indent=1), encoding="utf-8")
print(json.dumps({"frame_fit": {k: frame[k] for k in ["perm", "signs", "t_m", "t_from_aabb_centers_m", "translation_refit_pairs", "translation_mode_votes", "extent_error_m"]}, "summary": doc["summary"]}, ensure_ascii=False, indent=1))
