#!/usr/bin/env python3
"""K-DEL-04 审片汇总：读取各段 findings JSON（seg*.findings.json，每条 {op,type,part,description,frames,severity}），
写 装配动画/<RUN>/review-findings.v1.json（summary: segments, findings, sev1/2/3, by_type, by_op），供验收包 frame_review 块与验证器。
用法：aggregate_review_findings.py --case-dir <case> --run FILM_v007 --dir <findings 目录>"""
import argparse, collections, glob, json, os
from pathlib import Path
ap = argparse.ArgumentParser(); ap.add_argument("--case-dir", required=True); ap.add_argument("--run", required=True); ap.add_argument("--dir", required=True)
a = ap.parse_args(); RUN = Path(a.case_dir).resolve() / "装配动画" / a.run
rows, segs = [], []
for f in sorted(glob.glob(os.path.join(a.dir, "seg*.findings.json"))):
    d = json.loads(Path(f).read_text(encoding="utf-8")); segs.append(os.path.basename(f))
    for r in d.get("findings", []):
        r = dict(r); r["segment"] = os.path.basename(f).split(".")[0]
        try: r["severity"] = int(r.get("severity", 0))
        except Exception: r["severity"] = 0
        rows.append(r)
summary = {"segments": len(segs), "findings": len(rows), "sev3": sum(1 for r in rows if r["severity"] >= 3),
           "sev2": sum(1 for r in rows if r["severity"] == 2), "sev1": sum(1 for r in rows if r["severity"] == 1),
           "by_type": dict(collections.Counter(str(r.get("type", "?"))[:1] for r in rows)),
           "by_op": dict(collections.Counter(r.get("op", "?") for r in rows).most_common(20)),
           "gate": "K-DEL-04: sev3 == 0 才可交付"}
doc = {"$schema": "assembly-ontology.review-findings/v1", "run": a.run, "summary": summary, "findings": rows,
       "segment_files": segs}
(RUN / "review-findings.v1.json").write_text(json.dumps(doc, ensure_ascii=False, indent=1), encoding="utf-8")
print(f"[review] {summary}")
