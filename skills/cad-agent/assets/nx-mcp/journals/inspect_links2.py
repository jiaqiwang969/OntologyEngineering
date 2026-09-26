# -*- coding: utf-8 -*-
"""Read-only follow-up: resolve the SOURCE of every WAVE link (linked face / mirror body / linked datum)
via WaveLinkBuilder collectors and UF_WAVE calls, plus the full expression list of layout parts.
Config via env INSPECT_CFG -> {"asm": ..., "out": ..., "log": ..., "extra_parts": [...]}. Never saves
(builders are created only to read selections and are destroyed without Commit)."""
import json, os, time, traceback
import NXOpen
import NXOpen.UF
import NXOpen.Features

CFG_PATH = os.environ.get("INSPECT_CFG")
cfg = json.load(open(CFG_PATH, encoding="utf-8"))
ASM, OUT, LOG = cfg["asm"], cfg["out"], cfg.get("log", cfg["out"] + ".log")
EXTRA = set(cfg.get("extra_parts", []))
T0 = time.time()


def log(msg):
    with open(LOG, "a", encoding="utf-8") as f:
        f.write("[%7.1fs] %s\n" % (time.time() - T0, msg))


session = NXOpen.Session.GetSession()
ufs = NXOpen.UF.UFSession.GetUFSession()


def _get(tag):
    for path in ("TaggedObjectManager", "Utilities"):
        mgr = getattr(NXOpen, path, None)
        if mgr is None:
            continue
        if path == "Utilities":
            mgr = getattr(mgr, "NXObjectManager", None)
            if mgr is None:
                continue
        try:
            return mgr.Get(tag)
        except Exception:
            continue
    return None


def describe(obj):
    if obj is None:
        return None
    d = {"cls": type(obj).__name__}
    try:
        if isinstance(obj, NXOpen.BasePart):
            d["part"] = obj.Leaf
            return d
    except Exception:
        pass
    for attr in ("OwningComponent",):
        try:
            oc = getattr(obj, attr)
            if oc is not None:
                d["component"] = oc.DisplayName
                try:
                    d["component_part"] = oc.Prototype.OwningPart.Leaf if not isinstance(oc.Prototype, NXOpen.BasePart) else oc.Prototype.Leaf
                except Exception:
                    pass
                try:
                    d["component_path"] = " / ".join(reversed([c.DisplayName for c in _parents(oc)]))
                except Exception:
                    pass
        except Exception:
            pass
    try:
        op = obj.OwningPart
        if op is not None:
            d["owning_part"] = op.Leaf
    except Exception:
        pass
    try:
        if obj.Name:
            d["name"] = obj.Name
    except Exception:
        pass
    try:
        if isinstance(obj, NXOpen.Features.Feature):
            d["feature"] = obj.GetFeatureName()
    except Exception:
        pass
    try:
        if isinstance(obj, NXOpen.DatumPlane):
            o, n = obj.Origin, obj.Normal
            d["plane"] = [round(o.X, 2), round(o.Y, 2), round(o.Z, 2), round(n.X, 3), round(n.Y, 3), round(n.Z, 3)]
    except Exception:
        pass
    try:
        if isinstance(obj, NXOpen.Face):
            d["face_type"] = str(obj.SolidFaceType)
            b = obj.GetBody()
            d["body_component"] = b.OwningComponent.DisplayName if b.OwningComponent is not None else None
    except Exception:
        pass
    try:
        if isinstance(obj, NXOpen.Body):
            d["body_component"] = obj.OwningComponent.DisplayName if obj.OwningComponent is not None else None
    except Exception:
        pass
    return d


def _parents(comp):
    out = []
    c = comp
    while c is not None:
        out.append(c)
        try:
            c = c.Parent
        except Exception:
            break
    return out


def raw(res):
    """JSON-safe rendering of a UF result, resolving tags to object descriptions."""
    if isinstance(res, (list, tuple)):
        return [raw(x) for x in res]
    if isinstance(res, int) and res > 1000:
        return describe(_get(res)) or ("tag %d" % res)
    if isinstance(res, (int, float, bool, str)) or res is None:
        return res
    return str(res)[:120]


def uf_probe(tag):
    out = {}
    for name, args in (("AskLinkSource", (tag, True)), ("AskLinkSource", (tag, False)), ("AskLinkedFeatureInfo", (tag,)),
                       ("AskLinkMirrorData", (tag,)), ("AskLinkXform", (tag,)), ("AskLinkUpdateTime", (tag,)), ("AskLinkedFeatureMap", (tag,))):
        fn = getattr(ufs.Wave, name, None)
        if fn is None:
            continue
        key = name + ("" if len(args) == 1 else str(args[1]))
        try:
            out[key] = raw(fn(*args))
        except Exception as e:
            out[key] = "ERR " + str(e)[:100]
    return out


def collector_objects(b, attr):
    try:
        c = getattr(b, attr)
    except Exception as e:
        return None
    if c is None:
        return None
    try:
        objs = c.GetObjects()
        return [describe(o) for o in objs][:12]
    except Exception:
        pass
    try:
        objs = c.GetSelectedObjects()
        return [describe(o) for o in objs][:12]
    except Exception:
        pass
    return describe(c)


def builder_probe(part, feat):
    out = {}
    wlb = None
    try:
        wlb = part.BaseFeatures.CreateWaveLinkBuilder(feat)
        try:
            out["type"] = str(wlb.Type)
        except Exception:
            pass
        for sub in ("MirrorBodyBuilder", "ExtractFaceBuilder", "WaveDatumBuilder", "CompositeCurveBuilder", "WavePointBuilder", "WaveSketchBuilder", "WaveRoutingBuilder"):
            try:
                sb = getattr(wlb, sub)
            except Exception:
                continue
            if sb is None:
                continue
            rec = {}
            for attr in ("FaceChain", "ObjectToExtract", "ExtractBodyCollector", "BodyToExtract", "MirrorBodyCollector", "Bodies", "Plane", "MirrorPlane",
                         "DatumCollector", "Datum", "Section", "Associative", "FixAtCurrentTimestamp", "InheritDisplayProperties", "MakePositionIndependent", "Type"):
                try:
                    v = getattr(sb, attr)
                except Exception:
                    continue
                if v is None:
                    continue
                if isinstance(v, (bool, int, float, str)):
                    rec[attr] = v
                else:
                    r = collector_objects(sb, attr)
                    if r is not None:
                        rec[attr] = r
                    else:
                        rec[attr] = describe(v) or str(v)[:80]
            if rec:
                out[sub] = rec
    except Exception as e:
        out["builder_error"] = str(e)[:120]
    finally:
        try:
            if wlb is not None:
                wlb.Destroy()
        except Exception:
            pass
    return out


def expressions(part):
    res = []
    try:
        for e in part.Expressions:
            rec = {"name": e.Name}
            try:
                rec["rhs"] = e.RightHandSide[:80]
            except Exception:
                pass
            try:
                rec["value"] = round(e.Value, 4)
            except Exception:
                pass
            try:
                rec["units"] = e.Units.Name if e.Units is not None else ""
            except Exception:
                pass
            try:
                rec["desc"] = e.Description
            except Exception:
                pass
            res.append(rec)
    except Exception as ex:
        res.append({"_error": str(ex)[:80]})
    return res


def main():
    open(LOG, "w", encoding="utf-8").close()
    log("start " + ASM)
    report = {"assembly": ASM, "parts": {}, "errors": []}
    try:
        lo = session.Parts.LoadOptions
        lo.ComponentsToLoad = NXOpen.LoadOptions.LoadComponents.All
        lo.UsePartialLoading = False
        lo.AbortOnFailure = False
    except Exception as e:
        report["errors"].append("load options: %s" % e)
    base, status = session.Parts.OpenBaseDisplay(ASM)
    try:
        status.Dispose()
    except Exception:
        pass
    asm = session.Parts.Work
    log("opened " + asm.Leaf)
    seen = set()

    def handle(part):
        if part.Leaf in seen:
            return
        seen.add(part.Leaf)
        links = []
        try:
            for f in part.Features:
                try:
                    ft = f.FeatureType
                except Exception:
                    continue
                if "LINK" in ft or "EXTRACT" in ft or "MIRROR" in ft:
                    rec = {"type": ft, "name": f.GetFeatureName()}
                    try:
                        rec["user_name"] = f.Name
                    except Exception:
                        pass
                    rec["builder"] = builder_probe(part, f)
                    rec["uf"] = uf_probe(f.Tag)
                    try:
                        rec["status"] = [str(s) for s in f.GetFeatureInformationalStatus()][:4]
                    except Exception:
                        pass
                    links.append(rec)
        except Exception as e:
            report["errors"].append("%s features: %s" % (part.Leaf, e))
        if links or part.Leaf in EXTRA or part is asm:
            report["parts"][part.Leaf] = {"links": links}
            if part.Leaf in EXTRA or part is asm:
                report["parts"][part.Leaf]["expressions"] = expressions(part)
            log("recorded %s (%d links)" % (part.Leaf, len(links)))

    handle(asm)

    def walk(component):
        try:
            children = component.GetChildren()
        except Exception:
            return
        for child in children:
            try:
                proto = child.Prototype
                part = proto if isinstance(proto, NXOpen.BasePart) else proto.OwningPart
                handle(part)
            except Exception as e:
                report["errors"].append("prototype: %s" % e)
            walk(child)

    root = asm.ComponentAssembly.RootComponent
    if root is not None:
        walk(root)
    with open(OUT, "w", encoding="utf-8") as f:
        json.dump(report, f, ensure_ascii=False, indent=1, default=str)
    log("written " + OUT)
    try:
        session.Parts.CloseAll(NXOpen.BasePart.CloseModified.CloseModified, None)
    except Exception as e:
        log("close: %s" % e)
    log("done")


if __name__ == "__main__":
    try:
        main()
    except Exception:
        tb = traceback.format_exc()
        log("FATAL " + tb)
        try:
            with open(OUT + ".FATAL.txt", "w", encoding="utf-8") as f:
                f.write(tb)
        except Exception:
            pass
        print(tb)
