#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""One-shot MCP client for the AutoCAD 2024 service on dell-nb (stdlib only).

Each invocation opens an MCP stdio session to the Dell through scripts/dell_ssh.sh (or AUTOCAD_CALL_SSH)
(ssh -> relay_client.py -> loopback relay in the interactive session -> acad_mcp_server.py -> AutoCAD COM),
performs initialize / notifications/initialized / tools/list or tools/call, prints the result as JSON
and exits. Exit code 0 on success, 1 on tool or transport error (error JSON on stdout), 2 on usage error.

Usage:
  autocad_call.py --status                       # acad_status tool
  autocad_call.py --list-tools [--json]          # tool catalogue
  autocad_call.py <tool_name> ['<json-args>']    # e.g. autocad_call.py list_layers '{"with_counts": true}'
Options:
  --timeout SEC     per-request timeout (default 120; ssh connect adds ~5 s)
  --host IP         deprecated: direct addresses are rejected; fleet resolves dell-nb
  --image-out PATH  where to save an image result (capture_view); default ./autocad_capture_<ts>.png
  --raw             print the raw JSON-RPC response instead of the unwrapped result
"""
from __future__ import annotations

import argparse
import base64
import json
import os
import queue
import subprocess
import sys
import threading
import time

_SIBLING_SSH = os.path.join(os.path.dirname(os.path.abspath(__file__)), "dell_ssh.sh")
DELL_SSH = os.environ.get("AUTOCAD_CALL_SSH") or (_SIBLING_SSH if os.path.exists(_SIBLING_SSH) else os.path.expanduser("~/.local/bin/dell-ssh.sh"))
REMOTE_PY = os.environ.get("AUTOCAD_CALL_VENV_PYTHON")
REMOTE_CLIENT = os.environ.get("AUTOCAD_CALL_RELAY_CLIENT")


def _ps_literal(value: str) -> str:
    return "'" + value.replace("'", "''") + "'"


_PY_EXPR = _ps_literal(REMOTE_PY) if REMOTE_PY else "(Join-Path $env:USERPROFILE 'work\\venvs\\cad-mcp\\Scripts\\python.exe')"
_CLIENT_EXPR = _ps_literal(REMOTE_CLIENT) if REMOTE_CLIENT else "(Join-Path $env:USERPROFILE 'work\\cadmcp\\relay_client.py')"
REMOTE_CMD = (
    "[Console]::OutputEncoding=[Text.Encoding]::UTF8; [Console]::InputEncoding=[Text.Encoding]::UTF8; "
    f"& {_PY_EXPR} -X utf8 -u {_CLIENT_EXPR}"
)
PROTOCOL_VERSION = "2025-06-18"


class Endpoint:
    """Line-delimited JSON-RPC over the stdio of the ssh child process."""

    def __init__(self, timeout: float, env: dict):
        self.timeout = timeout
        self.p = subprocess.Popen([DELL_SSH, REMOTE_CMD], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                  stderr=subprocess.PIPE, env=env)
        self.q: queue.Queue = queue.Queue()
        self.stderr: list[str] = []
        self.noise: list[str] = []
        self.next_id = 1
        threading.Thread(target=self._read_stdout, daemon=True).start()
        threading.Thread(target=self._read_stderr, daemon=True).start()

    def _read_stdout(self):
        assert self.p.stdout is not None
        for raw in self.p.stdout:
            line = raw.rstrip(b"\r\n")
            if not line.strip():
                continue
            try:
                self.q.put(json.loads(line.decode("utf-8")))
            except Exception:
                self.noise.append(line.decode("utf-8", "replace")[:300])
        self.q.put(None)

    def _read_stderr(self):
        assert self.p.stderr is not None
        for raw in self.p.stderr:
            self.stderr.append(raw.decode("utf-8", "replace").rstrip())

    def send(self, obj: dict):
        assert self.p.stdin is not None
        self.p.stdin.write((json.dumps(obj, ensure_ascii=False) + "\n").encode("utf-8"))
        self.p.stdin.flush()

    def request(self, method: str, params: dict | None = None) -> dict:
        rid = self.next_id
        self.next_id += 1
        self.send({"jsonrpc": "2.0", "id": rid, "method": method, "params": params or {}})
        deadline = time.time() + self.timeout
        while True:
            remaining = deadline - time.time()
            if remaining <= 0:
                raise TimeoutError(f"timed out after {self.timeout:.0f}s waiting for {method}")
            try:
                msg = self.q.get(timeout=remaining)
            except queue.Empty:
                raise TimeoutError(f"timed out after {self.timeout:.0f}s waiting for {method}")
            if msg is None:
                raise ConnectionError("endpoint closed before answering " + method)
            if isinstance(msg, dict) and msg.get("id") == rid:
                return msg
            # notifications / server logs are ignored

    def close(self):
        try:
            if self.p.stdin:
                self.p.stdin.close()
        except Exception:
            pass
        try:
            self.p.wait(timeout=8)
        except Exception:
            self.p.kill()

    def diagnostics(self) -> dict:
        d: dict = {}
        if self.stderr:
            d["stderr"] = self.stderr[-12:]
        if self.noise:
            d["non_json_output"] = self.noise[-5:]
        rc = self.p.poll()
        if rc is not None:
            d["ssh_exit_code"] = rc
        return d


def unwrap_result(result: dict, image_out: str | None) -> tuple[object, bool]:
    """Turn an MCP tools/call result into plain JSON; saves image content to disk."""
    is_error = bool(result.get("isError"))
    if result.get("structuredContent") is not None and not is_error:
        return result["structuredContent"], is_error
    out = []
    for item in result.get("content", []):
        t = item.get("type")
        if t == "text":
            txt = item.get("text", "")
            try:
                out.append(json.loads(txt))
            except Exception:
                out.append(txt)
        elif t == "image":
            data = base64.b64decode(item.get("data", ""))
            path = image_out or os.path.abspath(f"autocad_capture_{time.strftime('%Y%m%d-%H%M%S')}.png")
            with open(path, "wb") as f:
                f.write(data)
            out.append({"image_saved": path, "mime_type": item.get("mimeType"), "bytes": len(data)})
        else:
            out.append(item)
    if len(out) == 1:
        return out[0], is_error
    return out, is_error


def emit(obj, code: int = 0):
    print(json.dumps(obj, ensure_ascii=False, indent=2))
    sys.exit(code)


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("tool", nargs="?", help="MCP tool name")
    ap.add_argument("args", nargs="?", default="{}", help="JSON object with the tool arguments")
    ap.add_argument("--list-tools", action="store_true")
    ap.add_argument("--status", action="store_true")
    ap.add_argument("--json", action="store_true", help="with --list-tools: full schemas")
    ap.add_argument("--timeout", type=float, default=120.0)
    ap.add_argument("--host", help="deprecated; direct Dell addresses are rejected")
    ap.add_argument("--image-out")
    ap.add_argument("--raw", action="store_true")
    a = ap.parse_args()

    if a.status:
        tool, args = "acad_status", {}
    elif a.list_tools:
        tool, args = None, {}
    elif a.tool:
        tool = a.tool
        try:
            args = json.loads(a.args) if a.args else {}
        except json.JSONDecodeError as e:
            emit({"error": f"arguments must be a JSON object: {e}"}, 2)
        if not isinstance(args, dict):
            emit({"error": "arguments must be a JSON object"}, 2)
    else:
        ap.print_help()
        sys.exit(2)

    if not os.access(DELL_SSH, os.X_OK):
        emit({"error": f"{DELL_SSH} not found or not executable"}, 1)
    env = dict(os.environ)
    if a.host:
        emit({"error": "--host bypasses fleet identity resolution; omit it"}, 2)

    ep = Endpoint(a.timeout, env)
    try:
        init = ep.request("initialize", {"protocolVersion": PROTOCOL_VERSION, "capabilities": {},
                                         "clientInfo": {"name": "autocad_call", "version": "1.0"}})
        if "error" in init:
            emit({"error": "initialize failed", "response": init, **ep.diagnostics()}, 1)
        ep.send({"jsonrpc": "2.0", "method": "notifications/initialized"})
        if tool is None:
            resp = ep.request("tools/list")
            if a.raw:
                emit(resp)
            if "error" in resp:
                emit({"error": resp["error"], **ep.diagnostics()}, 1)
            tools = resp["result"]["tools"]
            if a.json:
                emit(tools)
            emit({"server": init.get("result", {}).get("serverInfo"), "count": len(tools),
                  "tools": [{"name": t["name"], "description": (t.get("description") or "").split("\n")[0][:160]}
                            for t in tools]})
        resp = ep.request("tools/call", {"name": tool, "arguments": args})
        if a.raw:
            emit(resp, 0 if "error" not in resp else 1)
        if "error" in resp:
            emit({"error": resp["error"], "tool": tool, **ep.diagnostics()}, 1)
        payload, is_error = unwrap_result(resp["result"], a.image_out)
        if is_error or (isinstance(payload, dict) and "error" in payload):
            emit(payload if isinstance(payload, dict) else {"error": payload}, 1)
        emit(payload, 0)
    except (TimeoutError, ConnectionError, BrokenPipeError) as e:
        emit({"error": str(e), **ep.diagnostics()}, 1)
    finally:
        ep.close()


if __name__ == "__main__":
    import os as _cad_policy_os
    if _cad_policy_os.environ.get("CAD_AGENT_LEGACY_CAD") != "explicit":
        raise SystemExit("Retired CAD MCP entrypoint. Use scripts/nx_direct.py. Historical use needs explicit legacy scope and CAD_AGENT_LEGACY_CAD=explicit.")
    main()
