# -*- coding: utf-8 -*-
"""stdio <-> loopback relay client. Run from an SSH session; it forwards this process's stdin/stdout
byte-for-byte to the relay in the interactive session, which spawns acad_mcp_server.py there."""
import os
import socket
import sys
import threading

BASE = os.path.dirname(os.path.abspath(__file__))
PORT = int(os.environ.get("CADMCP_PORT", "39901"))
TOKEN_FILE = os.path.join(BASE, "relay.token")

if os.name == "nt":
    import msvcrt

    msvcrt.setmode(sys.stdin.fileno(), os.O_BINARY)
    msvcrt.setmode(sys.stdout.fileno(), os.O_BINARY)

inp = sys.stdin.buffer
out = sys.stdout.buffer


def fail(msg):
    sys.stderr.write(msg + "\n")
    sys.stderr.flush()
    sys.exit(2)


try:
    with open(TOKEN_FILE, "r", encoding="ascii") as f:
        tok = f.read().strip().encode()
except OSError as e:
    fail(f"relay token missing ({e}); is the CadMcpRelay task running?")

try:
    s = socket.create_connection(("127.0.0.1", PORT), timeout=5)
except OSError as e:
    fail(f"cannot reach relay on 127.0.0.1:{PORT}: {e}. Start the CadMcpRelay scheduled task on dell-nb.")
s.settimeout(None)
s.sendall(tok + b"\n")


def stdin_to_sock():
    try:
        while True:
            data = inp.read1(65536) if hasattr(inp, "read1") else inp.read(65536)
            if not data:
                break
            s.sendall(data)
    except Exception:
        pass
    finally:
        try:
            s.shutdown(socket.SHUT_WR)
        except Exception:
            pass


t = threading.Thread(target=stdin_to_sock, daemon=True)
t.start()
try:
    while True:
        data = s.recv(65536)
        if not data:
            break
        out.write(data)
        out.flush()
except Exception:
    pass
finally:
    try:
        s.close()
    except Exception:
        pass
    os._exit(0)
