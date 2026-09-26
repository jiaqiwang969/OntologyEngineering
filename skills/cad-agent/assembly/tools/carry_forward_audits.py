#!/usr/bin/env python3
"""增量审计承接（K-GOV 声明式来源）：新 run 里与旧 run 章节输入完全相同的工序，直接承接旧 run 的审计记录。

章节输入 = (工序, 动件序列的 occ/轴/向/行程/成员, 上下文集, 前驱闭包成员) 的规范 JSON 哈希；相同则成片口径穿模审计、
兄弟件审计、画面口径扫描的输入完全一致，结果可承接。承接的记录加 carried_from / chapter_input_sha256 字段，如实可追溯。
用法：carry_forward_audits.py --case-dir <case> --from FILM_v005 --to FILM_v006"""
import argparse, hashlib, json, shutil
from pathlib import Path
ap = argparse.ArgumentParser(); ap.add_argument("--case-dir", required=True); ap.add_argument("--from", dest="src", required=True); ap.add_argument("--to", dest="dst", required=True)
a = ap.parse_args(); C = Path(a.case_dir).resolve(); AD = C / "本体/S7-穿模审计"
def chapter_inputs(run):
    chain = json.loads((C / "装配动画" / run / "state-chain.json").read_text(encoding="utf-8"))
    state = json.loads((C / "装配动画" / run / "pipeline-state.v1.json").read_text(encoding="utf-8"))["assembly"]
    preds = state.get("predecessors", {}); members = {o["op"]: sorted(m["occ"] for m in o.get("movers", [])) for o in state.get("operations", [])}
    # 审计的在场集把 not_animated 静态件也算在工序成员里（它们在场、只是不动）：闭包哈希必须同口径，否则一个件改静态会让全部下游章节"变了"
    for it in state.get("not_animated", []):
        members[it["op"]] = sorted(set(members.get(it["op"], [])) | set(it.get("occs", [])))
    out = {}
    for ch in chain["chapters"]:
        if not ch.get("movers"): continue
        acc, st = set(), [ch["op"]]
        while st:
            cur = st.pop()
            for p in preds.get(cur, []):
                if p not in acc: acc.add(p); st.append(p)
        closure = sorted(x for p in acc for x in members.get(p, []))
        key = {"op": ch["op"], "movers": [[m["occ"], m["insertion_axis_world"], m["withdraw_sense"], m["approach_mm"], m.get("member_occs")] for m in ch["movers"]],
               "context": sorted(ch.get("context", [])), "static_withdrawn": sorted(ch.get("static_withdrawn", [])), "closure": closure}
        out[ch["op"]] = hashlib.sha256(json.dumps(key, sort_keys=True, ensure_ascii=False).encode()).hexdigest()
    return out
src, dst = chapter_inputs(a.src), chapter_inputs(a.dst)
same = [op for op in dst if src.get(op) == dst[op]]; changed = [op for op in dst if src.get(op) != dst[op]]
n = 0
for op in same:
    for suffix in ("film.real.json", "siblings.json", "visual.json"):
        s = AD / f"{op}.{a.src}.{suffix}"; d = AD / f"{op}.{a.dst}.{suffix}"
        if s.exists() and not d.exists():
            doc = json.loads(s.read_text(encoding="utf-8")); doc["carried_from"] = a.src; doc["chapter_input_sha256"] = dst[op]; doc["run"] = a.dst
            d.write_text(json.dumps(doc, ensure_ascii=False), encoding="utf-8"); n += 1
print(f"[carry] {a.src}→{a.dst}: unchanged chapters {len(same)}, changed {len(changed)} {changed[:12]}, records carried {n}")
