"""Command-line interface for reproducible JSON validation."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Sequence

from .cases import CASES
from .simulate import run_validation


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Validate archived wiper-linkage cases with NumPy."
    )
    parser.add_argument(
        "--case",
        action="append",
        dest="cases",
        metavar="NAME",
        help="case to run; repeat for multiple cases (default: all)",
    )
    parser.add_argument("--pretty", action="store_true", help="indent JSON output")
    parser.add_argument(
        "--output", type=Path, help="also write the JSON payload to this path"
    )
    parser.add_argument(
        "--list-cases", action="store_true", help="list available cases as JSON and exit"
    )
    return parser


def main(argv: Sequence[str] | None = None) -> int:
    args = _parser().parse_args(argv)
    if args.list_cases:
        print(json.dumps({"cases": list(CASES)}, ensure_ascii=False))
        return 0

    try:
        payload = run_validation(args.cases)
    except KeyError as exc:
        print(str(exc), file=sys.stderr)
        return 2

    indent = 2 if args.pretty else None
    text = json.dumps(payload, ensure_ascii=False, indent=indent, sort_keys=True)
    if args.output is not None:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(text + "\n", encoding="utf-8")
    print(text)
    return 0 if payload["passed"] else 1

