#!/usr/bin/env python3
"""把农场分片输出 {op}.chunk{s}-{e}.json 合并为 {op}.json。

校验：分片连续覆盖 [0,N)、无重叠、参数一致；合并后分片移入 chunks/ 子目录，
保证 merge_s7_generic 的 glob 只见到每工序一份权威文件。
"""
import argparse
import json
import re
import shutil
from collections import Counter
from pathlib import Path

CHUNK = re.compile(r"^(.*)\.chunk(\d+)-(\d+)\.json$")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--case-dir", required=True, type=Path)
    ap.add_argument("--dir", default=None, help="覆盖目标目录（默认 s7_runs；行走用 s7_seq_cert）")
    a = ap.parse_args()
    cfg = json.loads((a.case_dir / "robot-config.v1.json").read_text(encoding="utf-8"))
    RUNS = a.case_dir / (a.dir or cfg["paths"]["s7_runs"])
    stash = RUNS / "chunks"
    groups = {}
    for f in RUNS.glob("*.chunk*.json"):
        m = CHUNK.match(f.name)
        if m:
            groups.setdefault(m.group(1), []).append((int(m.group(2)), int(m.group(3)), f))
    for op, parts in sorted(groups.items()):
        parts.sort()
        cover = 0
        docs = []
        for c0, c1, f in parts:
            if c0 != cover:
                raise SystemExit(f"{op}: 分片不连续，{cover} 后接 {c0}")
            docs.append(json.loads(f.read_text(encoding="utf-8")))
            cover = c1
        total = docs[0].get("total_entities") or docs[0]["movers"]
        if cover != total:
            raise SystemExit(f"{op}: 覆盖 {cover} != 实体总数 {total}")
        key = "per_mover" if "per_mover" in docs[0] else "rows"
        per = [m for d in docs for m in d[key]]
        if key == "rows":  # 行走逆序输出：按 walk_index 升序恢复全工序次序
            per.sort(key=lambda r: r["walk_index"])
        out = dict(docs[0])
        out["chunk"] = None
        out["merged_from_chunks"] = [[c0, c1] for c0, c1, _ in parts]
        out[key] = per
        if key == "per_mover":
            out["released"] = sum(1 for x in per if x.get("verdict") == "RELEASES")
        else:
            out["movers"] = len(per)
            out["recovered"] = sum(1 for x in per if x.get("recovered"))
            out["stuck"] = sum(1 for x in per if x.get("verdict") == "STUCK")
        out["entity_kinds"] = dict(Counter(x.get("kind", "leaf") for x in per))
        out["seconds"] = round(sum(d["seconds"] for d in docs), 1)
        (RUNS / f"{op}.json").write_text(json.dumps(out, ensure_ascii=False), encoding="utf-8")
        stash.mkdir(exist_ok=True)
        for _, _, f in parts:
            shutil.move(str(f), stash / f.name)
        stat = (f"脱离 {out['released']}" if key == "per_mover"
                else f"救回 {out['recovered']} 真卡死 {out['stuck']}")
        print(f"{op}: 合并 {len(parts)} 片 → {len(per)} 实体，{stat}")
    if not groups:
        print("无分片文件")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
