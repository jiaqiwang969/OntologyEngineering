# -*- coding: utf-8 -*-
"""Loopback relay that runs in the INTERACTIVE Windows session (scheduled task CadMcpRelay).

Each TCP connection on 127.0.0.1:PORT (after a one-line token) gets its own acad_mcp_server.py
process; the connection's bytes are piped to the server's stdin and its stdout is piped back.
SSH sessions (session 0) reach AutoCAD's COM server in session 1 through this relay.
"""
import os
import secrets
import socket
import subprocess
import sys
import threading
import time

BASE = os.path.dirname(os.path.abspath(__file__))
PORT = int(os.environ.get("CADMCP_PORT", "39901"))
PY = os.environ.get("CADMCP_PYTHON", os.path.join(os.path.expanduser("~"), "work", "venvs", "cad-mcp", "Scripts", "python.exe"))
SERVER = os.path.join(BASE, "acad_mcp_server.py")
TOKEN_FILE = os.path.join(BASE, "relay.token")
LOG_DIR = os.path.join(BASE, "logs")
os.makedirs(LOG_DIR, exist_ok=True)
LOG = open(os.path.join(LOG_DIR, "relay.log"), "a", encoding="utf-8", buffering=1)


def log(msg):
    LOG.write(time.strftime("%Y-%m-%d %H:%M:%S ") + msg + "\n")


def token() -> bytes:
    if not os.path.exists(TOKEN_FILE):
        with open(TOKEN_FILE, "w", encoding="ascii") as f:
            f.write(secrets.token_hex(32))
    with open(TOKEN_FILE, "r", encoding="ascii") as f:
        return f.read().strip().encode()


def pump_sock_to_proc(conn, proc):
    try:
        while True:
            data = conn.recv(65536)
            if not data:
                break
            proc.stdin.write(data)
            proc.stdin.flush()
    except Exception as e:
        log(f"sock->proc ended: {e}")
    finally:
        try:
            proc.stdin.close()
        except Exception:
            pass


def pump_proc_to_sock(conn, proc):
    try:
        while True:
            data = proc.stdout.read1(65536) if hasattr(proc.stdout, "read1") else proc.stdout.read(65536)
            if not data:
                break
            conn.sendall(data)
    except Exception as e:
        log(f"proc->sock ended: {e}")
    finally:
        try:
            conn.shutdown(socket.SHUT_WR)
        except Exception:
            pass


def handle(conn, addr):
    conn.settimeout(15)
    try:
        buf = b""
        while b"\n" not in buf and len(buf) < 256:
            chunk = conn.recv(256)
            if not chunk:
                return
            buf += chunk
        if buf.split(b"\n", 1)[0].strip() != token():
            log(f"auth failed from {addr}")
            conn.sendall(b'{"error":"bad token"}\n')
            return
        rest = buf.split(b"\n", 1)[1] if b"\n" in buf else b""
        conn.settimeout(None)
        env = dict(os.environ, PYTHONIOENCODING="utf-8", PYTHONUTF8="1")
        errlog = open(os.path.join(LOG_DIR, "server_stderr.log"), "a", encoding="utf-8", errors="replace")
        proc = subprocess.Popen([PY, "-X", "utf8", "-u", SERVER], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                stderr=errlog, cwd=BASE, env=env,
                                creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
        log(f"session from {addr}: server pid {proc.pid}")
        if rest:
            proc.stdin.write(rest)
            proc.stdin.flush()
        t1 = threading.Thread(target=pump_sock_to_proc, args=(conn, proc), daemon=True)
        t2 = threading.Thread(target=pump_proc_to_sock, args=(conn, proc), daemon=True)
        t1.start()
        t2.start()
        t1.join()
        # client went away: give the server a moment to exit on stdin EOF, then kill it
        for _ in range(50):
            if proc.poll() is not None:
                break
            time.sleep(0.1)
        if proc.poll() is None:
            proc.kill()
        t2.join(timeout=5)
        log(f"session from {addr} closed (server exit {proc.returncode})")
    except Exception as e:
        log(f"handler error: {e}")
    finally:
        try:
            conn.close()
        except Exception:
            pass


def main():
    token()
    srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    try:
        srv.bind(("127.0.0.1", PORT))
    except OSError as e:
        log(f"cannot bind 127.0.0.1:{PORT}: {e} (another relay running?)")
        sys.exit(1)
    srv.listen(8)
    with open(os.path.join(BASE, "relay.pid"), "w") as f:
        f.write(str(os.getpid()))
    log(f"relay listening on 127.0.0.1:{PORT} (pid {os.getpid()}, session-interactive)")
    while True:
        conn, addr = srv.accept()
        threading.Thread(target=handle, args=(conn, addr), daemon=True).start()


if __name__ == "__main__":
    main()
