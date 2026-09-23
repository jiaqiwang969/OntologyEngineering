#!/usr/bin/env python3
"""Authenticated loopback relay for an MCP server in the CAD desktop session.

Run this in the interactive Windows user's session, not as a system service.
The 32-byte token file must be private to that user; use normal Windows ACLs.
"""

from __future__ import annotations

import hmac
import os
from pathlib import Path
import secrets
import socket
import subprocess
import sys
import threading


def token_at(path: Path) -> bytes:
    if not path.exists():
        path.write_text(secrets.token_hex(32), encoding="ascii")
    token = path.read_text(encoding="ascii").strip().encode("ascii")
    if len(token) != 64:
        raise ValueError("relay token must be 32 bytes encoded as hexadecimal")
    return token


def serve_connection(connection: socket.socket, token: bytes, argv: list[str]) -> None:
    with connection:
        connection.settimeout(10)
        initial = bytearray()
        while b"\n" not in initial and len(initial) <= 128:
            chunk = connection.recv(128)
            if not chunk:
                return
            initial.extend(chunk)
        if b"\n" not in initial:
            return
        presented, _, remainder = bytes(initial).partition(b"\n")
        if not hmac.compare_digest(presented.strip(), token):
            return
        connection.settimeout(None)
        process = subprocess.Popen(argv, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                   stderr=sys.stderr, creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
        assert process.stdin is not None and process.stdout is not None
        if remainder:
            process.stdin.write(remainder)
            process.stdin.flush()

        def incoming() -> None:
            try:
                while chunk := connection.recv(65536):
                    process.stdin.write(chunk)
                    process.stdin.flush()
            except OSError:
                pass
            finally:
                process.stdin.close()

        worker = threading.Thread(target=incoming, daemon=True)
        worker.start()
        try:
            while chunk := process.stdout.read1(65536):
                connection.sendall(chunk)
        except OSError:
            pass
        finally:
            if process.poll() is None:
                process.terminate()
            process.wait(timeout=5)


def main() -> int:
    base = Path(__file__).resolve().parent
    path = Path(os.environ.get("CAD_BRIDGE_TOKEN_FILE", base / "relay.token"))
    token = token_at(path)
    port = int(os.environ.get("CAD_BRIDGE_PORT", "39901"))
    server = Path(os.environ.get("CAD_BRIDGE_SERVER", base / "autocad_server.py"))
    python = os.environ.get("CAD_BRIDGE_PYTHON", sys.executable)
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as listener:
        listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        listener.bind(("127.0.0.1", port))
        listener.listen(8)
        while True:
            connection, _ = listener.accept()
            threading.Thread(target=serve_connection, args=(connection, token, [python, str(server)]),
                             daemon=True).start()


if __name__ == "__main__":
    raise SystemExit(main())
