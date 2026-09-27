#!/usr/bin/env python3
"""Unified engineering Jev knowledge entry for supplier and registered PDF sources."""
from pathlib import Path
import sys
sys.dont_write_bytecode = True

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
from ontology_engineering.misumi.cli import main  # noqa: E402

if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except KeyboardInterrupt:
        print("查询已中断。", file=sys.stderr)
        raise SystemExit(130)
