#!/usr/bin/env python3
"""S2 扩展：平面贴合 + 锥面配合界面图。

补 tools/build_interface_graph.py 只做圆柱面留下的缺口。K-IF-01 本来就允许
CYLINDER / PLANE / CONE 三类解析面，只提圆柱面会让靠端面贴合或锥面定位的配合整体失踪。

判据与 K-IF-01 **同构**：三条同时成立才算一个平面贴合界面
    ① 法向共线      两面世界法向夹角 <= NORMAL_ANGLE_MAX_DEG（法向无正负，取 |n_a·n_b|）
    ② 贴合距离      两面沿法向的距离 |(o_b-o_a)·n_a| <= FIT_GAP_MAX_MM
    ③ 法向投影重叠  两面在法向投影上的**真实重叠面积** >= MIN_OVERLAP_MM2
                    （真实重叠＝三角形对裁剪求交，不是 AABB 相交；AABB 只当预筛）

锥面配合另用一套（也是三条＋两条形状条件），见 cone_pairs 段。

**方向的正负几何给不出**：平面贴合只定出一条法向直线，插入是 +n 还是 -n 属于工艺信息，
本图一律标 withdraw_sense=null。不猜。

**不分桶**：①② 用 numpy 全量跑满 n(n-1)/2 对。
分桶（先把法向规范到半球、再按粗格 + 27 邻域）会**静默漏配对**——canon_dir 在半球
边界上不连续，两个只差 0.0535° 的反向法向会被翻到相隔 100 个粗格的地方，邻域够不着。
实测漏掉 6DUUFR_前壳 / 6DUUFH_后下壳 之间 103mm² 与 212mm² 两处真实贴合。
详见 plane_pairs 的注释。n=32263 时全量 numpy 只要几十秒，不值得为此冒漏解的风险。
重叠面积在**面 A 自己的** 2D 系里精确计算，不共享任何坐标系。

用法：python3 tools/build_plane_interface_graph.py [--angle 0.5] [--gap 0.05]
                [--min-overlap 1.0] [--min-area 1.0] [--sweep] [--jobs N]
输出：本体/S2-平面界面图.v1.json
      本体/S2-E类动件补轴.v1.json
"""
import argparse
import collections
import json
import math
import os
import re
import sys
import time
import unicodedata
from pathlib import Path

import numpy as np

# 参数化移植（重建-20260829）：路径由 CLI 注入；算法体与 X1 原文件逐行一致。
# S7 依赖设为可选：无 S7 时跳过 E 类补轴（那是 S7 之后的回补步骤）。
PLN = OCC = S7 = CYLGRAPH = OUT = OUT_E = None
RECORD_ID = None

# ---- 默认阈值。**全部落盘**，且 --sweep 会扫（K-PATH-03：报通过必须同报覆盖区间）----
D_ANGLE = 0.5        # deg  法向共线容差。与 K-IF-01 的 axis_angle_max_deg 取同一个数
D_GAP = 0.05         # mm   贴合距离容差
D_MIN_OVERLAP = 1.0  # mm²  法向投影真实重叠面积下限
D_MIN_AREA = 1.0     # mm²  参与配对的面的面积下限（不得低于抽取地板 0.5）
D_CONE_ANGLE = 1.0   # deg  锥面半角匹配容差
D_CONE_ECC = 0.08    # mm   锥面轴线偏心容差，沿用 K-IF-01
D_CONE_RGAP = 0.30   # mm   锥面在公共轴向站位上的半径差上限

TRI_PAIR_CAP = 40000  # 单对面的候选三角形对上限，超了退回 AABP 估计并标记


# ---------- 小工具 ----------
def canon_dir(d):
    """法向无正负：把方向规范到半球，使 ±d 落到同一个键。与 build_interface_graph.py 同义。"""
    for c in d:
        if abs(c) > 1e-9:
            return [-x for x in d] if c < 0 else list(d)
    return list(d)


def tangent_basis(n):
    """由法向定出平面内的一组正交基。**只在簇一级调用一次**，全簇共用，
    所以不存在『两个面各自定基、基不一致』的问题。"""
    n = np.asarray(n, float)
    n = n / np.linalg.norm(n)
    a = np.zeros(3)
    a[int(np.argmin(np.abs(n)))] = 1.0
    e1 = np.cross(n, a)
    e1 /= np.linalg.norm(e1)
    e2 = np.cross(n, e1)
    return e1, e2


def tri_clip_area(A, B):
    """三角形 A 被三角形 B 裁剪后的面积（Sutherland-Hodgman）。A/B 都是 3x2。"""
    if (B[1][0] - B[0][0]) * (B[2][1] - B[0][1]) - (B[1][1] - B[0][1]) * (B[2][0] - B[0][0]) < 0:
        B = B[::-1]
    poly = [(A[0][0], A[0][1]), (A[1][0], A[1][1]), (A[2][0], A[2][1])]
    for k in range(3):
        px, py = B[k][0], B[k][1]
        qx, qy = B[(k + 1) % 3][0], B[(k + 1) % 3][1]
        ex, ey = qx - px, qy - py
        new, n = [], len(poly)
        for i in range(n):
            cx, cy = poly[i]
            dx, dy = poly[(i + 1) % n]
            sc = ex * (cy - py) - ey * (cx - px)
            sd = ex * (dy - py) - ey * (dx - px)
            if sc >= 0.0:
                new.append((cx, cy))
            if (sc > 0.0) != (sd > 0.0) and sc != sd:
                t = sc / (sc - sd)
                new.append((cx + t * (dx - cx), cy + t * (dy - cy)))
        poly = new
        if not poly:
            return 0.0
    s = 0.0
    for i in range(len(poly)):
        x1, y1 = poly[i]
        x2, y2 = poly[(i + 1) % len(poly)]
        s += x1 * y2 - x2 * y1
    return abs(s) * 0.5


# ---------- 身份解析：K-ID-09。这是 build_interface_graph.py 里那套的忠实副本 ----------
# 不是抄图省事：三条纪律（有序候选键 / 两侧同归一化 / 宽松键只用于查询侧）在 X1 的 S2
# 一轮里连踩三次、每次都静默产出自洽的错图。这里原样复用，并在 summary 里把解析计数
# 与圆柱面图的解析计数**对账**——两边同一份 occurrence 清单、同一批零件名，
# 计数必须逐项相等，不等就是副本走样了。
CODE = re.compile(r"^[0-9A-Z]{6}_")
CONFIG_SUFFIX = re.compile(
    r"^(?:M|ST|Ø|φ)?\s*\d+(?:\.\d+)?\s*(?:[×xX*]\s*\d+(?:\.\d+)?)?\s*$|^M\d+(?:\.\d+)?$", re.I)


def nrm(x):
    """两侧必须用同一个归一化。曾经只在查询侧做了 NFKC 与 ╱→/，建键侧用原始字符串，
    结果 460 个 occurrence 名字明明一模一样却查不到。"""
    return unicodedata.normalize("NFKC", str(x or "").strip()).replace("╱", "/")


def keys(s):
    """候选键按**从严到宽**排序返回列表，不用集合。
    集合迭代顺序随 PYTHONHASHSEED 变，同一个 product 名的多个候选键若指向不同零件，
    两次运行会解到不同的几何。身份解析必须确定（K-ID-09）。"""
    s = nrm(s)
    cand = [s, CODE.sub("", s)]
    # 只有当后缀"长得像 SolidWorks 配置名"（含规格）时才去掉它。无条件 rsplit 会把
    # "6DUUED_手臂_关节01固定法兰1" 切出键 "手臂"，造成 38 个键冲突。
    if "_" in s:
        b, suf = s.rsplit("_", 1)
        if CONFIG_SUFFIX.match(suf):
            cand += [b, CODE.sub("", b)]
    out, seen = [], set()
    for k in cand:
        if k and k not in seen:
            seen.add(k)
            out.append(k)
    return out


def build_lut(by_name):
    """lut 只用**真实零件名**建键（全名 + 去编码前缀）。
    不要用"去配置后缀"的形式建键——那是给 occurrence 名做查询用的方向，
    反过来用会把不同零件挤到同一个键上。"""
    lut, owner, collisions = {}, {}, []
    for nm0 in sorted(by_name):
        nm = nrm(nm0)
        for k in (nm, CODE.sub("", nm)):
            if not k:
                continue
            if k in lut and lut[k] is not by_name[nm0]:
                collisions.append({"key": k, "kept": owner[k], "dropped": nm0})
            elif k not in lut:
                lut[k] = by_name[nm0]
                owner[k] = nm0
    return lut, collisions


# ---------- 实例化 ----------
def instantiate(pdoc, occs):
    """按零件定义提、按世界变换实例化（K-IF-03 / K-ID-02）。"""
    by_name = {}
    for f, v in pdoc.items():
        if "error" in v:
            continue
        by_name[(v.get("name") or "").replace(".step", "").strip()] = v
    lut, collisions = build_lut(by_name)

    P = {"occ": [], "o": [], "xw": [], "yw": [], "nw": [], "area": [],
         "tri": [], "proxy": [], "rev": []}
    C = {"occ": [], "p": [], "d": [], "ang": [], "rref": [], "v": [],
         "area": [], "uspan": []}
    miss, via = collections.Counter(), collections.Counter()
    for oi, o in enumerate(occs):
        rec = None
        for rank, k in enumerate(keys(o["product"])):
            if k in lut:
                rec = lut[k]
                via[f"rank{rank}"] += 1
                break
        if rec is None:
            miss[o["product"]] += 1
            continue
        W = o["world"]
        R = np.array([[W[i][j] for j in range(3)] for i in range(3)], float)
        T = np.array([W[i][3] for i in range(3)], float)
        for pf in rec.get("planes", ()):
            P["occ"].append(oi)
            P["o"].append(R @ np.array(pf["p"]) + T)
            P["xw"].append(R @ np.array(pf["xd"]))
            P["yw"].append(R @ np.array(pf["yd"]))
            P["nw"].append(R @ np.array(pf["n"]))
            P["area"].append(pf["area"])
            P["rev"].append(pf["rev"])
            t = pf.get("tri")
            if t:
                P["tri"].append(np.asarray(t, np.float64).reshape(-1, 3, 2))
                P["proxy"].append(False)
            else:
                # 没有三角化（或三角形超上限）的面：拿 uv AABB 当轮廓代理，两个三角形。
                # 这是**放宽**（矩形包住真实轮廓），会高估重叠，所以逐对标记、落盘计数。
                u0, u1, v0, v1 = pf["uv"]
                P["tri"].append(np.array([[[u0, v0], [u1, v0], [u1, v1]],
                                          [[u0, v0], [u1, v1], [u0, v1]]], np.float64))
                P["proxy"].append(True)
        for cf in rec.get("cones", ()):
            C["occ"].append(oi)
            C["p"].append(R @ np.array(cf["p"]) + T)
            C["d"].append(R @ np.array(cf["d"]))
            C["ang"].append(cf["半角rad"])
            C["rref"].append(cf["rref"])
            C["v"].append(cf["v"])
            C["area"].append(cf["area"])
            C["uspan"].append(cf["u_span_deg"])
    for k in ("o", "xw", "yw", "nw"):
        P[k] = np.asarray(P[k], float)
    P["occ"] = np.asarray(P["occ"], np.int32)
    P["area"] = np.asarray(P["area"], float)
    P["proxy"] = np.asarray(P["proxy"], bool)
    P["rev"] = np.asarray(P["rev"], np.int8)
    for k in ("p", "d"):
        C[k] = np.asarray(C[k], float) if C[k] else np.zeros((0, 3))
    for k in ("ang", "rref", "area", "uspan"):
        C[k] = np.asarray(C[k], float)
    C["v"] = np.asarray(C["v"], float) if len(C["v"]) else np.zeros((0, 2))
    C["occ"] = np.asarray(C["occ"], np.int32)
    return P, C, miss, via, collisions


# ---------- 平面配对 ----------
def prep_planes(P, log=print):
    """逐面预计算，与阈值无关，所以**只算一次**，整轮敏感性扫描共用。

      cn    规范化到半球的世界法向（法向无正负）
      foot  平面上离世界原点最近的点 = (o·cn)*cn。两个共面的面 foot 完全相同，
            所以 foot 就是平面的"轴垂足"类比，可以直接拿去分桶（K-IF-03 同一套做法）
      e1,e2 由**该面自己的** cn 定出的平面内正交基
      tri2  该面的三角形在自己的 (foot; e1,e2) 系里的 2D 坐标
      wbb   该面三角形的世界 3D AABB —— 预筛用它，不用任何共享坐标系，
            所以预筛不引入任何近似（共面且投影重叠 ⇒ 世界 AABB 必相交）
    """
    t0 = time.time()
    N = len(P["occ"])
    cn = np.empty((N, 3))
    for i in range(N):
        cn[i] = canon_dir(P["nw"][i])
    cn /= np.linalg.norm(cn, axis=1, keepdims=True)
    foot = (np.sum(P["o"] * cn, axis=1, keepdims=True)) * cn
    e1 = np.empty((N, 3))
    e2 = np.empty((N, 3))
    tri2 = [None] * N
    wbb = np.empty((N, 6))
    for i in range(N):
        a, b = tangent_basis(cn[i])
        e1[i], e2[i] = a, b
        loc = P["tri"][i]
        W = (P["o"][i] + loc[:, :, 0:1] * P["xw"][i] + loc[:, :, 1:2] * P["yw"][i])
        wbb[i] = (W[:, :, 0].min(), W[:, :, 0].max(), W[:, :, 1].min(),
                  W[:, :, 1].max(), W[:, :, 2].min(), W[:, :, 2].max())
        d = W - foot[i]
        tri2[i] = np.stack([d @ a, d @ b], axis=-1)
    log(f"  [prep] 逐面预计算 {N} 个面  {time.time()-t0:.1f}s")
    return {"cn": cn, "foot": foot, "e1": e1, "e2": e2, "tri2": tri2, "wbb": wbb}


def plane_pairs(P, Q, angle_deg, gap, min_overlap, min_area, log=print, chunk=512):
    """全量精确配对：①② 用 numpy 一次性跑满 n(n-1)/2 对，不分桶。

    **为什么不分桶**（这是本次踩到并证死的一个坑，值得写进判据）：
    按"法向规范到半球再按粗格 + 27 邻域"分桶会**静默漏配对**，因为
    canon_dir 在半球边界上是**不连续**的——两个只差 0.0535° 的反向法向

        n_a = [-4.46e-4, 0, +1]      n_b = [-4.89e-4, 0, -1]

    各自按"第一个非零分量为正"翻转后变成

        cn_a = [+4.46e-4, 0, -1]     cn_b = [+4.89e-4, 0, +1]

    z 分量一正一负，方向粗格差了 **100 格**，27 邻域根本够不着。
    实测代价：6DUUFR_前壳 与 6DUUFH_后下壳 之间 103mm² 和 212mm² 两处真实贴合面
    整对消失（暴力全量比对 1889 个面对 vs 分桶 1880 个，漏 1 个 occurrence 对）。
    任何"先把方向规范到半球、再按规范化后的方向分桶"的做法都继承这个不连续性。
    改用 |n_a·n_b| 直接判，符号无关，不需要规范化，也就没有这个边界。

    代价是 O(n²)，但 n=32263 时 numpy 全量只要几十秒——
    与 K-IF-03 的观测一致：瓶颈从来不是算力。
    """
    t0 = time.time()
    cosmax = math.cos(math.radians(angle_deg))
    sel = np.where(P["area"] >= min_area)[0]
    n = len(sel)
    NW = P["nw"][sel]
    NW = NW / np.linalg.norm(NW, axis=1, keepdims=True)
    O = P["o"][sel]
    OCCI = P["occ"][sel]
    WBB = Q["wbb"][sel]
    e1, e2, foot, tri2 = Q["e1"], Q["e2"], Q["foot"], Q["tri2"]

    st = collections.Counter()
    cand = []
    for s in range(0, n, chunk):
        e = min(n, s + chunk)
        # ① 法向共线（|n_a·n_b|，符号无关，所以不需要任何半球规范化）
        ok = np.abs(NW[s:e] @ NW.T) >= cosmax
        # ② 贴合距离 |(o_b-o_a)·n_a|，n_a 取行（低位下标）的法向，确定
        dist = np.abs((O @ NW[s:e].T).T - np.sum(O[s:e] * NW[s:e], axis=1)[:, None])
        ok &= dist <= gap
        ok &= OCCI[s:e][:, None] != OCCI[None, :]
        ii, jj = np.where(ok)
        ii = ii + s
        m = ii < jj
        if m.any():
            cand.append(np.stack([ii[m], jj[m]], 1))
    cand = np.concatenate(cand) if cand else np.zeros((0, 2), int)
    st["过①②的对"] = int(len(cand))

    # 世界 3D AABB 精确排除：两个面若投影重叠，必在同一平面上有公共点，
    # 因此它们的世界 AABB 必相交（放宽 gap，因为沿法向本就允许差 gap）。这一步不丢解。
    if len(cand):
        a, b = cand[:, 0], cand[:, 1]
        A_, B_ = WBB[a], WBB[b]
        keep = ~((B_[:, 0] > A_[:, 1] + gap) | (A_[:, 0] > B_[:, 1] + gap)
                 | (B_[:, 2] > A_[:, 3] + gap) | (A_[:, 2] > B_[:, 3] + gap)
                 | (B_[:, 4] > A_[:, 5] + gap) | (A_[:, 4] > B_[:, 5] + gap))
        cand = cand[keep]
    st["过AABB的对"] = int(len(cand))

    pairs = []
    for k in range(len(cand)):
        fa, fb = int(sel[cand[k, 0]]), int(sel[cand[k, 1]])
        # ③ 法向投影真实重叠面积。
        # A 的三角形已在 A 自己的 (foot; e1,e2) 系里；把 B 的**精确**搬进 A 的系：
        # B 的 2D 点对应的世界点 P 满足 P-foot_B ⊥ cn_B，所以
        #   x_A = x*(e1B·e1A) + y*(e2B·e1A) + (foot_B-foot_A)·e1A
        # 是恒等式，不是近似。
        TA = tri2[fa]
        M = np.array([[e1[fb] @ e1[fa], e2[fb] @ e1[fa]],
                      [e1[fb] @ e2[fa], e2[fb] @ e2[fa]]])
        sh = np.array([(foot[fb] - foot[fa]) @ e1[fa], (foot[fb] - foot[fa]) @ e2[fa]])
        TB = tri2[fb] @ M.T + sh
        ba = np.stack([TA[:, :, 0].min(1), TA[:, :, 0].max(1),
                       TA[:, :, 1].min(1), TA[:, :, 1].max(1)], 1)
        bx = np.stack([TB[:, :, 0].min(1), TB[:, :, 0].max(1),
                       TB[:, :, 1].min(1), TB[:, :, 1].max(1)], 1)
        m = ((ba[:, None, 0] <= bx[None, :, 1]) & (bx[None, :, 0] <= ba[:, None, 1])
             & (ba[:, None, 2] <= bx[None, :, 3]) & (bx[None, :, 2] <= ba[:, None, 3]))
        ti, tj = np.where(m)
        if len(ti) == 0:
            continue
        method = "TRI_CLIP"
        if len(ti) > TRI_PAIR_CAP:
            st["重叠退回AABB"] += 1
            method = "AABB_FALLBACK"
            ov = (max(0.0, min(ba[:, 1].max(), bx[:, 1].max()) - max(ba[:, 0].min(), bx[:, 0].min()))
                  * max(0.0, min(ba[:, 3].max(), bx[:, 3].max()) - max(ba[:, 2].min(), bx[:, 2].min())))
        else:
            ov = 0.0
            for x, y in zip(ti.tolist(), tj.tolist()):
                ov += tri_clip_area(TA[x], TB[y])
        if ov < min_overlap:
            continue
        dot = abs(float(P["nw"][fa] @ P["nw"][fb]))
        dist = abs(float((P["o"][fb] - P["o"][fa]) @ P["nw"][fa]))
        na = P["nw"][fa] * (-1 if P["rev"][fa] else 1)
        nb = P["nw"][fb] * (-1 if P["rev"][fb] else 1)
        cna = canon_dir(P["nw"][fa] / np.linalg.norm(P["nw"][fa]))
        pairs.append({
            "a_occ": int(P["occ"][fa]), "b_occ": int(P["occ"][fb]),
            "normal": [round(float(x), 9) for x in cna],
            "offset_mm": round(float(P["o"][fa] @ np.array(cna)), 5),
            "overlap_mm2": round(float(ov), 4),
            "gap_mm": round(dist, 6),
            "angle_deg": round(math.degrees(math.acos(min(1.0, dot))), 4),
            "area_a": float(P["area"][fa]), "area_b": float(P["area"][fb]),
            "opposed_outward": bool(float(na @ nb) < 0),
            "overlap_method": method,
            "proxy_footprint": bool(P["proxy"][fa] or P["proxy"][fb]),
        })
    st["参与面"] = int(n)
    st["全部对数"] = int(n * (n - 1) // 2)
    st["耗时秒"] = round(time.time() - t0, 1)
    return pairs, st




# ---------- 锥面配对 ----------
def cone_pairs(C, angle_deg, ecc_max, cone_ang_deg, rgap, min_area, log=print, chunk=512):
    """锥面配合：①轴向夹角 ②轴线偏心 ③轴向投影重叠，外加 ④半角匹配 ⑤公共站位半径匹配。
    沉头螺钉进锥形沉孔就是这一类——圆柱面图完全看不见它。

    与 plane_pairs 同样**不分桶**：分桶要先把轴向规范到半球，而 canon_dir 在半球边界上
    不连续（见 plane_pairs 的说明），会静默漏配对。这里 n=15772，全量 numpy 照样够快。
    """
    t0 = time.time()
    sel = np.where((C["area"] >= min_area) & (C["uspan"] >= 20.0))[0]
    n = len(sel)
    st = collections.Counter()
    st["参与面"] = int(n)
    if n < 2:
        st["耗时秒"] = round(time.time() - t0, 1)
        return [], st
    D = C["d"][sel]
    D = D / np.linalg.norm(D, axis=1, keepdims=True)
    PT, ANG, RREF, V, OCCI = C["p"][sel], C["ang"][sel], C["rref"][sel], C["v"][sel], C["occ"][sel]
    cosmax = math.cos(math.radians(angle_deg))
    angtol = math.radians(cone_ang_deg)

    cand = []
    for s in range(0, n, chunk):
        e = min(n, s + chunk)
        dots = D[s:e] @ D.T                                   # 带符号，(c,n)
        ok = np.abs(dots) >= cosmax
        # ② 偏心：w = P_j - P_i；t = w·d_i；ecc² = |w|² - t²
        W2 = (np.sum(PT * PT, axis=1)[None, :] - 2.0 * (PT[s:e] @ PT.T)
              + np.sum(PT[s:e] * PT[s:e], axis=1)[:, None])
        T = (PT @ D[s:e].T).T - np.sum(PT[s:e] * D[s:e], axis=1)[:, None]
        ok &= np.sqrt(np.maximum(0.0, W2 - T * T)) <= ecc_max
        # ④ 半角匹配（取绝对值：锥的开口方向随参数化走，反向轴上半角互为镜像）
        ok &= np.abs(np.abs(ANG[s:e])[:, None] - np.abs(ANG)[None, :]) <= angtol
        ok &= OCCI[s:e][:, None] != OCCI[None, :]
        ii, jj = np.where(ok)
        ii = ii + s
        m = ii < jj
        if m.any():
            cand.append(np.stack([ii[m], jj[m]], 1))
    cand = np.concatenate(cand) if cand else np.zeros((0, 2), int)
    st["过①②④的对"] = int(len(cand))

    out = []
    for k in range(len(cand)):
        i, j = int(cand[k, 0]), int(cand[k, 1])
        gi, gj = int(sel[i]), int(sel[j])
        da = D[i]
        w = PT[j] - PT[i]
        t = float(w @ da)
        ecc = math.sqrt(max(0.0, float(w @ w) - t * t))
        aa, ab = float(ANG[i]), float(ANG[j])
        sgn = 1.0 if float(da @ D[j]) > 0 else -1.0
        ca, cb = math.cos(aa), math.cos(ab)
        # ③ 轴向区间统一到 i 的参数化后比重叠：轴向偏移 = v*cos(半角)
        za = np.sort(V[i] * ca)
        zb = np.sort(t + sgn * (V[j] * cb))
        lo, hi = max(za[0], zb[0]), min(za[1], zb[1])
        if hi - lo <= 0:
            continue
        # ⑤ 公共站位（重叠中点）上的半径差
        zm = 0.5 * (lo + hi)
        ra = RREF[i] + (zm / ca if abs(ca) > 1e-9 else 0.0) * math.sin(aa)
        vb = ((zm - t) * sgn) / (cb if abs(cb) > 1e-9 else 1.0)
        rb = RREF[j] + vb * math.sin(ab)
        if abs(ra - rb) > rgap:
            continue
        cd = canon_dir(da)
        ft = PT[i] - float(PT[i] @ np.array(cd)) * np.array(cd)
        dot = abs(float(da @ D[j]))
        out.append({
            "a_occ": int(OCCI[i]), "b_occ": int(OCCI[j]),
            "axis_dir": [round(float(x), 9) for x in cd],
            "axis_foot": [round(float(x), 6) for x in ft],
            "overlap_mm": round(float(hi - lo), 5),
            "ecc_mm": round(ecc, 6),
            "angle_deg": round(math.degrees(math.acos(min(1.0, dot))), 4),
            "semi_angle_a_deg": round(math.degrees(aa), 4),
            "semi_angle_b_deg": round(math.degrees(ab), 4),
            "radius_gap_mm": round(abs(ra - rb), 5),
        })
    st["全部对数"] = int(n * (n - 1) // 2)
    st["耗时秒"] = round(time.time() - t0, 1)
    return out, st



# ---------- 汇总到 occurrence 对 ----------
NQ, OQ = 0.05, 0.5   # 法向量化格 / 偏移量化格（mm）


def rollup_planes(pairs, occs):
    op = collections.defaultdict(lambda: {"n": 0, "max_ov": 0.0, "min_gap": 1e9,
                                          "norms": {}, "proxy": 0, "opposed": 0})
    for p in pairs:
        k = (min(p["a_occ"], p["b_occ"]), max(p["a_occ"], p["b_occ"]))
        e = op[k]
        e["n"] += 1
        e["max_ov"] = max(e["max_ov"], p["overlap_mm2"])
        e["min_gap"] = min(e["min_gap"], p["gap_mm"])
        e["proxy"] += 1 if p["proxy_footprint"] else 0
        e["opposed"] += 1 if p["opposed_outward"] else 0
        nk = (tuple(round(x / NQ) for x in p["normal"]), round(p["offset_mm"] / OQ))
        cur = e["norms"].setdefault(nk, {"dir": p["normal"], "offset_mm": p["offset_mm"],
                                         "faces": 0, "max_ov": 0.0, "min_gap": 1e9})
        cur["faces"] += 1
        cur["max_ov"] = max(cur["max_ov"], p["overlap_mm2"])
        cur["min_gap"] = min(cur["min_gap"], p["gap_mm"])
    res = []
    for k, v in op.items():
        res.append({
            "a": occs[k[0]]["product"], "b": occs[k[1]]["product"],
            "a_occ": k[0], "b_occ": k[1], "faces": v["n"],
            "max_overlap_mm2": round(v["max_ov"], 4),
            "min_gap_mm": round(v["min_gap"], 6),
            "外法向相对的面对": v["opposed"], "轮廓用AABB代理的面对": v["proxy"],
            "normals": [{"dir": x["dir"], "offset_mm": x["offset_mm"], "faces": x["faces"],
                         "max_overlap_mm2": round(x["max_ov"], 4),
                         "min_gap_mm": round(x["min_gap"], 6),
                         "withdraw_sense": None}
                        for x in sorted(v["norms"].values(), key=lambda y: -y["max_ov"])],
        })
    res.sort(key=lambda x: -x["max_overlap_mm2"])
    return res


def main():
    global PLN, OCC, S7, CYLGRAPH, OUT, OUT_E, RECORD_ID
    ap = argparse.ArgumentParser()
    ap.add_argument("--geo", required=True, type=Path, help="S2 零件解析面（含 planes/cones 全字段）")
    ap.add_argument("--occ-file", required=True, type=Path)
    ap.add_argument("--cyl", required=True, type=Path, help="S2 共轴界面图（身份解析对账）")
    ap.add_argument("--out", required=True, type=Path)
    ap.add_argument("--out-e", type=Path, default=None)
    ap.add_argument("--s7", type=Path, default=None, help="S7 扫掠认证（有才跑 E 类补轴）")
    ap.add_argument("--record-id", required=True)
    ap.add_argument("--angle", type=float, default=D_ANGLE)
    ap.add_argument("--gap", type=float, default=D_GAP)
    ap.add_argument("--min-overlap", type=float, default=D_MIN_OVERLAP)
    ap.add_argument("--min-area", type=float, default=D_MIN_AREA)
    ap.add_argument("--cone-angle", type=float, default=D_CONE_ANGLE)
    ap.add_argument("--cone-ecc", type=float, default=D_CONE_ECC)
    ap.add_argument("--cone-rgap", type=float, default=D_CONE_RGAP)
    ap.add_argument("--sweep", action="store_true", help="扫阈值并把敏感性写进输出")
    ap.add_argument("--sweep-only", action="store_true", help="只扫，把结果写到 --sweep-out")
    ap.add_argument("--sweep-out", type=str, default="")
    a = ap.parse_args()
    PLN, OCC, CYLGRAPH, OUT = a.geo, a.occ_file, a.cyl, a.out
    OUT_E, S7, RECORD_ID = a.out_e, a.s7, a.record_id
    t0 = time.time()

    pdoc = json.loads(PLN.read_text(encoding="utf-8"))
    floor = pdoc["thresholds"]["area_floor_mm2"]
    if a.min_area < floor:
        print(f"[!] --min-area {a.min_area} 低于抽取地板 {floor}，"
              f"低于地板的面根本没落盘，扫这个区间没有意义", file=sys.stderr)
        return 2
    occs = json.loads(OCC.read_text(encoding="utf-8"))["occurrences"]
    P, C, miss, via, collisions = instantiate(pdoc["parts"], occs)
    print(f"[inst] 实例化平面 {len(P['occ'])}  锥面 {len(C['occ'])}  "
          f"未匹配 occurrence {sum(miss.values())}  {time.time()-t0:.1f}s", flush=True)

    # 与圆柱面图对账：同一份 occurrence 清单、同一批零件名，身份解析计数必须逐项相等
    xcheck = {"状态": "SKIPPED"}
    if CYLGRAPH.exists():
        rd = json.loads(CYLGRAPH.read_text(encoding="utf-8"))["resolution_determinism"]
        ref = rd.get("resolved_via") or rd.get("候选键按从严到宽排序、取第一个命中")
        refc = rd.get("key_collisions")
        if refc is None:
            refc = rd.get("键冲突（同一个键指向不同零件）", [])
        same = dict(via) == ref and len(collisions) == len(refc)
        xcheck = {"状态": "PASS" if same else "FAIL", "本图": dict(via), "圆柱面图": ref,
                  "本图键冲突": len(collisions),
                  "圆柱面图键冲突": len(refc),
                  "含义": "身份解析是同一套 K-ID-09 归一化的忠实副本，计数不等即副本走样"}
    print(f"[inst] 与圆柱面图的身份解析对账：{xcheck['状态']}", flush=True)

    cfgs = [(a.angle, a.gap, a.min_overlap, a.min_area)]
    if a.sweep or a.sweep_only:
        cfgs = []
        for g in (0.01, 0.05, 0.2, 0.5, 1.0):
            cfgs.append((a.angle, g, a.min_overlap, a.min_area))
        for ang in (0.1, 2.0, 5.0):
            cfgs.append((ang, a.gap, a.min_overlap, a.min_area))
        for mo in (0.5, 4.0, 10.0):
            cfgs.append((a.angle, a.gap, mo, a.min_area))
        for ma in (0.5, 4.0):
            cfgs.append((a.angle, a.gap, a.min_overlap, ma))
        base = (a.angle, a.gap, a.min_overlap, a.min_area)
        cfgs = [base] + [c for c in cfgs if c != base]

    Q = prep_planes(P)      # 与阈值无关，整轮扫描共用

    sens, main_pairs, main_st = [], None, None
    for ang, g, mo, ma in cfgs:
        tag = f"angle={ang} gap={g} min_overlap={mo} min_area={ma}"
        print(f"[sweep] {tag}", flush=True)
        prs, st = plane_pairs(P, Q, ang, g, mo, ma)
        rolled = rollup_planes(prs, occs)
        deg = collections.Counter()
        for r in rolled:
            deg[r["a_occ"]] += 1
            deg[r["b_occ"]] += 1
        row = {"thresholds": {"normal_angle_max_deg": ang, "fit_gap_max_mm": g,
                              "min_overlap_mm2": mo, "min_face_area_mm2": ma},
               "面对": len(prs), "occurrence配合对": len(rolled),
               "有平面贴合的occurrence": len(deg),
               "退回AABB的面对": int(st.get("重叠退回AABB", 0)),
               "耗时秒": st["耗时秒"]}
        sens.append(row)
        print(f"        → 面对 {row['面对']}  occ 对 {row['occurrence配合对']}  "
              f"occ 覆盖 {row['有平面贴合的occurrence']}  {st['耗时秒']}s", flush=True)
        if (ang, g, mo, ma) == (a.angle, a.gap, a.min_overlap, a.min_area):
            main_pairs, main_st = prs, st

    if a.sweep_only:
        Path(a.sweep_out).write_text(json.dumps(sens, ensure_ascii=False, indent=1), encoding="utf-8")
        print(f"写出 {a.sweep_out}")
        return 0

    cpairs, cst = cone_pairs(C, a.angle, a.cone_ecc, a.cone_angle, a.cone_rgap, a.min_area)
    print(f"[cone] 锥面配合面对 {len(cpairs)}  {cst.get('耗时秒')}s", flush=True)

    rolled = rollup_planes(main_pairs, occs)
    cop = collections.defaultdict(lambda: {"n": 0, "axes": {}})
    for p in cpairs:
        k = (min(p["a_occ"], p["b_occ"]), max(p["a_occ"], p["b_occ"]))
        cop[k]["n"] += 1
        ak = (tuple(round(x / NQ) for x in p["axis_dir"]), tuple(round(x / OQ) for x in p["axis_foot"]))
        cur = cop[k]["axes"].setdefault(ak, {"dir": p["axis_dir"], "foot": p["axis_foot"],
                                             "faces": 0, "max_overlap_mm": 0.0})
        cur["faces"] += 1
        cur["max_overlap_mm"] = max(cur["max_overlap_mm"], p["overlap_mm"])
    cone_rolled = [{"a": occs[k[0]]["product"], "b": occs[k[1]]["product"],
                    "a_occ": k[0], "b_occ": k[1], "faces": v["n"],
                    "axes": [{"dir": x["dir"], "foot": x["foot"], "faces": x["faces"],
                              "max_overlap_mm": round(x["max_overlap_mm"], 4),
                              "withdraw_sense": None}
                             for x in sorted(v["axes"].values(), key=lambda y: -y["faces"])]}
                   for k, v in cop.items()]
    cone_rolled.sort(key=lambda x: -x["faces"])

    deg = collections.Counter()
    for r in rolled:
        deg[r["a_occ"]] += 1
        deg[r["b_occ"]] += 1

    doc = {
        "$schema": "assembly-ontology.s2-plane-cone-interface-graph/v2",
        "record_id": RECORD_ID,
        "purpose": "平面贴合 + 锥面配合界面图。补 S2-共轴界面图.v1.json 只做圆柱面留下的缺口"
                   "（K-IF-01 允许 CYLINDER/PLANE/CONE 三类解析面，原实现只做了一类）。",
        "method_ref": "150-云台设计/本体/工程本体/判据库.v1.json#K-IF-01,K-IF-02,K-IF-03,K-ID-09,K-PATH-03",
        "inputs": {"零件平面锥面": str(PLN),
                   "occurrence清单": str(OCC),
                   "抽取地板mm2": floor},
        "criterion": {
            "平面贴合三条件": [
                "① 法向共线：|n_a·n_b| >= cos(normal_angle_max_deg)。法向无正负。",
                "② 贴合距离：|(o_b-o_a)·n_a| <= fit_gap_max_mm。",
                "③ 法向投影重叠：真实重叠面积 >= min_overlap_mm2。"
                "真实重叠＝三角形对逐对裁剪求交后求和，AABB 只用于预筛，不作判据。",
            ],
            "锥面配合五条件": [
                "① 轴向夹角 <= normal_angle_max_deg", "② 轴线偏心 <= cone_ecc_max_mm",
                "③ 轴向投影区间重叠 > 0（两轴反向时先翻到同一参数化）",
                "④ 半角差 <= cone_semi_angle_max_deg", "⑤ 公共轴向站位上的半径差 <= cone_radius_gap_max_mm",
            ],
            "方向正负": "平面贴合只定出一条法向直线；锥面配合只定出一条轴线。"
                        "插入是 +n 还是 -n **几何给不出**，一律 withdraw_sense=null，不猜。",
        },
        "thresholds": {
            "normal_angle_max_deg": a.angle, "fit_gap_max_mm": a.gap,
            "min_overlap_mm2": a.min_overlap, "min_face_area_mm2": a.min_area,
            "cone_semi_angle_max_deg": a.cone_angle, "cone_ecc_max_mm": a.cone_ecc,
            "cone_radius_gap_max_mm": a.cone_rgap,
            "extraction_area_floor_mm2": floor,
            "tri_pair_cap": TRI_PAIR_CAP,
            "note": "全部落盘。normal_angle 取与 K-IF-01 的 axis_angle_max_deg 同一个数；"
                    "fit_gap / min_overlap 是本次新标的，敏感性见 sensitivity 段。"
                    "min_face_area 不得低于 extraction_area_floor。",
        },
        "summary": {
            "occurrence": len(occs),
            "实例化平面": int(len(P["occ"])), "实例化锥面": int(len(C["occ"])),
            "参与配对的平面": int(main_st["参与面"]),
            "全量比对的对数": int(main_st["全部对数"]),
            "过①②的对": int(main_st["过①②的对"]),
            "再过世界AABB的对": int(main_st["过AABB的对"]),
            "命中平面面对": len(main_pairs),
            "平面occurrence配合对": len(rolled),
            "有平面贴合的occurrence": len(deg),
            "重叠退回AABB的面对": int(main_st.get("重叠退回AABB", 0)),
            "轮廓用AABB代理的面对": sum(1 for p in main_pairs if p["proxy_footprint"]),
            "外法向相对的面对": sum(1 for p in main_pairs if p["opposed_outward"]),
            "锥面参与面": int(cst.get("参与面", 0)), "命中锥面面对": len(cpairs),
            "锥面occurrence配合对": len(cone_rolled),
            "未匹配到零件几何的 occurrence": sum(miss.values()),
            "总耗时秒": round(time.time() - t0, 1),
        },
        "sensitivity": {
            "说明": "K-PATH-03：报通过必须同报覆盖区间。每行一组阈值，其余不变。"
                    "基线是第一行。所有数字可用本工具带对应参数复算。",
            "rows": sens,
        },
        "resolution_determinism": {
            "候选键按从严到宽排序、取第一个命中": dict(via),
            "键冲突（同一个键指向不同零件）": collisions,
            "与圆柱面图对账": xcheck,
        },
        "unmatched_products": dict(miss),
        "claim_boundary": [
            "只覆盖解析平面与解析锥面。B 样条/环面/球面上的配合仍不在任何一张 S2 图里。",
            f"面积 < {floor} mm² 的面在抽取阶段就丢了；参与配对的还要 >= "
            f"{a.min_area} mm²。比这更小的贴合面本图看不见。",
            "『重叠』是**几何投影重叠**，不是接触力学意义上的贴合。两块板隔着 0.05mm "
            "空气也照样算贴合——这正是阈值 fit_gap 的含义，不要读成『已确认接触』。",
            "**方向未定**：平面法向与锥面轴线都只是直线，插入正负号几何给不出，"
            "withdraw_sense 一律为 null。要定号必须另引工艺信息或粗端判据（K-DIR-02）。",
            "界面按最终装配位姿计算，某工步的候选轴可能混入更晚工步才形成的界面（K-IF-06）。",
            "①② 是全量比对（不分桶），世界 AABB 那一步不丢解（共面且投影重叠 ⇒ AABB 必相交），"
            "所以本图在给定阈值下是**完备**的：不存在因加速结构而漏掉的面对。"
            "曾用『法向规范到半球 + 粗格 27 邻域』分桶，实测漏 1 个 occurrence 对、9 个面对，"
            "根因是 canon_dir 在半球边界不连续——已改为全量，并把这条记进报告。",
            "2D 轮廓顶点带 <=0.25mm 噪声（见 S2-零件平面锥面.v1.json 的自检），"
            "重叠面积因此有同量级误差，不能当尺寸结论用。",
            f"{sum(1 for p in main_pairs if p['proxy_footprint'])} 个面对里至少一侧的轮廓是 "
            "uv AABB 代理（该面没有三角化或三角形超上限），矩形包住真实轮廓、会**高估**重叠。",
            "本图是几何配合关系，不含工艺顺序、不含紧固关系语义。",
        ],
        "cone_pairs": cone_rolled,
        "occurrence_pairs": rolled,
    }
    OUT.write_text(json.dumps(doc, ensure_ascii=False), encoding="utf-8")
    print(f"\n写出 {OUT}  ({OUT.stat().st_size/1e6:.1f} MB)")
    for k, v in doc["summary"].items():
        print(f"  {k}: {v}")

    if S7 is not None and Path(S7).exists() and OUT_E is not None:
        build_e_axes(rolled, cone_rolled, occs, doc["thresholds"])
    else:
        print("[e-axes] 无 S7 输入，E 类补轴跳过（S7 之后回补）")
    return 0


# ---------- E 类动件补轴 ----------
def build_e_axes(rolled, cone_rolled, occs, thr):
    """对 S7 里 105 个 E_无共轴界面 的动件，看平面/锥面界面能不能给出插入轴。

    **必须照抄 sweep_certify.py 的取轴规则**，否则"补上了"是假的：
      轴只能来自与**非本工序动件**的界面；优先取与更早工序已装件的界面。

    同时分三个 scope 落盘，因为"补不上"有两种完全不同的原因，混在一起就看不出该修哪儿：
      EARLIER_FIXED          与更早工序已装件的界面        —— 现行规则首选
      ANY_NON_MOVER          与任意非本工序动件的界面      —— 现行规则退路
      ANY_MATE_INCL_COMOVER  放开到含同工序动件（**诊断用，现行规则下不可用**）
    第三个 scope 回答的是"几何到底有没有给出轴"，第一二个回答的是"现行取轴规则用不用得上"。
    """
    s7 = json.loads(S7.read_text(encoding="utf-8"))
    cyl = json.loads(CYLGRAPH.read_text(encoding="utf-8"))

    def idx(s):
        return int(s.split("_")[1])

    ops = s7["operations"]
    rank = {o["op"]: i for i, o in enumerate(ops)}
    movers_of = {o["op"]: {idx(m["occ"]) for m in o["per_mover"]} for o in ops}
    owner = {}
    for o in ops:
        for m in o["per_mover"]:
            owner[idx(m["occ"])] = o["op"]

    def adj(pairs, kind):
        g = collections.defaultdict(dict)
        for p in pairs:
            if kind == "plane":
                ax = [{"dir": n["dir"], "faces": n["faces"],
                       "max_overlap_mm2": n["max_overlap_mm2"], "min_gap_mm": n["min_gap_mm"]}
                      for n in p["normals"]]
            elif kind == "cone":
                ax = [{"dir": n["dir"], "faces": n["faces"],
                       "max_overlap_mm": n["max_overlap_mm"]} for n in p["axes"]]
            else:
                ax = [{"dir": n["dir"], "faces": n["faces"],
                       "max_overlap_mm": n["max_overlap_mm"]} for n in p["axes"]]
            g[p["a_occ"]][p["b_occ"]] = ax
            g[p["b_occ"]][p["a_occ"]] = ax
        return g

    GP, GC = adj(rolled, "plane"), adj(cone_rolled, "cone")
    GY = adj(cyl["occurrence_pairs"], "cyl")

    def collect(mi, graphs, allow):
        """在给定的可用伙伴集合 allow 上，把 graphs 给出的轴去重收集起来。"""
        axset = {}
        for kind, G in graphs:
            for j, axs in G.get(mi, {}).items():
                if j not in allow:
                    continue
                for ax in axs:
                    k = (kind, tuple(round(x, 3) for x in ax["dir"]))
                    if k in axset:
                        continue
                    e = {"kind": kind, "dir": ax["dir"], "from_occ": j,
                         "from_product": occs[j]["product"], "faces": ax["faces"],
                         "withdraw_sense": None}
                    e.update({kk: vv for kk, vv in ax.items()
                              if kk in ("max_overlap_mm2", "min_gap_mm", "max_overlap_mm")})
                    axset[k] = e
        return sorted(axset.values(), key=lambda x: -x["faces"])

    recs, stat = [], collections.Counter()
    ALL = set(range(len(occs)))
    for o in ops:
        mv = movers_of[o["op"]]
        fixed_earlier = {i for i, ow in owner.items() if rank[ow] < rank[o["op"]]}
        nonmover = ALL - mv
        for m in o["per_mover"]:
            if m["klass"] != "E_无共轴界面":
                continue
            mi = idx(m["occ"])
            cy_all, pl_all, cn_all = (set(GY.get(mi, {})), set(GP.get(mi, {})), set(GC.get(mi, {})))
            rec = {"occ": m["occ"], "product": m["product"], "op": o["op"],
                   "part_class": m.get("part_class"), "原判": m["klass"],
                   "伙伴数": {"圆柱面": len(cy_all), "平面": len(pl_all), "锥面": len(cn_all)},
                   "伙伴中非本工序动件数": {"圆柱面": len(cy_all - mv), "平面": len(pl_all - mv),
                                            "锥面": len(cn_all - mv)},
                   "本工序动件数": len(mv)}
            rec["原判E的原因"] = ("圆柱面界面一个都没有" if not cy_all else
                                  "有圆柱面界面，但伙伴全是同工序动件——"
                                  "取轴规则要求伙伴是非动件，所以拿不到轴")

            # --- 现行规则：只能用非动件 ---
            got = None
            for scope, pool in (("EARLIER_FIXED", fixed_earlier & nonmover),
                                ("ANY_NON_MOVER", nonmover)):
                ax = collect(mi, (("plane", GP), ("cone", GC)), pool)
                if ax:
                    got = {"axis_source": scope, "axis_candidates": len(ax), "axes": ax}
                    break
            if got:
                rec.update(got)
                rec["补轴"] = True
                rec["新分类"] = ("B_轴向插装_平面法向" if got["axis_candidates"] == 1
                                 else "D_多轴候选_平面法向")
                rec["方向"] = "未定：平面法向/锥轴只给直线，正负几何给不出"
                stat["补上"] += 1
                stat[f"补上_{rec['新分类']}"] += 1
            else:
                rec["补轴"] = False
                if not pl_all and not cn_all:
                    why = "平面与锥面界面也一个都没有"
                elif not (pl_all | cn_all) - mv:
                    why = "有平面/锥面界面，但伙伴仍然全是同工序动件（卡在取轴规则，不是卡在几何）"
                else:
                    why = "未知（不该出现，请查）"
                rec["未补上原因"] = why
                stat["仍未补上"] += 1
                stat["未补上_" + why] += 1

            # --- 诊断 scope：放开同工序动件，只问"几何有没有给出轴" ---
            dp = collect(mi, (("plane", GP), ("cone", GC)), ALL - {mi})
            dy = collect(mi, (("cyl", GY),), ALL - {mi})
            rec["诊断_放开同工序动件"] = {
                "说明": "现行取轴规则下**不可用**，只用来区分『几何没给出轴』和『规则不让用』",
                "平面锥面候选轴数": len(dp), "圆柱面候选轴数": len(dy),
                "平面锥面给出的轴": dp[:4],
            }
            if dp:
                stat["诊断_放开后平面锥面能给出轴"] += 1
                if not dy:
                    stat["诊断_放开后只有平面锥面能给出轴(圆柱面给不出)"] += 1
            if not dp and not dy:
                stat["诊断_放开后三类面都给不出轴"] += 1
            recs.append(rec)

    doc = {
        "$schema": "x1.s2-e-class-axis-recovery/v1",
        "record_id": "X1-S2-EAXIS-V001",
        "purpose": "S7-扫掠认证.v2 里 105 个 E_无共轴界面 的动件，用平面贴合/锥面配合界面重新取轴。",
        "method_ref": "150-云台设计/本体/工程本体/判据库.v1.json#K-IF-01,K-IF-03,K-DIR-02",
        "取轴规则": "照抄 tools/sweep_certify.py：轴只能来自与**非本工序动件**的界面；"
                    "优先取与更早工序已装件（EARLIER_FIXED）的，没有才退到任意非动件（ANY_NON_MOVER）。"
                    "换规则的话增益数字不可比，所以规则原样保留、另开诊断 scope 说明差别。",
        "关键发现": "105 个 E 里 103 个**本来就有圆柱面界面**，判 E 不是因为『配合不在圆柱面上』，"
                    "而是因为它们的配合伙伴是**同一道工序里的另一个动件**，被取轴规则排除。"
                    "所以补平面/锥面并不能把它们变成可播——瓶颈在工序绑定的粒度，不在面型覆盖。",
        "thresholds": thr,
        "summary": {"E类动件": len(recs), **dict(sorted(stat.items()))},
        "claim_boundary": [
            "『补上轴』= 几何给出了一条插入直线，**不等于**扫掠认证会通过。"
            "认证要另跑 sweep_certify（本记录不含任何认证结论）。",
            "方向正负一律未定：平面法向与锥面轴线都只是直线（K-DIR-02 的粗端判据在这里没跑）。",
            "界面按最终装配位姿算，可能混入更晚工步才形成的界面（K-IF-06）。",
            "诊断_放开同工序动件 这一段**不是结论**，是用来定位瓶颈的：它在现行取轴规则下不可用。",
            "本记录只覆盖 S7-扫掠认证.v2 判为 E 的 105 个动件，不重判 B/D 类。",
        ],
        "movers": recs,
    }
    OUT_E.write_text(json.dumps(doc, ensure_ascii=False, indent=1), encoding="utf-8")
    print(f"\n写出 {OUT_E}")
    for k, v in doc["summary"].items():
        print(f"  {k}: {v}")
    return doc["summary"]




if __name__ == "__main__":
    sys.exit(main())
