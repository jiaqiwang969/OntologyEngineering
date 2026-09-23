#!/usr/bin/env python3
"""Forward MCP stdio from an SSH session to a Windows interactive-session relay."""

from __future__ import annotations

import os
from pathlib import Path
import socket
import sys
import threading


def main() -> int:
    if os.name == "nt":
        import msvcrt
        msvcrt.setmode(sys.stdin.fileno(), os.O_BINARY)
        msvcrt.setmode(sys.stdout.fileno(), os.O_BINARY)
    token_file = Path(os.environ.get("CAD_BRIDGE_TOKEN_FILE", Path(__file__).with_name("relay.token")))
    token = token_file.read_text(encoding="ascii").strip().encode("ascii")
    port = int(os.environ.get("CAD_BRIDGE_PORT", "39901"))
    with socket.create_connection(("127.0.0.1", port), timeout=10) as connection:
        connection.sendall(token + b"\n")

        def forward() -> None:
            while chunk := sys.stdin.buffer.read1(65536):
                connection.sendall(chunk)
            connection.shutdown(socket.SHUT_WR)

        thread = threading.Thread(target=forward, daemon=True)
        thread.start()
        while chunk := connection.recv(65536):
            sys.stdout.buffer.write(chunk)
            sys.stdout.buffer.flush()
        thread.join(timeout=2)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
