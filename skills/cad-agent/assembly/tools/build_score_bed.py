#!/usr/bin/env python3
"""配乐床构造（OLSK 新增，narrate-and-score 的确定性装配延伸）：把一段生成乐段按影片段落边界铺成全片长度。
节奏对应规则：每个影片段落（由 state-chain 章节按子系统分组）都从乐段的起点重新开始，段落边界处用 2 s 等功率交叉淡化；
终章前的段落末尾不做处理（衰减与地板交给 assemble.py --decay-from / --floor-db）。全过程确定性，落盘 receipt。
用法：python build_score_bed.py --cue cue.wav --chain 装配动画/FILM_v001/state-chain.json --out bed.wav --receipt bed.json
"""
import argparse, json, hashlib, subprocess, wave
from pathlib import Path
import numpy as np
ap = argparse.ArgumentParser()
ap.add_argument("--cue", required=True); ap.add_argument("--chain", required=True); ap.add_argument("--out", required=True)
ap.add_argument("--receipt", required=True); ap.add_argument("--xfade", type=float, default=2.0); ap.add_argument("--fps", type=int, default=30)
a = ap.parse_args()
def read_wav(p):
    r = subprocess.run(["ffmpeg", "-v", "error", "-i", p, "-f", "f32le", "-ac", "2", "-ar", "48000", "-"], capture_output=True, check=True)
    x = np.frombuffer(r.stdout, dtype=np.float32).reshape(-1, 2); return x, 48000
cue, sr = read_wav(a.cue)
chain = json.loads(Path(a.chain).read_text(encoding="utf-8")); ch = chain["chapters"]
groups = [("机架与底座", range(1, 7)), ("Y轴与龙门肩部", range(7, 18)), ("X轴与Z轴", range(18, 33)), ("主轴头与管线", range(33, 41)),
          ("机身梁与门窗框", range(41, 53)), ("电气与刀座", range(53, 59)), ("门窗与护罩", range(59, 79))]
def sec_of(op):
    if not op.startswith("WB-"): return "终章"
    n = int(op[3:].split(".")[0]); return next((g for g, r in groups if n in r), "其他")
bounds = []; cur = None
for x in ch:
    s = sec_of(x["op"])
    if s != cur: bounds.append({"section": s, "start_s": (x["start"] - 1) / a.fps}); cur = s
total = chain["frames"][1] / a.fps
for i, b in enumerate(bounds): b["end_s"] = bounds[i + 1]["start_s"] if i + 1 < len(bounds) else total
n_total = int(round(total * sr)); out = np.zeros((n_total, 2), dtype=np.float32); xf = int(a.xfade * sr)
def tile(length):
    reps = int(np.ceil(length / len(cue)) + 1); y = np.concatenate([cue] * reps, axis=0)[:length]; return y
for i, b in enumerate(bounds):
    s0 = int(round(b["start_s"] * sr)); s1 = int(round(b["end_s"] * sr)); seg = tile(s1 - s0 + (xf if i + 1 < len(bounds) else 0))
    # 段首淡入（除第一段）与段尾淡出，等功率
    if i > 0:
        w = np.sqrt(np.linspace(0, 1, xf, dtype=np.float32))[:, None]; seg[:xf] *= w
    if i + 1 < len(bounds):
        w = np.sqrt(np.linspace(1, 0, xf, dtype=np.float32))[:, None]; seg[-xf:] *= w
    e = min(n_total, s0 + len(seg)); out[s0:e] += seg[:e - s0]
peak = float(np.abs(out).max());
if peak > 0.98: out *= 0.98 / peak
pcm = (np.clip(out, -1, 1) * 32767).astype(np.int16)
with wave.open(a.out, "wb") as w:
    w.setnchannels(2); w.setsampwidth(2); w.setframerate(sr); w.writeframes(pcm.tobytes())
rec = {"cue": a.cue, "cue_sha256": hashlib.sha256(Path(a.cue).read_bytes()).hexdigest(), "cue_seconds": round(len(cue) / sr, 2),
       "chain": a.chain, "total_seconds": round(total, 2), "sections": bounds, "xfade_s": a.xfade, "peak_after": round(min(peak, 0.98), 3),
       "rule": "每段从乐段起点重启；边界 2 s 等功率交叉淡化；衰减/地板由 assemble.py 施加", "out": a.out, "out_sha256": hashlib.sha256(Path(a.out).read_bytes()).hexdigest()}
Path(a.receipt).write_text(json.dumps(rec, ensure_ascii=False, indent=1), encoding="utf-8")
print(json.dumps({k: rec[k] for k in ["cue_seconds", "total_seconds", "xfade_s"]}), [(b["section"], round(b["start_s"], 1)) for b in bounds])
