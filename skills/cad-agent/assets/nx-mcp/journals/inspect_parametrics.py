# -*- coding: utf-8 -*-
"""Standalone NX Open journal (run_journal.exe): how parametric is each part of an assembly?
For every unique part: feature types, expression counts (system p-numbers vs named / inter-part),
assembly constraints. Read-only: never saves, closes everything with CloseModified at the end.
Config via env INSPECT_CFG -> {"asm": "<prt>", "out": "<json>", "log": "<log>"}"""
import json, os, re, time, traceback, collections
import NXOpen

CFG_PATH = os.environ.get("INSPECT_CFG")
cfg = json.load(open(CFG_PATH, encoding="utf-8"))
ASM, OUT, LOG = cfg["asm"], cfg["out"], cfg.get("log", cfg["out"] + ".log")
T0 = time.time()
SYS_EXPR = re.compile(r"^p\d+$")


def log(msg):
    with open(LOG, "a", encoding="utf-8") as f:
        f.write("[%7.1fs] %s\n" % (time.time() - T0, msg))


session = NXOpen.Session.GetSession()


def expr_info(part):
    info = {"n_expressions": 0, "n_named": 0, "n_interpart": 0, "n_measure": 0, "named": []}
    try:
        for e in part.Expressions:
            info["n_expressions"] += 1
            try:
                name = e.Name
            except Exception:
                name = "?"
            rhs = ""
            try:
                rhs = e.RightHandSide
            except Exception:
                pass
            if "::" in rhs:
                info["n_interpart"] += 1
            try:
                if e.IsMeasurementExpression:
                    info["n_measure"] += 1
            except Exception:
                pass
            if not SYS_EXPR.match(name):
                info["n_named"] += 1
                if len(info["named"]) < 40:
                    info["named"].append("%s = %s" % (name, rhs[:60]))
    except Exception as ex:
        info["_error"] = str(ex)
    return info


def feat_info(part):
    info = {"n_features": 0, "feature_types": {}}
    c = collections.Counter()
    try:
        for f in part.Features:
            info["n_features"] += 1
            try:
                c[f.FeatureType] += 1
            except Exception:
                c["?"] += 1
    except Exception as ex:
        info["_error"] = str(ex)
    info["feature_types"] = dict(c.most_common())
    return info


def constraint_info(part):
    info = {"n_constraints": 0, "constraint_types": {}}
    try:
        pos = part.ComponentAssembly.Positioner
        c = collections.Counter()
        for k in pos.Constraints:
            info["n_constraints"] += 1
            try:
                c[str(k.ConstraintType).split(".")[-1]] += 1
            except Exception:
                c["?"] += 1
        info["constraint_types"] = dict(c.most_common())
    except Exception as ex:
        info["_error"] = str(ex)
    return info


def part_info(part):
    info = {"leaf": part.Leaf, "is_subassembly": False, "child_count": 0, "solid_bodies": 0, "facet_bodies": 0}
    try:
        rc = part.ComponentAssembly.RootComponent
        if rc is not None:
            n = len(rc.GetChildren())
            info["is_subassembly"] = n > 0
            info["child_count"] = n
    except Exception:
        pass
    try:
        info["solid_bodies"] = sum(1 for b in part.Bodies if b.IsSolidBody)
    except Exception:
        pass
    try:
        info["facet_bodies"] = sum(1 for _ in part.FacetedBodies)
    except Exception:
        pass
    info.update(feat_info(part))
    info["expr"] = expr_info(part)
    if info["is_subassembly"]:
        info["constraints"] = constraint_info(part)
    return info


def main():
    open(LOG, "w", encoding="utf-8").close()
    log("start " + ASM)
    report = {"assembly": ASM, "timings_s": {}, "parts": {}, "errors": [], "component_count": 0}
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
        report["unloaded"] = status.NumberUnloadedParts
        status.Dispose()
    except Exception:
        pass
    asm = session.Parts.Work
    log("opened %s in %.1fs" % (asm.Leaf, report["timings_s"]["open"]))
    parts = report["parts"]
    parts[asm.Leaf] = part_info(asm)
    parts[asm.Leaf]["instances"] = 1

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
