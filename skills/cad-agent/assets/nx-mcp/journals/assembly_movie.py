# -*- coding: utf-8 -*-
"""Assembly-sequence movie frames for the printing-press assemblies.

Play INSIDE the interactive NX (工具 > 操作记录 > 播放) with the target assembly DISPLAYED
(4色组印刷机 or 八色机总装). Reads work\\journals\\movie_plan_<key>.json, hides everything,
then flies each planned group of components in (MoveComponent by a shrinking offset) while exporting one PNG per
frame under the current Windows user's work\\163-caojun-print\\real\\out\\movie_<key>\\. Nothing is saved: every move / hide is
rolled back with UndoToMark at the end (also on error). Do NOT touch NX while it runs (3–6 minutes)."""
import json, math, os, time, traceback
import NXOpen
import NXOpen.UF
try:
    import NXOpen.Gateway
except Exception:
    pass

WORKROOT = os.path.join(os.path.expanduser("~"), "work")
JDIR = os.environ.get("NX_JOURNAL_DIR", os.path.join(WORKROOT, "journals"))
OUTROOT = os.environ.get("NX_MOVIE_OUTROOT", os.path.join(WORKROOT, "163-caojun-print", "real", "out"))
PLANS = {"4色组印刷机": "movie_plan_sise.json", "八色机总装": "movie_plan_baseji.json"}

session = NXOpen.Session.GetSession()
ufs = NXOpen.UF.UFSession.GetUFSession()
T0 = time.time()
LOGF = [None]


def log(msg):
    if LOGF[0]:
        with open(LOGF[0], "a", encoding="utf-8") as f:
            f.write("[%6.1fs] %s\n" % (time.time() - T0, msg))


def ease(t):
    return t * t * (3 - 2 * t)


def identity():
    m = NXOpen.Matrix3x3()
    m.Xx, m.Xy, m.Xz = 1.0, 0.0, 0.0
    m.Yx, m.Yy, m.Yz = 0.0, 1.0, 0.0
    m.Zx, m.Zy, m.Zz = 0.0, 0.0, 1.0
    return m


def leaf_of(c):
    proto = c.Prototype
    return proto.Leaf if isinstance(proto, NXOpen.BasePart) else proto.OwningPart.Leaf


class Exporter:
    def __init__(self, part, view):
        self.part, self.view, self.mode = part, view, None

    def export(self, path):
        self.view.UpdateDisplay()
        if self.mode in (None, "builder"):
            try:
                b = self.part.Views.CreateImageExportBuilder()
                try:
                    b.FileName = path
                    b.FileFormat = NXOpen.Gateway.ImageExportBuilder.FileFormats.Png
                    try:
                        b.BackgroundOption = NXOpen.Gateway.ImageExportBuilder.BackgroundOptions.CustomColor
                        b.SetCustomBackgroundColor([1.0, 1.0, 1.0])
                    except Exception:
                        pass
                    try:
                        b.EnhanceEdges = True
                    except Exception:
                        pass
                    try:
                        b.RegionMode = False
                    except Exception:
                        pass
                    b.Commit()
                finally:
                    b.Destroy()
                self.mode = "builder"
                return
            except Exception as e:
                if self.mode == "builder":
                    raise
                log("ImageExportBuilder failed (%s); trying UF_DISP_create_image" % str(e)[:120])
        fmt = None
        for name in ("Png", "PNG", "ImagePng", "ImageFormatPng"):
            fmt = getattr(getattr(NXOpen.UF.Disp, "ImageFormat", NXOpen.UF.Disp), name, None)
            if fmt is not None:
                break
        bg = None
        for name in ("White", "WHITE", "BackgroundWhite"):
            bg = getattr(getattr(NXOpen.UF.Disp, "BackgroundColor", NXOpen.UF.Disp), name, None)
            if bg is not None:
                break
        ufs.Disp.CreateImage(path, fmt, bg)
        self.mode = "uf"


PART_PATHS = {"4色组印刷机": os.path.join(WORKROOT, "163-caojun-print", "real", "4色组", "4色组印刷机.prt"),
              "八色机总装": os.path.join(WORKROOT, "163-caojun-print", "real", "八色机总装", "八色机总装.prt")}
ORDER = ["4色组印刷机", "八色机总装"]


def find_open(leaf):
    for p in session.Parts:
        try:
            if p.Leaf == leaf or p.Leaf.startswith(leaf):
                return p
        except Exception:
            continue
    return None


def set_display(part):
    try:
        session.Parts.SetDisplay(part, False, True)
        return "SetDisplay"
    except Exception as e1:
        session.Parts.SetActiveDisplay(part, NXOpen.DisplayPartOption.AllowAdditional, NXOpen.PartDisplayPartWorkPartOption.UseLast)
        return "SetActiveDisplay (%s)" % str(e1)[:60]


def process(leaf, plan_file):
    plan = json.load(open(os.path.join(JDIR, plan_file), encoding="utf-8"))
    key = plan["machine"]
    outdir = os.path.join(OUTROOT, "movie_" + key)
    os.makedirs(outdir, exist_ok=True)
    for f in os.listdir(outdir):
        if f.endswith(".png") or f in ("steps.json", "movie.log", "DONE.txt"):
            try:
                os.remove(os.path.join(outdir, f))
            except Exception:
                pass
    LOGF[0] = os.path.join(outdir, "movie.log")
    log("start %s -> %s" % (leaf, outdir))
    part = find_open(leaf)
    if part is None:
        part, st = session.Parts.OpenBaseDisplay(PART_PATHS[leaf])
        try:
            st.Dispose()
        except Exception:
            pass
        log("opened from disk (was not open)")
    else:
        log("displayed via %s" % set_display(part))
    disp = session.Parts.Display
    log("display part now: %s" % disp.Leaf)
    mark = session.SetUndoMark(NXOpen.Session.MarkVisibility.Visible, "assembly movie")
    frames = [0]
    steps_out = []
    try:
        run(plan, disp, outdir, frames, steps_out)
        log("frames written: %d" % frames[0])
    except Exception:
        log("FATAL " + traceback.format_exc())
    finally:
        try:
            session.UndoToMark(mark, "assembly movie")
            log("undo to mark done (positions and visibility restored)")
        except Exception as e:
            log("undo failed: %s" % e)
        try:
            session.DeleteUndoMark(mark, None)
        except Exception:
            pass
        try:
            if plan.get('_wcs_was') is not None:
                disp.WCS.Visibility = plan['_wcs_was']
        except Exception:
            pass
        try:
            v = disp.ModelingViews.WorkView
            v.Orient(NXOpen.View.Canned.Isometric, NXOpen.View.ScaleAdjustment.Fit)
            v.UpdateDisplay()
        except Exception:
            pass
        with open(os.path.join(outdir, "steps.json"), "w", encoding="utf-8") as f:
            json.dump({"machine": key, "frames": frames[0], "fps": plan.get("fps", 10), "steps": steps_out}, f, ensure_ascii=False, indent=1)
        with open(os.path.join(outdir, "DONE.txt"), "w", encoding="utf-8") as f:
            f.write("frames=%d elapsed=%.0fs\n" % (frames[0], time.time() - T0))
    return frames[0]


def main():
    orig = None
    try:
        orig = session.Parts.Display
    except Exception:
        pass
    results = []
    for leaf in ORDER:
        try:
            results.append((leaf, process(leaf, PLANS[leaf])))
        except Exception:
            log("FATAL(outer) " + traceback.format_exc())
            results.append((leaf, -1))
    if orig is not None:
        try:
            set_display(orig)
        except Exception:
            pass
    with open(os.path.join(OUTROOT, "movie_ALLDONE.txt"), "w", encoding="utf-8") as f:
        f.write("; ".join("%s=%d" % r for r in results) + "\n")
    lw = session.ListingWindow
    lw.Open()
    lw.WriteLine("装配视频帧已生成：" + "；".join("%s %d 帧" % r for r in results) + "（模型未保存，位置与显示已回退）")


def run(plan, disp, outdir, frames, steps_out):
    view = disp.ModelingViews.WorkView
    root = disp.ComponentAssembly.RootComponent
    top = list(root.GetChildren())
    log("top-level components: %d" % len(top))
    child_cache = {}

    def children(c):
        k = c.Tag
        if k not in child_cache:
            child_cache[k] = list(c.GetChildren())
        return child_cache[k]

    movers = {}   # occurrence tag -> (owning part, component object in that part's own context)

    def part_of(c):
        proto = c.Prototype
        return proto if isinstance(proto, NXOpen.BasePart) else proto.OwningPart

    def resolve(sel):
        if sel["scope"] == "root":
            inst = [c for c in top if leaf_of(c) == sel["part"] or (sel.get("prefix") and leaf_of(c).startswith(sel["part"]))]
            idxs = sel.get("indices")
            out = [inst[i] for i in idxs if i < len(inst)] if idxs is not None else inst
            for c in out:
                movers[c.Tag] = (disp, c)
            return out
        parents = [c for c in top if leaf_of(c) == sel["parent_part"]]
        if not parents:
            return []
        p = parents[min(sel.get("parent_index", 0), len(parents) - 1)]
        out = [c for c in children(p) if leaf_of(c) == sel["part"] or (sel.get("prefix") and leaf_of(c).startswith(sel["part"]))]
        try:
            ppart = part_of(p)
            own = list(ppart.ComponentAssembly.RootComponent.GetChildren())
            for c in out:
                match = [x for x in own if leaf_of(x) == leaf_of(c) and x.Name == c.Name] or [x for x in own if leaf_of(x) == leaf_of(c)]
                if match:
                    movers[c.Tag] = (ppart, match[0])
        except Exception as e:
            log("owning-part lookup failed for %s: %s" % (sel["part"], str(e)[:80]))
        return out

    def move(c, vec):
        owner, obj = movers.get(c.Tag, (None, None))
        if owner is None:
            return False
        owner.ComponentAssembly.MoveComponent(obj, vec, ident)
        return True

    # camera: fit everything once, then keep it
    try:
        view.RenderingStyle = NXOpen.View.RenderingStyleType.ShadedWithEdges
    except Exception as e:
        log("rendering style: %s" % e)
    view.Orient(NXOpen.View.Canned.Isometric, NXOpen.View.ScaleAdjustment.Fit)
    try:
        wcs_was = disp.WCS.Visibility
        disp.WCS.Visibility = False
        plan['_wcs_was'] = wcs_was
    except Exception as e:
        log("wcs: %s" % e)
    try:
        view.ZoomAboutPoint(NXOpen.View.ZoomDirection.Out if hasattr(NXOpen.View, 'ZoomDirection') else None, NXOpen.Point3d(0.0, 0.0, 0.0), 0.0)
    except Exception:
        pass
    view.UpdateDisplay()
    exp = Exporter(disp, view)

    def frame():
        frames[0] += 1
        exp.export(os.path.join(outdir, "f%05d.png" % frames[0]))

    # hide everything, record an intro frame
    session.DisplayManager.BlankObjects(top)
    for _ in range(plan.get("intro_frames", 6)):
        frame()
    log("export mode: %s" % exp.mode)
    fly_n, hold_n, dist = plan.get("fly_frames", 8), plan.get("hold_frames", 8), plan.get("fly_distance_mm", 900.0)
    ident = identity()
    asm = disp.ComponentAssembly
    for st in plan["steps"]:
        comps = []
        for sel in st["select"]:
            comps += resolve(sel)
        first = frames[0] + 1
        if not comps:
            log("step %d %s: no components resolved" % (st["id"], st["title"]))
            steps_out.append({"id": st["id"], "title": st["title"], "caption": st["caption"], "first_frame": first, "last_frame": first - 1, "components": 0})
            continue
        fx, fy, fz = st.get("fly", [0, 0, 1])
        off = (float(fx) * float(dist), float(fy) * float(dist), float(fz) * float(dist))
        moved = 0
        movable = []
        for c in comps:
            try:
                if move(c, NXOpen.Vector3d(float(off[0]), float(off[1]), float(off[2]))):
                    moved += 1
                    movable.append(c)
                else:
                    log("no mover for %s (reveal only)" % c.DisplayName)
            except Exception as e:
                log("move failed for %s: %s" % (c.DisplayName, str(e)[:80]))
                movers.pop(c.Tag, None)
        session.DisplayManager.ShowObjects(comps, NXOpen.DisplayManager.LayerSetting.ChangeLayerToSelectable)
        prev = 0.0
        for i in range(1, fly_n + 1):
            e = ease(i / float(fly_n))
            d = e - prev
            prev = e
            if moved:
                for c in movable:
                    try:
                        move(c, NXOpen.Vector3d(float(-off[0] * d), float(-off[1] * d), float(-off[2] * d)))
                    except Exception:
                        pass
            frame()
        for _ in range(hold_n):
            frame()
        steps_out.append({"id": st["id"], "title": st["title"], "caption": st["caption"], "first_frame": first, "last_frame": frames[0], "components": len(comps)})
        log("step %d %s: %d components, frames %d-%d" % (st["id"], st["title"], len(comps), first, frames[0]))
    for _ in range(plan.get("outro_frames", 20)):
        frame()


if __name__ == "__main__":
    main()
