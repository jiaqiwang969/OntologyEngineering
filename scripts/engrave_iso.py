#!/usr/bin/env python3
"""Compatibility entry for Semantica's source-bounded normative engraver."""

from pathlib import Path
import sys


SKILL_ROOT = Path(__file__).resolve().parents[1]
if str(SKILL_ROOT) not in sys.path:
    sys.path.insert(0, str(SKILL_ROOT))

from ontology_engineering.semantica_runtime import normative_engraver_main


if __name__ == "__main__":
    raise SystemExit(normative_engraver_main())
