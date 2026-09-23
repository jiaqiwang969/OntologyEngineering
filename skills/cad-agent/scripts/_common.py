#!/usr/bin/env python3
"""Shared runtime for the cad-agent Fusion job tools.

Everything in this module is deliberately dependency-free (standard library
only) so the tools run under the skill venv, the CadQuery venv, or the system
python3.  It carries the parts that every ad-hoc driver of 2026-09-05/06
re-implemented by hand, and that were the actual source of the failures:

  * transport-safe script rendering (ASCII-only literals, short source lines),
  * one guarded call at a time with an explicit result taxonomy,
  * retry only for guard-layer rejections that happened before any upstream
    request, fail-fast for everything else,
  * per-call receipts and a resumable state file.

Result classification and retry policy are NOT defined here.  They come from
``fusion_ops`` (the tested calling layer that ``fusion_call.py`` itself imports),
so the caller and these drivers can never disagree about what a result means:

    from fusion_ops import classify_report, RetryPolicy, TAXONOMY, EXIT_CODES

This module only adds the three driver-local pseudo-classifications that are not
caller results at all (``dry_run``, ``refused_busy``, ``refused_unsafe``) and the
driver-level exit-code map (see ``DRIVER_EXIT_CODES``).

Two line limits, deliberately different:

* ``FORMAT_MAX_LINE`` (400) is this module's **generator format target**.  Every
  literal we emit goes through ``py_json(indent=1)``, which keeps lines far below
  it; a line above it is reported as a format warning so a hand-written template
  cannot drift.
* ``fusion_ops.MAX_SCRIPT_LINE`` (2000), re-exported here as
  ``TRANSPORT_MAX_LINE``, is the **hard rejection threshold** shared with the
  caller.  Fusion's ``mcp_execute_script`` has been observed to accept a script
  whose longest line was ~8 KB, report ``success=true`` in 29 ms and execute
  nothing; we fail closed above the shared cap rather than put such a line on
  the wire.
"""

from __future__ import annotations

import json
import os
import re
import subprocess
import sys
import time
from dataclasses import dataclass, field, asdict
from pathlib import Path
from typing import Any, Callable, Iterable, Sequence

# fusion_ops.py ships next to this file in the skill's scripts/ directory; fall
# back to the installed skill when these tools are run from a proposal tree.
sys.path.insert(0, str(Path(__file__).resolve().parent))
try:
    from fusion_ops import (  # noqa: E402
        EXIT_CODES,
        MAX_SCRIPT_LINE as TRANSPORT_MAX_LINE,
        RetryPolicy,
        TAXONOMY,
        CallOutcome,
        classify_report,
        last_json_line,
    )
except ImportError:  # pragma: no cover - exercised only on a partial install
    _installed = Path(
        os.environ.get("CAD_AGENT_SKILL_DIR", str(Path(__file__).resolve().parents[1]))
    ) / "scripts"
    # APPEND, never insert: prepending the installed scripts directory would
    # shadow the sibling modules of whichever tree this file was loaded from
    # (it silently made the tests import the installed fusion_preflight.py).
    sys.path.append(str(_installed))
    from fusion_ops import (  # noqa: E402
        EXIT_CODES,
        MAX_SCRIPT_LINE as TRANSPORT_MAX_LINE,
        RetryPolicy,
        TAXONOMY,
        CallOutcome,
        classify_report,
        last_json_line,
    )

# --------------------------------------------------------------------------
# Locations
# --------------------------------------------------------------------------

DEFAULT_SKILL_DIR = Path(__file__).resolve().parents[1]


def skill_dir() -> Path:
    """Canonical cad-agent skill directory (override with CAD_AGENT_SKILL_DIR)."""
    return Path(os.environ.get("CAD_AGENT_SKILL_DIR", str(DEFAULT_SKILL_DIR)))


def fusion_call_script() -> Path:
    return skill_dir() / "scripts" / "fusion_call.py"


def skill_python() -> Path:
    return skill_dir() / ".venv" / "bin" / "python"


def state_root() -> Path:
    """Canonical guard safety state root.  Never per-project: a new cell must
    be able to see the in-flight/retirement markers of an older PID."""
    return Path(
        os.environ.get(
            "FUSION_CAD_STATE_DIR",
            str(Path.home() / ".local/state/cad-agent-fusion-mcp/local"),
        )
    )


PEEKABOO_DEFAULT = os.environ.get("FUSION_CAD_PEEKABOO", "/opt/homebrew/bin/peekaboo")
UPSTREAM_DEFAULT = os.environ.get("FUSION_CAD_UPSTREAM", "http://127.0.0.1:27182/mcp")
REQUIRED_PORT = 27182

# The runtime guard caps both budgets at 300 s, and in lazy activation the
# FIRST upstream request runs under the *startup* deadline -- so both are set.
GUARD_CALL_CAP_SECONDS = 300


# --------------------------------------------------------------------------
# Small process helpers
# --------------------------------------------------------------------------


@dataclass
class Cmd:
    argv: list[str]
    returncode: int
    stdout: str
    stderr: str
    elapsed: float

    @property
    def ok(self) -> bool:
        return self.returncode == 0


def run(argv: Sequence[str], timeout: float = 30.0, env: dict | None = None) -> Cmd:
    """Run one external command, never raising on a non-zero exit."""
    started = time.monotonic()
    try:
        proc = subprocess.run(
            list(argv),
            capture_output=True,
            text=True,
            timeout=timeout,
            env=env,
        )
        return Cmd(list(argv), proc.returncode, proc.stdout, proc.stderr, time.monotonic() - started)
    except subprocess.TimeoutExpired as exc:
        return Cmd(list(argv), 124, exc.stdout or "", f"timeout after {timeout}s", time.monotonic() - started)
    except OSError as exc:
        return Cmd(list(argv), 127, "", str(exc), time.monotonic() - started)


def run_json(argv: Sequence[str], timeout: float = 30.0) -> tuple[dict | None, Cmd]:
    completed = run(argv, timeout=timeout)
    if not completed.ok:
        return None, completed
    try:
        payload = json.loads(completed.stdout)
    except ValueError:
        return None, completed
    return (payload if isinstance(payload, dict) else None), completed


class Log:
    """Timestamped log to stdout and one append-only file."""

    def __init__(self, path: Path | None = None, echo: bool = True) -> None:
        self.path = path
        self.echo = echo
        if path is not None:
            path.parent.mkdir(parents=True, exist_ok=True)

    def __call__(self, message: str) -> None:
        line = "[%s] %s" % (time.strftime("%H:%M:%S"), message)
        if self.echo:
            print(line, flush=True)
        if self.path is not None:
            with open(self.path, "a", encoding="utf-8") as handle:
                handle.write(line + "\n")


# --------------------------------------------------------------------------
# Transport-safe script rendering
#
# Two independent 2026-09-06 failures live here:
#   * a raw UTF-8 STEP path inside a fusion_mcp_execute script came back as
#     "3 : The selected file does not exist." (transport mojibake), so every
#     literal that leaves this process must be pure ASCII;
#   * a ~7 KB single-line PARAMS literal produced delivered/success=true with
#     an EMPTY message after 29 ms -- nothing ran -- so generated literals must
#     be multi-line and every source line must stay short.
# --------------------------------------------------------------------------

#: Generator FORMAT target.  Not a rejection threshold: see the module
#: docstring.  py_json(indent=1) keeps generated literals far below it.
FORMAT_MAX_LINE = 400

#: Hard rejection threshold, imported from fusion_ops so the caller and these
#: drivers share one cap (fusion_ops.MAX_SCRIPT_LINE == 2000).
assert TRANSPORT_MAX_LINE >= FORMAT_MAX_LINE, "transport cap must not be below the format target"

PLACEHOLDER_PATTERN = re.compile(r"__[A-Z][A-Z0-9_]*__")


class RenderError(RuntimeError):
    pass


def ascii_escape(text: str) -> str:
    """Escape any non-ASCII character as a Python-source-safe \\uXXXX escape."""
    return text.encode("ascii", "backslashreplace").decode("ascii")


def py_str(value: object) -> str:
    """ASCII-only Python string literal for embedding in a generated script."""
    return ascii_escape(repr(str(value)))


def py_json(value: object, indent: int = 1) -> str:
    """ASCII-only, multi-line JSON literal (valid Python for list/dict/str/num).

    ``indent`` is never None: a single-line literal is exactly what silently
    broke the Fusion script executor.
    """
    return json.dumps(value, ensure_ascii=True, indent=indent)


def assert_transport_safe(
    script: str, label: str = "script", *, format_limit: int = FORMAT_MAX_LINE
) -> list[str]:
    """Fail closed on anything unsafe to put on the wire; warn on format drift.

    Raises ``RenderError`` for a non-ASCII character or for a line longer than
    the shared transport cap ``TRANSPORT_MAX_LINE``.  Returns (does not raise)
    a list of format warnings for lines longer than ``format_limit``: a line in
    that band is legal but means a template stopped using py_json(indent=1).
    """
    bad = [index for index, ch in enumerate(script) if ord(ch) > 127]
    if bad:
        raise RenderError(
            "%s contains %d non-ASCII characters (first at offset %d); "
            "escape them with ascii_escape()/py_str()" % (label, len(bad), bad[0])
        )
    warnings: list[str] = []
    for number, line in enumerate(script.splitlines(), start=1):
        if len(line) > TRANSPORT_MAX_LINE:
            raise RenderError(
                "%s line %d is %d chars (> the shared transport cap %d): a long "
                "generated literal makes Fusion return an empty message in ~30 ms "
                "without running the script; render literals with py_json(indent=1)"
                % (label, number, len(line), TRANSPORT_MAX_LINE)
            )
        if len(line) > format_limit:
            warnings.append(
                "%s line %d is %d chars (> the %d format target): render generated "
                "literals with py_json(indent=1)" % (label, number, len(line), format_limit)
            )
    return warnings


def render_template(template: str, mapping: dict[str, str], label: str = "script") -> str:
    """Substitute ``__KEY__`` placeholders and prove the result is transport-safe."""
    script = template
    for key, value in mapping.items():
        token = "__%s__" % key
        if token not in script:
            raise RenderError("%s: template has no placeholder %s" % (label, token))
        script = script.replace(token, value)
    left = PLACEHOLDER_PATTERN.findall(script)
    # Fusion API names never look like __FOO__, so any survivor is a bug.
    left = [item for item in left if item not in {"__NAME__", "__MAIN__"}]
    if left:
        raise RenderError("%s: unsubstituted placeholders %s" % (label, sorted(set(left))))
    for warning in assert_transport_safe(script, label):
        print("WARN %s" % warning, file=sys.stderr)
    return script


def load_template(name: str, search: Iterable[Path] | None = None) -> str:
    """Read one script template from ``templates/`` next to this module."""
    roots = list(search) if search is not None else [Path(__file__).resolve().parent / "templates"]
    for root in roots:
        candidate = Path(root) / name
        if candidate.is_file():
            return candidate.read_text(encoding="utf-8")
    raise FileNotFoundError("template %s not found in %s" % (name, [str(r) for r in roots]))


# --------------------------------------------------------------------------
# Result taxonomy -- owned by fusion_ops, not by this module
# --------------------------------------------------------------------------

#: Driver-local pseudo-classifications.  These never come back from the caller:
#: they describe a call this driver decided NOT to make.
LOCAL_CLASSIFICATIONS = ("dry_run", "refused_busy", "refused_unsafe")

#: Driver exit codes.  Deliberately NOT fusion_ops.EXIT_CODES: the caller exits
#: 0 for inner_business_error/inner_script_error/guard_* because "delivered,
#: read the payload" is a successful *transport* outcome.  A driver that is
#: importing 40 chunks must exit non-zero when a chunk failed, so the two maps
#: differ on purpose and both are kept visible in every receipt.
DRIVER_EXIT_CODES = {
    "delivered_ok": 0,
    "dry_run": 0,
    "transport_failure": 2,
    "inner_business_error": 3,
    "inner_script_error": 3,
    "no_output": 4,
    "response_truncated": 4,
    "upstream_serialization_error": 4,
    "refused_unsafe": 5,
    "guard_transient": 6,
    "upstream_timeout": 7,
    "guard_rejected": 8,
    "refused_busy": 9,
}

# Fail loudly at import time if fusion_ops grows a classification we do not map,
# instead of silently bucketing a new class into a default exit code.
_unmapped = set(TAXONOMY) - set(DRIVER_EXIT_CODES)
if _unmapped:
    raise RuntimeError(
        "fusion_ops.TAXONOMY has classifications this driver does not map: %s"
        % sorted(_unmapped)
    )

#: One shared retry policy object.  fusion_ops.RetryPolicy refuses to be
#: constructed with any class other than guard_transient, so a driver cannot
#: widen the retry surface by accident.
DEFAULT_RETRY_POLICY = RetryPolicy()


@dataclass
class CallResult:
    """One driver-visible call result: a fusion_ops CallOutcome plus job context.

    ``kind`` is exactly ``CallOutcome.classification`` (fusion_ops vocabulary),
    or one of ``LOCAL_CLASSIFICATIONS`` for a call this driver did not make.
    """

    kind: str
    stage: str
    tool: str
    exit_code: int
    elapsed_s: float
    duration_ms: float | None = None
    inner: dict | None = None
    text: str = ""
    detail: str = ""
    attempt: int = 1
    outcome: CallOutcome | None = None

    @property
    def ok(self) -> bool:
        return self.kind == "delivered_ok"

    @property
    def retryable(self) -> bool:
        return DEFAULT_RETRY_POLICY.classes.__contains__(self.kind)

    @property
    def caller_exit_code(self) -> int | None:
        """What fusion_call.py itself exited with, for the receipt."""
        return EXIT_CODES.get(self.kind)

    def summary(self, limit: int = 400) -> str:
        body = self.detail or (json.dumps(self.inner, ensure_ascii=False) if self.inner else self.text)
        return "%s (%.0fs, call %s ms): %s" % (
            self.kind,
            self.elapsed_s,
            "%.0f" % self.duration_ms if self.duration_ms is not None else "?",
            (body or "")[:limit],
        )

    @classmethod
    def local(cls, kind: str, stage: str, tool: str, *, detail: str = "",
              attempt: int = 1) -> "CallResult":
        if kind not in LOCAL_CLASSIFICATIONS:
            raise ValueError("not a driver-local classification: %r" % kind)
        return cls(kind=kind, stage=stage, tool=tool, exit_code=DRIVER_EXIT_CODES[kind],
                   elapsed_s=0.0, detail=detail, attempt=attempt)

    @classmethod
    def from_outcome(cls, outcome: CallOutcome, *, stage: str, tool: str,
                     elapsed_s: float, attempt: int = 1) -> "CallResult":
        return cls(
            kind=outcome.classification,
            stage=stage,
            tool=tool,
            exit_code=DRIVER_EXIT_CODES[outcome.classification],
            elapsed_s=elapsed_s,
            duration_ms=outcome.duration_ms,
            inner=outcome.inner,
            text=outcome.raw_text,
            detail=outcome.message,
            attempt=attempt,
            outcome=outcome,
        )


def _outcome_from_caller_json(body: dict) -> CallOutcome | None:
    """Rebuild a CallOutcome from what fusion_call.py already printed.

    The merged caller prints ``CallOutcome.to_json()`` on stdout for every
    result, so the classification is made exactly once, inside fusion_ops.
    """
    classification = body.get("classification")
    if classification not in TAXONOMY:
        return None
    return CallOutcome(
        classification=classification,
        exit_code=EXIT_CODES[classification],
        message=str(body.get("message", "")),
        payload=body.get("payload") if isinstance(body.get("payload"), dict) else None,
        raw_text=str(body.get("raw_text", "")),
        inner=body.get("inner") if isinstance(body.get("inner"), dict) else None,
        duration_ms=body.get("duration_ms"),
        delivery_ack_confirmed=body.get("delivery_ack_confirmed"),
        cell_stderr_tail=str(body.get("cell_stderr_tail", "")),
        stop_reason=body.get("stop_reason"),
        runtime_error=body.get("runtime_error") if isinstance(body.get("runtime_error"), dict) else None,
        detail=body.get("detail") if isinstance(body.get("detail"), dict) else {},
        attempts=int(body.get("attempts", 1) or 1),
    )


def _legacy_report(body: dict) -> dict:
    """Wrap a pre-taxonomy caller payload in a minimal acceptance report.

    Only reached when an older fusion_call.py is installed; classification still
    happens in fusion_ops.classify_report, never here.
    """
    return {
        "calls": [{
            "layers": {"transport_protocol": {
                "status": "success" if body.get("status") == "delivered" else "failure"}},
            "duration_ms": body.get("duration_ms"),
            "delivery_ack": {"confirmed": body.get("delivery_ack_confirmed")},
            "response": {"payload": body.get("payload")},
        }],
    }


def classify_caller_output(
    *,
    stage: str,
    tool: str,
    arguments: dict | None,
    exit_code: int,
    elapsed_s: float,
    stdout: str,
    stderr: str,
) -> CallResult:
    """Map one fusion_call.py invocation onto fusion_ops' taxonomy.

    Order: the caller's own printed classification (normal path) -> a raw
    acceptance report -> the caller's stderr JSON (it died before writing a
    report) -> transport_failure.
    """
    if exit_code == 124:
        outcome = CallOutcome(
            classification="upstream_timeout",
            exit_code=EXIT_CODES["upstream_timeout"],
            message="the caller was killed by its own wall-clock timeout. AMBIGUOUS: "
                    "the upstream request may have committed inside Fusion. Do not "
                    "retry, do not probe this Fusion PID.",
            cell_stderr_tail=stderr[-2000:],
        )
        return CallResult.from_outcome(outcome, stage=stage, tool=tool, elapsed_s=elapsed_s)

    for blob in (stdout, stderr):
        if not blob.strip():
            continue
        try:
            body = json.loads(blob)
        except ValueError:
            continue
        if not isinstance(body, dict):
            continue
        outcome = _outcome_from_caller_json(body)
        if outcome is not None:
            return CallResult.from_outcome(outcome, stage=stage, tool=tool, elapsed_s=elapsed_s)
        if "calls" in body:
            outcome = classify_report(body, tool=tool, arguments=arguments)
            return CallResult.from_outcome(outcome, stage=stage, tool=tool, elapsed_s=elapsed_s)
        if "payload" in body or "status" in body:
            outcome = classify_report(_legacy_report(body), tool=tool, arguments=arguments)
            return CallResult.from_outcome(outcome, stage=stage, tool=tool, elapsed_s=elapsed_s)

    outcome = CallOutcome(
        classification="transport_failure",
        exit_code=EXIT_CODES["transport_failure"],
        message="the caller exited %d without a classifiable result" % exit_code,
        cell_stderr_tail=(stderr or stdout)[-2000:],
    )
    return CallResult.from_outcome(outcome, stage=stage, tool=tool, elapsed_s=elapsed_s)


# --------------------------------------------------------------------------
# Guarded caller
# --------------------------------------------------------------------------


def another_cell_running() -> str:
    """Non-empty when a guarded cell / acceptance client is already running.

    One-shot calls are serialised by the guard's owner lease anyway, but a
    second driver in the same second produces confusing evidence, so every
    tool refuses instead of queuing.
    """
    completed = run(
        ["pgrep", "-fl", r"fusion_mcp_proxy\.(cell_server|acceptance_client)"],
        timeout=10.0,
    )
    if completed.returncode == 0 and completed.stdout.strip():
        return completed.stdout.strip()
    return ""


def fusion_pids() -> list[int]:
    completed = run(["pgrep", "-f", "MacOS/Autodesk Fusion$"], timeout=10.0)
    if completed.returncode != 0:
        return []
    return [int(line) for line in completed.stdout.split() if line.strip().isdigit()]


# --------------------------------------------------------------------------
# Guard markers
#
# A marker means the CLIENT lost the outcome of one request; it is not evidence
# that Fusion failed.  Two things can make it stop blocking:
#
#   * ``fusion_recovery.py recover --apply`` RENAMES it to
#     ``<stem>.superseded-<stamp>.json`` -- the file stays as history and must
#     never be globbed as an active marker again;
#   * a ``fusion-mcp.completion-evidence.v1`` receipt
#     (``fusion-mcp-completion-evidence-<uid>-<pid>-<hash>.json``) lifts a
#     still-named marker, which is exactly what the guard's ``evidence-gated``
#     enforcement does.
#
# The lifting predicate is NOT re-implemented here: we call the guard's own
# ``applog_evidence.find_lifting_receipt`` so the preflight can never bless a
# state the guard would reject, or block one it would accept.
# --------------------------------------------------------------------------

#: A marker file whose name contains this token is inactive history.
SUPERSEDED_MARKER_TOKEN = ".superseded-"

#: Only these marker kinds can be lifted by evidence (mirrors
#: applog_evidence.LIFTABLE_MARKER_KINDS); a quarantine never is.
LIFTABLE_MARKER_KINDS = {"retirement": "retirement", "in-flight": "inflight"}

RECOVERY_HINT = "%s/scripts/fusion_recovery.py evidence --marker %s"


def is_superseded_marker(path: Path) -> bool:
    return SUPERSEDED_MARKER_TOKEN in Path(path).name


def _markers(pattern: str, pid: int, root: Path | None, include_superseded: bool) -> list[Path]:
    base = root or state_root()
    if not Path(base).is_dir():
        return []
    found = sorted(Path(base).glob(pattern % pid))
    if include_superseded:
        return found
    return [path for path in found if not is_superseded_marker(path)]


def retirement_markers(pid: int, root: Path | None = None, *,
                       include_superseded: bool = False) -> list[Path]:
    return _markers("fusion-mcp-retired-*-%d-*.json", pid, root, include_superseded)


def inflight_markers(pid: int, root: Path | None = None, *,
                     include_superseded: bool = False) -> list[Path]:
    return _markers("fusion-mcp-inflight-*-%d-*.json", pid, root, include_superseded)


def quarantine_markers(pid: int, root: Path | None = None, *,
                       include_superseded: bool = False) -> list[Path]:
    return _markers("fusion-mcp-quarantined-*-%d-*.json", pid, root, include_superseded)


def superseded_markers(pid: int, root: Path | None = None) -> list[Path]:
    """Inactive history: markers a recovery already superseded."""
    base = root or state_root()
    if not Path(base).is_dir():
        return []
    return sorted(path for path in Path(base).glob("fusion-mcp-*-%d-*.json" % pid)
                  if is_superseded_marker(path))


def completion_evidence_receipts(pid: int, root: Path | None = None) -> list[Path]:
    base = root or state_root()
    if not Path(base).is_dir():
        return []
    return sorted(Path(base).glob("fusion-mcp-completion-evidence-*-%d-*.json" % pid))


_LIFT_PROBE = r"""
import json, sys
from pathlib import Path
from fusion_mcp_proxy.runtime_guard import MacOSProcessStartTokenProbe
from fusion_mcp_proxy.applog_evidence import (
    completion_receipt_path,
    find_lifting_receipt,
)

pid = int(sys.argv[1])
state = Path(sys.argv[2])
start_time = sys.argv[3]
targets = json.loads(sys.argv[4])
token = MacOSProcessStartTokenProbe()(pid)
receipt_path = completion_receipt_path(state, pid, token)
out = {"start_token": token, "receipt_path": str(receipt_path),
       "receipt_exists": receipt_path.is_file(), "lifted": {}}
for kind, path in targets:
    found = find_lifting_receipt(
        state, pid=pid, start_time=start_time, start_token=token,
        marker_path=Path(path), kind=kind,
    )
    out["lifted"][path] = None if found is None else {
        "receipt": str(receipt_path), "verdict": found.get("verdict"),
        "binding": found.get("binding"),
    }
print(json.dumps(out))
"""


def lifting_receipts(
    pid: int,
    targets: Sequence[tuple[str, Path]],
    *,
    root: Path | None = None,
    start_time: str,
    timeout: float = 60.0,
) -> tuple[dict[str, dict | None], str]:
    """Ask the guard's own evidence module which markers are lifted.

    Returns ``({marker_path_str: receipt_info_or_None}, note)``.  ``note`` is
    empty on the authoritative path and explains the degradation otherwise: the
    start token needs macOS libproc, which only the skill venv exposes, so
    without it we cannot verify the receipt binds THIS incarnation of the PID
    and must keep the marker blocking (fail closed).
    """
    liftable = [(kind, str(path)) for kind, path in targets
                if kind in LIFTABLE_MARKER_KINDS]
    if not liftable:
        return {}, ""
    python = skill_python()
    if not python.exists():
        return ({str(path): None for _kind, path in targets},
                "skill venv missing: cannot verify the process start token, so a "
                "completion-evidence receipt cannot be honoured here")
    completed = run(
        [str(python), "-c", _LIFT_PROBE, str(pid), str(root or state_root()),
         start_time, json.dumps(liftable)],
        timeout=timeout,
    )
    if not completed.ok:
        return ({str(path): None for _kind, path in targets},
                "evidence check failed: %s" % (completed.stderr or completed.stdout)[-200:])
    try:
        payload = json.loads(completed.stdout)
    except ValueError:
        return ({str(path): None for _kind, path in targets},
                "evidence check returned no JSON")
    return payload.get("lifted", {}), ""


@dataclass
class MarkerState:
    """What the guard's markers say about one live Fusion PID."""

    pid: int
    active_retirement: list[Path] = field(default_factory=list)
    active_inflight: list[Path] = field(default_factory=list)
    active_quarantine: list[Path] = field(default_factory=list)
    superseded: list[Path] = field(default_factory=list)
    receipts: list[Path] = field(default_factory=list)
    lifted: dict[str, dict] = field(default_factory=dict)
    blocking: list[Path] = field(default_factory=list)
    note: str = ""

    @property
    def usable(self) -> bool:
        return not self.blocking

    def recovery_hint(self, skill: Path | None = None) -> str:
        if not self.blocking:
            return ""
        target = self.blocking[0]
        if is_quarantine(target):
            return ("this is a QUARANTINE marker: evidence cannot lift it; "
                    "restart Fusion")
        return RECOVERY_HINT % (skill or skill_dir(), target)


def is_quarantine(path: Path) -> bool:
    return Path(path).name.startswith("fusion-mcp-quarantined-")


def marker_state(pid: int, root: Path | None = None, *,
                 start_time: str | None = None) -> MarkerState:
    """Active markers for ``pid``, with evidence-lifted ones removed.

    ``.superseded-`` files never count as active.  A still-named retirement or
    in-flight marker is lifted only when the guard's own
    ``find_lifting_receipt`` accepts it for this exact PID + start time +
    start token.
    """
    base = root or state_root()
    state = MarkerState(
        pid=pid,
        active_retirement=retirement_markers(pid, base),
        active_inflight=inflight_markers(pid, base),
        active_quarantine=quarantine_markers(pid, base),
        superseded=superseded_markers(pid, base),
        receipts=completion_evidence_receipts(pid, base),
    )
    targets: list[tuple[str, Path]] = (
        [("retirement", p) for p in state.active_retirement]
        + [("in-flight", p) for p in state.active_inflight]
    )
    if targets and start_time:
        lifted, note = lifting_receipts(pid, targets, root=base, start_time=start_time)
        state.note = note
        for path_text, info in lifted.items():
            if info:
                state.lifted[path_text] = info
    elif targets and not start_time:
        state.note = "no process start time available; markers kept blocking"

    # A quarantine is never liftable by evidence.
    state.blocking = (
        list(state.active_quarantine)
        + [p for p in state.active_retirement if str(p) not in state.lifted]
        + [p for p in state.active_inflight if str(p) not in state.lifted]
    )
    return state


class GuardedCaller:
    """One guarded one-shot Fusion call at a time, with receipts.

    Every call writes, under ``out_dir``:
        <stage>.py            the exact script sent (execute calls only)
        <stage>.args.json     the exact tool arguments
        <stage>.result.json   the caller's stdout
        <stage>.stderr        the caller's stderr
        <stage>.receipt.json  classification, timings, attempt count
    """

    def __init__(
        self,
        out_dir: Path,
        *,
        log: Log | None = None,
        peekaboo_mode: str = "local",
        timeout: float = float(GUARD_CALL_CAP_SECONDS),
        settle_seconds: float = 8.0,
        max_transient_retries: int = 6,
        transient_backoff: float = 45.0,
        dry_run: bool = False,
        static_check: bool = True,
    ) -> None:
        self.out_dir = Path(out_dir)
        self.out_dir.mkdir(parents=True, exist_ok=True)
        self.log = log or Log(self.out_dir / "calls.log")
        self.peekaboo_mode = peekaboo_mode
        self.timeout = float(timeout)
        self.settle_seconds = float(settle_seconds)
        # fusion_ops.RetryPolicy counts total launches and refuses any class
        # other than guard_transient, so the retry surface cannot widen here.
        self.retry_policy = RetryPolicy(attempts=int(max_transient_retries) + 1,
                                        delay_seconds=float(transient_backoff))
        self.max_transient_retries = int(max_transient_retries)
        self.transient_backoff = float(transient_backoff)
        self.dry_run = bool(dry_run)
        self.static_check = bool(static_check)
        self.calls: list[dict] = []

    # -- environment ------------------------------------------------------

    def env(self) -> dict:
        environment = dict(os.environ)
        environment["FUSION_CAD_PEEKABOO_MODE"] = self.peekaboo_mode
        # Both budgets at the guard cap: in lazy activation the first request's
        # effective deadline is the STARTUP timeout (2026-09-06: expired at
        # 240.02 s while FUSION_CAD_TIMEOUT_SECONDS was 280).
        environment["FUSION_CAD_TIMEOUT_SECONDS"] = str(GUARD_CALL_CAP_SECONDS)
        environment["FUSION_CAD_STARTUP_TIMEOUT_SECONDS"] = str(GUARD_CALL_CAP_SECONDS)
        return environment

    # -- static safety ----------------------------------------------------

    def check_script_safety(self, args_path: Path) -> tuple[bool, str]:
        python = skill_python()
        if not python.exists():
            return True, "skill venv missing; static check skipped"
        code = (
            "import json,sys\n"
            "from fusion_mcp_proxy.script_safety import reject_unsafe_execute_script\n"
            "reject_unsafe_execute_script('fusion_mcp_execute', json.load(open(sys.argv[1])))\n"
            "print('static OK')\n"
        )
        completed = run([str(python), "-c", code, str(args_path)], timeout=60.0)
        return completed.ok, (completed.stderr or completed.stdout)[-600:]

    # -- one call ---------------------------------------------------------

    def call(
        self,
        stage: str,
        tool: str,
        args: dict,
        *,
        script: str | None = None,
        retries: int | None = None,
    ) -> CallResult:
        out = self.out_dir
        args_path = out / (stage + ".args.json")
        args_path.write_text(json.dumps(args, ensure_ascii=False), encoding="utf-8")
        if script is not None:
            (out / (stage + ".py")).write_text(script, encoding="utf-8")

        if tool == "fusion_mcp_execute" and self.static_check:
            ok, detail = self.check_script_safety(args_path)
            if not ok:
                result = CallResult.local("refused_unsafe", stage, tool, detail=detail)
                self._receipt(result)
                return result

        if self.dry_run:
            self.log("%s: DRY-RUN, not calling Fusion (%s)" % (stage, tool))
            result = CallResult.local("dry_run", stage, tool)
            self._receipt(result)
            return result

        policy = (self.retry_policy if retries is None
                  else RetryPolicy(attempts=int(retries) + 1,
                                   delay_seconds=self.transient_backoff))
        attempt = 0
        while True:
            attempt += 1
            busy = another_cell_running()
            if busy:
                result = CallResult.local("refused_busy", stage, tool, detail=busy[:400],
                                          attempt=attempt)
                self.log("%s: REFUSED, another guarded cell is running: %s" % (stage, busy[:200]))
                self._receipt(result)
                return result

            self.log("%s: launching %s (attempt %d, mode=%s)" % (stage, tool, attempt, self.peekaboo_mode))
            started = time.monotonic()
            result_path = out / (stage + ".result.json")
            stderr_path = out / (stage + ".stderr")
            with open(result_path, "w", encoding="utf-8") as stdout_file, \
                    open(stderr_path, "w", encoding="utf-8") as stderr_file:
                proc = subprocess.run(
                    [
                        sys.executable if Path(sys.executable).name.startswith("python") else "python3",
                        "-B",
                        str(fusion_call_script()),
                        tool,
                        json.dumps(args, ensure_ascii=False),
                        "--timeout",
                        str(int(self.timeout)),
                    ],
                    stdout=stdout_file,
                    stderr=stderr_file,
                    env=self.env(),
                    cwd=str(out),
                )
            elapsed = time.monotonic() - started
            (out / (stage + ".exit")).write_text(
                "EXIT:%d elapsed:%.0fs finished:%s attempt:%d\n"
                % (proc.returncode, elapsed, time.strftime("%Y-%m-%d %H:%M:%S"), attempt),
                encoding="utf-8",
            )
            result = classify_caller_output(
                stage=stage,
                tool=tool,
                arguments=args,
                exit_code=proc.returncode,
                elapsed_s=elapsed,
                stdout=result_path.read_text(encoding="utf-8", errors="replace"),
                stderr=stderr_path.read_text(encoding="utf-8", errors="replace"),
            )
            result.attempt = attempt
            self._receipt(result)

            if result.ok:
                self.log("%s: OK %s" % (stage, result.summary()))
                return result
            # fusion_ops.RetryPolicy is the sole authority on what may be
            # retried; it has already refused anything but a pre-upstream
            # guard rejection, and classify_report has already refused to call
            # a late rejection transient (GUARD_TRANSIENT_MAX_MS).
            if result.outcome is not None and policy.should_retry(result.outcome, attempt):
                self.log(
                    "%s: %s (pre-upstream, no in-flight marker) -> wait %.0fs and retry: %s"
                    % (stage, result.kind, policy.delay_seconds, result.detail[:200])
                )
                time.sleep(policy.delay_seconds)
                continue
            self.log("%s: STOP %s" % (stage, result.summary(600)))
            return result

    def call_script(self, stage: str, script: str, *, retries: int | None = None) -> CallResult:
        assert_transport_safe(script, stage)
        args = {"featureType": "script", "object": {"script": script}}
        return self.call(stage, "fusion_mcp_execute", args, script=script, retries=retries)

    def settle(self, seconds: float | None = None) -> None:
        if self.dry_run:
            return
        time.sleep(self.settle_seconds if seconds is None else seconds)

    def _receipt(self, result: CallResult) -> None:
        record = {k: v for k, v in asdict(result).items() if k != "outcome"}
        record["at"] = time.strftime("%Y-%m-%dT%H:%M:%S")
        record["peekaboo_mode"] = self.peekaboo_mode
        record["classification"] = result.kind
        record["driver_exit_code"] = result.exit_code
        record["caller_exit_code"] = result.caller_exit_code
        if result.outcome is not None:
            record["caller_message"] = result.outcome.message
            record["cell_stderr_tail"] = result.outcome.cell_stderr_tail[-800:]
        record["schema"] = "cad-agent.fusion-call-receipt.v2"
        (self.out_dir / (result.stage + ".receipt.json")).write_text(
            json.dumps(record, ensure_ascii=False, indent=1), encoding="utf-8"
        )
        self.calls.append(record)


# --------------------------------------------------------------------------
# Resumable state
# --------------------------------------------------------------------------


class JobState:
    """Small JSON state file: what is already done, and the expected counts."""

    SCHEMA = "cad-agent.fusion-job-state.v1"

    def __init__(self, path: Path) -> None:
        self.path = Path(path)
        if self.path.exists():
            self.data = json.loads(self.path.read_text(encoding="utf-8"))
        else:
            self.data = {"schema": self.SCHEMA, "done": [], "expected_before": 0}
        self.data.setdefault("schema", self.SCHEMA)
        self.data.setdefault("done", [])
        self.data.setdefault("expected_before", 0)

    @property
    def done(self) -> list[str]:
        return self.data["done"]

    def mark_done(self, key: str, **fields: Any) -> None:
        if key not in self.data["done"]:
            self.data["done"].append(key)
        if fields:
            self.data[key] = fields
        self.save()

    def save(self) -> None:
        self.data["updated_at"] = time.strftime("%Y-%m-%dT%H:%M:%S")
        self.path.parent.mkdir(parents=True, exist_ok=True)
        self.path.write_text(json.dumps(self.data, ensure_ascii=False, indent=1), encoding="utf-8")


# --------------------------------------------------------------------------
# Present-skip / resume from a geometry readback
# --------------------------------------------------------------------------


def present_inventory(readback_result: Path | dict) -> tuple[set[str], int]:
    """Component names and occurrence count already in the document.

    Accepts either a caller result file (``<stage>.result.json``) or the inner
    readback dict itself, so a resume can be driven from evidence on disk.
    """
    if isinstance(readback_result, dict):
        inner: dict | None = readback_result
    else:
        raw = json.loads(Path(readback_result).read_text(encoding="utf-8"))
        if "component_solid_counts" in raw:
            inner = raw
        elif isinstance(raw.get("inner"), dict):
            # Current caller: fusion_ops already parsed the script's last JSON
            # line and printed it as "inner".
            inner = raw["inner"]
        else:
            # Pre-taxonomy caller: dig the script stdout out of the payload and
            # take its last JSON line (fusion_ops.last_json_line).
            payload = raw.get("payload") or {}
            content = payload.get("content") or []
            text = str(content[0].get("text", "")) if content else ""
            try:
                outer = json.loads(text) if text.strip() else {}
            except ValueError:
                outer = {}
            message = outer.get("message") if isinstance(outer, dict) else None
            if isinstance(message, dict):
                inner = message
            elif isinstance(message, str) and message.strip():
                inner = last_json_line(message)
            else:
                inner = None
    if not inner:
        raise ValueError("readback result carries no inner payload")
    names = set(inner.get("component_solid_counts", {}).keys())
    occurrences = int(inner.get("occurrences", 0))
    return names, occurrences


class ChunkSkip(RuntimeError):
    """A chunk is partially present: the cut no longer matches the document."""


def chunk_disposition(chunk_names: Sequence[str], present: set[str]) -> str:
    """``skip`` (all present), ``import`` (none present) or ``partial``.

    ``partial`` is never auto-healed: it means the manifest and the document
    disagree about the cut, and re-importing would duplicate occurrences.
    """
    if not present:
        return "import"
    already = [name for name in chunk_names if name in present]
    if not already:
        return "import"
    if len(already) == len(chunk_names):
        return "skip"
    return "partial"


# --------------------------------------------------------------------------
# caffeinate
# --------------------------------------------------------------------------


def under_caffeinate() -> bool:
    return os.environ.get("CAD_AGENT_CAFFEINATED") == "1"


def reexec_under_caffeinate(argv: Sequence[str] | None = None) -> None:
    """Re-exec this process under ``caffeinate -i`` exactly once.

    Host sleep during a guarded call retires the Fusion PID (2026-09-06 22:12,
    ``CancelledError: Cancelled via cancel scope``), which costs a Fusion
    restart.  The sleep/budget policy itself is owned elsewhere; this is only
    the mechanical assertion for the duration of one job.
    """
    if under_caffeinate():
        return
    command = list(argv) if argv is not None else [sys.executable, *sys.argv]
    os.environ["CAD_AGENT_CAFFEINATED"] = "1"
    os.execvp("caffeinate", ["caffeinate", "-i", *command])


# --------------------------------------------------------------------------
# Import cost model (chunk sizing)
# --------------------------------------------------------------------------

#: Measured on Fusion 2705.1.11 / this MacBook Pro, 2026-09-06:
#:   40 simple vehicle bodies         ~10 s   (0.25 s/body, empty document)
#:   10 side-plate/guide bodies       ~25 s   (2.5 s/body, blind cam slots)
#:  168 chain-link bodies             404 s   (2.4 s/body, 52 occurrences present)
#: The per-body cost grows with the occupancy of the target document, so the
#: model is linear in bodies with a floor that rises with existing occurrences.
IMPORT_SECONDS_PER_BODY = 2.6
IMPORT_FIXED_SECONDS = 12.0
GUARD_OVERHEAD_SECONDS = 100.0  # measured 8-100 s per guarded call

#: Policy ceiling, not a model output.  The linear model alone permits ~43
#: bodies, but the per-body cost keeps rising with the occupancy of the target
#: document (the 168-body chunk cost 2.4 s/body at 52 occurrences and had to be
#: recut at ~700), and ONE 300 s overrun costs a Fusion restart plus a manual
#: recovery.  20 is the cut that actually completed unattended on 2026-09-06
#: (``fusion_chunks_small20``).  Override deliberately with --max-bodies.
MAX_BODIES_CEILING = 20


def max_bodies_for_budget(
    *,
    call_cap_seconds: int = GUARD_CALL_CAP_SECONDS,
    guard_overhead_seconds: float = GUARD_OVERHEAD_SECONDS,
    seconds_per_body: float = IMPORT_SECONDS_PER_BODY,
    fixed_seconds: float = IMPORT_FIXED_SECONDS,
    safety: float = 0.6,
    ceiling: int | None = MAX_BODIES_CEILING,
) -> int:
    """Largest chunk to attempt in one guarded call: the linear cost model,
    clipped by the empirical ceiling.  Pass ``ceiling=None`` for the raw model.
    """
    usable = (call_cap_seconds - guard_overhead_seconds - fixed_seconds) * safety
    modelled = max(1, int(usable // seconds_per_body))
    return modelled if ceiling is None else min(modelled, ceiling)


def estimate_chunk_seconds(bodies: int, *, seconds_per_body: float = IMPORT_SECONDS_PER_BODY,
                           fixed_seconds: float = IMPORT_FIXED_SECONDS) -> float:
    return fixed_seconds + bodies * seconds_per_body


# --------------------------------------------------------------------------
# CLI helpers
# --------------------------------------------------------------------------


def die(message: str, code: int = 2) -> "NoReturn":  # type: ignore[valid-type]
    print(message, file=sys.stderr)
    raise SystemExit(code)


def exit_code_for(result: CallResult) -> int:
    """Driver exit code for one result (see DRIVER_EXIT_CODES for why it is not
    fusion_ops.EXIT_CODES)."""
    return DRIVER_EXIT_CODES.get(result.kind, 3)
