#!/usr/bin/env bash
# Portable local-only check; --fusion adds a non-mutating endpoint preflight.
set -euo pipefail
SKILL_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
case "${1:-}" in
  "") ;;
  --fusion) python3 "$SKILL_DIR/scripts/fusion_preflight.py" --health-probe tcp ;;
  --help|-h) echo "usage: doctor.sh [--fusion]"; exit 0 ;;
  *) echo "unknown option: $1" >&2; exit 2 ;;
esac
"$SKILL_DIR/.venv/bin/python" "$SKILL_DIR/scripts/verify_fusion_runtime_wheel.py" --installed
"$SKILL_DIR/.venv/bin/python" -c 'import jsonschema'
echo "cad-agent core: OK"
