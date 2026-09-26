#!/usr/bin/env python3
"""顺序约束审计：把各功能分析线的 order_constraints.json 汇总，对照影片章节顺序（state-chain）与 S3 台架分组逐条判定。

用法：audit_sequence_constraints.py --case-dir . --run FILM_v009 [--tracks-dir 功能属性/tracks] [--out 功能属性/order-audit.v1.json]
判定：
  MUST_BEFORE / ACCESS / WIRE_BEFORE_CLOSE / ADJUST_AFTER / SAFETY：from 章在 to 章之前才算满足（ADJUST_AFTER/SAFETY 若无 to 则只记录）
  MUST_AFTER：from 章在 to 章之后
  SAME_BENCH：from/to 同属 S3 bench_groups 的一组
  两端任一工序不在影片里（无章节）→ UNRESOLVED（不计违反，但单列）
"""
import argparse, json, re, pathlib, collections

OP_RE = re.compile(r"WB-\d+(?:\.\d+)?")


def op_of(s):
    if not s:
        return None
    m = OP_RE.search(str(s))
    return m.group(0) if m else None


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--case-dir", required=True)
    ap.add_argument("--run", required=True)
    ap.add_argument("--tracks-dir", default="功能属性/tracks")
    ap.add_argument("--out", default="功能属性/order-audit.v1.json")
    a = ap.parse_args()
    case = pathlib.Path(a.case_dir)
    chain = json.load(open(case / "装配动画" / a.run / "state-chain.json", encoding="utf-8"))
    chap = {c["op"]: i + 1 for i, c in enumerate(chain["chapters"])}
    s3 = json.load(open(case / "本体/S3-工序实例绑定.v1.json", encoding="utf-8"))
    order_index = {o["op"]: o.get("order_index") for o in s3["operations"]}
    # 本 run 冻结的工序内播放序（v1/v2 由 pipeline-state.tracked_inputs.order.path 决定）
    try:
        st = json.load(open(case / "装配动画" / a.run / "pipeline-state.v1.json", encoding="utf-8"))
        _op_path = ((st.get("tracked_inputs") or {}).get("order") or {}).get("path") or "本体/S7-工序内播放序.v1.json"
    except Exception:
        _op_path = "本体/S7-工序内播放序.v1.json"
    po = json.load(open(case / _op_path, encoding="utf-8"))
    s8n = {n["occ"]: n for n in json.load(open(case / "本体/S8-连接关系图.v1.json", encoding="utf-8"))["nodes"]}
    s3m = {}
    play = {}
    for o in s3["operations"]:
        for m in o["movers"]:
            s3m[m["occ_id"]] = " | ".join([m.get("product") or ""] + [str(x) for x in (m.get("via_glb_names") or [])])
    for opn, spec in po.get("ops", {}).items():
        play[opn] = [s3m.get(e["occ"]) or s8n.get(e["occ"], {}).get("product") or e["occ"] for e in spec.get("order", [])]
    bench_of = {}
    for g in s3.get("bench_groups", []):
        for op in g.get("ops", []):
            bench_of[op] = g["group_id"]
    rows, per_track = [], collections.OrderedDict()
    for f in sorted((case / a.tracks_dir).glob("*/order_constraints.json")):
        track = f.parent.name
        try:
            data = json.load(open(f, encoding="utf-8"))
        except Exception as e:  # noqa
            per_track[track] = {"error": str(e)}
            continue
        if isinstance(data, dict):
            data = data.get("constraints") or data.get("rows") or []
        per_track.setdefault(track, {"n": 0, "violated": 0, "satisfied": 0, "unresolved": 0, "info": 0})
        for r in data:
            kind = str(r.get("kind", "")).upper()
            src, dst = op_of(r.get("from")), op_of(r.get("to"))
            ci, cj = chap.get(src), chap.get(dst)
            verdict, detail = "INFO", ""
            if kind in ("MUST_BEFORE", "ACCESS", "WIRE_BEFORE_CLOSE", "ADJUST_AFTER", "SAFETY", "MUST_AFTER") and src and dst and src == dst:
                # 工序内约束：parts[0] 必须先于 parts[1:]（按产品名子串匹配本 run 冻结的播放序）
                parts = [str(x) for x in (r.get("parts") or []) if x]
                seq = play.get(src) or []
                def _idx(name):
                    key = name.lower().strip()
                    hits = [i for i, pn in enumerate(seq) if key and (key in pn.lower() or pn.lower() in key)]
                    return min(hits) if hits else None
                if len(parts) < 2 or not seq:
                    verdict, detail = "UNRESOLVED", "工序内约束缺 parts 或该工序无播放序"
                else:
                    i0 = _idx(parts[0]); rest = [(pn, _idx(pn)) for pn in parts[1:]]
                    known = [(pn, i) for pn, i in rest if i is not None]
                    if i0 is None or not known:
                        verdict, detail = "UNRESOLVED", f"零件名未匹配到播放序：{parts[0] if i0 is None else ''} {[pn for pn, i in rest if i is None]}"
                    else:
                        bad = [pn for pn, i in known if (i < i0 if kind != "MUST_AFTER" else i > i0)]
                        verdict = "VIOLATED" if bad else "SATISFIED"
                        detail = f"工序内序：{parts[0]}@{i0} vs " + ", ".join(f"{pn}@{i}" for pn, i in known) + (f" 违反：{bad}" if bad else "")
                        kind = kind + "(INTRA)"
            elif kind in ("MUST_BEFORE", "ACCESS", "WIRE_BEFORE_CLOSE", "ADJUST_AFTER", "SAFETY", "MUST_AFTER"):
                if src is None or dst is None:
                    verdict = "INFO" if kind in ("ADJUST_AFTER", "SAFETY") else "UNRESOLVED"
                    detail = "缺 from/to 工序号"
                elif ci is None or cj is None:
                    verdict = "UNRESOLVED"
                    detail = f"影片无章节：{src if ci is None else ''} {dst if cj is None else ''}".strip()
                else:
                    ok = (ci > cj) if kind == "MUST_AFTER" else (ci < cj)
                    verdict = "SATISFIED" if ok else "VIOLATED"
                    detail = f"章序 {src}={ci} {'>' if ci > cj else '<' if ci < cj else '='} {dst}={cj}"
            elif kind == "SAME_BENCH":
                if src is None or dst is None:
                    verdict, detail = "UNRESOLVED", "缺 from/to 工序号"
                else:
                    bi, bj = bench_of.get(src), bench_of.get(dst)
                    verdict = "SATISFIED" if (bi is not None and bi == bj) else "VIOLATED"
                    detail = f"台架组 {src}={bi} {dst}={bj}"
            else:
                detail = f"未知 kind={kind}"
            row = {"track": track, "kind": kind, "from": src, "to": dst, "from_raw": r.get("from"), "to_raw": r.get("to"),
                   "why": r.get("why"), "source": r.get("source"), "confidence": r.get("confidence"),
                   "parts": r.get("parts"), "agent_verdict": r.get("violated_in_v009"), "agent_evidence": r.get("evidence"),
                   "verdict": verdict, "detail": detail, "chapter_from": ci, "chapter_to": cj}
            rows.append(row)
            t = per_track[track]
            t["n"] += 1
            t[{"VIOLATED": "violated", "SATISFIED": "satisfied", "UNRESOLVED": "unresolved"}.get(verdict, "info")] += 1
    summary = {"constraints": len(rows),
               "violated": sum(1 for r in rows if r["verdict"] == "VIOLATED"),
               "satisfied": sum(1 for r in rows if r["verdict"] == "SATISFIED"),
               "unresolved": sum(1 for r in rows if r["verdict"] == "UNRESOLVED"),
               "info": sum(1 for r in rows if r["verdict"] == "INFO"),
               "violated_high_conf": sum(1 for r in rows if r["verdict"] == "VIOLATED" and str(r.get("confidence")) in ("高", "high", "HIGH")),
               "by_kind": dict(collections.Counter(r["kind"] for r in rows)),
               "per_track": per_track}
    out = {"$schema": "assembly-ontology.order-audit/v1", "run": a.run, "basis": {"state_chain": f"装配动画/{a.run}/state-chain.json", "s3": "本体/S3-工序实例绑定.v1.json", "play_order": _op_path, "tracks_dir": a.tracks_dir},
           "summary": summary, "rows": rows,
           "claim_boundary": ["判定只比较影片章节顺序（= Workbook 步序）与 S3 台架分组，不读几何；约束本身来自功能分析线（含推断），置信度见每行。",
                              "工序内动件先后（S3 movers 次序）未在此核对；见各线 notes.md 的 film_v009_observations。"]}
    (case / a.out).parent.mkdir(parents=True, exist_ok=True)
    json.dump(out, open(case / a.out, "w", encoding="utf-8"), ensure_ascii=False, indent=1)
    md = [f"# 顺序约束审计（{a.run}）", "",
          f"约束 {summary['constraints']} 条：违反 {summary['violated']}（高置信 {summary['violated_high_conf']}）· 满足 {summary['satisfied']} · 未解析 {summary['unresolved']} · 仅记录 {summary['info']}", "",
          "| 线 | 类型 | from | to | 判定 | 置信 | 来源 | 理由 |", "|---|---|---|---|---|---|---|---|"]
    for r in sorted(rows, key=lambda r: (r["verdict"] != "VIOLATED", r["track"], r["chapter_from"] or 0)):
        if r["verdict"] in ("VIOLATED", "UNRESOLVED"):
            md.append(f"| {r['track']} | {r['kind']} | {r['from']} | {r['to']} | {r['verdict']} | {r.get('confidence') or ''} | {r.get('source') or ''} | {str(r.get('why') or '').replace('|', '/')[:120]} |")
    open(str(case / a.out).replace(".json", ".md"), "w", encoding="utf-8").write("\n".join(md) + "\n")
    print(json.dumps(summary, ensure_ascii=False))


if __name__ == "__main__":
    main()
