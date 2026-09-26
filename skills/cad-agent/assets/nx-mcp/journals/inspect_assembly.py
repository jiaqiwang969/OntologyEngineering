# -*- coding: utf-8 -*-
"""Read-only inspection of the CURRENT work part (run via nx_run_journal after nx_open_part).
Prints one JSON document: part attributes, component tree, unique parts with attributes, materials,
body counts, drawing sheets and (when the UF binding allows) bounding boxes. Never saves anything."""
import json
import NXOpen
import NXOpen.UF

session = NXOpen.Session.GetSession()
ufs = NXOpen.UF.UFSession.GetUFSession()
work = session.Parts.Work


def attrs(obj):
    out = {}
    try:
        for a in obj.GetUserAttributes():
            try:
                if a.Type == NXOpen.NXObject.AttributeType.String:
                    v = a.StringValue
                elif a.Type == NXOpen.NXObject.AttributeType.Integer:
                    v = a.IntegerValue
                elif a.Type == NXOpen.NXObject.AttributeType.Real:
                    v = a.RealValue
                elif a.Type == NXOpen.NXObject.AttributeType.Boolean:
                    v = a.BooleanValue
                else:
                    v = a.StringValue
                if v not in ("", None):
                    out[a.Title] = v
            except Exception:
                pass
    except Exception as e:
        out["_error"] = str(e)
    return out


def bbox(tag):
    fn = getattr(ufs.Modl, "AskBoundingBoxExact", None)
    if fn is None:
        return None
    try:
        mn, dirs, dist = fn(tag, NXOpen.Tag.Null)
        if len(dirs) == 9:
            dirs = [dirs[0:3], dirs[3:6], dirs[6:9]]
        pts = []
        for k in ((0, 0, 0), (1, 0, 0), (0, 1, 0), (0, 0, 1), (1, 1, 0), (1, 0, 1), (0, 1, 1), (1, 1, 1)):
            pts.append([mn[i] + sum(k[j] * dist[j] * dirs[j][i] for j in range(3)) for i in range(3)])
        xs, ys, zs = zip(*pts)
        return [min(xs), min(ys), min(zs), max(xs), max(ys), max(zs)]
    except Exception as e:
        return {"_error": str(e)}


def part_info(part):
    info = {"path": part.FullPath, "units": str(part.PartUnits), "attrs": attrs(part),
            "solid_bodies": 0, "sheet_bodies": 0, "facet_bodies": 0, "sheets": [], "materials": []}
    bb = None
    try:
        for b in part.Bodies:
            if b.IsSolidBody:
                info["solid_bodies"] += 1
                box = bbox(b.Tag)
                if isinstance(box, list):
                    bb = box if bb is None else [min(bb[0], box[0]), min(bb[1], box[1]), min(bb[2], box[2]),
                                                 max(bb[3], box[3]), max(bb[4], box[4]), max(bb[5], box[5])]
            elif b.IsSheetBody:
                info["sheet_bodies"] += 1
    except Exception as e:
        info["_bodies_error"] = str(e)
    try:
        info["facet_bodies"] = len(list(part.FacetedBodies))
    except Exception:
        pass
    if bb:
        info["bbox"] = bb
        info["size"] = [bb[3] - bb[0], bb[4] - bb[1], bb[5] - bb[2]]
    try:
        info["sheets"] = [s.Name for s in part.DrawingSheets]
    except Exception:
        pass
    try:
        info["materials"] = [m.Name for m in part.MaterialManager.PhysicalMaterials.GetUsedMaterials()]
    except Exception:
        pass
    return info


report = {"work_part": work.Leaf, "work_part_attrs": attrs(work), "components": [], "parts": {}}
parts = report["parts"]


def walk(component, level):
    for child in component.GetChildren():
        rec = {"level": level, "name": child.Name, "display_name": child.DisplayName}
        part = None
        try:
            proto = child.Prototype
            part = proto if isinstance(proto, NXOpen.BasePart) else proto.OwningPart
            rec["part"] = part.Leaf
        except Exception as e:
            rec["part_error"] = str(e)
        for key, fn in (("suppressed", lambda: child.IsSuppressed), ("reference_set", lambda: child.ReferenceSet)):
            try:
                rec[key] = fn()
            except Exception:
                pass
        try:
            origin, _m = child.GetPosition()
            rec["origin"] = [round(origin.X, 3), round(origin.Y, 3), round(origin.Z, 3)]
        except Exception:
            pass
        report["components"].append(rec)
        if part is not None:
            if part.Leaf not in parts:
                parts[part.Leaf] = part_info(part)
                parts[part.Leaf]["instances"] = 0
            parts[part.Leaf]["instances"] += 1
        walk(child, level + 1)


root = work.ComponentAssembly.RootComponent
if root is not None:
    walk(root, 1)
report["component_count"] = len(report["components"])
print(json.dumps(report, ensure_ascii=False, indent=1, default=str))
