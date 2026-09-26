# -*- coding: utf-8 -*-
"""NX MCP bridge for the INTERACTIVE NX GUI.

Play this file from NX:  工具(Tools) > 操作记录(Journal) > 播放(Play) > start_nx_bridge_gui.py

It starts the DreamEnding/NX_MCP bridge inside the running NX GUI process and keeps the NX window
alive while it waits for requests: between bridge pumps it runs a bounded Win32 message pump
(PeekMessageW / TranslateMessage / DispatchMessageW), so the graphics window keeps repainting and
tools such as nx_screenshot / nx_set_view see a real display.

Configuration comes from start_nx_bridge_gui.json next to this file (the NX GUI process does not
have the NX_MCP_* environment variables). Stop it by creating the stop file (stop-bridge.ps1 does
that) or wait for max_runtime_s. Also runs unchanged under run_journal.exe (batch), where the
message pump simply finds no messages.

STATUS: the message-pump approach is UNVERIFIED inside the NX GUI (validated only in batch mode).
Pilot it only with no unsaved production work open. Stop the batch bridge first: both variants
share the descriptor file %LOCALAPPDATA%\\nx-mcp\\bridge.json and the stop file.
"""

from __future__ import annotations

import ctypes
import ctypes.wintypes as wt
import json
import os
import socket
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
CONFIG_PATH = HERE / "start_nx_bridge_gui.json"
WORKROOT = Path.home() / "work"
DEFAULTS = {
    "nx_mcp_src": str(WORKROOT / "NX_MCP" / "src"),
    "workspace": str(WORKROOT),
    "stop_file": str(WORKROOT / "nxmcp" / "stop.flag"),
    "descriptor_path": None,
    "enable_experimental": True,
    "enable_journal": True,
    "max_runtime_s": 8 * 3600,
    "pump_timeout_s": 0.02,
    "idle_sleep_s": 0.01,
    "max_messages_per_cycle": 64,
    "log_file": str(WORKROOT / "nxmcp" / "bridge-gui.log"),
}

PM_REMOVE = 0x0001
WM_QUIT = 0x0012


class MSG(ctypes.Structure):
    _fields_ = [
        ("hwnd", wt.HWND),
        ("message", wt.UINT),
        ("wParam", wt.WPARAM),
        ("lParam", wt.LPARAM),
        ("time", wt.DWORD),
        ("pt", wt.POINT),
    ]


_user32 = ctypes.windll.user32
_user32.PeekMessageW.argtypes = [ctypes.POINTER(MSG), wt.HWND, wt.UINT, wt.UINT, wt.UINT]
_user32.PeekMessageW.restype = wt.BOOL
_user32.TranslateMessage.argtypes = [ctypes.POINTER(MSG)]
_user32.DispatchMessageW.argtypes = [ctypes.POINTER(MSG)]


def pump_windows_messages(max_messages: int) -> tuple[int, bool]:
    """Dispatch up to max_messages pending window messages. Returns (count, saw_wm_quit)."""
    msg = MSG()
    handled = 0
    while handled < max_messages and _user32.PeekMessageW(ctypes.byref(msg), None, 0, 0, PM_REMOVE):
        if msg.message == WM_QUIT:
            return handled, True
        _user32.TranslateMessage(ctypes.byref(msg))
        _user32.DispatchMessageW(ctypes.byref(msg))
        handled += 1
    return handled, False


def load_config() -> dict:
    cfg = dict(DEFAULTS)
    if CONFIG_PATH.is_file():
        cfg.update(json.loads(CONFIG_PATH.read_text(encoding="utf-8")))
    return cfg


def other_bridge_alive(descriptor_path: Path) -> str | None:
    try:
        data = json.loads(descriptor_path.read_text(encoding="utf-8"))
        with socket.create_connection((data["host"], int(data["port"])), timeout=1.0):
            return f"pid {data.get('pid')} port {data.get('port')}"
    except Exception:
        return None


def main() -> None:
    cfg = load_config()
    log_path = Path(cfg["log_file"])
    log_path.parent.mkdir(parents=True, exist_ok=True)

    def log(message: str) -> None:
        line = f"[{time.strftime('%Y-%m-%d %H:%M:%S')}] {message}"
        print(line)
        with open(log_path, "a", encoding="utf-8") as handle:
            handle.write(line + "\n")

    src = Path(cfg["nx_mcp_src"])
    if str(src) not in sys.path:
        sys.path.insert(0, str(src))
    os.environ["NX_MCP_WORKSPACE"] = cfg["workspace"]
    os.environ["NX_MCP_ALLOW_UNVERIFIED_PYTHON_BRIDGE"] = "1"
    os.environ["NX_MCP_ENABLE_EXPERIMENTAL"] = "1" if cfg["enable_experimental"] else "0"
    os.environ["NX_MCP_ENABLE_JOURNAL"] = "1" if cfg["enable_journal"] else "0"

    from nx_mcp.bridge import default_descriptor_path
    from nx_mcp.nx_bridge import pump_bridge, start_bridge, stop_bridge

    descriptor_path = Path(cfg["descriptor_path"]) if cfg["descriptor_path"] else default_descriptor_path()
    alive = other_bridge_alive(descriptor_path)
    if alive:
        log(f"another NX MCP bridge is already serving ({alive}); stop it first (stop-bridge.ps1)")
        return

    stop_file = Path(cfg["stop_file"])
    stop_file.unlink(missing_ok=True)
    descriptor = start_bridge(cfg["workspace"], descriptor_path=descriptor_path, allow_unverified_threading=True)
    log(f"NX MCP GUI bridge started on {descriptor.host}:{descriptor.port} ({descriptor.nx_version}); "
        f"stop file: {stop_file}; max runtime {cfg['max_runtime_s']} s")
    deadline = time.monotonic() + float(cfg["max_runtime_s"])
    requests = 0
    try:
        while time.monotonic() < deadline:
            if stop_file.exists():
                log("stop file found")
                break
            handled = pump_bridge(timeout=float(cfg["pump_timeout_s"]))
            requests += handled
            messages, quitting = pump_windows_messages(int(cfg["max_messages_per_cycle"]))
            if quitting:
                log("WM_QUIT received; leaving")
                break
            if not handled and not messages:
                time.sleep(float(cfg["idle_sleep_s"]))
        else:
            log("max runtime reached")
    finally:
        stop_bridge()
        log(f"NX MCP GUI bridge stopped after {requests} request(s)")


if __name__ == "__main__":
    main()
