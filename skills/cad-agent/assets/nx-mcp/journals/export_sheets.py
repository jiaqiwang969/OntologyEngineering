# -*- coding: utf-8 -*-
"""Batch, read-only: open parts of the pilot copy that carry their own drawing sheets, report whether the drawing views
are out of date, update them (in session only, never saved) and export each sheet to PDF. cfg: {"parts": [...], "out_dir", "out", "log"}"""
import json, os, time, traceback
import NXOpen
import NXOpen.Drawings
cfg = json.load(open(os.environ["INSPECT_CFG"], encoding="utf-8"))
session = NXOpen.Session.GetSession()
T0 = time.time(); rep = {"parts": []}
def log(m):
    with open(cfg["log"], "a", encoding="utf-8") as f: f.write("[%6.1fs] %s\n" % (time.time() - T0, m))
os.makedirs(cfg["out_dir"], exist_ok=True)
for path in cfg["parts"]:
    rec = {"part": path, "sheets": []}
    try:
        part, st = session.Parts.OpenBaseDisplay(path)
        try: st.Dispose()
        except Exception: pass
        part = session.Parts.Display
        views = list(part.DraftingViews)
        rec["n_views"] = len(views)
        try:
            rec["out_of_date_before"] = sum(1 for v in views if v.IsOutOfDate)
        except Exception as e:
            rec["out_of_date_before"] = "n/a: %s" % str(e)[:60]
        try:
            part.DraftingViews.UpdateViews(views)
            rec["updated"] = True
        except Exception as e:
            rec["updated"] = "failed: %s" % str(e)[:80]
        try:
            rec["out_of_date_after"] = sum(1 for v in views if v.IsOutOfDate)
        except Exception:
            pass
        for sheet in part.DrawingSheets:
            srec = {"sheet": sheet.Name}
            try:
                sheet.Open()
            except Exception as e:
                srec["open"] = str(e)[:60]
            pdf = os.path.join(cfg["out_dir"], "%s_%s.pdf" % (part.Leaf.replace(" ", "_"), sheet.Name))
            try:
                b = part.PlotManager.CreatePrintPdfbuilder()
                try:
                    b.Scale = 1.0
                    b.Size = NXOpen.PrintPDFBuilder.SizeOption.ScaleFactor
                    b.Colors = NXOpen.PrintPDFBuilder.Color.AsDisplayed
                    b.Filename = pdf
                    b.Append = False
                    try:
                        b.SourceBuilder.SetSheets([sheet])
                    except Exception as e2:
                        srec["setsheets"] = str(e2)[:60]
                    b.Commit()
                    srec["pdf"] = pdf; srec["bytes"] = os.path.getsize(pdf) if os.path.exists(pdf) else 0
                finally:
                    b.Destroy()
            except Exception as e:
                srec["pdf_error"] = str(e)[:120]
            rec["sheets"].append(srec)
        session.Parts.CloseAll(NXOpen.BasePart.CloseModified.CloseModified, None)
        log("%s views=%s ood=%s->%s sheets=%d" % (part.Leaf, rec.get("n_views"), rec.get("out_of_date_before"), rec.get("out_of_date_after"), len(rec["sheets"])))
    except Exception:
        rec["error"] = traceback.format_exc()[-300:]; log("ERROR " + rec["error"])
        try: session.Parts.CloseAll(NXOpen.BasePart.CloseModified.CloseModified, None)
        except Exception: pass
    rep["parts"].append(rec)
with open(cfg["out"], "w", encoding="utf-8") as f: json.dump(rep, f, ensure_ascii=False, indent=1)
