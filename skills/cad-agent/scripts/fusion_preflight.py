#!/usr/bin/env python3
"""Pre-job readiness check for guarded Fusion work -- no MCP request is sent.

Everything here is external evidence only: process table, TCP listener, guard
state markers, Peekaboo censuses, power assertions, disk.  Nothing in this file
talks to Fusion's MCP endpoint except the optional ``--health-probe`` (which is
a plain TCP connect or an HTTP GET of ``/health``, never an MCP request), and
that probe is off by default when a job may already be in flight.

    fusion_preflight.py                       # human summary, exit 3 on blockers
    fusion_preflight.py --json                # machine readable
    fusion_preflight.py --health-probe none   # skip every socket touch
    fusion_preflight.py --expect-doc FA04_V020_...   # require that document window

Exit codes: 0 = no blockers (warnings allowed), 3 = blockers, 2 = internal error.

The window classification below MIRRORS ``fusion_mcp_proxy.runtime_guard``.
Keep the constants in ``GUARD MIRROR`` in sync with the guard, or the preflight
will bless a state the guard then rejects.
"""

from __future__ import annotations

import argparse
import json
import math
import os
import re
import shutil
import socket
import stat
import subprocess
import sys
import time
from dataclasses import dataclass, field, asdict
from pathlib import Path
from typing import Any, Callable, Sequence

sys.path.insert(0, str(Path(__file__).resolve().parent))
from _common import (  # noqa: E402
    Cmd,
    PEEKABOO_DEFAULT,
    REQUIRED_PORT,
    UPSTREAM_DEFAULT,
    is_quarantine,
    is_superseded_marker,
    marker_state,
    run,
    skill_dir,
    state_root,
)

# ==========================================================================
# GUARD MIRROR -- keep in sync with fusion_mcp_proxy/runtime_guard.py
# ==========================================================================

KNOWN_WHITE_AUXILIARY_TITLE = "正在加载其他模块 - Autodesk Fusion"
KNOWN_WHITE_AUXILIARY_SIZE = (860.0, 536.0)

BLOCKING_TITLE_PARTS = (
    "正在加载", "加载中", "正在准备",
    "正在保存", "另存为", "保存为",
    "安全警告", "安全验证", "恢复文档",
    "恢复文件", "文档恢复", "正在恢复",
    "服务器警告", "服务器验证警告",
    "loading...", "loading…", "saving...", "saving…",
    "recovery documents", "document recovery", "security warning",
    "security verification", "server warning",
)
BLOCKING_TITLE_EXACT = frozenset({
    "加载", "保存", "安全", "恢复",
    "loading", "save", "save as", "saving", "security", "recovery", "recovering",
})
BLOCKING_TITLE_PREFIXES = (
    "加载", "保存", "另存为", "安全", "恢复",
    "save ", "save-", "save:", "save as ", "saving ", "security ", "security:",
    "loading ", "loading:", "preparing ", "recovery ", "recovery:", "recovering ",
)
PRIMARY_FUSION_WINDOW_TITLE_PATTERN = re.compile(
    r"^.+\s+-\s+Autodesk Fusion(?:\s+\([^()\r\n]+\))?$", re.IGNORECASE
)

PEEKABOO_CODESIGN_IDENTIFIER = "boo.peekaboo.peekaboo"
PEEKABOO_CODESIGN_TEAM = "Y5PE65HELJ"
PEEKABOO_LOCAL_ONDEMAND_BUILD = "3.2.1 (3.2.1)"
PEEKABOO_LOCAL_ONDEMAND_OPERATIONS = frozenset(
    {"permissionsStatus", "listApplications", "listWindows", "captureWindow"}
)
#: The guard pins this exact 7-token argv for the on-demand daemon.  A daemon
#: auto-spawned by an ordinary peekaboo CLI call runs `--mode auto ...
#: --idle-timeout-seconds 300` and is rejected as "not the pinned signed daemon".
PINNED_DAEMON_TAIL = ("daemon", "run", "--mode", "manual", "--bridge-socket")

BRIDGE_SOCKET = Path.home() / "Library/Application Support/Peekaboo/bridge.sock"

# ==========================================================================
# Check plumbing
# ==========================================================================

PASS, WARN, FAIL = "pass", "warn", "fail"


@dataclass
class Check:
    name: str
    status: str
    detail: str
    data: dict = field(default_factory=dict)
    remedy: str = ""


class Report:
    def __init__(self) -> None:
        self.checks: list[Check] = []
        self.started = time.time()

    def add(self, name: str, status: str, detail: str, *, data: dict | None = None,
            remedy: str = "") -> Check:
        check = Check(name, status, detail, data or {}, remedy)
        self.checks.append(check)
        return check

    @property
    def blockers(self) -> list[Check]:
        return [c for c in self.checks if c.status == FAIL]

    @property
    def warnings(self) -> list[Check]:
        return [c for c in self.checks if c.status == WARN]

    def as_dict(self) -> dict:
        return {
            "schema": "cad-agent.fusion-preflight.v1",
            "generated_at": time.strftime("%Y-%m-%dT%H:%M:%S"),
            "elapsed_s": round(time.time() - self.started, 3),
            "blocker_count": len(self.blockers),
            "warning_count": len(self.warnings),
            "ready": not self.blockers,
            "checks": [asdict(c) for c in self.checks],
        }

    def human(self) -> str:
        icon = {PASS: "PASS", WARN: "WARN", FAIL: "FAIL"}
        lines = []
        for check in self.checks:
            lines.append("%-4s %-28s %s" % (icon[check.status], check.name, check.detail))
            if check.remedy and check.status != PASS:
                lines.append("     -> %s" % check.remedy)
        lines.append("----")
        if self.blockers:
            lines.append("preflight: BLOCKED (%d blockers, %d warnings)"
                         % (len(self.blockers), len(self.warnings)))
        else:
            lines.append("preflight: READY (%d warnings)" % len(self.warnings))
        return "\n".join(lines)


# ==========================================================================
# Pure parsers (unit-testable with recorded command output)
# ==========================================================================


def parse_lsof_tcp_listener(text: str) -> list[dict]:
    """Parse ``lsof -nP -iTCP:<port> -sTCP:LISTEN`` default (non -F) output."""
    rows = []
    for line in text.splitlines():
        if not line.strip() or line.startswith("COMMAND"):
            continue
        parts = line.split()
        if len(parts) < 9 or not parts[1].isdigit():
            continue
        rows.append({"command": parts[0], "pid": int(parts[1]), "user": parts[2],
                     "name": parts[-1]})
    return rows


def parse_lsof_socket_listeners(text: str) -> list[dict]:
    """Parse ``lsof -n -U -a -F0pcufn -- <sock>`` NUL-field output.

    Fields: p<pid> c<command> u<uid> f<fd> n<name>, records separated by NUL.
    """
    rows: list[dict] = []
    current: dict = {}
    for chunk in text.replace("\n", "\0").split("\0"):
        if not chunk:
            continue
        tag, value = chunk[0], chunk[1:]
        if tag == "p":
            if current.get("pid") is not None:
                rows.append(current)
            current = {"pid": int(value) if value.isdigit() else None}
        elif tag == "c":
            current["command"] = value
        elif tag == "u":
            current["uid"] = int(value) if value.isdigit() else None
        elif tag == "n":
            current["name"] = value
    if current.get("pid") is not None:
        rows.append(current)
    return [row for row in rows if row.get("pid")]


def parse_codesign_display(text: str) -> dict:
    fields = {}
    for line in text.splitlines():
        if "=" in line:
            key, value = line.split("=", 1)
            fields.setdefault(key.strip(), value.strip())
    return fields


def codesign_identity_ok(fields: dict) -> tuple[bool, str]:
    cdhash = fields.get("CDHash", "")
    if fields.get("Identifier") != PEEKABOO_CODESIGN_IDENTIFIER:
        return False, "Identifier=%r" % fields.get("Identifier")
    if fields.get("TeamIdentifier") != PEEKABOO_CODESIGN_TEAM:
        return False, "TeamIdentifier=%r" % fields.get("TeamIdentifier")
    if re.fullmatch(r"[0-9a-f]{40,64}", cdhash) is None:
        return False, "CDHash=%r" % cdhash
    return True, cdhash[:16]


def parse_pmset_assertions(text: str) -> dict:
    """Summarise ``pmset -g assertions``: system-wide flags plus the owners of
    PreventUserIdleSystemSleep / PreventSystemSleep."""
    systemwide: dict[str, int] = {}
    owners: list[dict] = []
    in_owner_list = False
    for line in text.splitlines():
        stripped = line.strip()
        if stripped.startswith("Listed by owning process"):
            in_owner_list = True
            continue
        if not in_owner_list:
            match = re.match(r"^([A-Za-z]+)\s+(\d+)$", stripped)
            if match:
                systemwide[match.group(1)] = int(match.group(2))
            continue
        match = re.match(r"^pid (\d+)\((.+?)\): \[[^\]]*\]\s+\S+\s+(\S+)\s+named: \"(.*)\"", stripped)
        if match:
            owners.append({
                "pid": int(match.group(1)),
                "process": match.group(2),
                "assertion": match.group(3),
                "name": match.group(4),
            })
    sleep_holders = [o for o in owners
                     if o["assertion"] in {"PreventUserIdleSystemSleep", "PreventSystemSleep"}]
    return {
        "systemwide": systemwide,
        "owners": owners,
        "sleep_holders": sleep_holders,
        "caffeinate_holders": [o for o in sleep_holders if o["process"] == "caffeinate"],
    }


def window_title(row: object) -> str:
    if not isinstance(row, dict):
        return ""
    return str(row.get("title", row.get("window_title", "")) or "").strip()


def window_bounds(row: object) -> tuple[float, float, float, float] | None:
    if not isinstance(row, dict):
        return None
    raw = row.get("bounds")
    values: tuple | None = None
    if isinstance(raw, list) and len(raw) == 2 and all(
        isinstance(pair, list) and len(pair) == 2 for pair in raw
    ):
        values = (raw[0][0], raw[0][1], raw[1][0], raw[1][1])
    elif isinstance(raw, dict):
        values = (raw.get("x"), raw.get("y"), raw.get("width"), raw.get("height"))
    if values is None:
        return None
    if any(isinstance(v, bool) or not isinstance(v, (int, float)) for v in values):
        return None
    out = tuple(float(v) for v in values)
    if not all(math.isfinite(v) for v in out) or out[2] <= 0 or out[3] <= 0:
        return None
    return out  # type: ignore[return-value]


def is_blocking_title(title: str) -> bool:
    lowered = title.casefold()
    return (
        lowered in BLOCKING_TITLE_EXACT
        or any(part in lowered for part in BLOCKING_TITLE_PARTS)
        or any(lowered.startswith(prefix) for prefix in BLOCKING_TITLE_PREFIXES)
    )


def is_primary_fusion_window(row: object) -> bool:
    if not isinstance(row, dict) or row.get("isMinimized") is True:
        return False
    title = window_title(row)
    bounds = window_bounds(row)
    window_id = row.get("windowID")
    return (
        title != KNOWN_WHITE_AUXILIARY_TITLE
        and PRIMARY_FUSION_WINDOW_TITLE_PATTERN.fullmatch(title) is not None
        and not is_blocking_title(title)
        and isinstance(window_id, int) and not isinstance(window_id, bool) and window_id > 0
        and bounds is not None
        and bounds[2] > KNOWN_WHITE_AUXILIARY_SIZE[0]
        and bounds[3] > KNOWN_WHITE_AUXILIARY_SIZE[1]
    )


def classify_windows(rows: Sequence[object]) -> dict:
    """Reproduce the guard's accept/reject decision on one window census."""
    titles = [window_title(row) for row in rows]
    auxiliary = [t for t in titles if t == KNOWN_WHITE_AUXILIARY_TITLE]
    other_blocking = [t for t in titles if t != KNOWN_WHITE_AUXILIARY_TITLE and is_blocking_title(t)]
    primaries = [
        {"window_id": row.get("windowID"), "title": window_title(row), "bounds": window_bounds(row)}
        for row in rows if is_primary_fusion_window(row)
    ]
    problems: list[str] = []
    if not rows:
        problems.append("Fusion has no verified top-level windows")
    if other_blocking:
        problems.append("blocking window(s): %s" % ", ".join(sorted(set(other_blocking))))
    if len(auxiliary) > 1:
        problems.append("module-loading auxiliary is not uniquely stable (%d copies)" % len(auxiliary))
    if auxiliary and len(primaries) != 1:
        problems.append(
            "auxiliary proof needs exactly one primary Fusion document window, found %d"
            % len(primaries)
        )
    return {
        "window_count": len(rows),
        "titles": titles,
        "auxiliary_count": len(auxiliary),
        "blocking_titles": sorted(set(other_blocking)),
        "primary_windows": primaries,
        "problems": problems,
        "guard_would_accept": not problems,
    }


def parse_ps_args(text: str) -> list[str]:
    """argv tokens of ``ps -ww -p PID -o args=`` output (space separated).

    macOS gives no argv separator, so a path containing spaces cannot be split
    reliably.  The daemon argv the guard pins ends with the bridge socket path,
    which does contain spaces -- so the comparison is done on the head tokens
    plus a suffix match, never on a naive split.
    """
    return text.strip().split()


def pinned_daemon_argv_ok(args_line: str, executable: str, socket_path: str) -> tuple[bool, str]:
    """True when ``ps -o args=`` matches the exact pinned manual-daemon argv."""
    line = args_line.strip()
    expected = " ".join([executable, *PINNED_DAEMON_TAIL, socket_path])
    if line == expected:
        return True, "exact pinned manual daemon"
    if " --mode auto" in line or "--idle-timeout-seconds" in line:
        return False, "auto-spawned daemon (--mode auto): the guard rejects it and it self-exits after 5 idle minutes"
    return False, "argv is not the pinned manual daemon: %r" % line[:200]


# ==========================================================================
# Peekaboo mode probing
# ==========================================================================


@dataclass
class ModeProbe:
    mode: str
    healthy: bool
    reason: str
    elapsed_s: float = 0.0
    evidence: dict = field(default_factory=dict)


def probe_codesign(peekaboo: str, runner: Callable[..., Cmd] = run,
                   timeout: float = 5.0) -> tuple[bool, str, dict]:
    resolved = Path(peekaboo)
    if not resolved.is_absolute():
        return False, "peekaboo path is not absolute", {}
    try:
        real = resolved.resolve(strict=True)
        info = real.stat()
    except OSError as exc:
        return False, "peekaboo is unavailable: %s" % exc, {}
    if not stat.S_ISREG(info.st_mode) or info.st_uid not in {0, os.geteuid()} \
            or stat.S_IMODE(info.st_mode) & 0o022 or not os.access(real, os.X_OK):
        return False, "peekaboo ownership or mode is unsafe", {"path": str(real)}
    verify = runner([
        "/usr/bin/codesign", "--verify", "--strict", "--verbose=2", "--requirements",
        '=anchor apple generic and identifier "%s" and certificate leaf[subject.OU] = "%s"'
        % (PEEKABOO_CODESIGN_IDENTIFIER, PEEKABOO_CODESIGN_TEAM),
        str(real),
    ], timeout=timeout)
    if not verify.ok:
        return False, "codesign --verify failed: %s" % (verify.stderr or verify.stdout)[-160:], {}
    display = runner(["/usr/bin/codesign", "--display", "--verbose=4", str(real)], timeout=timeout)
    fields = parse_codesign_display(display.stdout + "\n" + display.stderr)
    ok, detail = codesign_identity_ok(fields)
    return ok, ("signer %s" % detail if ok else "signer identity not trusted: %s" % detail), {
        "resolved": str(real), "cdhash": fields.get("CDHash", "")[:16],
    }


def _peekaboo_windows(peekaboo: str, pid: int, extra: Sequence[str],
                      runner: Callable[..., Cmd], timeout: float) -> tuple[dict | None, Cmd]:
    completed = runner([
        peekaboo, "list", "windows", "--pid", str(pid),
        "--include-details", "off_screen,bounds,ids", "--json", *extra,
    ], timeout=timeout)
    if not completed.ok:
        return None, completed
    try:
        payload = json.loads(completed.stdout)
    except ValueError:
        return None, completed
    return (payload if isinstance(payload, dict) else None), completed


def probe_mode_local(peekaboo: str, pid: int, *, runner: Callable[..., Cmd] = run,
                     timeout: float = 3.0) -> ModeProbe:
    started = time.monotonic()
    payload, completed = _peekaboo_windows(peekaboo, pid, ["--no-remote"], runner, timeout)
    elapsed = time.monotonic() - started
    if payload is None or payload.get("success") is not True:
        return ModeProbe("local", False, "list windows --no-remote failed: %s"
                         % (completed.stderr or completed.stdout)[-160:], elapsed)
    rows = (payload.get("data") or {}).get("windows") or []
    if not rows:
        return ModeProbe("local", False, "census returned no windows", elapsed)
    debug = " ".join(payload.get("debug_logs") or [])
    if "local (in-process)" not in debug:
        return ModeProbe("local", False, "runtime host is not local in-process: %s" % debug[-120:], elapsed)
    return ModeProbe("local", True, "in-process census, %d windows" % len(rows), elapsed,
                     {"windows": rows})


def probe_mode_local_ondemand(peekaboo: str, pid: int, *, runner: Callable[..., Cmd] = run,
                              timeout: float = 3.0,
                              socket_path: Path | None = None) -> ModeProbe:
    started = time.monotonic()
    sock = Path(socket_path) if socket_path is not None else BRIDGE_SOCKET
    evidence: dict = {"socket": str(sock)}

    # 1. socket identity -- exactly what _local_ondemand_socket_identity requires
    try:
        meta = sock.lstat()
        parent = sock.parent.stat()
    except OSError as exc:
        return ModeProbe("local-ondemand", False,
                         "bridge socket is unavailable (%s); run: peekaboo daemon start" % exc,
                         time.monotonic() - started, evidence)
    if not stat.S_ISSOCK(meta.st_mode) or meta.st_uid != os.geteuid() \
            or stat.S_IMODE(meta.st_mode) != 0o600 or meta.st_nlink != 1:
        return ModeProbe("local-ondemand", False, "bridge socket identity or mode is unsafe",
                         time.monotonic() - started, evidence)
    if not stat.S_ISDIR(parent.st_mode) or parent.st_uid != os.geteuid() \
            or stat.S_IMODE(parent.st_mode) & 0o022:
        return ModeProbe("local-ondemand", False, "bridge socket parent directory is unsafe",
                         time.monotonic() - started, evidence)

    # 2. a listener must exist, and its argv must be the pinned manual daemon
    listener = runner(["/usr/sbin/lsof", "-n", "-U", "-a", "-F0pcufn", "--", str(sock)],
                      timeout=timeout)
    rows = parse_lsof_socket_listeners(listener.stdout)
    if not listener.ok or not rows:
        return ModeProbe(
            "local-ondemand", False,
            "bridge socket has no listener (stale socket file); run: "
            "peekaboo daemon stop && peekaboo daemon start",
            time.monotonic() - started, evidence)
    daemon_pid = rows[0]["pid"]
    evidence["daemon_pid"] = daemon_pid
    args = runner(["ps", "-ww", "-p", str(daemon_pid), "-o", "args="], timeout=timeout)
    ok, detail = pinned_daemon_argv_ok(args.stdout, peekaboo, str(sock))
    evidence["daemon_argv"] = args.stdout.strip()[:300]
    if not ok:
        return ModeProbe("local-ondemand", False,
                         "%s; run: peekaboo daemon stop && peekaboo daemon start" % detail,
                         time.monotonic() - started, evidence)

    # 3. the bridge handshake the guard pins
    status = runner([peekaboo, "bridge", "status", "--json", "--bridge-socket", str(sock)],
                    timeout=timeout)
    try:
        payload = json.loads(status.stdout) if status.ok else {}
    except ValueError:
        payload = {}
    selected = ((payload.get("data") or {}).get("selected") or {})
    handshake = selected.get("handshake") or {}
    client = (payload.get("data") or {}).get("client") or {}
    problems = []
    if selected.get("source") != "remote" or handshake.get("hostKind") != "onDemand":
        problems.append("hostKind=%r source=%r" % (handshake.get("hostKind"), selected.get("source")))
    if selected.get("socketPath") not in (None, str(sock)):
        problems.append("socketPath drifted to %r" % selected.get("socketPath"))
    if handshake.get("build") and handshake.get("build") != PEEKABOO_LOCAL_ONDEMAND_BUILD:
        problems.append("build=%r (guard pins %r)" % (handshake.get("build"),
                                                      PEEKABOO_LOCAL_ONDEMAND_BUILD))
    if client and (client.get("bundleIdentifier") != PEEKABOO_CODESIGN_IDENTIFIER
                   or client.get("teamIdentifier") != PEEKABOO_CODESIGN_TEAM):
        problems.append("client identity mismatch")
    for key in ("supportedOperations", "enabledOperations"):
        ops = handshake.get(key)
        if isinstance(ops, list) and not PEEKABOO_LOCAL_ONDEMAND_OPERATIONS.issubset(ops):
            problems.append("%s missing %s" % (
                key, sorted(PEEKABOO_LOCAL_ONDEMAND_OPERATIONS.difference(ops))))
    permissions = handshake.get("permissions") or {}
    for name in ("screenRecording", "accessibility", "postEvent"):
        if permissions and permissions.get(name) is not True:
            problems.append("permission %s not granted" % name)
    evidence["handshake"] = {k: handshake.get(k) for k in ("hostKind", "build")}
    if problems:
        return ModeProbe("local-ondemand", False, "bridge handshake: " + "; ".join(problems),
                         time.monotonic() - started, evidence)

    # 4. the cheapest positive proof: an actual census through the bridge
    census, completed = _peekaboo_windows(peekaboo, pid, ["--bridge-socket", str(sock)],
                                          runner, timeout)
    elapsed = time.monotonic() - started
    if census is None or census.get("success") is not True:
        return ModeProbe("local-ondemand", False, "census through the bridge failed: %s"
                         % (completed.stderr or completed.stdout)[-160:], elapsed, evidence)
    debug = " ".join(census.get("debug_logs") or [])
    if "remote onDemand" not in debug:
        return ModeProbe("local-ondemand", False,
                         "census was not served by the on-demand host: %s" % debug[-120:],
                         elapsed, evidence)
    rows2 = (census.get("data") or {}).get("windows") or []
    evidence["windows"] = rows2
    return ModeProbe("local-ondemand", True,
                     "pinned manual daemon pid %d, %d windows" % (daemon_pid, len(rows2)),
                     elapsed, evidence)


def probe_mode_bridge(peekaboo: str, pid: int, *, runner: Callable[..., Cmd] = run,
                      timeout: float = 3.0) -> ModeProbe:
    """mode=bridge needs the Peekaboo GUI app as the bridge host (hostKind gui).

    NOTE: this probe deliberately calls ``bridge status`` WITHOUT --no-remote
    and WITHOUT --bridge-socket, which is the one peekaboo invocation shape that
    can auto-spawn a ``--mode auto`` daemon.  It is therefore only run when
    ``--probe-bridge`` is given.
    """
    started = time.monotonic()
    status = runner([peekaboo, "bridge", "status", "--json"], timeout=timeout)
    elapsed = time.monotonic() - started
    try:
        payload = json.loads(status.stdout) if status.ok else {}
    except ValueError:
        payload = {}
    selected = ((payload.get("data") or {}).get("selected") or {})
    handshake = selected.get("handshake") or {}
    if selected.get("source") != "remote" or handshake.get("hostKind") != "gui":
        return ModeProbe("bridge", False,
                         "no GUI bridge host (hostKind=%r); start the Peekaboo app"
                         % handshake.get("hostKind"), elapsed)
    return ModeProbe("bridge", True, "GUI bridge host available", elapsed)


MODE_PREFERENCE = ("local", "local-ondemand", "bridge")


def select_peekaboo_mode(
    peekaboo: str,
    pid: int,
    *,
    runner: Callable[..., Cmd] = run,
    budget: float = 3.0,
    probe_bridge: bool = False,
    preference: Sequence[str] = MODE_PREFERENCE,
) -> tuple[str | None, list[ModeProbe], dict]:
    """Probe the modes inside one wall-clock budget and pick the best healthy one.

    Hard rule: never invoke a bare peekaboo subcommand (no --no-remote and no
    --bridge-socket) unless ``probe_bridge`` was requested -- that shape
    auto-spawns a ``--mode auto`` daemon, which then fails the guard's pinned
    manual-daemon check and self-exits after 5 idle minutes, leaving a stale
    socket.  Probing must never change the state it measures.
    """
    started = time.monotonic()
    signed_ok, signed_detail, signed_evidence = probe_codesign(
        peekaboo, runner=runner, timeout=min(budget, 5.0))
    probes: list[ModeProbe] = []
    meta = {"codesign_ok": signed_ok, "codesign_detail": signed_detail, **signed_evidence}
    if not signed_ok:
        return None, probes, meta

    for mode in preference:
        remaining = budget - (time.monotonic() - started)
        if remaining <= 0.2:
            probes.append(ModeProbe(mode, False, "probe budget exhausted"))
            continue
        step = min(remaining, budget)
        if mode == "local":
            probes.append(probe_mode_local(peekaboo, pid, runner=runner, timeout=step))
        elif mode == "local-ondemand":
            probes.append(probe_mode_local_ondemand(peekaboo, pid, runner=runner, timeout=step))
        elif mode == "bridge":
            if not probe_bridge:
                probes.append(ModeProbe("bridge", False,
                                        "not probed (needs the Peekaboo GUI app; a bare "
                                        "peekaboo call would auto-spawn an auto daemon)"))
            else:
                probes.append(probe_mode_bridge(peekaboo, pid, runner=runner, timeout=step))

    meta["elapsed_s"] = round(time.monotonic() - started, 3)
    healthy = [p.mode for p in probes if p.healthy]
    chosen = next((m for m in preference if m in healthy), None)
    return chosen, probes, meta


# ==========================================================================
# Individual checks
# ==========================================================================


def check_fusion_process(report: Report, runner: Callable[..., Cmd] = run,
                         want_pid: int | None = None) -> tuple[int | None, str | None]:
    completed = runner(["pgrep", "-f", "MacOS/Autodesk Fusion$"], timeout=10.0)
    pids = [int(x) for x in completed.stdout.split() if x.strip().isdigit()]
    if not pids:
        report.add("fusion-process", FAIL, "no Autodesk Fusion process is running",
                   remedy="start Fusion and enable its MCP endpoint on port %d" % REQUIRED_PORT)
        return None, None
    if len(pids) > 1:
        report.add("fusion-process", FAIL, "more than one Fusion process: %s" % pids,
                   data={"pids": pids},
                   remedy="quit the extra Fusion instances; the guard binds exactly one PID")
        return None, None
    pid = pids[0]
    if want_pid is not None and want_pid != pid:
        report.add("fusion-process", FAIL,
                   "expected Fusion pid %d but %d is running" % (want_pid, pid),
                   data={"pid": pid, "expected": want_pid})
        return pid, None
    started = runner(["ps", "-p", str(pid), "-o", "lstart="], timeout=10.0).stdout.strip()
    report.add("fusion-process", PASS, "pid %d, started %s" % (pid, started),
               data={"pid": pid, "start_time": started})
    return pid, started


def check_port(report: Report, fusion_pid: int | None, *, upstream: str = UPSTREAM_DEFAULT,
               health_probe: str = "tcp", runner: Callable[..., Cmd] = run) -> None:
    host_port = upstream.split("://", 1)[-1].split("/", 1)[0]
    host, _, port_text = host_port.partition(":")
    port = int(port_text or REQUIRED_PORT)
    if port != REQUIRED_PORT:
        report.add("mcp-port", FAIL,
                   "upstream port is %d, but this machine's guard requires %d" % (port, REQUIRED_PORT),
                   remedy="set Fusion's MCP port back to %d (a dynamic port such as 49892 is "
                          "rejected by the guard)" % REQUIRED_PORT)
        return
    completed = runner(["/usr/sbin/lsof", "-nP", "-iTCP:%d" % port, "-sTCP:LISTEN"], timeout=15.0)
    rows = parse_lsof_tcp_listener(completed.stdout)
    if not rows:
        report.add("mcp-port", FAIL, "nothing is listening on %s:%d" % (host, port),
                   remedy="in Fusion turn the MCP server on and pin it to port %d" % REQUIRED_PORT)
        return
    owner = rows[0]
    if fusion_pid is not None and owner["pid"] != fusion_pid:
        report.add("mcp-port", FAIL,
                   "port %d is owned by pid %d (%s), not by Fusion pid %d"
                   % (port, owner["pid"], owner["command"], fusion_pid),
                   data={"listener": owner})
        return
    detail = "%s:%d listening, owned by Fusion pid %d" % (host, port, owner["pid"])
    if health_probe == "none":
        report.add("mcp-port", PASS, detail + " (health probe skipped)", data={"listener": owner})
        return
    if health_probe == "tcp":
        try:
            socket.create_connection((host, port), 2).close()
            report.add("mcp-port", PASS, detail + ", TCP connect ok", data={"listener": owner})
        except OSError as exc:
            report.add("mcp-port", FAIL, detail + ", but TCP connect failed: %s" % exc)
        return
    # http: a plain GET of /health -- NOT an MCP request
    import urllib.request  # local import: never needed for the default paths
    try:
        with urllib.request.urlopen("http://%s:%d/health" % (host, port), timeout=3) as response:
            body = response.read(512).decode("utf-8", "replace")
        status = PASS if '"ok"' in body or '"status"' in body else WARN
        report.add("mcp-port", status, detail + ", /health -> %s" % body.strip()[:120],
                   data={"listener": owner, "health": body[:200]})
    except Exception as exc:  # noqa: BLE001 - any failure is evidence
        report.add("mcp-port", WARN, detail + ", /health probe failed: %s" % exc,
                   data={"listener": owner})


def check_guard_state(report: Report, fusion_pid: int | None, root: Path, *,
                      start_time: str | None = None) -> None:
    """Marker verdict for the live PID, honouring agent B's evidence path.

    A retirement is NOT automatically fatal any more.  ``fusion_recovery.py``
    can prove from Fusion's own AppLog that the request completed and the
    document was saved; it then writes a
    ``fusion-mcp.completion-evidence.v1`` receipt and renames the markers to
    ``*.superseded-<stamp>.json``.  Both outcomes are recognised here, using
    the guard's own ``find_lifting_receipt`` so this check can never disagree
    with what the guard will do.
    """
    if not root.is_dir():
        report.add("guard-state", FAIL, "guard state root is missing: %s" % root,
                   remedy="run the skill's setup.sh, or set FUSION_CAD_STATE_DIR to the "
                          "canonical owner-only root")
        return
    if fusion_pid is None:
        report.add("guard-state", WARN, "no Fusion pid, markers not evaluated")
        return

    state = marker_state(fusion_pid, root, start_time=start_time)
    lock = root / ("fusion-mcp-owner-%d-%d.lock" % (os.geteuid(), fusion_pid))
    data = {
        "active_retirement": [p.name for p in state.active_retirement],
        "active_inflight": [p.name for p in state.active_inflight],
        "active_quarantine": [p.name for p in state.active_quarantine],
        "superseded": [p.name for p in state.superseded],
        "completion_evidence": [p.name for p in state.receipts],
        "lifted": state.lifted,
        "blocking": [p.name for p in state.blocking],
        "owner_lock": lock.name if lock.exists() else None,
        "note": state.note,
    }

    if state.blocking:
        blocker = state.blocking[0]
        reason = ""
        try:
            reason = str(json.loads(blocker.read_text(encoding="utf-8")).get("reason", ""))[:160]
        except Exception:  # noqa: BLE001
            pass
        if is_quarantine(blocker):
            report.add("guard-state", FAIL,
                       "Fusion pid %d is QUARANTINED (%s)" % (fusion_pid, blocker.name),
                       data=data,
                       remedy="a quarantine cannot be lifted by evidence: restart Fusion; "
                              "this PID may never receive another MCP request")
            return
        kind = "RETIRED" if blocker.name.startswith("fusion-mcp-retired-") else "IN-FLIGHT"
        remedy = (
            "first try the evidence path -- it needs no restart: %s . "
            "It reads Fusion's own AppLog and, if the request provably completed "
            "and the document was saved, writes a completion-evidence receipt "
            "(add `recover --marker <path> --apply` to supersede the markers). "
            "ONLY if that refuses: quit and restart Fusion -- and note that a "
            "resume loop waiting for a NEW pid will never terminate while this "
            "pid is alive."
        ) % state.recovery_hint()
        if state.note:
            remedy += " NOTE: %s." % state.note
        report.add("guard-state", FAIL,
                   "Fusion pid %d has an ACTIVE %s marker (%s): %s"
                   % (fusion_pid, kind, blocker.name, reason),
                   data=data, remedy=remedy)
        return

    detail_bits = []
    if state.lifted:
        detail_bits.append("%d marker(s) lifted by a completion-evidence receipt"
                           % len(state.lifted))
    if state.superseded:
        detail_bits.append("%d superseded marker(s) retained as history"
                           % len(state.superseded))
    detail = "no active blocking marker for pid %d" % fusion_pid
    if detail_bits:
        detail += " (" + "; ".join(detail_bits) + ")"
    status = PASS
    remedy = ""
    if state.note:
        status = WARN
        remedy = state.note
    report.add("guard-state", status, detail, data=data, remedy=remedy)


def check_windows(report: Report, probe: ModeProbe | None, *, expect_doc: str | None) -> None:
    if probe is None or not probe.healthy:
        report.add("window-census", WARN, "no healthy Peekaboo mode; window census not evaluated")
        return
    rows = probe.evidence.get("windows") or []
    verdict = classify_windows(rows)
    titled = [t for t in verdict["titles"] if t]
    detail = "%d windows, %d titled, auxiliary=%d, primary canvases=%d" % (
        verdict["window_count"], len(titled), verdict["auxiliary_count"],
        len(verdict["primary_windows"]))
    if verdict["guard_would_accept"]:
        status, remedy = PASS, ""
    else:
        status = FAIL
        remedy = ("close the extra Fusion document window / dismiss the modal so exactly one "
                  "primary canvas remains; the guard's auxiliary proof needs it")
    check = report.add("window-census", status,
                       detail + ("" if verdict["guard_would_accept"]
                                 else " -- " + "; ".join(verdict["problems"])),
                       data=verdict, remedy=remedy)
    if expect_doc:
        found = [p for p in verdict["primary_windows"] if expect_doc in (p["title"] or "")]
        if found:
            report.add("target-document", PASS, "window for %r is open: %s"
                       % (expect_doc, found[0]["title"]))
        else:
            report.add("target-document", FAIL,
                       "no open Fusion window whose title contains %r" % expect_doc,
                       data={"primary_windows": verdict["primary_windows"]},
                       remedy="open the target document in Fusion (or let the import tool "
                              "activate it) before starting the job")


def check_power(report: Report, runner: Callable[..., Cmd] = run, *, require_caffeinate: bool = True) -> None:
    completed = runner(["pmset", "-g", "assertions"], timeout=10.0)
    if not completed.ok:
        report.add("power-assertions", WARN, "pmset failed: %s" % completed.stderr[-120:])
        return
    parsed = parse_pmset_assertions(completed.stdout)
    holders = parsed["sleep_holders"]
    caffeinated = parsed["caffeinate_holders"]
    if caffeinated:
        report.add("power-assertions", PASS,
                   "system sleep held off by caffeinate pid %s" % ", ".join(
                       str(h["pid"]) for h in caffeinated),
                   data=parsed)
        return
    status = WARN if not require_caffeinate else WARN
    report.add("power-assertions", status,
               "no caffeinate assertion; %d other sleep holders" % len(holders), data=parsed,
               remedy="run the job under `caffeinate -i` (or pass --caffeinate): host sleep "
                      "during a guarded call retires the Fusion PID")


def check_disk(report: Report, paths: Sequence[Path], min_free_gb: float) -> None:
    worst = None
    data = {}
    for path in paths:
        target = Path(path)
        while not target.exists() and target != target.parent:
            target = target.parent
        try:
            usage = shutil.disk_usage(target)
        except OSError as exc:
            data[str(path)] = "unavailable: %s" % exc
            continue
        free_gb = usage.free / (1024 ** 3)
        data[str(path)] = round(free_gb, 1)
        worst = free_gb if worst is None else min(worst, free_gb)
    if worst is None:
        report.add("disk-space", WARN, "could not measure free space", data=data)
    elif worst < min_free_gb:
        report.add("disk-space", FAIL, "only %.1f GiB free (need %.1f)" % (worst, min_free_gb),
                   data=data, remedy="free space: every chunk save writes a new Fusion version "
                                     "and the tools write per-call receipts")
    else:
        report.add("disk-space", PASS, "%.1f GiB free" % worst, data=data)


def check_busy(report: Report, runner: Callable[..., Cmd] = run) -> None:
    completed = runner(["pgrep", "-fl", r"fusion_mcp_proxy\.(cell_server|acceptance_client)"],
                       timeout=10.0)
    if completed.returncode == 0 and completed.stdout.strip():
        report.add("guarded-cell-busy", FAIL, "another guarded cell is running: %s"
                   % completed.stdout.strip().splitlines()[0][:160],
                   remedy="wait for it to finish; never run two drivers against one Fusion PID")
    else:
        report.add("guarded-cell-busy", PASS, "no guarded cell is running")


def check_step_roots(report: Report, roots: Sequence[Path]) -> None:
    if not roots:
        return
    non_ascii = [str(p) for p in roots if any(ord(ch) > 127 for ch in str(p))]
    missing = [str(p) for p in roots if not Path(p).exists()]
    if missing:
        report.add("step-paths", FAIL, "missing: %s" % ", ".join(missing))
        return
    if non_ascii:
        report.add("step-paths", PASS,
                   "%d path(s) contain non-ASCII characters; the tools embed them as "
                   "\\uXXXX escapes automatically" % len(non_ascii),
                   data={"non_ascii": non_ascii})
    else:
        report.add("step-paths", PASS, "%d path(s) present, all ASCII" % len(roots))


def check_skill(report: Report) -> None:
    root = skill_dir()
    caller = root / "scripts" / "fusion_call.py"
    verifier = root / "scripts" / "verify_fusion_runtime_wheel.py"
    python = root / ".venv" / "bin" / "python"
    if not caller.is_file():
        report.add("skill-runtime", FAIL, "caller missing: %s" % caller,
                   remedy="set CAD_AGENT_SKILL_DIR to the canonical cad-agent skill directory")
        return
    if not python.exists():
        report.add("skill-runtime", FAIL, "skill venv missing: %s" % python,
                   remedy="run %s/setup.sh once" % root)
        return
    if not verifier.is_file():
        report.add("skill-runtime", FAIL, "Fusion runtime verifier missing: %s" % verifier)
        return
    try:
        verified = subprocess.run(
            [str(python), str(verifier), "--installed"],
            capture_output=True, text=True, timeout=15, check=False,
        )
    except (OSError, subprocess.TimeoutExpired) as exc:
        report.add("skill-runtime", FAIL, "Fusion runtime verification failed: %s" % exc)
        return
    if verified.returncode != 0:
        report.add("skill-runtime", FAIL,
                   "Fusion-only runtime missing or legacy semantic module remains",
                   remedy="run %s/setup.sh after removing the legacy cad-agent package from this venv" % root)
        return
    report.add("skill-runtime", PASS, "locked Fusion-only runtime verified at %s" % root)


# ==========================================================================
# main
# ==========================================================================


def build_report(args: argparse.Namespace, runner: Callable[..., Cmd] = run) -> Report:
    report = Report()
    check_skill(report)
    pid, start_time = check_fusion_process(report, runner, want_pid=args.fusion_pid)
    check_port(report, pid, upstream=args.upstream, health_probe=args.health_probe, runner=runner)
    check_guard_state(report, pid, Path(args.state_dir), start_time=start_time)
    check_busy(report, runner)

    chosen: str | None = None
    probes: list[ModeProbe] = []
    meta: dict = {}
    healthy_probe: ModeProbe | None = None
    if pid is not None:
        chosen, probes, meta = select_peekaboo_mode(
            args.peekaboo, pid, runner=runner, budget=args.probe_budget,
            probe_bridge=args.probe_bridge)
        table = {p.mode: {"healthy": p.healthy, "reason": p.reason, "elapsed_s": round(p.elapsed_s, 3)}
                 for p in probes}
        if chosen is None:
            report.add("peekaboo-mode", FAIL,
                       "no healthy Peekaboo mode (%s)" % meta.get("codesign_detail", ""),
                       data={"modes": table, **meta},
                       remedy="peekaboo daemon stop && peekaboo daemon start, then re-run")
        else:
            report.add("peekaboo-mode", PASS,
                       "recommend --peekaboo-mode %s (probe %.2fs)" % (chosen, meta.get("elapsed_s", 0.0)),
                       data={"recommended": chosen, "modes": table, **meta})
            healthy_probe = next((p for p in probes if p.mode == chosen and p.healthy), None)
            for probe in probes:
                if not probe.healthy and probe.mode != "bridge":
                    report.add("peekaboo-%s" % probe.mode, WARN, probe.reason,
                               remedy="peekaboo daemon stop && peekaboo daemon start"
                               if probe.mode == "local-ondemand" else "")
    check_windows(report, healthy_probe, expect_doc=args.expect_doc)
    check_power(report, runner)
    disk_paths = [Path(args.state_dir), Path.home()]
    if args.out_dir:
        disk_paths.append(Path(args.out_dir))
    check_disk(report, disk_paths, args.min_free_gb)
    check_step_roots(report, [Path(p) for p in (args.step_root or [])])
    return report


def main(argv: Sequence[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--json", action="store_true", help="print the machine-readable report")
    parser.add_argument("--fusion-pid", type=int, default=None,
                        help="require this exact Fusion pid")
    parser.add_argument("--upstream", default=UPSTREAM_DEFAULT)
    parser.add_argument("--peekaboo", default=PEEKABOO_DEFAULT)
    parser.add_argument("--state-dir", default=str(state_root()))
    parser.add_argument("--probe-budget", type=float, default=3.0,
                        help="wall-clock budget for the whole Peekaboo mode probe")
    parser.add_argument("--probe-bridge", action="store_true",
                        help="also probe mode=bridge (needs the Peekaboo GUI app; a bare "
                             "peekaboo call can auto-spawn a --mode auto daemon)")
    parser.add_argument("--health-probe", choices=("none", "tcp", "http"), default="tcp")
    parser.add_argument("--min-free-gb", type=float, default=5.0)
    parser.add_argument("--out-dir", default=None, help="job output directory (disk check)")
    parser.add_argument("--step-root", action="append", default=[],
                        help="STEP file or directory the job will read (repeatable)")
    parser.add_argument("--expect-doc", default=None,
                        help="require an open Fusion window whose title contains this text")
    args = parser.parse_args(list(argv) if argv is not None else None)

    try:
        report = build_report(args)
    except Exception as exc:  # noqa: BLE001
        print("preflight internal error: %r" % exc, file=sys.stderr)
        return 2
    if args.json:
        json.dump(report.as_dict(), sys.stdout, ensure_ascii=False, indent=1)
        sys.stdout.write("\n")
    else:
        print(report.human())
    return 3 if report.blockers else 0


if __name__ == "__main__":
    sys.exit(main())
