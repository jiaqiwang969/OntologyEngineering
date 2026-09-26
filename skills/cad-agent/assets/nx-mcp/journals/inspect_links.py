# -*- coding: utf-8 -*-
"""Standalone NX Open journal (run_journal.exe), read-only: decompose an assembly and record every
parameter-driving mechanism it contains — WAVE links (source part / status), features that are driven
by expressions, sketches, datum planes/CSYS, reference sets, arrangements, component patterns.
Config via env INSPECT_CFG -> {"asm": "<prt>", "out": "<json>", "log": "<log>"}. Never saves."""
import json, os, re, time, traceback, collections
import NXOpen
import NXOpen.UF
import NXOpen.Features

CFG_PATH = os.environ.get("INSPECT_CFG")
cfg = json.load(open(CFG_PATH, encoding="utf-8"))
ASM, OUT, LOG = cfg["asm"], cfg["out"], cfg.get("log", cfg["out"] + ".log")
T0 = time.time()
SYS = re.compile(r"^p\d+(_[xyz])?('\d+)?$")
SRC_FUNS = ["AskLinkSource", "AskLinkSourceGeom", "AskLinkSourceGeometry", "AskLinkSourceObject", "AskLinkedGeometry",
            "AskLinkGeometry", "AskLinkSourcePart", "AskLinkPartOcc", "AskSourcePart"]


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


def objname(tag):
    try:
        obj = _get(tag)
    except Exception as e:
        return "tag %s (%s)" % (tag, str(e)[:40])
    if obj is None:
        return "tag %s" % tag
    out = type(obj).__name__
    try:
        if isinstance(obj, NXOpen.BasePart):
            return "part:" + obj.Leaf
    except Exception:
        pass
    for attr in ("OwningComponent", "OwningPart"):
        try:
            o = getattr(obj, attr)
            if o is not None:
                out += "@" + (o.DisplayName if attr == "OwningComponent" else o.Leaf)
                break
        except Exception:
            continue
    try:
        n = obj.Name
        if n:
            out += ":" + n
    except Exception:
        pass
    try:
        if isinstance(obj, NXOpen.Features.Feature):
            out += ":" + obj.GetFeatureName()
    except Exception:
        pass
    return out


def unpack_tags(res):
    tags = []
    if isinstance(res, (list, tuple)):
        for x in res:
            if isinstance(x, (list, tuple)):
                tags.extend(unpack_tags(x))
            elif isinstance(x, int) and x > 1000:
                tags.append(x)
    elif isinstance(res, int) and res > 1000:
        tags.append(res)
    return tags


def link_info(feat):
    info = {}
    tag = feat.Tag
    try:
        st = ufs.Wave.AskLinkStatus(tag)
        info["status"] = [bool(x) for x in st] if isinstance(st, (list, tuple)) else str(st)
    except Exception as e:
        info["status_error"] = str(e)[:80]
    for fn in SRC_FUNS:
        f = getattr(ufs.Wave, fn, None)
        if f is None:
            continue
        try:
            res = f(tag)
            info["source_fn"] = fn
            info["source"] = [objname(t) for t in unpack_tags(res)][:4] or str(res)[:120]
            break
        except Exception as e:
            info.setdefault("source_errors", []).append("%s: %s" % (fn, str(e)[:60]))
    try:
        info["parents"] = [p.GetFeatureName() for p in feat.GetParents()][:6]
    except Exception:
        pass
    return info


def expr_str(e):
    try:
        name = e.Name
    except Exception:
        name = "?"
    rhs = ""
    try:
        rhs = e.RightHandSide
    except Exception:
        pass
    val = None
    try:
        val = round(e.Value, 4)
    except Exception:
        pass
    units = ""
    try:
        units = e.Units.Name if e.Units is not None else ""
    except Exception:
        pass
    return {"name": name, "rhs": rhs[:80], "value": val, "units": units}


def part_info(part):
    info = {"leaf": part.Leaf, "child_count": 0, "features": [], "brep_features": 0, "links": [], "sketches": [],
            "datums": [], "driving_expressions": [], "formula_expressions": [], "n_expressions": 0, "n_sys_expressions": 0,
            "reference_sets": [], "arrangements": [], "component_patterns": 0, "constraints": 0}
    try:
        rc = part.ComponentAssembly.RootComponent
        if rc is not None:
            info["child_count"] = len(rc.GetChildren())
    except Exception:
        pass
    used_by_feature = collections.defaultdict(list)
    try:
        n = 0
        for f in part.Features:
            n += 1
            if n > 400:
                info["features_truncated"] = True
                break
            try:
                ft = f.FeatureType
            except Exception:
                ft = "?"
            if ft == "BREP":
                info["brep_features"] += 1
                continue
            rec = {"type": ft, "cls": type(f).__name__}
            try:
                rec["name"] = f.GetFeatureName()
            except Exception:
                pass
            try:
                if f.Name:
                    rec["user_name"] = f.Name
            except Exception:
                pass
            try:
                rec["suppressed"] = f.Suppressed
            except Exception:
                pass
            try:
                ex = [e.Name for e in f.GetExpressions()]
                if ex:
                    rec["expressions"] = ex[:20]
                    for nm in ex:
                        used_by_feature[nm].append(rec.get("name", ft))
            except Exception:
                pass
            if "LINK" in ft or "EXTRACT" in ft or "MIRROR" in ft:
                rec["link"] = link_info(f)
                info["links"].append(rec)
            if ft in ("DATUM_PLANE", "DATUM_AXIS", "DATUM_CSYS", "EXTRACT_DATUM_PLANE"):
                try:
                    for ent in f.GetEntities():
                        if isinstance(ent, NXOpen.DatumPlane):
                            o, nrm = ent.Origin, ent.Normal
                            rec["plane"] = {"origin": [round(o.X, 2), round(o.Y, 2), round(o.Z, 2)], "normal": [round(nrm.X, 3), round(nrm.Y, 3), round(nrm.Z, 3)]}
                            break
                except Exception:
                    pass
                info["datums"].append(rec)
            info["features"].append(rec)
    except Exception as e:
        info["features_error"] = str(e)[:100]
    try:
        for sk in part.Sketches:
            rec = {"name": sk.Name}
            try:
                rec["geometry"] = len(sk.GetAllGeometry())
            except Exception:
                pass
            try:
                rec["internal"] = sk.IsInternal
            except Exception:
                pass
            info["sketches"].append(rec)
    except Exception as e:
        info["sketches_error"] = str(e)[:80]
    try:
        for e in part.Expressions:
            info["n_expressions"] += 1
            es = expr_str(e)
            nm = es["name"]
            if SYS.match(nm):
                info["n_sys_expressions"] += 1
            rhs = es["rhs"].strip()
            is_formula = bool(rhs) and not re.match(r"^-?\d+(\.\d+)?$", rhs) and not nm.startswith("Sheet_Metal") and not nm.startswith("SM_")
            if is_formula:
                info["formula_expressions"].append(es)
            if nm in used_by_feature and len(info["driving_expressions"]) < 60:
                es["features"] = used_by_feature[nm][:4]
                info["driving_expressions"].append(es)
    except Exception as e:
        info["expressions_error"] = str(e)[:80]
    try:
        info["reference_sets"] = [r.Name for r in part.GetAllReferenceSets()]
    except Exception:
        pass
    try:
        info["arrangements"] = [a.Name for a in part.ComponentAssembly.Arrangements]
    except Exception:
        pass
    try:
        info["component_patterns"] = sum(1 for _ in part.ComponentAssembly.ComponentPatterns)
    except Exception:
        pass
    try:
        info["constraints"] = sum(1 for _ in part.ComponentAssembly.Positioner.Constraints)
    except Exception:
        pass
    try:
        info["is_family_template"] = bool(part.IsFamilyTemplate)
    except Exception:
        pass
    try:
        info["is_family_member"] = bool(part.IsFamilyMember)
    except Exception:
        pass
    return info


def main():
    open(LOG, "w", encoding="utf-8").close()
    log("start " + ASM)
    report = {"assembly": ASM, "timings_s": {}, "parts": {}, "errors": [], "component_count": 0,
              "wave_api": [n for n in dir(ufs.Wave) if not n.startswith("_")]}
    try:
        lo = session.Parts.LoadOptions
        lo.ComponentsToLoad = NXOpen.LoadOptions.LoadComponents.All
        lo.UsePartialLoading = False
        lo.AbortOnFailure = False
    except Exception as e:
        report["errors"].append("load options: %s" % e)
    t = time.time()
    base, status = session.Parts.OpenBaseDisplay(ASM)
    report["timings_s"]["open"] = round(time.time() - t, 1)
    try:
        status.Dispose()
    except Exception:
        pass
    asm = session.Parts.Work
    log("opened %s" % asm.Leaf)
    parts = report["parts"]
    parts[asm.Leaf] = part_info(asm)
    parts[asm.Leaf]["instances"] = 1
    # interpart expression references at session level
    try:
        ipe = []
        for e in asm.Expressions:
            try:
                if "::" in e.RightHandSide:
                    ipe.append(e.Name + " = " + e.RightHandSide[:80])
            except Exception:
                pass
        report["assembly_interpart_expressions"] = ipe[:40]
    except Exception:
        pass

    def walk(component):
        try:
            children = component.GetChildren()
        except Exception as e:
            report["errors"].append("GetChildren: %s" % e)
            return
        for child in children:
            report["component_count"] += 1
            part = None
            try:
                proto = child.Prototype
                part = proto if isinstance(proto, NXOpen.BasePart) else proto.OwningPart
            except Exception as e:
                report["errors"].append("prototype: %s" % e)
            if part is not None:
                if part.Leaf not in parts:
                    parts[part.Leaf] = part_info(part)
                    parts[part.Leaf]["instances"] = 0
                    if len(parts) % 50 == 0:
                        log("parts inspected: %d" % len(parts))
                parts[part.Leaf]["instances"] += 1
            walk(child)

    t = time.time()
    root = asm.ComponentAssembly.RootComponent
    if root is not None:
        walk(root)
    report["timings_s"]["walk"] = round(time.time() - t, 1)
    report["unique_parts"] = len(parts)
    report["timings_s"]["total"] = round(time.time() - T0, 1)
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
