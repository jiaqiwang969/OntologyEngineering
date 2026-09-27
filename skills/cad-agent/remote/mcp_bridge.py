#!/usr/bin/env python3
"""Retired CAD MCP compatibility entrypoint; no transport implementation."""
import sys


def main(argv=None):
    print('Retired CAD MCP entrypoint. This route is permanently disabled. Use scripts/nx_direct.py for reviewed NXOpen/Journal jobs.', file=sys.stderr)
    return 64


if __name__ == "__main__":
    raise SystemExit(main())
