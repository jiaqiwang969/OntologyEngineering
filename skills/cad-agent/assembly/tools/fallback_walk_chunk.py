#!/usr/bin/env python3
"""行走分片回退：某实体在新序（v2）下的序列感知行走跑不完（OLSK 实测 WB-02.1 加强板单件 > 2 h），
用旧序目录里同一实体的行走行写成新序目录的分片文件，行内标注 fallback（来源与理由），供合并与可播表消费。
判据边界：旧序下该实体移出时的剩余集与新序不同，回退结果只作为"可播行程候选"，最终由穿模/画面口径审计裁定。
用法：fallback_walk_chunk.py --case-dir . --op WB-02.1 --chunk 1 --old-dir 本体/S7-序列感知认证-延长150 --new-dir 本体/S7-序列感知认证-v2 --play-order 本体/S7-工序内播放序.v2.json
"""
import argparse, json, glob
from pathlib import Path


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--case-dir", required=True, type=Path)
    ap.add_argument("--op", required=True)
    ap.add_argument("--chunk", required=True, type=int)
    ap.add_argument("--old-dir", default="本体/S7-序列感知认证-延长150")
    ap.add_argument("--new-dir", default="本体/S7-序列感知认证-v2")
    ap.add_argument("--play-order", default="本体/S7-工序内播放序.v2.json")
    a = ap.parse_args()
    C = a.case_dir
    po = json.loads((C / a.play_order).read_text(encoding="utf-8"))
    order = [e["occ"] for e in po["ops"][a.op]["order"]]
    occ = order[a.chunk]
    old = json.loads((C / a.old_dir / f"{a.op}.json").read_text(encoding="utf-8"))
    row = next((r for r in old["rows"] if r["occ"] == occ), None)
    if row is None:
        raise SystemExit(f"{a.op}: 旧序记录里没有 {occ}")
    tmpl_files = sorted(glob.glob(str(C / a.new_dir / f"{a.op}.chunk*.json")))
    if not tmpl_files:
        raise SystemExit(f"{a.op}: 新序目录没有可作模板的分片")
    tmpl = json.loads(Path(tmpl_files[0]).read_text(encoding="utf-8"))
    row = dict(row)
    row["walk_index"] = a.chunk
    row["fallback"] = {"from": f"{a.old_dir}/{a.op}.json", "why": "新序行走超时（> 2.5 h）；旧序同实体行走行回退，剩余集不同，最终由审计裁定", "old_walk_index": next((r["walk_index"] for r in old["rows"] if r["occ"] == occ), None)}
    out = dict(tmpl)
    out.update({"chunk": [a.chunk, a.chunk + 1], "movers": 1, "recovered": int(bool(row.get("recovered"))), "stuck": int(row.get("verdict") == "STUCK"), "seconds": 0.0, "rows": [row],
                "fallback_note": "本分片为旧序回退（见 rows[].fallback）"})
    dst = C / a.new_dir / f"{a.op}.chunk{a.chunk}-{a.chunk + 1}.json"
    dst.write_text(json.dumps(out, ensure_ascii=False), encoding="utf-8")
    print(f"wrote {dst} occ={occ} product={row.get('product')} verdict={row.get('verdict')} clear={row['best'].get('clear_run_mm')}")


if __name__ == "__main__":
    main()
