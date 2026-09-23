#!/usr/bin/env python3
"""One-shot Fusion MCP call for skill-based (non-resident) invocation.

Spawns a fresh fusion-mcp cell, performs exactly one guarded tool call
through the acceptance client, prints the inner payload, and exits.  No
resident process, no MCP registration: nothing to leak, nothing to retire.

Usage:
    fusion_call.py <tool> '<json-arguments>' [--timeout SECONDS]
                   [--long-request] [--dirty-recovery-receipt PATH]
                   [--max-response-bytes N] [--fail-on CLASS[,CLASS...]]

Budgets.  Four nested timers govern one call and they must be strictly
ordered, innermost tightest.  On 2026-09-06 the cell budget and the acceptance
client budget were both 300 s; the outer one won the race by milliseconds and
turned a clean in-cell TimeoutError into an externally delivered
CancelledError, i.e. an ambiguous delivery and a durable PID retirement.  This
script now derives the outer budgets from the cell budget and refuses to let
them collide:

    guard/upstream operation budget   T          (cell --timeout-seconds)
    acceptance client read timeout    T + 90 s
    acceptance subprocess kill        T + 210 s
    activation budget (separate)      <= 300 s   (cell --startup-timeout-seconds)

--long-request raises T up to 1800 s for one long CAD operation (a 168-body
STEP importToTarget measured 404.6 s inside Fusion).  It changes nothing else:
still exactly one call in the plan, still one in-flight marker, still no retry,
still the same guard.  The host is additionally held awake for the duration.

Examples:
    fusion_call.py fusion_mcp_read '{"queryType": "projects"}'
    fusion_call.py fusion_mcp_read '{"queryType": "document", "operation": "recent"}'
    fusion_call.py --long-request --timeout 900 fusion_mcp_execute "$(cat args.json)"

Every result carries a stable ``classification`` from ``fusion_ops.TAXONOMY``
alongside the verbatim ``raw_text`` it was derived from, the four derived
``budgets``, and — on failure — the cell's own stderr tail, which is where the
real cause of a startup failure lives.  The legacy top-level keys ``status``,
``duration_ms``, ``delivery_ack_confirmed`` and ``payload`` are still printed
so existing drivers keep parsing.

Exit codes: 0 = delivered, read ``classification`` (``delivered_ok``,
``inner_business_error``, ``inner_script_error``, ``guard_transient``,
``guard_rejected``); 2 = transport/session failure or upstream timeout
(``transport_failure``, ``upstream_timeout``); 3 = usage error; 4 = the
transport said yes but nothing usable came back (``no_output``,
``response_truncated``, ``upstream_serialization_error``) — in particular a
script that Fusion reported as ``success=true`` while it produced no output at
all, and a reply Fusion computed but could not encode; 5 = a classification
listed in ``--fail-on``.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
import tempfile
from pathlib import Path

# The taxonomy, the no-output rule and the guard-transient patterns have exactly
# one implementation, shared with every driver through fusion_ops.
sys.path.insert(0, str(Path(__file__).resolve().parent))
from fusion_ops import EXIT_CODES, TAXONOMY, classify_report  # noqa: E402

# Interpreter resolution, in order: this canonical skill's bundled venv; an
# explicit CAD_AGENT_ROOT fallback for repository-bound work; the repository
# venv when this file itself lives at <repo>/.agents/skills/cad-agent/scripts/.
# Every machine-specific value can be overridden without editing this file.
import os
import socket

SKILL_DIR = Path(__file__).resolve().parents[1]
_root_override = os.environ.get("CAD_AGENT_ROOT")


def _venv_python(root: Path) -> Path:
    """Interpreter path inside a venv: Windows puts it in Scripts/, POSIX in bin/."""
    if os.name == "nt":
        return root / ".venv" / "Scripts" / "python.exe"
    return root / ".venv" / "bin" / "python"


if _venv_python(SKILL_DIR).exists():
    VENV_PYTHON = _venv_python(SKILL_DIR)
    # Do not leak a stale repository root into the detached cell process.
    os.environ.pop("CAD_AGENT_ROOT", None)
elif _root_override:
    VENV_PYTHON = _venv_python(Path(_root_override))
else:
    VENV_PYTHON = _venv_python(SKILL_DIR.parents[2])
if not VENV_PYTHON.exists():
    sys.stderr.write(
        f"no runtime environment: {VENV_PYTHON} missing. For the standalone "
        "skill run its setup.sh once; for repo mode set CAD_AGENT_ROOT.\n"
    )
    sys.exit(3)

CELL_ARGV = [
    str(VENV_PYTHON),
    "-m",
    "fusion_mcp_proxy.cell_server",
    "--node-id",
    os.environ.get("FUSION_CAD_NODE_ID", "fusion-local"),
    "--expected-hostname",
    os.environ.get("FUSION_CAD_HOSTNAME", socket.gethostname()),
    "--upstream",
    os.environ.get("FUSION_CAD_UPSTREAM", "http://127.0.0.1:27182/mcp"),
    "--peekaboo",
    os.environ.get("FUSION_CAD_PEEKABOO", "/opt/homebrew/bin/peekaboo"),
    "--peekaboo-mode",
    os.environ.get("FUSION_CAD_PEEKABOO_MODE", "local-ondemand"),
    "--state-directory",
    os.environ.get(
        "FUSION_CAD_STATE_DIR",
        str(Path.home() / ".local/state/cad-agent-fusion-mcp/local"),
    ),
    "--marker-enforcement",
    os.environ.get("FUSION_CAD_MARKER_ENFORCEMENT", "evidence-gated"),
]

# The cell's operation budget.  --long-request raises its ceiling; the
# activation budget stays separate and small because it only covers local
# libproc/lease/window work.
DEFAULT_CELL_TIMEOUT_SECONDS = float(
    os.environ.get("FUSION_CAD_TIMEOUT_SECONDS", "240")
)
DEFAULT_LONG_CELL_TIMEOUT_SECONDS = float(
    os.environ.get("FUSION_CAD_LONG_TIMEOUT_SECONDS", "1800")
)
MAX_LONG_CELL_TIMEOUT_SECONDS = 1800.0
MAX_CELL_TIMEOUT_SECONDS = 300.0
STARTUP_TIMEOUT_SECONDS = min(
    float(os.environ.get("FUSION_CAD_STARTUP_TIMEOUT_SECONDS", "240")), 300.0
)
# Slack between the nested budgets.  Large enough that the cell always wins the
# race and can convert its own expiry into a guarded, non-ambiguous error.
ACCEPTANCE_SLACK_SECONDS = 90.0
SUBPROCESS_SLACK_SECONDS = 120.0
RECOVERY_HINT = (
    "If this was a budget/transport loss, do NOT restart Fusion yet. Run: "
    "fusion_recovery.py evidence --marker ~/.local/state/cad-agent-fusion-mcp/"
    "local/fusion-mcp-retired-<uid>-<pid>-<digest>.json"
)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("tool")
    parser.add_argument("arguments", help="JSON object of tool arguments")
    parser.add_argument(
        "--timeout",
        type=float,
        default=None,
        help=(
            "operation budget in seconds for the Fusion request itself "
            f"(default {DEFAULT_CELL_TIMEOUT_SECONDS:g}, or "
            f"{DEFAULT_LONG_CELL_TIMEOUT_SECONDS:g} with --long-request). The "
            "acceptance and subprocess budgets are derived from it."
        ),
    )
    parser.add_argument(
        "--long-request",
        action="store_true",
        help=(
            "one long CAD operation: raise the operation ceiling to "
            "1800 s and hold a macOS power assertion for its duration. Exactly "
            "one request is still sent and nothing is ever retried."
        ),
    )
    parser.add_argument(
        "--max-response-bytes",
        type=int,
        default=1_000_000,
        help=(
            "capture bound for the inner payload; the acceptance client drops "
            "the payload silently above it, which used to print an empty "
            "payload with status delivered"
        ),
    )
    parser.add_argument(
        "--fail-on",
        default="",
        help=(
            "comma-separated classifications that should exit 5 instead of 0, "
            "for drivers that want a hard stop (for example "
            "guard_rejected,inner_script_error)"
        ),
    )
    parser.add_argument(
        "--dirty-recovery-receipt",
        type=Path,
        help=(
            "owner-only exact-request receipt for one reviewed call against a "
            "pre-existing modified Fusion document"
        ),
    )
    args = parser.parse_args()

    ceiling = (
        MAX_LONG_CELL_TIMEOUT_SECONDS if args.long_request else MAX_CELL_TIMEOUT_SECONDS
    )
    cell_timeout = args.timeout
    if cell_timeout is None:
        cell_timeout = (
            DEFAULT_LONG_CELL_TIMEOUT_SECONDS
            if args.long_request
            else DEFAULT_CELL_TIMEOUT_SECONDS
        )
    if not 0 < cell_timeout <= ceiling:
        print(
            f"--timeout must be in (0, {ceiling:g}]"
            + ("" if args.long_request else "; pass --long-request for up to 1800"),
            file=sys.stderr,
        )
        return 3
    acceptance_timeout = cell_timeout + ACCEPTANCE_SLACK_SECONDS
    subprocess_timeout = acceptance_timeout + SUBPROCESS_SLACK_SECONDS
    budgets = {
        "cell_operation_seconds": cell_timeout,
        "acceptance_read_seconds": acceptance_timeout,
        "subprocess_kill_seconds": subprocess_timeout,
        "activation_seconds": STARTUP_TIMEOUT_SECONDS,
        "long_request": args.long_request,
    }

    try:
        arguments = json.loads(args.arguments)
        if not isinstance(arguments, dict):
            raise ValueError("arguments must be a JSON object")
    except ValueError as exc:
        print(f"invalid arguments JSON: {exc}", file=sys.stderr)
        return 3

    fail_on = {name.strip() for name in args.fail_on.split(",") if name.strip()}
    unknown = sorted(fail_on - set(TAXONOMY))
    if unknown:
        print(f"unknown --fail-on classification: {', '.join(unknown)}", file=sys.stderr)
        return 3

    plan = {
        "schema": "fusion-mcp.stdio-acceptance-plan.v1",
        "calls": [
            {
                "id": "skill-call",
                "tool": args.tool,
                "arguments": arguments,
                "expect_inner_success": None,
                "capture_response": True,
            }
        ],
    }

    cell_argv = list(CELL_ARGV) + [
        "--timeout-seconds",
        str(cell_timeout),
        "--startup-timeout-seconds",
        str(STARTUP_TIMEOUT_SECONDS),
    ]
    if args.long_request:
        cell_argv.append("--long-request")
    if args.dirty_recovery_receipt is not None:
        cell_argv.extend(
            [
                "--dirty-recovery-receipt",
                str(args.dirty_recovery_receipt.expanduser().resolve()),
            ]
        )

    with tempfile.TemporaryDirectory(prefix="fusion-skill-call-") as tmp:
        plan_path = Path(tmp) / "plan.json"
        report_path = Path(tmp) / "report.json"
        plan_path.write_text(json.dumps(plan), encoding="utf-8")
        proc = subprocess.run(
            [
                str(VENV_PYTHON),
                "-m",
                "fusion_mcp_proxy.acceptance_client",
                "--plan",
                str(plan_path),
                "--output",
                str(report_path),
                "--timeout-seconds",
                str(acceptance_timeout),
                "--max-response-bytes",
                str(args.max_response_bytes),
                *(["--long-request"] if args.long_request else []),
                "--",
                *cell_argv,
            ],
            capture_output=True,
            text=True,
            timeout=subprocess_timeout,
        )
        if not report_path.exists():
            # The acceptance client itself died before it could write evidence.
            sys.stderr.write(
                json.dumps(
                    {
                        "status": "failed",
                        "classification": "transport_failure",
                        "message": (
                            "the acceptance client exited "
                            f"{proc.returncode} without writing a report"
                        ),
                        "budgets": budgets,
                        "recovery_hint": RECOVERY_HINT,
                        "acceptance_client_stdout_tail": proc.stdout[-2000:],
                        "acceptance_client_stderr_tail": proc.stderr[-2000:],
                    },
                    indent=2,
                    ensure_ascii=False,
                )
                + "\n"
            )
            return 2
        report = json.loads(report_path.read_text(encoding="utf-8"))

    outcome = classify_report(report, tool=args.tool, arguments=arguments)
    body = outcome.to_json()
    # Legacy top-level keys: existing drivers parse status/duration_ms/
    # delivery_ack_confirmed/payload and must keep working unchanged.
    body["status"] = "delivered" if outcome.exit_code == 0 else "failed"
    body.setdefault("duration_ms", None)
    body.setdefault("delivery_ack_confirmed", None)
    body.setdefault("payload", {})
    body["budgets"] = budgets
    # The cell's own stderr is the only place a startup failure explains itself
    # (for example "timeout_seconds must be finite and between 0 and 300"); it
    # used to be discarded with the temporary directory.
    body.setdefault("cell_stderr_tail", "")
    if outcome.exit_code != 0:
        body["recovery_hint"] = RECOVERY_HINT

    json.dump(body, sys.stdout, indent=2, ensure_ascii=False)
    sys.stdout.write("\n")
    if outcome.exit_code == 0 and outcome.classification not in fail_on:
        return 0

    sys.stderr.write(f"{outcome.classification}: {outcome.message}\n")
    if outcome.cell_stderr_tail:
        sys.stderr.write("--- cell stderr (tail) ---\n")
        sys.stderr.write(outcome.cell_stderr_tail.rstrip() + "\n")
    if outcome.classification in fail_on and outcome.exit_code == 0:
        return 5
    return EXIT_CODES[outcome.classification]


if __name__ == "__main__":
    sys.exit(main())
