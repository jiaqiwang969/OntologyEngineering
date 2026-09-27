#!/usr/bin/env python3
"""Compatibility entry; use scripts/jev_knowledge.py for engineering knowledge."""
import sys
sys.dont_write_bytecode = True
from jev_knowledge import main

if __name__ == "__main__":
    raise SystemExit(main())
