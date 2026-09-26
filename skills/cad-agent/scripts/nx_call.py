#!/usr/bin/env python3
"""One-shot CLI for the DreamEnding/NX_MCP sidecar running on dell-nb (Siemens NX 2412).

Each invocation opens an SSH session to the Dell through the bundled fleet-routed helper, starts the MCP
sidecar there (`python -m nx_mcp.server`, stdio transport), performs a complete MCP session
(initialize -> notifications/initialized -> tools/list or tools/call) and prints the result as JSON.
The sidecar forwards tool calls to the NX-side bridge (a journal running inside NX on the Dell);
start that bridge first with `nx_bridge.sh start`.

Usage:
  nx_call.py [--timeout S] [--raw] <tool_name> ['<json-args>']
  nx_call.py [--timeout S] --list-tools
  nx_call.py [--timeout S] --status

Exit codes: 0 success, 1 transport / protocol / usage failure, 2 the tool returned an error.
Environment: NX_CALL_SSH (override the SSH helper path; default: scripts/dell_ssh.sh next to this file),
             NX_CALL_WORKSPACE (override NX_MCP_WORKSPACE), NX_CALL_VENV_PYTHON (sidecar interpreter on the Dell).
Standard library only.
"""

from __future__ import annotations

import argparse
import json
import os
import queue
import subprocess
import sys
import threading
import time

_SIBLING_SSH = os.path.join(os.path.dirname(os.path.abspath(__file__)), "dell_ssh.sh")
DEFAULT_SSH = _SIBLING_SSH if os.path.exists(_SIBLING_SSH) else os.path.expanduser("~/.local/bin/dell-ssh.sh")
DEFAULT_WORKSPACE = os.environ.get("NX_CALL_WORKSPACE")
VENV_PYTHON = os.environ.get("NX_CALL_VENV_PYTHON")
PROTOCOL_VERSION = "2025-06-18"


def _ps_literal(value: str) -> str:
    return "'" + value.replace("'", "''") + "'"


def remote_command(workspace: str | None) -> str:
    workspace_expr = _ps_literal(workspace) if workspace else "(Join-Path $env:USERPROFILE 'work')"
    python_expr = _ps_literal(VENV_PYTHON) if VENV_PYTHON else "(Join-Path $env:USERPROFILE 'work\\venvs\\nx-mcp\\Scripts\\python.exe')"
    return (
        "[Console]::OutputEncoding=[Text.Encoding]::UTF8; "
        "[Console]::InputEncoding=[Text.Encoding]::UTF8; "
        f"$env:NX_MCP_WORKSPACE={workspace_expr}; "
        "$env:NX_MCP_ENABLE_EXPERIMENTAL='1'; "
        "$env:NX_MCP_ENABLE_JOURNAL='1'; "
        "$env:PYTHONUTF8='1'; "
        f"& {python_expr} -X utf8 -u -m nx_mcp.server"
    )


class McpSession:
    """Minimal newline-delimited JSON-RPC client over the sidecar's stdio (tolerates CRLF)."""

    def __init__(self, argv: list[str], timeout: float) -> None:
        self.timeout = timeout
        self.proc = subprocess.Popen(
            argv, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE
        )
        self.lines: queue.Queue[bytes | None] = queue.Queue()
        self.stderr: list[str] = []
        self._next_id = 0
        threading.Thread(target=self._pump_stdout, daemon=True).start()
        threading.Thread(target=self._pump_stderr, daemon=True).start()

    def _pump_stdout(self) -> None:
        assert self.proc.stdout is not None
        for line in iter(self.proc.stdout.readline, b""):
            self.lines.put(line)
        self.lines.put(None)

    def _pump_stderr(self) -> None:
        assert self.proc.stderr is not None
        for line in iter(self.proc.stderr.readline, b""):
            self.stderr.append(line.decode("utf-8", "replace").rstrip("\r\n"))

    def send(self, message: dict) -> None:
        assert self.proc.stdin is not None
        self.proc.stdin.write((json.dumps(message, ensure_ascii=False) + "\n").encode("utf-8"))
        self.proc.stdin.flush()

    def request(self, method: str, params: dict | None = None) -> dict:
        self._next_id += 1
        request_id = self._next_id
        self.send({"jsonrpc": "2.0", "id": request_id, "method": method, "params": params or {}})
        deadline = time.monotonic() + self.timeout
        while True:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise TimeoutError(f"no response to {method} within {self.timeout:g}s")
            try:
                raw = self.lines.get(timeout=min(remaining, 1.0))
            except queue.Empty:
                if self.proc.poll() is not None:
                    raise ConnectionError(
                        f"sidecar exited with code {self.proc.returncode} before answering {method}"
                    )
                continue
            if raw is None:
                raise ConnectionError(
                    f"sidecar closed stdout before answering {method} (exit {self.proc.poll()})"
                )
            text = raw.decode("utf-8", "replace").strip()
            if not text:
                continue
            try:
                message = json.loads(text)
            except json.JSONDecodeError:
                self.stderr.append(f"[non-json stdout] {text[:300]}")
                continue
            if message.get("id") == request_id:
                if "error" in message:
                    raise RuntimeError(json.dumps(message["error"], ensure_ascii=False))
                return message.get("result", {})
            # notifications / log messages from the server are ignored

    def notify(self, method: str, params: dict | None = None) -> None:
        self.send({"jsonrpc": "2.0", "method": method, "params": params or {}})

    def close(self) -> None:
        try:
            if self.proc.stdin:
                self.proc.stdin.close()
            self.proc.wait(timeout=10)
        except Exception:
            self.proc.kill()


def unwrap_tool_result(result: dict) -> tuple[dict | list | str, bool]:
    """Return (payload, is_error) from an MCP tools/call result."""
    is_error = bool(result.get("isError"))
    if "structuredContent" in result and result["structuredContent"] is not None:
        return result["structuredContent"], is_error
    texts = [c.get("text", "") for c in result.get("content", []) if c.get("type") == "text"]
    text = "\n".join(texts).strip()
    # Legacy tools return JSON as text; errors arrive as "Error executing tool x: {json}"
    for candidate in (text, text[text.find("{"):] if "{" in text else ""):
        if candidate:
            try:
                return json.loads(candidate), is_error
            except json.JSONDecodeError:
                pass
    return text, is_error


def emit(payload, exit_code: int) -> int:
    print(json.dumps(payload, ensure_ascii=False, indent=2))
    return exit_code


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("tool", nargs="?", help="MCP tool name, e.g. nx_status, nx_open_part")
    parser.add_argument("args", nargs="?", default="{}", help="JSON object with the tool arguments")
    parser.add_argument("--list-tools", action="store_true", help="print the tool catalogue")
    parser.add_argument("--status", action="store_true", help="shortcut for nx_status")
    parser.add_argument("--timeout", type=float, default=300.0, help="seconds to wait per request (default 300)")
    parser.add_argument("--raw", action="store_true", help="print the raw MCP result instead of the unwrapped payload")
    parser.add_argument("--ssh", default=os.environ.get("NX_CALL_SSH", DEFAULT_SSH), help="SSH helper path")
    parser.add_argument("--workspace", default=DEFAULT_WORKSPACE,
                        help="NX_MCP_WORKSPACE on the Dell (file arguments are relative to it)")
    opts = parser.parse_args(argv)

    if opts.status:
        opts.tool, opts.args = "nx_status", "{}"
    if not opts.list_tools and not opts.tool:
        parser.print_usage(sys.stderr)
        return emit({"status": "error", "code": "USAGE", "message": "tool name required (or --list-tools / --status)"}, 1)
    try:
        arguments = json.loads(opts.args) if not opts.list_tools else {}
        if not isinstance(arguments, dict):
            raise ValueError("tool arguments must be a JSON object")
    except ValueError as error:
        return emit({"status": "error", "code": "USAGE", "message": f"bad JSON arguments: {error}"}, 1)

    if not os.path.exists(opts.ssh):
        return emit({"status": "error", "code": "SSH_HELPER_MISSING", "message": opts.ssh}, 1)

    session = None
    try:
        session = McpSession([opts.ssh, remote_command(opts.workspace)], opts.timeout)
        init = session.request("initialize", {
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": {"name": "nx_call", "version": "0.1"},
        })
        session.notify("notifications/initialized")
        if opts.list_tools:
            result = session.request("tools/list")
            tools = result.get("tools", [])
            payload = {
                "server": init.get("serverInfo"),
                "count": len(tools),
                "tools": [
                    {
                        "name": t["name"],
                        "description": t.get("description", ""),
                        "required": t.get("inputSchema", {}).get("required", []),
                        "properties": list(t.get("inputSchema", {}).get("properties", {}).keys()),
                    }
                    for t in tools
                ],
            }
            return emit(payload if not opts.raw else result, 0)
        result = session.request("tools/call", {"name": opts.tool, "arguments": arguments})
        if opts.raw:
            return emit(result, 2 if result.get("isError") else 0)
        payload, is_error = unwrap_tool_result(result)
        return emit(payload, 2 if is_error else 0)
    except (TimeoutError, ConnectionError, RuntimeError, OSError) as error:
        tail = (session.stderr[-5:] if session else [])
        return emit({"status": "error", "code": type(error).__name__.upper(), "message": str(error), "stderr_tail": tail}, 1)
    finally:
        if session is not None:
            session.close()


if __name__ == "__main__":
    import os as _cad_policy_os
    if _cad_policy_os.environ.get("CAD_AGENT_LEGACY_CAD") != "explicit":
        raise SystemExit("Retired CAD MCP entrypoint. Use scripts/nx_direct.py. Historical use needs explicit legacy scope and CAD_AGENT_LEGACY_CAD=explicit.")
    sys.exit(main(sys.argv[1:]))
