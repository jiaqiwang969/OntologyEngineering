#!/usr/bin/env python3
"""K-SCN-10 动作可读性测量（表达层门禁，只读场景，不改 .blend）。

对 state-chain 每个动件在 f0 / 中程 / f1 三帧，用 EEVEE Cryptomatte(Object) 合成器 matte 数可见像素，
并用网格顶点投影求屏幕包围盒与 f0→f1 的屏幕位移。输出 装配动画/<RUN>/readability.v1.json：
  visible_px（中程帧可见像素）、bbox_px（投影包围盒面积）、size_px=sqrt(bbox_px)、shift_px、visible_frac=visible_px/bbox_px。
用法：blender -b --factory-startup -P measure_readability.py -- --case-dir <case> --run FILM_v005 [--scale 0.25] [--samples 4]
"""
import json, math, sys, time
from pathlib import Path
import bpy, numpy as np
from bpy_extras.object_utils import world_to_camera_view

argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
def arg(n, d): return argv[argv.index(n) + 1] if n in argv else d
CASE = Path(arg("--case-dir", "")).resolve(); RUN = CASE / "装配动画" / arg("--run", "FILM_v005")
SCALE = float(arg("--scale", 0.25)); SAMPLES = int(arg("--samples", 4)); LIMIT = int(arg("--limit", 0))
OPS = set(arg("--ops", "").split(",")) - {""}
MIN_PX = float(arg("--min-size-px", 24)); MIN_SHIFT_PX = float(arg("--min-shift-px", 12)); MIN_FRAC = float(arg("--min-visible-frac", 0.25))
bpy.ops.wm.open_mainfile(filepath=str(RUN / "assembly_source.blend"))
sc = bpy.context.scene; cam = sc.camera
chain = json.loads((RUN / "state-chain.json").read_text(encoding="utf-8"))
W, H = int(sc.render.resolution_x * SCALE), int(sc.render.resolution_y * SCALE)
sc.render.resolution_percentage = int(SCALE * 100); sc.eevee.taa_render_samples = SAMPLES
sc.render.image_settings.file_format = "PNG"; sc.render.film_transparent = False
vl = sc.view_layers[0]; vl.use_pass_cryptomatte_object = True; vl.pass_cryptomatte_depth = 2
# Blender 5.x：合成器是独立节点组（scene.compositing_node_group），Composite 节点已由组输出取代
tree = bpy.data.node_groups.new("readability_comp", "CompositorNodeTree")
sc.compositing_node_group = tree; sc.use_nodes = True
tree.interface.new_socket("Image", in_out="OUTPUT", socket_type="NodeSocketColor")
rl = tree.nodes.new("CompositorNodeRLayers"); cm = tree.nodes.new("CompositorNodeCryptomatteV2")
cm.source = "RENDER"; cm.layer_name = f"{vl.name}.CryptoObject"
out = tree.nodes.new("CompositorNodeOutputFile"); outdir = RUN / "readability_mattes"; outdir.mkdir(exist_ok=True)
out.directory = str(outdir)
# Blender 5.2：输出项须显式创建（连线到 __extend__ 口在后台模式下不生成项）；格式按项设 PNG
try:
    item = out.file_output_items.new(socket_type="FLOAT", name="matte")
except TypeError:
    item = out.file_output_items.new("FLOAT", "matte")
# Blender 5.2 后台模式下 File Output 节点被锁在 OPEN_EXR_MULTILAYER；用无压缩 EXR，自带最小读取器解 matte.V
try:
    out.format.exr_codec = "NONE"
except Exception as exc:
    print("[readability] exr codec note:", exc)
sys.path.insert(0, str(Path(__file__).resolve().parent))
from exr_reader_min import read_channels
sock = next((s_ for s_ in out.inputs if s_.name == "matte"), out.inputs[0])
tree.links.new(rl.outputs["Image"], cm.inputs["Image"]); tree.links.new(cm.outputs["Matte"], sock)
gout = tree.nodes.new("NodeGroupOutput"); tree.links.new(rl.outputs["Image"], gout.inputs[0])
sc.render.filepath = str(outdir / "dummy")
def members(m):
    return m.get("member_occs") or [m["occ"]]
def matte_px(frame, names):
    cm.matte_id = ", ".join(names); sc.frame_set(frame)
    for old_ in outdir.glob("*.exr"): old_.unlink()
    bpy.ops.render.render(write_still=False)
    exrs = sorted(outdir.glob("*.exr"))
    if not exrs:
        raise SystemExit(f"[readability] 合成器未写出 matte EXR（目录内: {[f.name for f in outdir.glob('*')]}）")
    chans, _hdr = read_channels(str(exrs[-1]))
    key = next((k for k in chans if k.lower().endswith("matte.v") or "matte" in k.lower()), list(chans)[0])
    px = chans[key]
    for p_ in exrs: p_.unlink(missing_ok=True)
    return int((px > 0.5).sum())
def bbox_px(frame, names):
    sc.frame_set(frame); xs, ys = [], []
    dg = bpy.context.evaluated_depsgraph_get()
    for nm in names:
        ob = bpy.data.objects.get(nm)
        if ob is None or ob.hide_render: continue
        eo = ob.evaluated_get(dg); me = eo.to_mesh(); mw = ob.matrix_world
        vs = me.vertices; step = max(1, len(vs) // 400)
        for i in range(0, len(vs), step):
            c = world_to_camera_view(sc, cam, mw @ vs[i].co)
            if c.z > 0: xs.append(c.x * W); ys.append(c.y * H)
        eo.to_mesh_clear()
    if not xs: return 0.0, None
    # 投影轮廓面积用凸包（细长斜置件的包围盒大半是空的，会把可见占比压得虚低）
    pts = sorted(set((min(W, max(0.0, x)), min(H, max(0.0, y))) for x, y in zip(xs, ys)))
    def cross(o, a_, b_): return (a_[0] - o[0]) * (b_[1] - o[1]) - (a_[1] - o[1]) * (b_[0] - o[0])
    lower, upper = [], []
    for q in pts:
        while len(lower) >= 2 and cross(lower[-2], lower[-1], q) <= 0: lower.pop()
        lower.append(q)
    for q in reversed(pts):
        while len(upper) >= 2 and cross(upper[-2], upper[-1], q) <= 0: upper.pop()
        upper.append(q)
    hull = lower[:-1] + upper[:-1]
    area = 0.0
    for k in range(len(hull)):
        x0_, y0_ = hull[k]; x1_, y1_ = hull[(k + 1) % len(hull)]; area += x0_ * y1_ - x1_ * y0_
    area = abs(area) / 2.0
    return area, ((min(xs) + max(xs)) / 2, (min(ys) + max(ys)) / 2)
rows = []; t0 = time.time(); n = 0
for ch in chain["chapters"]:
    if OPS and ch["op"] not in OPS: continue
    for m in ch.get("movers", []):
        if LIMIT and n >= LIMIT: break
        names = [x for x in members(m) if bpy.data.objects.get(x)]
        if not names: continue
        f0, f1 = m["f0"], m["f1"]; mid = (f0 + f1) // 2
        f1m = max(f0 + 1, f1 - 1)   # 就位帧取 f1-1：f1 与下一机位首帧重合，相机已切走
        vis_mid = matte_px(mid, names); vis_f1 = matte_px(f1m, names)
        if OPS: print("[readability-debug]", ch["op"], m["occ"], names, "mid", mid, "vis", vis_mid, "hide_render", [bpy.data.objects[x].hide_render for x in names])
        bb_mid, c_mid = bbox_px(mid, names); _, c0 = bbox_px(f0, names); _, c1 = bbox_px(f1m, names)
        shift = math.hypot(c1[0] - c0[0], c1[1] - c0[1]) if (c0 and c1) else None
        # 换算到成片分辨率的像素（测量在 SCALE 缩放下进行）
        size_full = math.sqrt(bb_mid) / SCALE; shift_full = None if shift is None else shift / SCALE
        frac = (vis_mid / bb_mid) if bb_mid > 0 else 0.0
        fails = []
        if size_full < MIN_PX: fails.append("SIZE")
        if shift_full is None or shift_full < MIN_SHIFT_PX: fails.append("SHIFT")
        if frac < MIN_FRAC: fails.append("VISIBLE")
        rows.append({"op": ch["op"], "occ": m["occ"], "product": m.get("product", "")[:40], "f0": f0, "f1": f1,
                     "visible_px_mid": round(vis_mid / SCALE / SCALE), "visible_px_seated": round(vis_f1 / SCALE / SCALE),
                     "hull_px_mid": round(bb_mid / SCALE / SCALE), "size_px": round(size_full, 1),
                     "shift_px": None if shift_full is None else round(shift_full, 1),
                     "visible_frac_mid": round(frac, 3), "verdict": "READABLE" if not fails else "UNREADABLE:" + "+".join(fails)})
        n += 1
    if LIMIT and n >= LIMIT: break
summary = {"movers": len(rows), "readable": sum(1 for r in rows if r["verdict"] == "READABLE"),
           "unreadable_size": sum(1 for r in rows if "SIZE" in r["verdict"]),
           "unreadable_shift": sum(1 for r in rows if "SHIFT" in r["verdict"]),
           "unreadable_visible": sum(1 for r in rows if "VISIBLE" in r["verdict"])}
doc = {"$schema": "assembly-ontology.readability/v1", "run": RUN.name, "measure_res": [W, H], "samples": SAMPLES,
       "thresholds_full_res_px": {"min_size_px": MIN_PX, "min_shift_px": MIN_SHIFT_PX, "min_visible_frac": MIN_FRAC},
       "criterion": "K-SCN-10：中程帧 Cryptomatte 可见像素 / 投影凸包像素；f0→f1 屏幕位移像素；数值已换算到成片分辨率",
       "seconds": round(time.time() - t0, 1), "summary": summary, "rows": rows}
(RUN / "readability.v1.json").write_text(json.dumps(doc, ensure_ascii=False, indent=1), encoding="utf-8")
print(f"[readability] {summary} {doc['seconds']} s → {RUN / 'readability.v1.json'}")
