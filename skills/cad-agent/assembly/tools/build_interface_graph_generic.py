#!/usr/bin/env python3
"""S2：全局共轴界面图（参数化移植版，算法与 X1 build_interface_graph.py 逐行一致）。

判据 K-IF-01（三条同时成立才算配对）：
    ① 轴向夹角 ≤ --angle    ② 轴线偏心 ≤ --ecc    ③ 轴向投影区间有重叠
分桶：方向粗格（27 邻域）→ 垂足粗格（27 邻域）→ 组内精判，避免 O(n²)。
名称解析：候选键从严到宽有序列表（禁集合迭代，K-GOV-03 确定性）。

用法：python3 build_interface_graph_generic.py --geo S2零件解析面 --occ S1清单 --out OUT
"""
import argparse
import collections
import itertools
import json
import math
import re
import time
import unicodedata
from pathlib import Path

MIN_AREA = 1.0        # mm²，滤掉碎面
MIN_U_DEG = 20.0      # 圆周张角太小的不算配合面


def norm3(v):
    n = math.sqrt(sum(x * x for x in v))
    return [x / n for x in v] if n > 1e-12 else [0.0, 0.0, 1.0]


def canon_dir(d):
    for c in d:
        if abs(c) > 1e-9:
            return [x / (1 if c > 0 else -1) for x in d] if c < 0 else list(d)
    return list(d)


def foot(p, d):
    t = sum(p[i] * d[i] for i in range(3))
    return [p[i] - t * d[i] for i in range(3)]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--geo", required=True, type=Path)
    ap.add_argument("--occ", required=True, type=Path)
    ap.add_argument("--out", required=True, type=Path)
    ap.add_argument("--record-id", required=True)
    ap.add_argument("--angle", type=float, default=0.5)
    ap.add_argument("--ecc", type=float, default=0.08)
    a = ap.parse_args()
    COS_MAX = math.cos(math.radians(a.angle))

    t0 = time.time()
    geo = json.loads(a.geo.read_text(encoding="utf-8"))["parts"]
    occd = json.loads(a.occ.read_text(encoding="utf-8"))
    occs = occd["occurrences"]

    by_name = {}
    for f, v in geo.items():
        if "error" in v:
            continue
        nm = (v.get("name") or f).replace(".step", "").strip()
        by_name[nm] = v["cylinders"]

    CODE = re.compile(r"^[0-9A-Z]{6}_")
    CONFIG_SUFFIX = re.compile(
        r"^(?:M|ST|Ø|φ)?\s*\d+(?:\.\d+)?\s*(?:[×xX*]\s*\d+(?:\.\d+)?)?\s*$|^M\d+(?:\.\d+)?$", re.I)

    def nrm(x):
        return unicodedata.normalize("NFKC", str(x or "").strip()).replace("╱", "/")

    def keys(s):
        s = nrm(s)
        cand = [s, CODE.sub("", s)]
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

    items, miss, resolved_via = [], collections.Counter(), collections.Counter()
    for oi, o in enumerate(occs):
        cs = None
        for rank, k in enumerate(keys(o["product"])):
            if k in lut:
                cs = lut[k]
                resolved_via[f"rank{rank}"] += 1
                break
        if cs is None:
            miss[o["product"]] += 1
            continue
        W = o["world"]
        R = [[W[i][j] for j in range(3)] for i in range(3)]
        T = [W[i][3] for i in range(3)]
        for c in cs:
            if c["area"] < MIN_AREA or c["u_span_deg"] < MIN_U_DEG:
                continue
            p = [sum(R[i][k] * c["p"][k] for k in range(3)) + T[i] for i in range(3)]
            d = norm3([sum(R[i][k] * c["d"][k] for k in range(3)) for i in range(3)])
            items.append({"occ": oi, "p": p, "d": d, "r": c["r"], "v": c["v"], "area": c["area"]})
    print(f"[graph] 实例化圆柱面 {len(items)}  未匹配 occurrence {sum(miss.values())}  {time.time()-t0:.0f}s",
          flush=True)

    DC, FC = 0.02, 0.15
    dbins = collections.defaultdict(list)
    for i, it in enumerate(items):
        cd = canon_dir(it["d"])
        it["cd"] = cd
        it["ft"] = foot(it["p"], cd)
        dbins[tuple(round(x / DC) for x in cd)].append(i)

    NB = [(-1, 0, 1)] * 3
    pairs, checked = [], 0
    for key, idxs in dbins.items():
        pool = []
        for off in itertools.product(*NB):
            pool += dbins.get(tuple(key[i] + off[i] for i in range(3)), [])
        pool = sorted(set(pool))
        if len(pool) < 2:
            continue
        fbins = collections.defaultdict(list)
        for i in pool:
            fbins[tuple(round(x / FC) for x in items[i]["ft"])].append(i)
        seen = set()
        for fk, fidx in fbins.items():
            grp = []
            for off in itertools.product(*NB):
                grp += fbins.get(tuple(fk[i] + off[i] for i in range(3)), [])
            grp = sorted(set(grp))
            for i, j in itertools.combinations(grp, 2):
                if (i, j) in seen:
                    continue
                seen.add((i, j))
                A, B = items[i], items[j]
                if A["occ"] == B["occ"]:
                    continue
                checked += 1
                dot = abs(sum(A["d"][k] * B["d"][k] for k in range(3)))
                if dot < COS_MAX:
                    continue
                w = [B["p"][k] - A["p"][k] for k in range(3)]
                t = sum(w[k] * A["d"][k] for k in range(3))
                ecc = math.sqrt(max(0.0, sum(w[k] * w[k] for k in range(3)) - t * t))
                if ecc > a.ecc:
                    continue
                sgn = 1.0 if sum(A["d"][k] * B["d"][k] for k in range(3)) > 0 else -1.0
                bv = [t + sgn * B["v"][0], t + sgn * B["v"][1]]
                lo = max(A["v"][0], min(bv)), min(A["v"][1], max(bv))
                ov = lo[1] - lo[0]
                if ov <= 0:
                    continue
                cd = canon_dir(A["d"])
                ft = foot(A["p"], cd)
                pairs.append({"a_occ": A["occ"], "b_occ": B["occ"],
                              "axis_dir": [round(x, 9) for x in cd],
                              "axis_foot": [round(x, 6) for x in ft],
                              "r_a": A["r"], "r_b": B["r"],
                              "radial_gap_mm": round(abs(A["r"] - B["r"]), 5),
                              "overlap_mm": round(ov, 5),
                              "ecc_mm": round(ecc, 6),
                              "angle_deg": round(math.degrees(math.acos(min(1.0, dot))), 4)})
    print(f"[graph] 精判 {checked} 对，命中界面 {len(pairs)}  {time.time()-t0:.0f}s", flush=True)

    op = collections.defaultdict(lambda: {"n": 0, "max_overlap": 0.0, "min_gap": 1e9, "axes": {}})
    AXQ = 0.05
    for p in pairs:
        k = (min(p["a_occ"], p["b_occ"]), max(p["a_occ"], p["b_occ"]))
        e = op[k]
        e["n"] += 1
        e["max_overlap"] = max(e["max_overlap"], p["overlap_mm"])
        e["min_gap"] = min(e["min_gap"], p["radial_gap_mm"])
        ak = (tuple(round(x / AXQ) for x in p["axis_dir"]),
              tuple(round(x / 0.5) for x in p["axis_foot"]))
        cur = e["axes"].setdefault(ak, {"dir": p["axis_dir"], "foot": p["axis_foot"],
                                        "faces": 0, "max_overlap": 0.0, "min_gap": 1e9})
        cur["faces"] += 1
        cur["max_overlap"] = max(cur["max_overlap"], p["overlap_mm"])
        cur["min_gap"] = min(cur["min_gap"], p["radial_gap_mm"])

    def pn(i):
        return occs[i]["product"]

    occ_pairs = [{"a": pn(k[0]), "b": pn(k[1]), "a_occ": k[0], "b_occ": k[1],
                  "a_uid": occs[k[0]]["occ_uid"], "b_uid": occs[k[1]]["occ_uid"],
                  "faces": v["n"], "max_overlap_mm": round(v["max_overlap"], 4),
                  "min_radial_gap_mm": round(v["min_gap"], 5),
                  "axes": [{"dir": ax["dir"], "foot": ax["foot"], "faces": ax["faces"],
                            "max_overlap_mm": round(ax["max_overlap"], 4),
                            "min_radial_gap_mm": round(ax["min_gap"], 5)}
                           for ax in sorted(v["axes"].values(), key=lambda x: -x["faces"])]}
                 for k, v in op.items()]
    occ_pairs.sort(key=lambda x: -x["faces"])

    deg = collections.Counter()
    for k in op:
        deg[k[0]] += 1
        deg[k[1]] += 1
    isolated = [i for i in range(len(occs)) if deg[i] == 0]

    doc = {
        "$schema": "assembly-ontology.s2-coaxial-interface-graph/v2",
        "record_id": a.record_id,
        "purpose": "全局共轴界面图：不同实例之间共享同一条轴线的配合界面。",
        "method_ref": "X1 build_interface_graph.py 参数化移植；判据 K-IF-01/02/03 不变。",
        "thresholds": {"axis_angle_max_deg": a.angle, "axis_eccentricity_max_mm": a.ecc,
                       "axial_overlap": "required", "min_face_area_mm2": MIN_AREA,
                       "min_circumferential_span_deg": MIN_U_DEG,
                       "note": "沿用 Orbita/X1 的标定值；换装配的敏感性必须另行报出。"},
        "summary": {
            "occurrence": len(occs),
            "实例化圆柱面": len(items),
            "未匹配到零件几何的occurrence": sum(miss.values()),
            "精判对数": checked,
            "命中面对": len(pairs),
            "occurrence配合对": len(occ_pairs),
            "无任何共轴配合的occurrence": len(isolated),
            "耗时秒": round(time.time() - t0, 1),
        },
        "unmatched_products": dict(miss),
        "resolution_determinism": {"resolved_via": dict(resolved_via), "key_collisions": collisions},
        "isolated_occurrences": [{"occ": i, "uid": occs[i]["occ_uid"], "product": occs[i]["product"]}
                                 for i in isolated],
        "claim_boundary": [
            "只用圆柱面。平面贴合与锥面配合不在本图内，『无共轴配合』不等于『没有配合』。",
            "界面图按最终装配位姿计算，某工步的候选轴可能混入更晚工步才形成的界面（K-IF-06）。",
            "阈值沿用 X1/Orbita 标定值，未按本装配重标。",
            "本图是几何配合关系，不含工艺顺序、不含紧固关系语义。",
        ],
        "occurrence_pairs": occ_pairs,
    }
    a.out.write_text(json.dumps(doc, ensure_ascii=False), encoding="utf-8")
    print(f"\n写出 {a.out}")
    for k, v in doc["summary"].items():
        print(f"  {k}: {v}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
