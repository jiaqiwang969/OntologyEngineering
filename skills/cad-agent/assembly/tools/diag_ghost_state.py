# 诊断：给定帧，统计场景里对象颜色 alpha（Object Info Alpha 驱动材质透明）分布、隐藏状态、动件状态
import bpy, sys, json, collections
argv = sys.argv[sys.argv.index("--")+1:] if "--" in sys.argv else []
frames = [int(x) for x in argv[0].split(",")]
sc = bpy.context.scene
chain = json.load(open(argv[1])) if len(argv) > 1 else None
def chapter_at(f):
    if not chain: return None
    for i,c in enumerate(chain['chapters']):
        if c['start'] <= f <= c['end']: return (i+1, c)
    return None
for f in frames:
    sc.frame_set(f)
    ch = chapter_at(f)
    movers = set()
    if ch:
        for m in ch[1].get('movers', []):
            if m.get('f0') is not None and m['f0'] <= f <= m.get('f1', 10**9): movers.add(m.get('occ') or m.get('product'))
    buckets = collections.Counter(); vis = 0; hidden = 0; ghosted = []
    for ob in sc.objects:
        if ob.type != 'MESH': continue
        if ob.hide_render or ob.hide_viewport:
            hidden += 1; continue
        vis += 1
        a = round(ob.color[3], 2)
        buckets[a] += 1
        if a < 0.99:
            d = ob.dimensions; ghosted.append((max(d), ob.name[:34], a))
    ghosted.sort(reverse=True)
    mv = []
    for ob in sc.objects:
        if ob.type == 'MESH' and any(k and k in ob.name for k in movers if k):
            mv.append((ob.name[:34], round(ob.color[3],2), ob.hide_render))
    print(f"FRAME {f} ch={ch[0] if ch else '?'} op={ch[1]['op'] if ch else '?'} visible={vis} hidden={hidden} alpha_buckets={dict(sorted(buckets.items()))}")
    print("   biggest ghosted:", [(round(s), n, a) for s, n, a in ghosted[:8]])
    print("   movers:", mv[:6])
