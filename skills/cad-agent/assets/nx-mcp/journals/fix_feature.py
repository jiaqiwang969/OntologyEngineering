# -*- coding: utf-8 -*-
"""Delete named features in ONE part of the pilot copy (mod folder only), update, save. cfg: {"asm": part path, "delete": ["W_plus_end"], "out", "log"}"""
import json, os, traceback
import NXOpen
cfg = json.load(open(os.environ["INSPECT_CFG"], encoding="utf-8"))
assert "\\mod\\" in cfg["asm"].lower()
session = NXOpen.Session.GetSession()
out = {"part": cfg["asm"], "deleted": [], "features": []}
try:
    part, st = session.Parts.OpenBaseDisplay(cfg["asm"])
    try: st.Dispose()
    except Exception: pass
    part = session.Parts.Display
    mark = session.SetUndoMark(NXOpen.Session.MarkVisibility.Invisible, "fix")
    for f in part.Features:
        try:
            out["features"].append([f.GetFeatureName(), f.Name])
            if f.Name in cfg["delete"] or f.GetFeatureName() in cfg["delete"]:
                session.UpdateManager.AddToDeleteList(f)
                out["deleted"].append(f.GetFeatureName())
        except Exception as e:
            out["features"].append(["?", str(e)[:60]])
    session.UpdateManager.DoUpdate(mark)
    xs, ys, zs = [], [], []
    for b in part.Bodies:
        if not b.IsSolidBody:
            continue
        for f in b.GetFaces():
            for e in f.GetEdges():
                try:
                    v1, v2 = e.GetVertices()
                except Exception:
                    continue
                for pt in (v1, v2):
                    xs.append(pt.X); ys.append(pt.Y); zs.append(pt.Z)
    if xs:
        out["size"] = [round(max(xs) - min(xs), 2), round(max(ys) - min(ys), 2), round(max(zs) - min(zs), 2)]
    if cfg["delete"]:
        part.Save(NXOpen.BasePart.SaveComponents.FalseValue, NXOpen.BasePart.CloseAfterSave.FalseValue)
        out["saved"] = True
except Exception:
    out["fatal"] = traceback.format_exc()[-600:]
with open(cfg["out"], "w", encoding="utf-8") as fh:
    json.dump(out, fh, ensure_ascii=False, indent=1)
try: session.Parts.CloseAll(NXOpen.BasePart.CloseModified.CloseModified, None)
except Exception: pass
