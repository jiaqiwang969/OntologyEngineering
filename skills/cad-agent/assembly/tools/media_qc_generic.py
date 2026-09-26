#!/usr/bin/env python3
"""媒体 QC：全解码、帧数、faststart、framemd5、视频哈希（X1+ A10 要求；X1 遗留缺口补齐）。

用法：python3 media_qc_generic.py --case-dir <dir> --run FILM_v004 --mp4 <path>
输出：<run>/media-qc.v1.json
"""
import argparse
import hashlib
import json
import subprocess
from pathlib import Path


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--case-dir", required=True, type=Path)
    ap.add_argument("--run", required=True)
    ap.add_argument("--mp4", required=True, type=Path)
    a = ap.parse_args()
    D = a.case_dir / "装配动画" / a.run
    chain = json.loads((D / "state-chain.json").read_text(encoding="utf-8"))
    expect_frames = chain["frames"][1]

    probe = json.loads(subprocess.run(
        ["ffprobe", "-v", "error", "-select_streams", "v:0",
         "-count_frames", "-show_entries",
         "stream=nb_read_frames,codec_name,pix_fmt,width,height,r_frame_rate",
         "-show_entries", "format=duration,size",
         "-of", "json", str(a.mp4)], capture_output=True, text=True).stdout)
    st = probe["streams"][0]
    nframes = int(st["nb_read_frames"])

    dec = subprocess.run(["ffmpeg", "-v", "error", "-i", str(a.mp4), "-f", "null", "-"],
                         capture_output=True, text=True)
    full_decode_pass = dec.returncode == 0 and not dec.stderr.strip()

    # OLSK 增量：只对视频流做 framemd5（带配乐的成片含音频帧，行数会超过视频帧数）
    md5run = subprocess.run(["ffmpeg", "-v", "error", "-i", str(a.mp4), "-map", "0:v:0", "-f", "framemd5", "-"],
                            capture_output=True, text=True)
    md5_lines = [l for l in md5run.stdout.splitlines() if l and not l.startswith("#")]
    framemd5_digest = hashlib.sha256("\n".join(md5_lines).encode()).hexdigest()

    head = a.mp4.read_bytes()[:4096]
    faststart = b"moov" in head

    doc = {
        "$schema": "assembly-ontology.media-qc/v1",
        "run": a.run,
        "mp4": {"path": str(a.mp4), "bytes": a.mp4.stat().st_size,
                "sha256": hashlib.sha256(a.mp4.read_bytes()).hexdigest()},
        "stream": {"codec": st["codec_name"], "pix_fmt": st["pix_fmt"],
                   "width": st["width"], "height": st["height"],
                   "fps": st["r_frame_rate"], "frames": nframes,
                   "duration_s": float(probe["format"]["duration"])},
        "checks": {
            "frame_count_matches_chain": nframes == expect_frames,
            "expected_frames": expect_frames,
            "full_decode_pass": full_decode_pass,
            "decode_stderr": dec.stderr[-400:] if dec.stderr.strip() else None,
            "faststart_moov_at_head": faststart,
            "framemd5_lines": len(md5_lines),
            "framemd5_sha256": framemd5_digest,
        },
        "status": ("PASS" if (nframes == expect_frames and full_decode_pass and faststart
                              and len(md5_lines) == nframes) else "FAIL"),
    }
    out = D / "media-qc.v1.json"
    out.write_text(json.dumps(doc, ensure_ascii=False, indent=1), encoding="utf-8")
    print(f"写出 {out}  status={doc['status']}  frames={nframes}/{expect_frames} "
          f"decode={'OK' if full_decode_pass else 'FAIL'} faststart={faststart}")
    return 0 if doc["status"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
