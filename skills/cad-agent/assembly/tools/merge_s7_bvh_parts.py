#!/usr/bin/env python3
"""合并 sweep_certify_bvh_generic --out-suffix 分片输出为 本体/S7-扫掠认证-bvh.v1.json（OLSK 增量）。
只拼接 operations，按 S3 绑定工序序排序；分片元数据逐片保留。"""
import json, glob, sys
from pathlib import Path
case = Path(sys.argv[1] if len(sys.argv) > 1 else ".")
parts = sorted(glob.glob(str(case / "本体/S7-扫掠认证-bvh-part*.v1.json")) + glob.glob(str(case / "本体/S7-扫掠认证-bvh-remote*.v1.json")))
if not parts: sys.exit("no parts")
bind = json.loads((case / "本体/S3-工序实例绑定.v1.json").read_text(encoding="utf-8"))
rank = {o["op"]: i for i, o in enumerate(bind["operations"])}
ops, meta = [], []
for p in parts:
    d = json.loads(Path(p).read_text(encoding="utf-8"))
    ops += d.get("operations", []); meta.append({"part": p, **{k: v for k, v in d.items() if k != "operations"}})
seen=set(); ops=[o for o in ops if not (o['op'] in seen or seen.add(o['op']))]  # 同一工序多片重复时取先到者
ops.sort(key=lambda o: rank.get(o["op"], 10**6))
base = json.loads(Path(parts[0]).read_text(encoding="utf-8"))
doc = {**{k: v for k, v in base.items() if k != "operations"}, "merged_from_parts": meta, "operations": ops}
(case / "本体/S7-扫掠认证-bvh.v1.json").write_text(json.dumps(doc, ensure_ascii=False), encoding="utf-8")
print("merged", len(parts), "parts,", len(ops), "operations")
