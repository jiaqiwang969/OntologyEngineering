#!/usr/bin/env python3
"""Evidence-based recovery from a Fusion MCP PID retirement -- no restart.

A retirement marker records that the *client* lost the outcome of one request.
It is not evidence that Fusion failed.  Twice on 2026-09-06 the client retired
a PID on a budget while Fusion completed the work and saved the document; the
documented recovery (quit and relaunch Fusion) was pure loss.

This command reads Fusion's own append-only AppLog -- a witness neither the
proxy nor this tool can influence -- and lifts the retirement only when every
one of the enumerated conditions holds.  Missing or ambiguous evidence refuses:
fail closed, and the operator falls back to the ordinary triage runbook.

    fusion_recovery.py evidence --latest
    fusion_recovery.py evidence --marker <retirement.json>
    fusion_recovery.py recover  --marker <retirement.json> --apply

Exit codes: 0 = evidence complete (and, with --apply, receipt written and the
markers superseded); 3 = usage/IO error; 4 = evidence incomplete, refused.

It never sends an MCP request to Fusion.
"""

from __future__ import annotations

import os
import socket  # noqa: F401  (kept for parity with fusion_call.py's env contract)
import subprocess
import sys
from pathlib import Path

SKILL_DIR = Path(__file__).resolve().parents[1]
_root_override = os.environ.get("CAD_AGENT_ROOT")


def _venv_python(root: Path) -> Path:
    if os.name == "nt":
        return root / ".venv" / "Scripts" / "python.exe"
    return root / ".venv" / "bin" / "python"


if _venv_python(SKILL_DIR).exists():
    VENV_PYTHON = _venv_python(SKILL_DIR)
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

STATE_DIRECTORY = Path(
    os.environ.get(
        "FUSION_CAD_STATE_DIR",
        str(Path.home() / ".local/state/cad-agent-fusion-mcp/local"),
    )
).expanduser()


def _latest_marker() -> Path | None:
    markers = sorted(
        (
            path
            for path in STATE_DIRECTORY.glob(
                f"fusion-mcp-retired-{os.geteuid()}-*.json"
            )
            if ".superseded-" not in path.name
        ),
        key=lambda path: path.stat().st_mtime,
        reverse=True,
    )
    return markers[0] if markers else None


def main() -> int:
    argv = sys.argv[1:]
    if not argv or argv[0] in {"-h", "--help"}:
        sys.stdout.write(__doc__ or "")
        return 0 if argv else 3
    if "--latest" in argv:
        marker = _latest_marker()
        if marker is None:
            sys.stderr.write(
                f"no un-superseded retirement marker under {STATE_DIRECTORY}\n"
            )
            return 3
        index = argv.index("--latest")
        argv[index : index + 1] = ["--marker", str(marker)]
        sys.stderr.write(f"fusion_recovery: using latest marker {marker}\n")
    completed = subprocess.run(
        [str(VENV_PYTHON), "-m", "fusion_mcp_proxy.applog_evidence", *argv],
        check=False,
    )
    return completed.returncode


if __name__ == "__main__":
    sys.exit(main())
