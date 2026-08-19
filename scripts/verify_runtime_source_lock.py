#!/usr/bin/env python3
"""Verify the vendored Semantica wheel against the pinned source lock."""

from __future__ import annotations

from pathlib import Path
import sys


SKILL_ROOT = Path(__file__).resolve().parents[1]
if str(SKILL_ROOT) not in sys.path:
    sys.path.insert(0, str(SKILL_ROOT))

from ontology_engineering.semantica_runtime import read_runtime_source_lock  # noqa: E402


def main() -> int:
    read_runtime_source_lock(verify_vendored_artifact=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
