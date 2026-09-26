# -*- coding: utf-8 -*-
"""Play inside the interactive NX. Renders the ORIGINAL 4色组印刷机 (650) and the pilot COPY (850) from identical
cameras (top / right / isometric / deck close-up), colouring stretched parts red and moved parts blue in both.
Nothing is saved: colours are undone, CloseAll uses CloseModified. Leaves the copy displayed at the end."""
import json, os, time, traceback
import NXOpen
import NXOpen.UF
try:
    import NXOpen.Gateway
except Exception:
    pass
WORKROOT = os.path.join(os.path.expanduser("~"), "work")
BASE = os.environ.get("NX_PRINT_PROJECT_ROOT", os.path.join(WORKROOT, "163-caojun-print"))
ORIG = BASE + r"\real\4色组\4色组印刷机.prt"
MOD = BASE + r"\mod\4seW650\4色组印刷机.prt"
OUT = BASE + r"\real\out\cmp"
PLAN = json.load(open(os.path.join(WORKROOT, "journals", "pilot_plan_sise.json"), encoding="utf-8"))
RED = {w["part"] for w in PLAN["width_driven"]}
BLUE = {p["part"] for p in PLAN["position_driven"]}
DECK_CENTRE = (-2800.0, 0.0, 1200.0)
session = NXOpen.Session.GetSession()
ufs = NXOpen.UF.UFSession.GetUFSession()
os.makedirs(OUT, exist_ok=True)
T0 = time.time()


def log(m):
    with open(os.path.join(OUT, "cmp.log"), "a", encoding="utf-8") as f:
        f.write("[%6.1fs] %s\n" % (time.time() - T0, m))


def leaf_of(c):
    proto = c.Prototype
    return proto.Leaf if isinstance(proto, NXOpen.BasePart) else proto.OwningPart.Leaf


def export(part, view, path):
    view.UpdateDisplay()
    try:
        b = part.Views.CreateImageExportBuilder()
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
            b.Commit()
        finally:
            b.Destroy()
        return "builder"
    except Exception as e:
        log("builder failed %s; UF fallback" % str(e)[:80])
        fmt = getattr(getattr(NXOpen.UF.Disp, "ImageFormat", NXOpen.UF.Disp), "Png", None)
        bg = getattr(getattr(NXOpen.UF.Disp, "BackgroundColor", NXOpen.UF.Disp), "White", None)
        ufs.Disp.CreateImage(path, fmt, bg)
        return "uf"


def colour(disp):
    root = disp.ComponentAssembly.RootComponent
    red, blue = [], []

    def walk(c):
        for ch in c.GetChildren():
            try:
                lf = leaf_of(ch)
                if lf in RED:
                    red.append(ch)
                elif lf in BLUE:
                    blue.append(ch)
            except Exception:
                pass
            walk(ch)
    walk(root)
    for objs, col in ((red, 186), (blue, 211)):
        if not objs:
            continue
        dm = session.DisplayManager.NewDisplayModification()
        try:
            dm.ApplyToAllFaces = True
            dm.NewColor = col
            dm.Apply(objs)
        finally:
            dm.Dispose()
    log("coloured red %d blue %d" % (len(red), len(blue)))


def cam_get(view):
    m = view.Matrix
    o = None
    for name in ("Origin",):
        try:
            o = getattr(view, name)
        except Exception:
            pass
    if o is None:
        try:
            o = view.GetOrigin()
        except Exception:
            o = NXOpen.Point3d(0.0, 0.0, 0.0)
    return (m, o, view.Scale)


def cam_set(view, cam):
    m, o, s = cam
    view.SetRotationTranslationScale(m, o, s)


def render_set(disp, tag, cams=None):
    view = disp.ModelingViews.WorkView
    try:
        view.RenderingStyle = NXOpen.View.RenderingStyleType.ShadedWithEdges
    except Exception:
        pass
    try:
        disp.WCS.Visibility = False
    except Exception:
        pass
    if cams is None:
        log("view api: %s" % [n for n in dir(view) if "Scale" in n or "Origin" in n or "Rotation" in n or "Matrix" in n])
    out = {}
    for name, canned in (("top", NXOpen.View.Canned.Top), ("right", NXOpen.View.Canned.Right), ("iso", NXOpen.View.Canned.Isometric)):
        if cams is None:
            view.Orient(canned, NXOpen.View.ScaleAdjustment.Fit)
            out[name] = cam_get(view)
        else:
            cam_set(view, cams[name])
        export(disp, view, os.path.join(OUT, "%s_%s.png" % (tag, name)))
        log("%s %s exported" % (tag, name))
    # deck close-up (top view, 4x)
    if cams is None:
        view.Orient(NXOpen.View.Canned.Top, NXOpen.View.ScaleAdjustment.Fit)
        m, o, s = cam_get(view)
        cam = (m, NXOpen.Point3d(*DECK_CENTRE), s * 4.0)
        try:
            cam_set(view, cam)
            out["deck"] = cam
        except Exception as e:
            log("deck cam failed: %s" % str(e)[:100])
            out["deck"] = (m, o, s)
    else:
        cam_set(view, cams["deck"])
    export(disp, view, os.path.join(OUT, "%s_deck.png" % tag))
    # deck close-up from the right (side) view too
    if cams is None:
        view.Orient(NXOpen.View.Canned.Right, NXOpen.View.ScaleAdjustment.Fit)
        m, o, s = cam_get(view)
        cam = (m, NXOpen.Point3d(*DECK_CENTRE), s * 2.5)
        try:
            cam_set(view, cam)
            out["deckright"] = cam
        except Exception:
            out["deckright"] = (m, o, s)
    else:
        cam_set(view, cams["deckright"])
    export(disp, view, os.path.join(OUT, "%s_deckright.png" % tag))
    return out


def open_asm(path):
    lo = session.Parts.LoadOptions
    lo.ComponentsToLoad = NXOpen.LoadOptions.LoadComponents.All
    lo.UsePartialLoading = False
    lo.AbortOnFailure = False
    p, st = session.Parts.OpenBaseDisplay(path)
    try:
        st.Dispose()
    except Exception:
        pass
    return session.Parts.Display


def main():
    for f in os.listdir(OUT):
        try:
            os.remove(os.path.join(OUT, f))
        except Exception:
            pass
    log("start")
    session.Parts.CloseAll(NXOpen.BasePart.CloseModified.CloseModified, None)
    disp = open_asm(ORIG)
    log("opened original %s" % disp.FullPath)
    mark = session.SetUndoMark(NXOpen.Session.MarkVisibility.Invisible, "cmp")
    colour(disp)
    cams = render_set(disp, "orig")
    session.UndoToMark(mark, None)
    session.Parts.CloseAll(NXOpen.BasePart.CloseModified.CloseModified, None)
    disp = open_asm(MOD)
    log("opened copy %s" % disp.FullPath)
    mark2 = session.SetUndoMark(NXOpen.Session.MarkVisibility.Invisible, "cmp2")
    colour(disp)
    render_set(disp, "mod", cams)
    session.UndoToMark(mark2, None)
    try:
        v = disp.ModelingViews.WorkView
        v.Orient(NXOpen.View.Canned.Isometric, NXOpen.View.ScaleAdjustment.Fit)
        v.UpdateDisplay()
    except Exception:
        pass
    with open(os.path.join(OUT, "DONE.txt"), "w") as f:
        f.write("ok %.0fs\n" % (time.time() - T0))
    log("done")


if __name__ == "__main__":
    try:
        main()
    except Exception:
        log("FATAL " + traceback.format_exc())
        with open(os.path.join(OUT, "DONE.txt"), "w") as f:
            f.write("FATAL\n")
