#!/usr/bin/env python3
"""Freeze the current development graph selection into a hash-bound manifest."""

from __future__ import annotations

import argparse
import sys
from pathlib import Path
from typing import Sequence

import yaml

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "eval"))
from run_eval import SHAPES, main_data_paths  # noqa: E402

from capstone import MANIFEST, build_bundle_manifest_document  # noqa: E402


def parse_args(argv: Sequence[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--output",
        default="-",
        help="output path relative to the repository, or '-' for stdout",
    )
    return parser.parse_args(argv)


def main(argv: Sequence[str] | None = None) -> int:
    args = parse_args(argv)
    document = build_bundle_manifest_document(main_data_paths(), SHAPES)
    rendered = yaml.safe_dump(
        document,
        allow_unicode=True,
        sort_keys=False,
        width=1000,
    )
    if args.output == "-":
        print(rendered, end="")
        return 0

    output = Path(args.output)
    if not output.is_absolute():
        output = ROOT / output
    output = output.resolve()
    try:
        output.relative_to(ROOT)
    except ValueError:
        print(f"refusing to write outside repository root: {output}", file=sys.stderr)
        return 2
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(rendered, encoding="utf-8")
    print(f"[FROZEN] {output.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
