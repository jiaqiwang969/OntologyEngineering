# -*- coding: utf-8 -*-
"""紧固件层（O-26）：官方 STEP 不含机器紧固件（只有阀/电气子件里的 136 颗 M3），而手册 GLB 每一步的节点里有 2487 颗
（HF-screw M12-20 ×268、B-screw M5-8 ×179、C-screw M4-16 ×150 …）。本脚本在 Blender 里导入手册 GLB，
按步根节点取出紧固件网格，用 S3-GLB 桥接的 frame_fit 反变换到 STEP 世界坐标（mm），导出 STL + 索引：
  网格导出-紧固件/<op>/<name>.stl，本体/S1-紧固件层.v1.json（op、名称、规格、世界中心、主轴（螺钉轴）、bbox）。
下游：紧固件按工序成组作为 kind=fastener_set 动件（沿各自螺钉轴短行程），HUD 仍按手册清单陈述。
用法：blender -b --python tools/extract_glb_fasteners.py -- --case-dir . [--glb <path>] [--out-dir 网格导出-紧固件]
"""
import bpy, sys, json, re, os, math
import mathutils
import numpy as np

argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
def arg(k, d=None):
    return argv[argv.index(k) + 1] if k in argv else d
CASE = arg("--case-dir", ".")
GLB = arg("--glb", None)
if GLB is None:
    raise SystemExit("--glb is required: pass the manual GLB from the project source")
OUTD = os.path.join(CASE, arg("--out-dir", "网格导出-紧固件"))
FAST = re.compile(r"(?i)screw|bolt|\bnut\b|washer|t-nut|tnut")

bridge = json.load(open(os.path.join(CASE, "本体/S3-GLB桥接.v1.json"), encoding="utf-8"))
ff = bridge["frame_fit"]
scale = float(ff["scale"])                       # STEP mm → GLB m
R = np.array(ff["R_step_to_glb"], dtype=float)    # p_glb = scale * R @ p_step + t
t = np.array(ff["t_m"], dtype=float)
Rinv = R.T

def glb_to_step(p):
    return (Rinv @ (np.array(p, dtype=float) - t)) / scale

bpy.ops.wm.read_factory_settings(use_empty=True)
bpy.ops.import_scene.gltf(filepath=GLB)
objs = list(bpy.data.objects)

def root_of(o):
    while o.parent is not None:
        o = o.parent
    return o

def step_of_root(name):
    m = re.match(r"^(\d+(?:\.\d+)?)\s", name)
    if not m:
        return None
    s = m.group(1)
    return "WB-" + (s if "." in s else s.zfill(2)) if len(s.split(".")[0]) >= 2 else "WB-" + s.zfill(2)

os.makedirs(OUTD, exist_ok=True)
rows = []
n_written = 0
for o in objs:
    if o.type != "MESH" or not FAST.search(o.name):
        continue
    root = root_of(o)
    op = step_of_root(root.name)
    if op is None:
        continue
    me = o.data
    # 世界坐标顶点（GLB m）→ STEP mm
    mw = o.matrix_world
    pts = np.array([[(mw @ v.co).x, (mw @ v.co).y, (mw @ v.co).z] for v in me.vertices], dtype=float)
    if len(pts) < 4:
        continue
    pts_step = np.array([glb_to_step(p) for p in pts])
    ctr = pts_step.mean(axis=0)
    # 主轴 = 协方差最大特征向量（螺钉轴）；bbox
    cov = np.cov((pts_step - ctr).T)
    w, v = np.linalg.eigh(cov)
    axis = v[:, int(np.argmax(w))]
    lo, hi = pts_step.min(axis=0), pts_step.max(axis=0)
    spec = re.sub(r"\.\d+$", "", o.name)
    safe = re.sub(r"[^0-9A-Za-z_.-]+", "_", o.name)
    opd = os.path.join(OUTD, op)
    os.makedirs(opd, exist_ok=True)
    # 导出 STL（STEP mm 坐标）：写 ASCII STL 最省事
    faces = [tuple(p.vertices) for p in me.polygons]
    with open(os.path.join(opd, safe + ".stl"), "w") as f:
        f.write(f"solid {safe}\n")
        for tri in faces:
            if len(tri) < 3:
                continue
            for k in range(1, len(tri) - 1):
                a, b, c = pts_step[tri[0]], pts_step[tri[k]], pts_step[tri[k + 1]]
                n = np.cross(b - a, c - a); nn = np.linalg.norm(n); n = n / nn if nn > 0 else n
                f.write(f" facet normal {n[0]:.6f} {n[1]:.6f} {n[2]:.6f}\n  outer loop\n")
                for p in (a, b, c):
                    f.write(f"   vertex {p[0]:.4f} {p[1]:.4f} {p[2]:.4f}\n")
                f.write("  endloop\n endfacet\n")
        f.write(f"endsolid {safe}\n")
    n_written += 1
    rows.append({"op": op, "glb_root": root.name, "glb_node": o.name, "spec": spec, "stl": f"{op}/{safe}.stl",
                 "center_mm": [round(float(x), 3) for x in ctr], "axis": [round(float(x), 6) for x in axis],
                 "bbox_min_mm": [round(float(x), 3) for x in lo], "bbox_max_mm": [round(float(x), 3) for x in hi],
                 "length_mm": round(float(np.ptp(pts_step @ axis)), 3), "verts": int(len(pts))})

out = {"$schema": "assembly-ontology.fastener-layer/v1", "source": {"glb": GLB, "frame_fit": ff, "bridge": "本体/S3-GLB桥接.v1.json"},
       "rule": "手册 GLB 每步根节点下名称含 screw/bolt/nut/washer/t-nut 的网格 = 该步紧固件；坐标经 frame_fit 反变换到 STEP 世界（mm）；主轴取顶点协方差最大特征向量",
       "summary": {"fasteners": len(rows), "ops": len({r['op'] for r in rows}), "specs": len({r['spec'] for r in rows})},
       "rows": rows,
       "claim_boundary": ["紧固件几何来自手册 GLB（非 STEP），位置精度受 frame_fit 影响（桥接 extent_error 7.9 mm、最近邻中位 70 mm 前拟合后 3593 对复核）；只用于表达与画面口径审计，不作为 STEP 身份。"]}
json.dump(out, open(os.path.join(CASE, "本体/S1-紧固件层.v1.json"), "w", encoding="utf-8"), ensure_ascii=False, indent=1)
print("FASTENER_LAYER_DONE", out["summary"])
