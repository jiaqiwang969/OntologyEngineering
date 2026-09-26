#!/usr/bin/env python3
"""工序内播放序 v2（功能/安装语义版，K-SEQ）。

v1 的序 = S7 几何认证序 + 图约束（结构件前/紧固件后/耦合对末尾）。审片（v010p 预览）暴露的问题：
垫圈先于丝杠"悬空出场"、滑块先于导轨、支架先于它所挂的板——几何上每件都能走，但顺序在安装方式上不成立。

v2 规则（每工序内，逐件贪心）：
  1. 种子：优先 seed-overrides（功能分析线给的承载件）；否则在非紧固件里选“已与前序工序在场件接触”且最大者；台架首工序无前序接触则取最大件。
  2. 支撑：下一件必须与已放置件（本工序已放 + 前序闭包在场件）有 S8 接触（COAX/PLANE/CONE；PLANE_LOOSE 计弱接触）；
     候选按 (强接触, 尺寸) 排序；无任何接触候选时取最大者并记 unsupported。
  3. 紧固件：在其所夹件全部放置之后，按“最后一个被夹件的位置”排序放入；无被夹件记录的紧固件放到本工序末尾。
  4. 耦合对（pair_id）保持在末尾（沿用 v1 约束）。
输出：本体/S7-工序内播放序.v2.json（同 v1 结构 + functional_sort），以及变更台账。
用法：build_play_order_functional.py --case-dir . [--seeds 功能属性/seed-overrides.v1.json] [--out 本体/S7-工序内播放序.v2.json]
"""
import argparse, json, re, collections
from pathlib import Path

STRONG = ("COAX", "PLANE", "CONE")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--case-dir", required=True, type=Path)
    ap.add_argument("--play-order", default="本体/S7-工序内播放序.v1.json")
    ap.add_argument("--seeds", default="功能属性/seed-overrides.v1.json")
    ap.add_argument("--out", default="本体/S7-工序内播放序.v2.json")
    a = ap.parse_args()
    C = a.case_dir
    cfg = json.loads((C / "robot-config.v1.json").read_text(encoding="utf-8"))
    P = cfg["paths"]
    v1 = json.loads((C / a.play_order).read_text(encoding="utf-8"))
    s8 = json.loads((C / P["s8_graph"]).read_text(encoding="utf-8"))
    s3 = json.loads((C / "本体/S3-工序实例绑定.v1.json").read_text(encoding="utf-8"))
    occs = json.loads((C / P["s1_manifest"]).read_text(encoding="utf-8"))["occurrences"]
    man = json.loads((C / P["mesh_dir"] / "manifest.json").read_text(encoding="utf-8"))["parts"]
    seeds = {}
    if (C / a.seeds).exists():
        seeds = {r["op"]: r for r in json.loads((C / a.seeds).read_text(encoding="utf-8")).get("seeds", [])}

    # 尺寸：occ → bbox 对角（网格清单按 stem）
    CODE = re.compile(r"[^0-9A-Za-z_\-一-鿿]+")
    lut = {k: k for k in man}
    lut.update({man[k]["name"].strip(): k for k in man})

    def stem_for(prod):
        s0 = prod.strip()
        for k in (s0, CODE.sub("", s0), s0.replace(" ", "_"), "_" + s0.replace(" ", "_")):
            if k in lut:
                return lut[k]
        for k in man:
            if man[k]["name"].strip() == s0:
                return k
        return None
    size = {}
    for o in occs:
        st = stem_for(o["product"])
        size[o["occ_id"]] = float(man[st].get("bbox_diag_mm") or 0.0) if st and st in man else 0.0

    node = {n["occ"]: n for n in s8["nodes"]}
    adj = collections.defaultdict(dict)
    for e in s8["edges"]:
        k = e["kind"]
        w = 2 if k in STRONG else 1
        adj[e["a"]][e["b"]] = max(adj[e["a"]].get(e["b"], 0), w)
        adj[e["b"]][e["a"]] = max(adj[e["b"]].get(e["a"], 0), w)
    fastens = {k: set(v) for k, v in s8.get("fastens", {}).items()}

    s3ops = {o["op"]: o for o in s3["operations"]}
    order_ops = [o["op"] for o in sorted(s3["operations"], key=lambda o: o["order_index"])]
    members = {}
    for o in s3["operations"]:
        for m in o["movers"]:
            members[m["occ_id"]] = list(m.get("member_occ_ids") or [m["occ_id"]])
    preds = {o["op"]: list(o.get("predecessors") or []) for o in s3["operations"]}

    def closure(op, acc):
        for p in preds.get(op, []):
            if p not in acc:
                acc.add(p)
                closure(p, acc)
        return acc

    out = json.loads(json.dumps(v1))
    ledger = []
    for op in order_ops:
        spec = out["ops"].get(op)
        if not spec:
            continue
        order = list(spec["order"])
        ids = [e["occ"] for e in order]
        present = set()
        for p in closure(op, set()):
            for e in v1["ops"].get(p, {}).get("order", []):
                present.update(members.get(e["occ"]) or [e["occ"]])
        ent = {}
        for e in order:
            mids = members.get(e["occ"]) or [e["occ"]]
            is_f = all(node.get(m, {}).get("is_fastener") for m in mids) if mids else False
            ent[e["occ"]] = {"entry": e, "members": mids, "size": max([size.get(m, 0.0) for m in mids] or [0.0]), "is_fastener": is_f, "pair": bool(e.get("pair_id"))}

        def contact(eid, placed_members):
            best = 0
            for m in ent[eid]["members"]:
                for n, w in adj.get(m, {}).items():
                    if n in placed_members and w > best:
                        best = w
            return best
        core = [i for i in ids if not ent[i]["pair"] and not ent[i]["is_fastener"]]
        fast = [i for i in ids if not ent[i]["pair"] and ent[i]["is_fastener"]]
        tail = [i for i in ids if ent[i]["pair"]]
        placed, placed_m, unsupported = [], set(present), []
        seed = None
        sd = seeds.get(op)
        if sd:
            for i in core:
                if sd.get("occ") == i or (sd.get("product") and any(sd["product"] in (node.get(m, {}).get("product") or "") for m in ent[i]["members"])):
                    seed = i
                    break
        if seed is None and core:
            seed = max(core, key=lambda i: (contact(i, placed_m) > 0, ent[i]["size"]))
        rest = [i for i in core if i != seed]
        if seed is not None:
            placed.append(seed)
            placed_m |= set(ent[seed]["members"])
        while rest:
            scored = sorted(rest, key=lambda i: (-contact(i, placed_m), -ent[i]["size"]))
            nxt = scored[0]
            if contact(nxt, placed_m) == 0:
                unsupported.append(nxt)
            placed.append(nxt)
            placed_m |= set(ent[nxt]["members"])
            rest.remove(nxt)
        # 紧固件：所夹件全部放置后，按最后被夹件的位置插入
        pos = {i: k for k, i in enumerate(placed)}
        owner_pos = {}
        for i in fast:
            clamped = set()
            for m in ent[i]["members"]:
                clamped |= fastens.get(m, set())
            owners = [j for j in placed if set(ent[j]["members"]) & clamped]
            owner_pos[i] = max((pos[j] for j in owners), default=len(placed) + 10**6)
        fast_sorted = sorted(fast, key=lambda i: (owner_pos[i], -ent[i]["size"]))
        merged = []
        fi = 0
        for k, i in enumerate(placed):
            merged.append(i)
            while fi < len(fast_sorted) and owner_pos[fast_sorted[fi]] == k:
                merged.append(fast_sorted[fi])
                fi += 1
        merged += fast_sorted[fi:] + tail
        assert sorted(merged) == sorted(ids), op
        moved = sum(1 for k, i in enumerate(merged) if i != ids[k])
        spec["order"] = [ent[i]["entry"] for i in merged]
        spec["functional_sort"] = {"rule": "SEED_THEN_SUPPORTED_CONTACT_GREEDY;FASTENER_AFTER_CLAMPED;PAIR_TAIL", "seed": seed,
                                   "seed_source": ("override" if sd and seed is not None and (sd.get("occ") == seed or sd.get("product")) else "largest_anchored"),
                                   "unsupported": unsupported, "moved_slots": moved}
        ledger.append({"op": op, "n": len(ids), "moved_slots": moved, "seed": seed, "seed_product": (node.get(ent[seed]["members"][0], {}).get("product") if seed else None),
                       "unsupported": [(i, node.get(ent[i]["members"][0], {}).get("product")) for i in unsupported],
                       "v1": ids, "v2": merged})
    out["$schema"] = "assembly-ontology.play-order/v2"
    out["functional_sort"] = {"rule": "见文件头；每工序 functional_sort 记录种子/未支撑件/移动槽位", "ops_changed": sum(1 for r in ledger if r["moved_slots"]),
                              "moved_slots": sum(r["moved_slots"] for r in ledger), "unsupported_total": sum(len(r["unsupported"]) for r in ledger),
                              "seeds_file": a.seeds if seeds else None, "basis": {"v1": a.play_order, "s8": P["s8_graph"], "s3": "本体/S3-工序实例绑定.v1.json"}}
    (C / a.out).write_text(json.dumps(out, ensure_ascii=False, indent=1), encoding="utf-8")
    (C / a.out.replace(".json", ".ledger.json")).write_text(json.dumps(ledger, ensure_ascii=False, indent=1), encoding="utf-8")
    print(json.dumps(out["functional_sort"], ensure_ascii=False))
    for r in ledger:
        if r["moved_slots"] or r["unsupported"]:
            print(f"  {r['op']:9s} n={r['n']:2d} moved={r['moved_slots']:2d} seed={str(r['seed_product'])[:40]!s:40s} unsupported={len(r['unsupported'])}")


if __name__ == "__main__":
    main()
