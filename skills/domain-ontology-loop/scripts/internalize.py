#!/usr/bin/env python3
"""Unified entry for the Semantica-native semantic engagement lifecycle."""

from pathlib import Path
import sys


SKILL_ROOT = Path(__file__).resolve().parents[3]
if str(SKILL_ROOT) not in sys.path:
    sys.path.insert(0, str(SKILL_ROOT))

from ontology_engineering.semantic_engagement import main


if __name__ == "__main__":
    raise SystemExit(main())
