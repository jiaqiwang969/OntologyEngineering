# -*- coding: utf-8 -*-
"""Standalone NX Open journal (run_journal.exe): open an assembly read-only, dump a full report to JSON.

Config: the current Windows user's work\\journals\\inspect_config.json
  {"asm": "<absolute .prt path>", "out": "<absolute .json path>", "log": "<absolute .log path>",
   "volume": true, "volume_max_bytes": 5000000, "occ_bbox": true}
Never saves anything. Closes all parts at the end without saving.
"""
import json, os, sys, time, traceback
import NXOpen
import NXOpen.UF

CFG_PATH = os.environ.get("INSPECT_CFG") or os.path.join(os.path.expanduser("~"), "work", "journals", "inspect_config.json")
cfg = json.load(open(CFG_PATH, encoding="utf-8"))
ASM, OUT, LOG = cfg["asm"], cfg["out"], cfg.get("log", cfg["out"] + ".log")
DO_VOLUME = cfg.get("volume", True)
VOL_MAX = cfg.get("volume_max_bytes", 5_000_000)
DO_OCC = cfg.get("occ_bbox", True)
T0 = time.time()


def log(msg):
    with open(LOG, "a", encoding="utf-8") as f:
        f.write("[%7.1fs] %s\n" % (time.time() - T0, msg))


session = NXOpen.Session.GetSession()
ufs = NXOpen.UF.UFSession.GetUFSession()


def attrs(obj):
    out = {}
    try:
        for a in obj.GetUserAttributes():
            try:
                t = a.Type
                if t == NXOpen.NXObject.AttributeType.String:
                    v = a.StringValue
                elif t == NXOpen.NXObject.AttributeType.Integer:
                    v = a.IntegerValue
                elif t == NXOpen.NXObject.AttributeType.Real:
                    v = a.RealValue
                elif t == NXOpen.NXObject.AttributeType.Boolean:
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


def bbox_edges(body):
    """Axis-aligned box from edge vertices (exact for prismatic parts, approximate on curved faces)."""
    try:
        xs, ys, zs = [], [], []
        for e in body.GetEdges():
            try:
                p1, p2 = e.GetVertices()
                for pt in (p1, p2):
                    xs.append(pt.X); ys.append(pt.Y); zs.append(pt.Z)
            except Exception:
                continue
        if not xs:
            return None
        return [min(xs), min(ys), min(zs), max(xs), max(ys), max(zs)]
    except Exception:
        return None


def bbox(tag, body=None):
    fn = getattr(ufs.Modl, "AskBoundingBoxExact", None)
    if fn is None:
        return bbox_edges(body) if body is not None else None
    try:
        mn, dirs, dist = fn(tag, NXOpen.Tag.Null)
        if len(dirs) == 9:
            dirs = [dirs[0:3], dirs[3:6], dirs[6:9]]
        pts = []
        for k in ((0, 0, 0), (1, 0, 0), (0, 1, 0), (0, 0, 1), (1, 1, 0), (1, 0, 1), (0, 1, 1), (1, 1, 1)):
            pts.append([mn[i] + sum(k[j] * dist[j] * dirs[j][i] for j in range(3)) for i in range(3)])
        xs, ys, zs = zip(*pts)
        return [min(xs), min(ys), min(zs), max(xs), max(ys), max(zs)]
    except Exception:
        return bbox_edges(body) if body is not None else None


def merge(a, b):
    if b is None:
        return a
    if a is None:
        return list(b)
    return [min(a[0], b[0]), min(a[1], b[1]), min(a[2], b[2]), max(a[3], b[3]), max(a[4], b[4]), max(a[5], b[5])]


_unit_sets = [["SquareMilliMeter", "CubicMilliMeter", "Kilogram", "MilliMeter", "MilliMeter", "Newton"],
              ["SquareMilliMeter", "CubicMilliMeter", "Kilogram", "MilliMeter", "KilogramMilliMeterSquared", "Newton"]]


def volume(part, bodies):
    for names in _unit_sets:
        try:
            uc = part.UnitCollection
            units = [uc.FindObject(n) for n in names]
            mb = part.MeasureManager.NewMassProperties(units, 0.99, bodies)
            v = mb.Volume
            try:
                mb.Dispose()
            except Exception:
                pass
            return v
        except Exception:
            continue
    return None


def part_info(part):
    info = {"leaf": part.Leaf, "path": part.FullPath, "units": str(part.PartUnits), "attrs": attrs(part),
            "solid_bodies": 0, "sheet_bodies": 0, "facet_bodies": 0, "sheets": [], "materials": [],
            "file_bytes": None, "is_subassembly": False, "child_count": 0}
    try:
        info["file_bytes"] = os.path.getsize(part.FullPath)
    except Exception:
        pass
    solids = []
    bb = None
    try:
        for b in part.Bodies:
            if b.IsSolidBody:
                info["solid_bodies"] += 1
                solids.append(b)
                bb = merge(bb, bbox(b.Tag, b))
            elif b.IsSheetBody:
                info["sheet_bodies"] += 1
    except Exception as e:
        info["_bodies_error"] = str(e)
    try:
        for fb in part.FacetedBodies:
            info["facet_bodies"] += 1
            bb = merge(bb, bbox(fb.Tag, None))
    except Exception:
        pass
    if bb:
        info["bbox"] = [round(x, 3) for x in bb]
        info["size"] = [round(bb[3] - bb[0], 3), round(bb[4] - bb[1], 3), round(bb[5] - bb[2], 3)]
    if DO_VOLUME and solids and (info["file_bytes"] or 0) <= VOL_MAX and len(solids) <= 20:
        v = volume(part, solids)
        if v is not None:
            info["volume_mm3"] = round(v, 1)
    try:
        info["sheets"] = [s.Name for s in part.DrawingSheets]
    except Exception:
        pass
    try:
        info["materials"] = [m.Name for m in part.MaterialManager.PhysicalMaterials.GetUsedMaterials()]
    except Exception:
        pass
    try:
        rc = part.ComponentAssembly.RootComponent
        if rc is not None:
            n = len(rc.GetChildren())
            info["is_subassembly"] = n > 0
            info["child_count"] = n
    except Exception:
        pass
    return info


def parts_lists(part):
    res = []
    try:
        raw = ufs.Plist.AskTags()
    except Exception as e:
        return {"_error": str(e)}
    tags = []
    if isinstance(raw, (list, tuple)):
        for x in raw:
            if isinstance(x, (list, tuple)):
                tags.extend(int(v) for v in x)
            elif isinstance(x, int) and x > 1000:
                tags.append(x)
    for pl in tags:
        table = []
        try:
            nrows = ufs.Tabnot.AskNmRows(pl)
            ncols = ufs.Tabnot.AskNmColumns(pl)
            for r in range(nrows):
                row = ufs.Tabnot.AskNthRow(pl, r)
                cells = []
                for c in range(ncols):
                    col = ufs.Tabnot.AskNthColumn(pl, c)
                    cell = ufs.Tabnot.AskCellAtRowCol(row, col)
                    txt = ""
                    for fn in ("AskEvaluatedCellText", "AskCellText"):
                        f = getattr(ufs.Tabnot, fn, None)
                        if f is None:
                            continue
                        try:
                            txt = f(cell)
                            break
                        except Exception:
                            continue
                    cells.append(txt)
                table.append(cells)
        except Exception as e:
            table.append(["_error", str(e)])
        res.append(table)
    return res


def main():
    open(LOG, "w", encoding="utf-8").close()
    log("start " + ASM)
    report = {"assembly": ASM, "timings_s": {}, "load_status": [], "components": [], "parts": {}, "errors": []}
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
        for i in range(status.NumberUnloadedParts):
            report["load_status"].append({"part": status.GetPartName(i), "status": status.GetStatusDescription(i)})
        status.Dispose()
    except Exception as e:
        report["errors"].append("load status: %s" % e)
    asm = session.Parts.Work
    log("opened %s in %.1fs, unloaded=%d" % (asm.Leaf, report["timings_s"]["open"], len(report["load_status"])))
    report["assembly_leaf"] = asm.Leaf
    report["assembly_attrs"] = attrs(asm)
    report["assembly_units"] = str(asm.PartUnits)
    try:
        report["assembly_sheets"] = [s.Name for s in asm.DrawingSheets]
    except Exception:
        report["assembly_sheets"] = []

    comps = report["components"]
    parts = report["parts"]
    leaf_records = []

    def walk(component, level, parent_idx):
        try:
            children = component.GetChildren()
        except Exception as e:
            report["errors"].append("GetChildren: %s" % e)
            return
        for child in children:
            idx = len(comps)
            rec = {"idx": idx, "parent": parent_idx, "level": level, "name": child.Name,
                   "display_name": child.DisplayName, "children": 0}
            part = None
            try:
                proto = child.Prototype
                part = proto if isinstance(proto, NXOpen.BasePart) else proto.OwningPart
                rec["part"] = part.Leaf
            except Exception as e:
                rec["part_error"] = str(e)
            try:
                rec["suppressed"] = child.IsSuppressed
            except Exception:
                pass
            try:
                rec["reference_set"] = child.ReferenceSet
            except Exception:
                pass
            try:
                origin, _m = child.GetPosition()
                rec["origin"] = [round(origin.X, 3), round(origin.Y, 3), round(origin.Z, 3)]
            except Exception:
                pass
            comps.append(rec)
            if parent_idx is not None:
                comps[parent_idx]["children"] += 1
            if part is not None:
                if part.Leaf not in parts:
                    parts[part.Leaf] = part_info(part)
                    parts[part.Leaf]["instances"] = 0
                    if len(parts) % 50 == 0:
                        log("parts inspected: %d" % len(parts))
                parts[part.Leaf]["instances"] += 1
                if not parts[part.Leaf]["is_subassembly"]:
                    leaf_records.append((child, part, rec))
            walk(child, level + 1, idx)

    t = time.time()
    root = asm.ComponentAssembly.RootComponent
    if root is not None:
        walk(root, 1, None)
    report["timings_s"]["walk"] = round(time.time() - t, 1)
    report["component_count"] = len(comps)
    report["unique_part_count"] = len(parts)
    log("walk done: %d components, %d unique parts" % (len(comps), len(parts)))

    if DO_OCC:
        t = time.time()
        abb = None
        done = 0
        for child, part, rec in leaf_records:
            cb = None
            pb = parts.get(part.Leaf, {}).get("bbox")
            if pb:
                try:
                    o, m = child.GetPosition()
                    R = [[m.Xx, m.Xy, m.Xz], [m.Yx, m.Yy, m.Yz], [m.Zx, m.Zy, m.Zz]]
                    for k in ((0, 0, 0), (1, 0, 0), (0, 1, 0), (0, 0, 1), (1, 1, 0), (1, 0, 1), (0, 1, 1), (1, 1, 1)):
                        lx = pb[0] + k[0] * (pb[3] - pb[0]); ly = pb[1] + k[1] * (pb[4] - pb[1]); lz = pb[2] + k[2] * (pb[5] - pb[2])
                        # NX Matrix3x3 rows are the component axes expressed in the parent frame: p = o + lx*X + ly*Y + lz*Z
                        wx = o.X + lx * R[0][0] + ly * R[1][0] + lz * R[2][0]
                        wy = o.Y + lx * R[0][1] + ly * R[1][1] + lz * R[2][1]
                        wz = o.Z + lx * R[0][2] + ly * R[1][2] + lz * R[2][2]
                        cb = merge(cb, [wx, wy, wz, wx, wy, wz])
                except Exception as e:
                    rec["bbox_asm_error"] = str(e)
            if cb:
                rec["bbox_asm"] = [round(x, 2) for x in cb]
                abb = merge(abb, cb)
            done += 1
            if done % 500 == 0:
                log("envelope: %d/%d" % (done, len(leaf_records)))
        report["assembly_bbox"] = [round(x, 2) for x in abb] if abb else None
        if abb:
            report["assembly_size"] = [round(abb[3] - abb[0], 1), round(abb[4] - abb[1], 1), round(abb[5] - abb[2], 1)]
        report["timings_s"]["occ_bbox"] = round(time.time() - t, 1)
        log("assembly bbox done")

    try:
        report["parts_lists"] = parts_lists(asm)
    except Exception as e:
        report["errors"].append("parts_lists: %s" % e)
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
