#!/usr/bin/env python3
"""K-DEL-04 审片分段表：从 state-chain 生成章节表（op/start/end/movers f0 f1）与 N 段审片任务（每段章序连续），
供多智能体并行逐帧审片。输出 装配动画/<RUN>/review-segments.v1.json，并打印每段的提示词要点。
用法：film_review_segments.py --case-dir <case> --run FILM_v005 [--segments 10] [--mp4 <path>]"""
import argparse, json
from pathlib import Path
ap = argparse.ArgumentParser(); ap.add_argument("--case-dir", required=True); ap.add_argument("--run", required=True)
ap.add_argument("--segments", type=int, default=10); ap.add_argument("--mp4", default=None)
a = ap.parse_args(); RUN = Path(a.case_dir).resolve() / "装配动画" / a.run
c = json.loads((RUN / "state-chain.json").read_text(encoding="utf-8"))
rows = []
for i, x in enumerate(c["chapters"], start=1):
    rows.append({"index": i, "op": x["op"], "start": x["start"], "end": x["end"], "finale": bool(x.get("finale")),
                 "presence_only": bool(x.get("presence_only")),
                 "movers": [{"occ": m["occ"], "product": m.get("product", "")[:40], "f0": m["f0"], "f1": m["f1"],
                             "approach_mm": m.get("approach_mm")} for m in x.get("movers", [])]})
n = len(rows); per = -(-n // a.segments)
segs = [{"seg": k, "chapters": [r["index"] for r in rows[k * per:(k + 1) * per]],
         "ops": [r["op"] for r in rows[k * per:(k + 1) * per]]} for k in range(a.segments) if rows[k * per:(k + 1) * per]]
mp4 = a.mp4 or next((str(p) for p in sorted(RUN.glob("*_scored.mp4"))), None)
doc = {"$schema": "assembly-ontology.review-segments/v1", "run": a.run, "fps": 30, "frames": c["frames"], "mp4": mp4,
       "defect_taxonomy": {"a": "动件穿过/切入其他件", "b": "静止件相互重叠或嵌入", "c": "玻璃/透明件不透明或发黑",
                           "d": "机位：主体过小/出画/俯视仰视错误", "e": "虚化/半透明/X 光假象", "f": "材质颜色异常",
                           "g": "其他：悬浮、弹入无位移、方向错误、远处飞入"},
       "severity": {"1": "轻微", "2": "影响理解", "3": "该章无法作为指导"},
       "gate": "K-DEL-04：三级缺陷为零才可交付；每段报告随验收包",
       "chapters": rows, "segments": segs}
(RUN / "review-segments.v1.json").write_text(json.dumps(doc, ensure_ascii=False, indent=1), encoding="utf-8")
print(f"[review] {n} chapters → {len(segs)} segments; mp4={mp4}")
for s in segs: print(f"  seg{s['seg']}: chapters {s['chapters'][0]}–{s['chapters'][-1]} ({s['ops'][0]} … {s['ops'][-1]})")
