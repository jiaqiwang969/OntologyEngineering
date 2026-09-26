# 渲染指定帧列表（预览用）：blender -b <blend> -P render_frames_list.py -- <frames.json> <outdir> [samples]
import bpy, sys, json, os
argv=sys.argv[sys.argv.index("--")+1:]
frames=json.load(open(argv[0]))["frames"]; out=argv[1]; samples=int(argv[2]) if len(argv)>2 else 48
os.makedirs(out, exist_ok=True)
sc=bpy.context.scene; sc.eevee.taa_render_samples=samples
sc.render.image_settings.file_format='PNG'
for f in frames:
    sc.frame_set(f); sc.render.filepath=os.path.join(out, f"r_{f:05d}.png"); bpy.ops.render.render(write_still=True)
print("PREVIEW_DONE", len(frames))
