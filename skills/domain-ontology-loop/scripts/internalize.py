#!/usr/bin/env python3
"""Compatibility entry for Semantica's governed ontology lifecycle."""

from pathlib import Path
import sys


SKILL_ROOT = Path(__file__).resolve().parents[3]
if str(SKILL_ROOT) not in sys.path:
    sys.path.insert(0, str(SKILL_ROOT))

from ontology_engineering.semantica_runtime import governed_ontology_main


if __name__ == "__main__":
    raise SystemExit(governed_ontology_main())
