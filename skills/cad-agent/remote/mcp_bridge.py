#!/usr/bin/env python3
"""One-shot MCP stdio bridge for an explicitly configured CAD host.

The bridge transports JSON-RPC; it does not install NX, AutoCAD, or an MCP
server. A tool call is sent once, including when the response is ambiguous.
"""

from __future__ import annotations

import argparse
import base64
import json
import os
from pathlib import Path
import queue
import re
import subprocess
import sys
import threading
import time


PROTOCOL = "2025-06-18"
HOST = re.compile(r"^[A-Za-z0-9_.@-]+$")


def ps_quote(value: str) -> str:
    return "'" + value.replace("'", "''") + "'"


def command_for(profile: dict) -> list[str]:
    transport = profile.get("transport")
    if transport == "local_stdio":
        argv = profile.get("argv")
        if not isinstance(argv, list) or not argv or not all(isinstance(x, str) and x for x in argv):
            raise ValueError("local_stdio requires a nonempty argv array")
        return argv
    if transport != "ssh_windows":
        raise ValueError("transport must be ssh_windows or local_stdio")
    host = profile.get("host")
    if not isinstance(host, str) or not HOST.fullmatch(host) or host.startswith("-"):
        raise ValueError("host must be an SSH alias or user@host without shell syntax")
    python = profile.get("python")
    entry = profile.get("entry")
    if not isinstance(python, str) or not python or not isinstance(entry, dict):
        raise ValueError("ssh_windows requires python and entry")
    if set(entry) == {"module"} and isinstance(entry["module"], str):
        if not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_.]*", entry["module"]):
            raise ValueError("invalid Python module")
        invocation = f"& {ps_quote(python)} -X utf8 -u -m {entry['module']}"
    elif set(entry) == {"script"} and isinstance(entry["script"], str) and entry["script"]:
        invocation = f"& {ps_quote(python)} -X utf8 -u {ps_quote(entry['script'])}"
    else:
        raise ValueError("entry must contain exactly module or script")
    env = profile.get("environment", {})
    if not isinstance(env, dict) or any(not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", k)
                                         or not isinstance(v, str) for k, v in env.items()):
        raise ValueError("environment must contain string environment variables")
    assignments = " ".join(f"$env:{k}={ps_quote(v)};" for k, v in sorted(env.items()))
    script = ("[Console]::OutputEncoding=[Text.Encoding]::UTF8; "
              "[Console]::InputEncoding=[Text.Encoding]::UTF8; "
              "$env:PYTHONUTF8='1'; " + assignments + " " + invocation)
    encoded = base64.b64encode(script.encode("utf-16le")).decode("ascii")
    return ["ssh", "-T", "-o", "BatchMode=yes", "-o", "ConnectTimeout=10", host,
            "powershell.exe", "-NoProfile", "-NonInteractive", "-EncodedCommand", encoded]


class Session:
    def __init__(self, argv: list[str], timeout: float):
        self.timeout = timeout
        self.proc = subprocess.Popen(argv, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                     stderr=subprocess.PIPE)
        self.lines: queue.Queue[bytes | None] = queue.Queue()
        self.stderr: list[str] = []
        self.next_id = 0
        threading.Thread(target=self._stdout, daemon=True).start()
        threading.Thread(target=self._stderr, daemon=True).start()

    def _stdout(self) -> None:
        assert self.proc.stdout is not None
        for line in self.proc.stdout:
            self.lines.put(line)
        self.lines.put(None)

    def _stderr(self) -> None:
        assert self.proc.stderr is not None
        for line in self.proc.stderr:
            self.stderr.append(line.decode("utf-8", "replace").rstrip())

    def send(self, message: dict) -> None:
        assert self.proc.stdin is not None
        self.proc.stdin.write((json.dumps(message, ensure_ascii=False) + "\n").encode())
        self.proc.stdin.flush()

    def request(self, method: str, params: dict | None = None) -> dict:
        self.next_id += 1
        rid = self.next_id
        self.send({"jsonrpc": "2.0", "id": rid, "method": method, "params": params or {}})
        deadline = time.monotonic() + self.timeout
        while True:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise TimeoutError(f"{method}: no response within {self.timeout:g}s; execution state unknown")
            try:
                raw = self.lines.get(timeout=remaining)
            except queue.Empty as error:
                raise TimeoutError(f"{method}: no response within {self.timeout:g}s; execution state unknown") from error
            if raw is None:
                raise ConnectionError(f"remote MCP stdout closed before {method} responded")
            try:
                response = json.loads(raw)
            except (ValueError, UnicodeDecodeError):
                continue
            if isinstance(response, dict) and response.get("id") == rid:
                if "error" in response:
                    raise RuntimeError(json.dumps(response["error"], ensure_ascii=False))
                return response.get("result", {})

    def close(self) -> None:
        try:
            if self.proc.stdin:
                self.proc.stdin.close()
            try:
                self.proc.wait(timeout=2)
            except subprocess.TimeoutExpired:
                self.proc.terminate()
                try:
                    self.proc.wait(timeout=2)
                except subprocess.TimeoutExpired:
                    self.proc.kill()
                    self.proc.wait()
        finally:
            if self.proc.stdout:
                self.proc.stdout.close()
            if self.proc.stderr:
                self.proc.stderr.close()


def run(profile: dict, tool: str | None, arguments: dict, timeout: float) -> dict:
    session = Session(command_for(profile), timeout)
    try:
        init = session.request("initialize", {"protocolVersion": PROTOCOL, "capabilities": {},
                                               "clientInfo": {"name": "oe-cad-bridge", "version": "1"}})
        session.send({"jsonrpc": "2.0", "method": "notifications/initialized"})
        if tool is None:
            result = session.request("tools/list")
            return {"server": init.get("serverInfo"), "tools": result.get("tools", [])}
        return session.request("tools/call", {"name": tool, "arguments": arguments})
    finally:
        session.close()


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--config", type=Path, required=True)
    parser.add_argument("--profile", required=True)
    parser.add_argument("--list-tools", action="store_true")
    parser.add_argument("--timeout", type=float, default=120.0)
    parser.add_argument("tool", nargs="?")
    parser.add_argument("arguments", nargs="?", default="{}")
    args = parser.parse_args(argv)
    if not args.list_tools and not args.tool:
        parser.error("provide a tool or --list-tools")
    if not 0 < args.timeout <= 3600:
        parser.error("timeout must be in (0, 3600]")
    try:
        config = json.loads(args.config.read_text(encoding="utf-8"))
        profile = config["profiles"][args.profile]
        values = json.loads(args.arguments)
        if not isinstance(values, dict):
            raise ValueError("tool arguments must be a JSON object")
        result = run(profile, None if args.list_tools else args.tool, values, args.timeout)
        print(json.dumps(result, ensure_ascii=False, indent=2))
        return 2 if result.get("isError") else 0
    except (OSError, KeyError, ValueError, RuntimeError, TimeoutError, ConnectionError) as error:
        print(json.dumps({"status": "UNKNOWN" if isinstance(error, TimeoutError) else "ERROR",
                          "error": str(error)}, ensure_ascii=False), file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
