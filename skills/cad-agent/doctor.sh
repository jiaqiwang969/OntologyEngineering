#!/bin/bash
# Local by default. --profile explicitly requests the read-only NX host probe.
set -euo pipefail
SKILL_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
CAD_PY="$SKILL_DIR/.venv/bin/python"
if [ ! -x "$CAD_PY" ]; then CAD_PY=python3; fi
exec "$CAD_PY" "$SKILL_DIR/scripts/cad_doctor.py" "$@"
