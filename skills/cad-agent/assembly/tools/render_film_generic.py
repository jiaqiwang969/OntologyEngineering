#!/usr/bin/env python3
"""渲染装配动画的帧序列（参数化移植版）。

只打开 build_assembly_animation_generic.py 存下的源场景渲染，不改场景内容。

用法：blender -b --factory-startup -P render_film_generic.py -- --case-dir <robot dir> --run FILM_v001 [--every 1] [--from 1] [--to 0]
"""
import sys
from pathlib import Path

import bpy

argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []


def arg(n, d):
    return argv[argv.index(n) + 1] if n in argv else d


CASE = Path(arg("--case-dir", "")).resolve()
RUN = CASE / "装配动画" / arg("--run", "FILM_v001")
EVERY = int(arg("--every", 1))
F0 = int(arg("--from", 1))
F1 = int(arg("--to", 0))

bpy.ops.wm.open_mainfile(filepath=str(RUN / "assembly_source.blend"))
sc = bpy.context.scene
# OLSK 增量（2026-09-03，v005）：--samples 提高 EEVEE 时间抗锯齿采样数，DITHERED 半透明（虚化/极淡台架件）
# 在默认采样下呈颗粒噪点；只改渲染质量参数，不改场景内容。
_smp = int(arg("--samples", 0))
if _smp > 0:
    sc.eevee.taa_render_samples = _smp
f1 = F1 or sc.frame_end
out = RUN / "frames_raw"
out.mkdir(exist_ok=True)
sc.render.image_settings.file_format = "PNG"
sc.render.film_transparent = False
n = 0
for f in range(F0, f1 + 1, EVERY):
    sc.frame_set(f)
    sc.render.filepath = str(out / f"r_{f:05d}")
    bpy.ops.render.render(write_still=True)
    n += 1
    if n % 25 == 0:
        print(f"[render] {f}/{f1}", flush=True)
print(f"[render] 完成 {n} 帧 → {out}")
