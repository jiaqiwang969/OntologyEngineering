#!/usr/bin/env python3
"""Small AutoCAD COM MCP server; run inside the signed-in Windows desktop.

Requires AutoCAD and pywin32 installed separately. Inspection is the default;
editing requires an explicit environment flag and a workspace copy.
"""

from __future__ import annotations

import json
import os
from pathlib import Path
import sys
from typing import Any


TOOLS = [
    {"name": "acad_status", "description": "Report the running AutoCAD and active drawing.",
     "inputSchema": {"type": "object", "properties": {}}},
    {"name": "list_documents", "description": "List open drawing names and full paths.",
     "inputSchema": {"type": "object", "properties": {}}},
    {"name": "drawing_info", "description": "Inspect units, entities and the current drawing.",
     "inputSchema": {"type": "object", "properties": {}}},
    {"name": "list_layers", "description": "List layer names and display states.",
     "inputSchema": {"type": "object", "properties": {}}},
    {"name": "list_entities", "description": "List a bounded set of model-space entity identities.",
     "inputSchema": {"type": "object", "properties": {"limit": {"type": "integer", "minimum": 1, "maximum": 1000}}}},
    {"name": "get_entity", "description": "Inspect a model-space entity by AutoCAD handle.",
     "inputSchema": {"type": "object", "properties": {"handle": {"type": "string"}}, "required": ["handle"]}},
    {"name": "open_drawing", "description": "Open a drawing, read-only unless write mode is explicitly enabled.",
     "inputSchema": {"type": "object", "properties": {"path": {"type": "string"},
                                                     "read_only": {"type": "boolean"}}, "required": ["path"]}},
    {"name": "add_line", "description": "Add one line to a workspace drawing in enabled write mode.",
     "inputSchema": {"type": "object", "properties": {"start": {"type": "array"},
                                                     "end": {"type": "array"}}, "required": ["start", "end"]}},
    {"name": "save_drawing", "description": "Save the active workspace drawing in enabled write mode.",
     "inputSchema": {"type": "object", "properties": {}}},
]
_COM_READY = False


def com_app() -> Any:
    global _COM_READY
    import pythoncom
    import win32com.client
    if not _COM_READY:
        pythoncom.CoInitialize()
        _COM_READY = True
    return win32com.client.GetActiveObject(os.environ.get("CAD_BRIDGE_PROGID", "AutoCAD.Application"))


def summary(entity: Any) -> dict:
    result = {"handle": str(entity.Handle), "type": str(entity.ObjectName),
              "layer": str(entity.Layer)}
    for key in ("TextString", "Radius", "Length", "Area"):
        try:
            value = getattr(entity, key)
            if isinstance(value, (str, float, int)):
                result[key] = value
        except Exception:
            pass
    return result


def writable(document: Any) -> None:
    if os.environ.get("CAD_BRIDGE_ALLOW_WRITE") != "1":
        raise PermissionError("write mode is disabled")
    workspace = os.environ.get("CAD_BRIDGE_WORKSPACE")
    if not workspace:
        raise PermissionError("CAD_BRIDGE_WORKSPACE must be configured for write mode")
    document_path = Path(str(document.FullName)).resolve()
    if not document_path.is_relative_to(Path(workspace).resolve()):
        raise PermissionError("active drawing is outside the configured workspace")


def invoke(name: str, args: dict) -> dict:
    app = com_app()
    if name == "acad_status":
        document = app.ActiveDocument
        return {"application_version": str(app.Version), "active_document": str(document.FullName)}
    if name == "list_documents":
        return {"documents": [{"name": str(doc.Name), "path": str(doc.FullName)}
                              for doc in app.Documents]}
    if name == "open_drawing":
        path = Path(args["path"]).resolve(strict=True)
        read_only = args.get("read_only", True)
        if read_only is not True:
            if os.environ.get("CAD_BRIDGE_ALLOW_WRITE") != "1":
                raise PermissionError("write mode is disabled")
            workspace = os.environ.get("CAD_BRIDGE_WORKSPACE")
            if not workspace or not path.is_relative_to(Path(workspace).resolve()):
                raise PermissionError("writable drawing must be inside CAD_BRIDGE_WORKSPACE")
        document = app.Documents.Open(str(path), bool(read_only))
        return {"name": str(document.Name), "path": str(document.FullName), "read_only": bool(read_only)}
    document = app.ActiveDocument
    if name == "drawing_info":
        return {"name": str(document.Name), "path": str(document.FullName),
                "modelspace_count": int(document.ModelSpace.Count),
                "layers_count": int(document.Layers.Count),
                "insunits": int(document.GetVariable("INSUNITS"))}
    if name == "list_layers":
        return {"layers": [{"name": str(layer.Name), "on": bool(layer.LayerOn),
                            "frozen": bool(layer.Freeze)} for layer in document.Layers]}
    if name == "list_entities":
        limit = args.get("limit", 100)
        if type(limit) is not int or not 1 <= limit <= 1000:
            raise ValueError("limit must be an integer from 1 to 1000")
        count = int(document.ModelSpace.Count)
        return {"total": count, "returned": min(count, limit),
                "entities": [summary(document.ModelSpace.Item(i)) for i in range(min(count, limit))]}
    if name == "get_entity":
        return summary(document.HandleToObject(str(args["handle"])))
    if name == "add_line":
        writable(document)
        import pythoncom
        import win32com.client
        points = []
        for key in ("start", "end"):
            point = args[key]
            if not isinstance(point, list) or len(point) not in (2, 3):
                raise ValueError(f"{key} must contain two or three coordinates")
            points.append(win32com.client.VARIANT(pythoncom.VT_ARRAY | pythoncom.VT_R8,
                                                  [float(x) for x in (point + [0.0])[:3]]))
        entity = document.ModelSpace.AddLine(points[0], points[1])
        return summary(entity)
    if name == "save_drawing":
        writable(document)
        document.Save()
        return {"saved": str(document.FullName)}
    raise ValueError("unknown tool: " + name)


def handle(message: dict) -> dict | None:
    method = message.get("method")
    if method == "notifications/initialized":
        return None
    rid = message.get("id")
    try:
        if method == "initialize":
            result = {"protocolVersion": "2025-06-18", "capabilities": {"tools": {}},
                      "serverInfo": {"name": "oe-autocad-com", "version": "1"}}
        elif method == "tools/list":
            result = {"tools": TOOLS}
        elif method == "tools/call":
            params = message.get("params") or {}
            result = {"content": [{"type": "text", "text": json.dumps(
                invoke(params["name"], params.get("arguments") or {}), ensure_ascii=False)}]}
        else:
            return {"jsonrpc": "2.0", "id": rid,
                    "error": {"code": -32601, "message": "method not found"}}
        return {"jsonrpc": "2.0", "id": rid, "result": result}
    except Exception as error:
        if method == "tools/call":
            return {"jsonrpc": "2.0", "id": rid,
                    "result": {"isError": True, "content": [{"type": "text", "text": str(error)}]}}
        return {"jsonrpc": "2.0", "id": rid,
                "error": {"code": -32603, "message": str(error)}}


def main() -> None:
    for line in sys.stdin:
        try:
            response = handle(json.loads(line))
        except (ValueError, TypeError) as error:
            response = {"jsonrpc": "2.0", "id": None,
                        "error": {"code": -32700, "message": str(error)}}
        if response is not None:
            print(json.dumps(response, ensure_ascii=False), flush=True)


if __name__ == "__main__":
    main()
