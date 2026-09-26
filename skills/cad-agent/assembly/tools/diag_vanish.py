import bpy, sys, json
argv = sys.argv[sys.argv.index("--")+1:]
chain = json.load(open(argv[0]))
cases = [c.split(":") for c in argv[1:]]   # frame:chapter_index(1-based)
sc = bpy.context.scene
for fr_s, ci_s in cases:
    f = int(fr_s); ci = int(ci_s); ch = chain['chapters'][ci-1]
    keys = []
    for m in ch.get('movers', []):
        keys.append((m.get('occ'), m.get('product'), m.get('f0'), m.get('f1')))
    sc.frame_set(f)
    print(f"FRAME {f} ch{ci} {ch['op']} [{ch['start']}-{ch['end']}]")
    for occ, prod, f0, f1 in keys:
        obs = [ob for ob in sc.objects if ob.type=='MESH' and occ and (ob.name==occ or ob.name.startswith(occ+'.') or ob.name.startswith(occ+'_'))]
        for ob in obs:
            mw = ob.matrix_world.translation
            print(f"   {ob.name[:24]:24s} {str(prod)[:30]:30s} f0={f0} f1={f1} alpha={ob.color[3]:.2f} hide_r={ob.hide_render} hide_v={ob.hide_viewport} loc=({mw.x:.0f},{mw.y:.0f},{mw.z:.0f}) dim={max(ob.dimensions):.0f}")
        if not obs: print("   (no object for", occ, prod, ")")
