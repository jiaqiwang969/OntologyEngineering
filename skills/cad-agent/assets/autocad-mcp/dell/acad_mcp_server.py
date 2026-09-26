# -*- coding: utf-8 -*-
"""autocad-mcp: FastMCP stdio server that drives the running AutoCAD 2024 through COM.

It reuses CAD-MCP (daobataotie/CAD-MCP, MIT) for the drawing primitives and adds the
inspection / file / view / capture tools an engineering-drawing workflow needs.
Runs in the interactive Windows session (started by relay_server.py) so the AutoCAD
window is visible on the desktop.
"""
from __future__ import annotations

import ctypes
import io
import json
import logging
import math
import os
import re
import sys
import time
from typing import Any, Optional

BASE = os.path.dirname(os.path.abspath(__file__))
CAD_MCP_SRC = os.path.join(BASE, "CAD-MCP", "src")
sys.path.insert(0, CAD_MCP_SRC)
sys.dont_write_bytecode = True

LOG_DIR = os.path.join(BASE, "logs")
os.makedirs(LOG_DIR, exist_ok=True)
logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s %(name)s %(levelname)s %(message)s",
    handlers=[logging.FileHandler(os.path.join(LOG_DIR, "acad_mcp_server.log"), encoding="utf-8"),
              logging.StreamHandler(sys.stderr)],
)
log = logging.getLogger("autocad-mcp")

import pythoncom  # noqa: E402
import win32com.client  # noqa: E402
import win32con  # noqa: E402
import win32gui  # noqa: E402
import win32ui  # noqa: E402
from mcp.server.fastmcp import FastMCP  # noqa: E402
from mcp.server.fastmcp.utilities.types import Image  # noqa: E402

from cad_controller import CADController  # noqa: E402  (CAD-MCP)

WORKSPACE = os.environ.get("CADMCP_WORKSPACE", os.path.join(os.path.expanduser("~"), "work"))
PROGID = os.environ.get("CADMCP_PROGID", "AutoCAD.Application")

mcp = FastMCP(
    "autocad-mcp",
    instructions=(
        "Drives the AutoCAD 2024 instance running on dell-nb's desktop via COM. Start with acad_status. "
        "Coordinates are drawing units (mm for these drawings). Handles are AutoCAD entity handles "
        "(hex strings) and stay valid for the life of the drawing. Never overwrite a user's drawing: "
        "use save_drawing_as to a new file under " + WORKSPACE + "."
    ),
)

ctl = CADController()
_last_ok = 0.0

RPC_E_CALL_REJECTED = -2147418111
RPC_E_SERVERCALL_RETRYLATER = -2147417846


# --------------------------------------------------------------------------- COM helpers
def V(*xyz: float):
    """3-D point as a COM VARIANT array of doubles."""
    p = list(xyz) + [0.0] * (3 - len(xyz))
    return win32com.client.VARIANT(pythoncom.VT_ARRAY | pythoncom.VT_R8, [float(v) for v in p[:3]])


def VF(values):
    return win32com.client.VARIANT(pythoncom.VT_ARRAY | pythoncom.VT_R8, [float(v) for v in values])


def r(v, nd=4):
    if isinstance(v, float):
        return round(v, nd)
    if isinstance(v, (tuple, list)):
        return [r(x, nd) for x in v]
    return v


def with_retry(fn, tries=6, delay=0.5):
    """Retry COM calls while AutoCAD is busy (RPC_E_CALL_REJECTED / RETRYLATER)."""
    last = None
    for i in range(tries):
        try:
            return fn()
        except pythoncom.com_error as e:  # type: ignore[attr-defined]
            last = e
            if e.hresult in (RPC_E_CALL_REJECTED, RPC_E_SERVERCALL_RETRYLATER):
                time.sleep(delay * (i + 1))
                continue
            raise
    raise last  # type: ignore[misc]


def connect(force: bool = False):
    """Ensure ctl.app / ctl.doc point at the live AutoCAD instance and its active document."""
    global _last_ok
    if force or ctl.app is None:
        if not ctl.start_cad():
            raise RuntimeError("AutoCAD is not reachable (start_cad failed). Is AutoCAD running on the desktop?")
    try:
        ver = with_retry(lambda: ctl.app.Version)
    except Exception as e:  # stale COM proxy (AutoCAD restarted) -> reconnect once
        log.warning("COM proxy stale (%s); reconnecting", e)
        ctl.app = None
        ctl.doc = None
        if not ctl.start_cad():
            raise RuntimeError("AutoCAD is not reachable after reconnect attempt") from e
        ver = with_retry(lambda: ctl.app.Version)
    ctl.doc = _active_document(ctl.app)
    _last_ok = time.time()
    return ver


def _wait_ready(d, seconds: float = 8.0):
    """A document returned by Documents.Add/Open may not answer for a moment; wait until .Name works."""
    deadline = time.time() + seconds
    while True:
        try:
            d.Name
            return d
        except Exception:
            if time.time() > deadline:
                raise
            time.sleep(0.3)


def _active_document(a, seconds: float = 8.0):
    """Active document, tolerating the moments right after Close/Add when AutoCAD has none selected."""
    deadline = time.time() + seconds
    while True:
        try:
            if with_retry(lambda: a.Documents.Count) == 0:
                return _wait_ready(with_retry(lambda: a.Documents.Add("acadiso.dwt")))
            d = with_retry(lambda: a.ActiveDocument)
            d.Name
            return d
        except Exception:
            if time.time() > deadline:
                # last resort: activate the first open document
                d = a.Documents.Item(0)
                _safe(lambda: d.Activate())
                return _wait_ready(d, 3)
            time.sleep(0.4)


def app():
    connect()
    return ctl.app


def doc():
    connect()
    return ctl.doc


def err(e: Exception, hint: str | None = None) -> dict:
    msg = str(e)
    if isinstance(e, pythoncom.com_error):  # type: ignore[attr-defined]
        try:
            msg = f"COM error {e.hresult}: {e.excepinfo[2] if e.excepinfo else e.strerror}"
        except Exception:
            pass
        if e.hresult in (RPC_E_CALL_REJECTED, RPC_E_SERVERCALL_RETRYLATER):
            hint = hint or "AutoCAD is busy: a command or dialog is open on the desktop. Finish/cancel it (Esc) and retry."
    out = {"error": msg}
    if hint:
        out["hint"] = hint
    log.error("tool error: %s", msg)
    return out


def _space(space: str = "model"):
    d = doc()
    s = (space or "model").lower()
    if s == "model":
        return d.ModelSpace
    if s == "paper":
        return d.PaperSpace
    return d.Layouts.Item(space).Block


def _bbox(e):
    """[min, max] corners. pywin32's lazy type-info binding returns the [out] params as a tuple;
    fall back to explicit by-ref VARIANTs, then to a geometric estimate."""
    try:
        res = e.GetBoundingBox()
        if isinstance(res, (tuple, list)) and len(res) == 2 and res[0] is not None:
            return [r(list(res[0])), r(list(res[1]))]
    except Exception:
        pass
    try:
        mn = win32com.client.VARIANT(pythoncom.VT_BYREF | pythoncom.VT_VARIANT, None)
        mx = win32com.client.VARIANT(pythoncom.VT_BYREF | pythoncom.VT_VARIANT, None)
        res = e.GetBoundingBox(mn, mx)
        if mn.value is not None:
            return [r(list(mn.value)), r(list(mx.value))]
        if isinstance(res, (tuple, list)) and len(res) == 2 and res[0] is not None:
            return [r(list(res[0])), r(list(res[1]))]
    except Exception:
        pass
    pts = []
    for getter in (lambda: [list(e.StartPoint), list(e.EndPoint)],
                   lambda: [[e.Center[0] - e.Radius, e.Center[1] - e.Radius, 0], [e.Center[0] + e.Radius, e.Center[1] + e.Radius, 0]],
                   lambda: [list(e.InsertionPoint)],
                   lambda: [list(e.Coordinates)[i:i + 2] + [0] for i in range(0, len(list(e.Coordinates)), 2)]):
        try:
            pts = getter()
            if pts:
                break
        except Exception:
            continue
    if not pts:
        raise RuntimeError("bounding box unavailable for this entity")
    xs = [p[0] for p in pts]
    ys = [p[1] for p in pts]
    return [r([min(xs), min(ys), 0.0]), r([max(xs), max(ys), 0.0])]


_ACI_NAMES = {256: "ByLayer", 0: "ByBlock"}


_MTEXT_CODES = re.compile(r"\\[ACcFfHhKkLlOoQqSTtUuWw][^;]*;|\\[A-Za-z]|[{}]")


def strip_mtext(s: str) -> str:
    s = s.replace("\\P", "\n").replace("\\~", " ")
    s = _MTEXT_CODES.sub("", s)
    return s.replace("%%d", "°").replace("%%c", "Ø").replace("%%p", "±").replace("%%D", "°").replace("%%C", "Ø").replace("%%P", "±")


def _safe(fn, default=None):
    try:
        return fn()
    except Exception:
        return default


def summarize(e, detail: bool = False) -> dict:
    """Compact description of one AutoCAD entity."""
    t = e.ObjectName
    d: dict[str, Any] = {"handle": e.Handle, "type": t.replace("AcDb", ""), "layer": e.Layer}
    color = _safe(lambda: e.color)
    if color is not None:
        d["color"] = _ACI_NAMES.get(color, color)
    try:
        if t == "AcDbLine":
            d.update(start=r(list(e.StartPoint)), end=r(list(e.EndPoint)), length=r(e.Length))
        elif t == "AcDbCircle":
            d.update(center=r(list(e.Center)), radius=r(e.Radius))
        elif t == "AcDbArc":
            d.update(center=r(list(e.Center)), radius=r(e.Radius),
                     start_angle_deg=r(math.degrees(e.StartAngle)), end_angle_deg=r(math.degrees(e.EndAngle)))
        elif t == "AcDbEllipse":
            d.update(center=r(list(e.Center)), major_radius=r(e.MajorRadius), minor_radius=r(e.MinorRadius))
        elif t in ("AcDbPolyline", "AcDb2dPolyline", "AcDb3dPolyline"):
            c = list(e.Coordinates)
            step = 2 if t == "AcDbPolyline" else 3
            pts = [c[i:i + step] for i in range(0, len(c), step)]
            d.update(closed=bool(e.Closed), n_vertices=len(pts), length=r(_safe(lambda: e.Length)))
            area = _safe(lambda: e.Area)
            if area:
                d["area"] = r(area)
            d["vertices"] = r(pts) if (detail or len(pts) <= 8) else r(pts[:8]) + ["..."]
        elif t == "AcDbText":
            d.update(text=e.TextString, position=r(list(e.InsertionPoint)), height=r(e.Height),
                     rotation_deg=r(math.degrees(e.Rotation)))
        elif t == "AcDbMText":
            d.update(text=strip_mtext(e.TextString), position=r(list(e.InsertionPoint)), height=r(e.Height),
                     width=r(e.Width))
            if detail:
                d["raw_text"] = e.TextString
        elif t == "AcDbMLeader":
            d.update(text=strip_mtext(_safe(lambda: e.TextString, "")))
        elif "Dimension" in t:
            d.update(measurement=r(_safe(lambda: e.Measurement)), text_override=_safe(lambda: e.TextOverride, ""),
                     dim_style=_safe(lambda: e.StyleName), text_position=r(_safe(lambda: list(e.TextPosition))))
            if detail:
                d["decimal_places"] = _safe(lambda: e.PrimaryUnitsPrecision)
        elif t == "AcDbBlockReference":
            d.update(name=_safe(lambda: e.EffectiveName) or e.Name, insertion=r(list(e.InsertionPoint)),
                     scale=r(_safe(lambda: e.XScaleFactor)), rotation_deg=r(math.degrees(e.Rotation)))
            if _safe(lambda: e.HasAttributes):
                d["attributes"] = {a.TagString: a.TextString for a in e.GetAttributes()}
        elif t == "AcDbAttributeDefinition":
            d.update(tag=e.TagString, prompt=_safe(lambda: e.PromptString), default=e.TextString)
        elif t == "AcDbHatch":
            d.update(pattern=_safe(lambda: e.PatternName), area=r(_safe(lambda: e.Area)))
        elif t == "AcDbSpline":
            d.update(n_control_points=_safe(lambda: e.NumberOfControlPoints))
        elif t == "AcDbPoint":
            d.update(coordinates=r(list(e.Coordinates)))
        if detail:
            d["bbox"] = _bbox(e)
            d.update(linetype=_safe(lambda: e.Linetype), lineweight=_safe(lambda: e.Lineweight),
                     object_id=_safe(lambda: e.ObjectID), visible=_safe(lambda: e.Visible))
    except Exception as ex:  # keep going even if one property misbehaves
        d["warning"] = f"partial: {ex}"
    return d


def _iter(space_obj):
    n = space_obj.Count
    for i in range(n):
        try:
            yield space_obj.Item(i)
        except Exception:
            continue


def _doc_info(d) -> dict:
    return {
        "name": d.Name, "path": _safe(lambda: d.FullName, ""), "saved": bool(_safe(lambda: d.Saved, True)),
        "read_only": bool(_safe(lambda: d.ReadOnly, False)), "active": _safe(lambda: d.Active, None),
    }


# --------------------------------------------------------------------------- status / documents
@mcp.tool()
def acad_status() -> dict:
    """Connection check: AutoCAD version, open documents, active document, current layer/space and extents."""
    try:
        ver = connect()
        a, d = ctl.app, ctl.doc
        ext_min = _safe(lambda: list(d.GetVariable("EXTMIN")))
        ext_max = _safe(lambda: list(d.GetVariable("EXTMAX")))
        return {
            "connected": True, "autocad_version": ver, "progid": PROGID,
            "documents": [_doc_info(a.Documents.Item(i)) for i in range(a.Documents.Count)],
            "active_document": _doc_info(d),
            "active_space": "model" if d.ActiveSpace == 1 else "paper",
            "active_layout": _safe(lambda: d.ActiveLayout.Name),
            "current_layer": _safe(lambda: d.ActiveLayer.Name),
            "insunits": _safe(lambda: d.GetVariable("INSUNITS")),
            "model_extents": {"min": r(ext_min), "max": r(ext_max)},
            "model_entity_count": _safe(lambda: d.ModelSpace.Count),
            "workspace": WORKSPACE,
        }
    except Exception as e:
        return err(e, "AutoCAD may not be running on the desktop, or is showing a startup/sign-in dialog.")


@mcp.tool()
def list_documents() -> dict:
    """List all drawings open in AutoCAD."""
    try:
        a = app()
        return {"documents": [_doc_info(a.Documents.Item(i)) for i in range(a.Documents.Count)]}
    except Exception as e:
        return err(e)


@mcp.tool()
def open_drawing(path: str, read_only: bool = False) -> dict:
    """Open an existing DWG/DXF (absolute Windows path) and make it the active document."""
    try:
        if not os.path.isfile(path):
            return {"error": f"file not found: {path}"}
        a = app()
        d = _wait_ready(with_retry(lambda: a.Documents.Open(path, read_only)))
        ctl.doc = d
        with_retry(lambda: a.ZoomExtents())
        return {"opened": _doc_info(d), "model_entity_count": d.ModelSpace.Count,
                "layouts": [d.Layouts.Item(i).Name for i in range(d.Layouts.Count)]}
    except Exception as e:
        return err(e, "Check the path (use C:\\... with backslashes) and that no modal dialog is open in AutoCAD.")


@mcp.tool()
def new_drawing(template: str = "acadiso.dwt") -> dict:
    """Create a new drawing from a template (default metric acadiso.dwt) and make it active."""
    try:
        a = app()
        d = _wait_ready(with_retry(lambda: a.Documents.Add(template)))
        ctl.doc = d
        return {"created": _doc_info(d)}
    except Exception as e:
        return err(e)


@mcp.tool()
def activate_document(name: str) -> dict:
    """Make the open drawing whose Name (e.g. 'Drawing1.dwg') or FullName matches the active one."""
    try:
        a = app()
        for i in range(a.Documents.Count):
            d = a.Documents.Item(i)
            if d.Name.lower() == name.lower() or _safe(lambda: d.FullName, "").lower() == name.lower():
                d.Activate()
                ctl.doc = d
                return {"active_document": _doc_info(d)}
        return {"error": f"no open document named {name}"}
    except Exception as e:
        return err(e)


@mcp.tool()
def save_drawing_as(path: str, overwrite: bool = False) -> dict:
    """Save the active drawing to a new DWG path (creates folders). Refuses to overwrite unless overwrite=true."""
    try:
        if os.path.exists(path) and not overwrite:
            return {"error": f"{path} exists; pass overwrite=true to replace it"}
        os.makedirs(os.path.dirname(path), exist_ok=True)
        d = doc()
        with_retry(lambda: d.SaveAs(path))
        return {"saved": _doc_info(d)}
    except Exception as e:
        return err(e)


@mcp.tool()
def close_drawing(save: bool = False) -> dict:
    """Close the active drawing (save=false discards changes). Another open drawing becomes active."""
    try:
        d = doc()
        name = d.Name
        with_retry(lambda: d.Close(save))
        ctl.doc = None
        time.sleep(0.5)
        a = ctl.app
        remaining = with_retry(lambda: a.Documents.Count)
        active = None
        if remaining:
            ctl.doc = _active_document(a)
            active = _doc_info(ctl.doc)
        return {"closed": name, "remaining_documents": remaining, "active_document": active}
    except Exception as e:
        return err(e)


# --------------------------------------------------------------------------- inspection
@mcp.tool()
def drawing_info() -> dict:
    """Overview of the active drawing: units, extents, layouts, layer/block counts, entity type histogram."""
    try:
        d = doc()
        hist: dict[str, int] = {}
        ms = d.ModelSpace
        for e in _iter(ms):
            k = e.ObjectName.replace("AcDb", "")
            hist[k] = hist.get(k, 0) + 1
        return {
            "document": _doc_info(d),
            "insunits": _safe(lambda: d.GetVariable("INSUNITS")), "lunits": _safe(lambda: d.GetVariable("LUNITS")),
            "measurement": _safe(lambda: d.GetVariable("MEASUREMENT")),
            "dimscale": _safe(lambda: d.GetVariable("DIMSCALE")), "ltscale": _safe(lambda: d.GetVariable("LTSCALE")),
            "extents": {"min": r(_safe(lambda: list(d.GetVariable("EXTMIN")))),
                        "max": r(_safe(lambda: list(d.GetVariable("EXTMAX"))))},
            "limits": {"min": r(_safe(lambda: list(d.GetVariable("LIMMIN")))),
                       "max": r(_safe(lambda: list(d.GetVariable("LIMMAX"))))},
            "layouts": [d.Layouts.Item(i).Name for i in range(d.Layouts.Count)],
            "layer_count": d.Layers.Count, "block_definition_count": d.Blocks.Count,
            "model_entity_count": ms.Count, "model_entity_types": hist,
            "current_layer": _safe(lambda: d.ActiveLayer.Name),
            "dim_styles": [d.DimStyles.Item(i).Name for i in range(min(d.DimStyles.Count, 50))],
            "text_styles": [d.TextStyles.Item(i).Name for i in range(min(d.TextStyles.Count, 50))],
        }
    except Exception as e:
        return err(e)


@mcp.tool()
def list_layers(with_counts: bool = False) -> dict:
    """List layers (state, colour, linetype). with_counts=true also counts model-space entities per layer."""
    try:
        d = doc()
        counts: dict[str, int] = {}
        if with_counts:
            for e in _iter(d.ModelSpace):
                counts[e.Layer] = counts.get(e.Layer, 0) + 1
        out = []
        for i in range(d.Layers.Count):
            L = d.Layers.Item(i)
            rec = {"name": L.Name, "on": bool(L.LayerOn), "frozen": bool(L.Freeze), "locked": bool(L.Lock),
                   "color": _safe(lambda: L.color), "linetype": _safe(lambda: L.Linetype),
                   "lineweight": _safe(lambda: L.Lineweight), "plottable": bool(_safe(lambda: L.Plottable, True))}
            if with_counts:
                rec["entities"] = counts.get(L.Name, 0)
            out.append(rec)
        return {"current_layer": _safe(lambda: d.ActiveLayer.Name), "layers": out}
    except Exception as e:
        return err(e)


@mcp.tool()
def list_entities(layer: Optional[str] = None, types: Optional[list[str]] = None, space: str = "model",
                  limit: int = 200, offset: int = 0, detail: bool = False,
                  window: Optional[list[float]] = None) -> dict:
    """List entities with a compact summary each.
    Filters: layer name; types = substrings of the AutoCAD class name (e.g. ["Line","Text","BlockReference","Dimension"]);
    space = "model" | "paper" | a layout name; window = [xmin, ymin, xmax, ymax] keeps entities whose bbox intersects it.
    Paginate with limit/offset. detail=true adds bbox, linetype, lineweight, full vertices."""
    try:
        sp = _space(space)
        total = sp.Count
        out = []
        matched = 0
        tl = [t.lower() for t in (types or [])]
        for e in _iter(sp):
            try:
                if layer and e.Layer.lower() != layer.lower():
                    continue
                if tl and not any(t in e.ObjectName.lower() for t in tl):
                    continue
                if window:
                    (x0, y0, _), (x1, y1, _) = _bbox(e)
                    if x1 < window[0] or x0 > window[2] or y1 < window[1] or y0 > window[3]:
                        continue
            except Exception:
                continue
            matched += 1
            if matched <= offset:
                continue
            if len(out) >= limit:
                continue
            out.append(summarize(e, detail))
        return {"space": space, "total_in_space": total, "matched": matched, "offset": offset,
                "returned": len(out), "entities": out}
    except Exception as e:
        return err(e)


@mcp.tool()
def get_entity(handle: str) -> dict:
    """Full property dump of one entity by handle (from list_entities/read_texts)."""
    try:
        e = doc().HandleToObject(handle)
        return summarize(e, detail=True)
    except Exception as e:
        return err(e, "Handle not found in the active drawing.")


@mcp.tool()
def read_texts(layer: Optional[str] = None, space: str = "model", include_dimensions: bool = True,
               include_attributes: bool = True, limit: int = 500) -> dict:
    """Extract all readable text: TEXT, MTEXT (formatting stripped), MLEADER text, dimension text and block
    attribute values, with positions. Use it to read title blocks, notes and BOM tables."""
    try:
        sp = _space(space)
        items = []
        for e in _iter(sp):
            if len(items) >= limit:
                break
            try:
                t = e.ObjectName
                if layer and e.Layer.lower() != layer.lower():
                    continue
                if t == "AcDbText":
                    items.append({"handle": e.Handle, "kind": "text", "text": e.TextString,
                                  "position": r(list(e.InsertionPoint)), "height": r(e.Height), "layer": e.Layer})
                elif t == "AcDbMText":
                    items.append({"handle": e.Handle, "kind": "mtext", "text": strip_mtext(e.TextString),
                                  "position": r(list(e.InsertionPoint)), "height": r(e.Height), "layer": e.Layer})
                elif t == "AcDbMLeader":
                    txt = strip_mtext(_safe(lambda: e.TextString, ""))
                    if txt:
                        items.append({"handle": e.Handle, "kind": "mleader", "text": txt, "layer": e.Layer})
                elif include_dimensions and "Dimension" in t:
                    ov = _safe(lambda: e.TextOverride, "")
                    m = _safe(lambda: e.Measurement)
                    items.append({"handle": e.Handle, "kind": t.replace("AcDb", ""), "measurement": r(m),
                                  "text_override": ov, "position": r(_safe(lambda: list(e.TextPosition))),
                                  "layer": e.Layer})
                elif include_attributes and t == "AcDbBlockReference" and _safe(lambda: e.HasAttributes):
                    attrs = {a.TagString: a.TextString for a in e.GetAttributes()}
                    items.append({"handle": e.Handle, "kind": "block_attributes",
                                  "block": _safe(lambda: e.EffectiveName) or e.Name,
                                  "position": r(list(e.InsertionPoint)), "attributes": attrs, "layer": e.Layer})
            except Exception:
                continue
        return {"space": space, "count": len(items), "items": items}
    except Exception as e:
        return err(e)


@mcp.tool()
def find_text(pattern: str, regex: bool = False, space: str = "model") -> dict:
    """Find entities whose text (TEXT/MTEXT/attributes/dimension override) contains pattern (case-insensitive)."""
    try:
        rx = re.compile(pattern if regex else re.escape(pattern), re.I)
        res = read_texts(space=space, limit=100000)
        hits = []
        for it in res.get("items", []):
            hay = " ".join(str(v) for v in [it.get("text", ""), it.get("text_override", ""),
                                           " ".join(f"{k}={v}" for k, v in it.get("attributes", {}).items())])
            if rx.search(hay):
                hits.append(it)
        return {"pattern": pattern, "count": len(hits), "items": hits}
    except Exception as e:
        return err(e)


@mcp.tool()
def list_blocks(include_references: bool = True) -> dict:
    """Block definitions (name, entity count, xref/dynamic flags) and, optionally, every block reference in
    model space with insertion point and attribute values."""
    try:
        d = doc()
        defs = []
        for i in range(d.Blocks.Count):
            b = d.Blocks.Item(i)
            name = b.Name
            if name.startswith("*"):
                continue
            defs.append({"name": name, "entities": _safe(lambda: b.Count), "is_xref": bool(_safe(lambda: b.IsXRef, False)),
                         "is_dynamic": bool(_safe(lambda: b.IsDynamicBlock, False)), "is_layout": bool(_safe(lambda: b.IsLayout, False))})
        refs = []
        if include_references:
            for e in _iter(d.ModelSpace):
                if e.ObjectName == "AcDbBlockReference":
                    refs.append(summarize(e))
        return {"definitions": defs, "references": refs}
    except Exception as e:
        return err(e)


@mcp.tool()
def list_dimensions(space: str = "model") -> dict:
    """All dimensions with type, measurement, override text, style and text position."""
    try:
        sp = _space(space)
        out = [summarize(e) for e in _iter(sp) if "Dimension" in e.ObjectName]
        return {"count": len(out), "dimensions": out}
    except Exception as e:
        return err(e)


# --------------------------------------------------------------------------- view / capture
@mcp.tool()
def zoom(mode: str = "extents", window: Optional[list[float]] = None, center: Optional[list[float]] = None,
         magnification: float = 1.0, handle: Optional[str] = None) -> dict:
    """Change the view: mode = "extents" | "window" (window=[x1,y1,x2,y2]) | "center" (center=[x,y], magnification)
    | "entity" (handle, zooms to its bbox with margin) | "previous". Also regenerates."""
    try:
        a, d = app(), doc()
        if mode == "extents":
            with_retry(lambda: a.ZoomExtents())
        elif mode == "window" and window:
            with_retry(lambda: a.ZoomWindow(V(window[0], window[1]), V(window[2], window[3])))
        elif mode == "center" and center:
            with_retry(lambda: a.ZoomCenter(V(center[0], center[1]), float(magnification)))
        elif mode == "entity" and handle:
            (x0, y0, _), (x1, y1, _) = _bbox(d.HandleToObject(handle))
            mx = max((x1 - x0), (y1 - y0), 1.0) * 0.25
            with_retry(lambda: a.ZoomWindow(V(x0 - mx, y0 - mx), V(x1 + mx, y1 + mx)))
        elif mode == "previous":
            with_retry(lambda: a.ZoomPrevious())
        else:
            return {"error": f"unsupported zoom mode/arguments: {mode}"}
        _safe(lambda: d.Regen(1))
        return {"ok": True, "mode": mode}
    except Exception as e:
        return err(e)


def _capture_hwnd(hwnd: int, max_width: int) -> bytes:
    from PIL import Image as PILImage

    if win32gui.IsIconic(hwnd):
        win32gui.ShowWindow(hwnd, win32con.SW_RESTORE)
        time.sleep(0.5)
    left, top, right, bottom = win32gui.GetWindowRect(hwnd)
    w, h = max(right - left, 1), max(bottom - top, 1)
    hwnd_dc = win32gui.GetWindowDC(hwnd)
    mfc_dc = win32ui.CreateDCFromHandle(hwnd_dc)
    save_dc = mfc_dc.CreateCompatibleDC()
    bmp = win32ui.CreateBitmap()
    bmp.CreateCompatibleBitmap(mfc_dc, w, h)
    save_dc.SelectObject(bmp)
    try:
        ok = ctypes.windll.user32.PrintWindow(hwnd, save_dc.GetSafeHdc(), 2)  # PW_RENDERFULLCONTENT
        info = bmp.GetInfo()
        raw = bmp.GetBitmapBits(True)
        im = PILImage.frombuffer("RGB", (info["bmWidth"], info["bmHeight"]), raw, "raw", "BGRX", 0, 1)
        ext = im.convert("L").getextrema()
        if not ok or ext[1] < 8:  # PrintWindow gave nothing usable -> copy from the screen instead
            scr = win32gui.GetDC(0)
            scr_dc = win32ui.CreateDCFromHandle(scr)
            save_dc.BitBlt((0, 0), (w, h), scr_dc, (left, top), win32con.SRCCOPY)
            raw = bmp.GetBitmapBits(True)
            im = PILImage.frombuffer("RGB", (info["bmWidth"], info["bmHeight"]), raw, "raw", "BGRX", 0, 1)
            scr_dc.DeleteDC()
            win32gui.ReleaseDC(0, scr)
    finally:
        win32gui.DeleteObject(bmp.GetHandle())
        save_dc.DeleteDC()
        mfc_dc.DeleteDC()
        win32gui.ReleaseDC(hwnd, hwnd_dc)
    if max_width and im.width > max_width:
        im = im.resize((max_width, int(im.height * max_width / im.width)), PILImage.LANCZOS)
    buf = io.BytesIO()
    im.save(buf, format="PNG", optimize=True)
    return buf.getvalue()


@mcp.tool()
def capture_view(zoom_extents: bool = False, max_width: int = 1400, save_path: Optional[str] = None) -> Image:
    """Screenshot of the AutoCAD window as it is on the desktop (PNG, downscaled to max_width). Use it to SEE the
    drawing. zoom_extents=true first fits the whole drawing. Optionally also saves the PNG to save_path."""
    a, d = app(), doc()
    if zoom_extents:
        with_retry(lambda: a.ZoomExtents())
    _safe(lambda: d.Regen(1))
    time.sleep(0.4)
    hwnd = int(with_retry(lambda: a.HWND))
    png = _capture_hwnd(hwnd, max_width)
    if save_path:
        os.makedirs(os.path.dirname(save_path), exist_ok=True)
        with open(save_path, "wb") as f:
            f.write(png)
    return Image(data=png, format="png")


@mcp.tool()
def regen() -> dict:
    """Regenerate all viewports of the active drawing."""
    try:
        doc().Regen(1)
        return {"ok": True}
    except Exception as e:
        return err(e)


# --------------------------------------------------------------------------- export
@mcp.tool()
def export_pdf(path: str, layout: Optional[str] = None, paper: Optional[str] = None,
               plot_area: str = "extents", landscape: Optional[bool] = None) -> dict:
    """Plot a layout (default: active layout / model space) to PDF with 'DWG To PDF.pc3', fit to paper, centred.
    paper = canonical media name (e.g. 'ISO_A3_(420.00_x_297.00_MM)'); the reply lists available names on failure."""
    try:
        d = doc()
        os.makedirs(os.path.dirname(path), exist_ok=True)
        lay = d.Layouts.Item(layout) if layout else d.ActiveLayout
        _safe(lambda: d.SetVariable("BACKGROUNDPLOT", 0))
        lay.ConfigName = "DWG To PDF.pc3"
        lay.RefreshPlotDeviceInfo()
        if paper:
            lay.CanonicalMediaName = paper
        lay.PlotType = {"extents": 1, "display": 0, "limits": 2, "layout": 5}.get(plot_area, 1)
        lay.UseStandardScale = True
        lay.StandardScale = 0  # scale to fit
        lay.CenterPlot = True
        if landscape is None:
            mn = _safe(lambda: list(d.GetVariable("EXTMIN")), [0, 0, 0])
            mx = _safe(lambda: list(d.GetVariable("EXTMAX")), [1, 0, 0])
            landscape = (mx[0] - mn[0]) >= (mx[1] - mn[1])
        lay.PlotRotation = 1 if landscape else 0
        d.Plot.QuietErrorMode = True
        ok = with_retry(lambda: d.Plot.PlotToFile(path, "DWG To PDF.pc3"))
        time.sleep(0.5)
        return {"ok": bool(ok), "path": path, "exists": os.path.exists(path),
                "size_bytes": os.path.getsize(path) if os.path.exists(path) else 0,
                "layout": lay.Name, "paper": _safe(lambda: lay.CanonicalMediaName)}
    except Exception as e:
        names = _safe(lambda: list(doc().ActiveLayout.GetCanonicalMediaNames()), [])
        out = err(e)
        out["available_paper_sizes"] = [n for n in names if "A3" in n or "A4" in n or "A2" in n][:30]
        return out


@mcp.tool()
def export_dxf(path: str) -> dict:
    """Export the whole active drawing to DXF at path (keeps the DWG as the active document)."""
    try:
        d = doc()
        os.makedirs(os.path.dirname(path), exist_ok=True)
        base = path[:-4] if path.lower().endswith(".dxf") else path
        ss = None
        for name in ("MCP_EXPORT", "MCP_EXPORT2"):
            try:
                ss = d.SelectionSets.Add(name)
                break
            except Exception:
                try:
                    d.SelectionSets.Item(name).Delete()
                except Exception:
                    pass
        try:
            with_retry(lambda: d.Export(base, "DXF", ss))
        finally:
            _safe(lambda: ss.Delete())
        out = base + ".dxf"
        return {"ok": os.path.exists(out), "path": out}
    except Exception as e:
        return err(e)


# --------------------------------------------------------------------------- editing
@mcp.tool()
def create_layer(name: str, color: Optional[int] = None, make_current: bool = False) -> dict:
    """Create (or fetch) a layer, optionally set its ACI colour (1 red,2 yellow,3 green,4 cyan,5 blue,6 magenta,7 white)."""
    try:
        d = doc()
        ctl.create_layer(name)
        L = d.Layers.Item(name)
        if color is not None:
            L.color = int(color)
        if make_current:
            d.ActiveLayer = L
        return {"layer": name, "color": _safe(lambda: L.color), "current": bool(make_current)}
    except Exception as e:
        return err(e)


@mcp.tool()
def set_layer_state(name: str, on: Optional[bool] = None, frozen: Optional[bool] = None,
                    locked: Optional[bool] = None, make_current: bool = False) -> dict:
    """Turn a layer on/off, freeze/thaw, lock/unlock, or make it current."""
    try:
        d = doc()
        L = d.Layers.Item(name)
        if on is not None:
            L.LayerOn = bool(on)
        if frozen is not None:
            L.Freeze = bool(frozen)
        if locked is not None:
            L.Lock = bool(locked)
        if make_current:
            d.ActiveLayer = L
        _safe(lambda: d.Regen(1))
        return {"layer": name, "on": bool(L.LayerOn), "frozen": bool(L.Freeze), "locked": bool(L.Lock)}
    except Exception as e:
        return err(e)


def _ent_result(obj, what: str) -> dict:
    if obj is None or obj is False:
        return {"error": f"{what} failed (see logs/acad_mcp_server.log)"}
    try:
        return {"ok": True, "entity": summarize(obj)}
    except Exception:
        return {"ok": True}


@mcp.tool()
def draw_line(x1: float, y1: float, x2: float, y2: float, layer: Optional[str] = None,
              color: Optional[int] = None) -> dict:
    """Draw a line in model space."""
    try:
        return _ent_result(ctl.draw_line((x1, y1, 0), (x2, y2, 0), layer, color), "draw_line")
    except Exception as e:
        return err(e)


@mcp.tool()
def draw_circle(cx: float, cy: float, radius: float, layer: Optional[str] = None, color: Optional[int] = None) -> dict:
    """Draw a circle."""
    try:
        return _ent_result(ctl.draw_circle((cx, cy, 0), radius, layer, color), "draw_circle")
    except Exception as e:
        return err(e)


@mcp.tool()
def draw_arc(cx: float, cy: float, radius: float, start_angle_deg: float, end_angle_deg: float,
             layer: Optional[str] = None, color: Optional[int] = None) -> dict:
    """Draw an arc (angles in degrees, counter-clockwise)."""
    try:
        return _ent_result(ctl.draw_arc((cx, cy, 0), radius, start_angle_deg, end_angle_deg, layer, color), "draw_arc")
    except Exception as e:
        return err(e)


@mcp.tool()
def draw_polyline(points: list[list[float]], closed: bool = False, layer: Optional[str] = None,
                  color: Optional[int] = None) -> dict:
    """Draw a lightweight polyline through points [[x,y],...]."""
    try:
        pts = [(p[0], p[1], 0) for p in points]
        return _ent_result(ctl.draw_polyline(pts, closed, layer, color), "draw_polyline")
    except Exception as e:
        return err(e)


@mcp.tool()
def draw_rectangle(x1: float, y1: float, x2: float, y2: float, layer: Optional[str] = None,
                   color: Optional[int] = None) -> dict:
    """Draw a rectangle from two opposite corners."""
    try:
        return _ent_result(ctl.draw_rectangle((x1, y1, 0), (x2, y2, 0), layer, color), "draw_rectangle")
    except Exception as e:
        return err(e)


@mcp.tool()
def add_text(text: str, x: float, y: float, height: float = 2.5, rotation_deg: float = 0,
             layer: Optional[str] = None, color: Optional[int] = None) -> dict:
    """Add single-line TEXT at (x, y)."""
    try:
        return _ent_result(ctl.draw_text((x, y, 0), text, height, rotation_deg, layer, color), "add_text")
    except Exception as e:
        return err(e)


@mcp.tool()
def add_mtext(text: str, x: float, y: float, width: float = 100, height: float = 2.5,
              layer: Optional[str] = None, color: Optional[int] = None) -> dict:
    """Add multi-line MTEXT (use \\n for line breaks) with the given wrap width."""
    try:
        d = doc()
        m = with_retry(lambda: d.ModelSpace.AddMText(V(x, y), float(width), text.replace("\n", "\\P")))
        m.Height = float(height)
        if layer:
            ctl.create_layer(layer)
            m.Layer = layer
        if color is not None:
            m.color = int(color)
        return {"ok": True, "entity": summarize(m)}
    except Exception as e:
        return err(e)


@mcp.tool()
def add_dimension(x1: float, y1: float, x2: float, y2: float, text_x: Optional[float] = None,
                  text_y: Optional[float] = None, text_height: float = 3.5, layer: Optional[str] = None,
                  color: Optional[int] = None) -> dict:
    """Add a linear dimension between two points; text_x/text_y place the dimension line/text."""
    try:
        tp = (text_x, text_y, 0) if text_x is not None and text_y is not None else None
        return _ent_result(ctl.add_dimension((x1, y1, 0), (x2, y2, 0), tp, text_height, layer, color), "add_dimension")
    except Exception as e:
        return err(e)


@mcp.tool()
def draw_hatch(points: list[list[float]], pattern: str = "SOLID", scale: float = 1.0,
               layer: Optional[str] = None, color: Optional[int] = None) -> dict:
    """Hatch the closed polygon given by points [[x,y],...]."""
    try:
        pts = [(p[0], p[1], 0) for p in points]
        return _ent_result(ctl.draw_hatch(pts, pattern, scale, layer, color), "draw_hatch")
    except Exception as e:
        return err(e)


@mcp.tool()
def modify_entities(handles: list[str], layer: Optional[str] = None, color: Optional[int] = None,
                    linetype: Optional[str] = None, lineweight: Optional[int] = None) -> dict:
    """Change layer / colour / linetype / lineweight of the given entities."""
    try:
        d = doc()
        done, failed = [], []
        for h in handles:
            try:
                e = d.HandleToObject(h)
                if layer:
                    ctl.create_layer(layer)
                    e.Layer = layer
                if color is not None:
                    e.color = int(color)
                if linetype:
                    e.Linetype = linetype
                if lineweight is not None:
                    e.Lineweight = ctl.validate_lineweight(int(lineweight))
                done.append(h)
            except Exception as ex:
                failed.append({"handle": h, "error": str(ex)})
        _safe(lambda: d.Regen(1))
        return {"modified": done, "failed": failed}
    except Exception as e:
        return err(e)


@mcp.tool()
def move_entities(handles: list[str], dx: float, dy: float, copy: bool = False) -> dict:
    """Move (or copy then move) entities by a displacement."""
    try:
        d = doc()
        out = []
        for h in handles:
            e = d.HandleToObject(h)
            if copy:
                e = e.Copy()
            e.Move(V(0, 0), V(dx, dy))
            out.append(e.Handle)
        _safe(lambda: d.Regen(1))
        return {"handles": out}
    except Exception as e:
        return err(e)


@mcp.tool()
def delete_entities(handles: list[str]) -> dict:
    """Erase entities by handle (irreversible except via AutoCAD's own UNDO)."""
    try:
        d = doc()
        done, failed = [], []
        for h in handles:
            try:
                d.HandleToObject(h).Delete()
                done.append(h)
            except Exception as ex:
                failed.append({"handle": h, "error": str(ex)})
        _safe(lambda: d.Regen(1))
        return {"deleted": done, "failed": failed}
    except Exception as e:
        return err(e)


@mcp.tool()
def send_command(command: str) -> dict:
    """Send a raw command-line string to AutoCAD (a newline is appended), e.g. '_.ZOOM _E' or a LISP expression
    '(command \"_.PURGE\" \"_A\" \"*\" \"_N\")'. Commands that open dialogs will block AutoCAD until closed on the desktop."""
    try:
        d = doc()
        with_retry(lambda: d.SendCommand(command.rstrip("\n") + "\n"))
        return {"sent": command}
    except Exception as e:
        return err(e)


@mcp.tool()
def get_variable(name: str) -> dict:
    """Read an AutoCAD system variable (e.g. DIMSCALE, INSUNITS, CLAYER, DWGNAME)."""
    try:
        v = doc().GetVariable(name)
        return {"name": name, "value": r(list(v)) if isinstance(v, tuple) else v}
    except Exception as e:
        return err(e)


@mcp.tool()
def set_variable(name: str, value: Any) -> dict:
    """Set an AutoCAD system variable (numbers or strings)."""
    try:
        d = doc()
        d.SetVariable(name, value)
        return {"name": name, "value": d.GetVariable(name)}
    except Exception as e:
        return err(e)


# --------------------------------------------------------------------------- main
def main() -> None:
    pythoncom.CoInitialize()
    try:
        ctypes.windll.shcore.SetProcessDpiAwareness(2)
    except Exception:
        pass
    log.info("autocad-mcp starting (pid %s, progid %s, workspace %s)", os.getpid(), PROGID, WORKSPACE)
    try:
        connect()
        log.info("connected to AutoCAD %s", ctl.app.Version)
    except Exception as e:
        log.warning("AutoCAD not connected at startup: %s (tools will retry)", e)
    mcp.run()


if __name__ == "__main__":
    main()
