#!/usr/bin/env python3
"""Place and register the source library belonging to this skill."""
from pathlib import Path
import sys

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from ontology_engineering.source_library import main

if __name__ == "__main__":
    raise SystemExit(main())
