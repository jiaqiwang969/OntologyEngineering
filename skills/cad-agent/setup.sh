#!/bin/bash
# Shared engineering helpers only. NXOpen is supplied by the licensed NX host.
set -euo pipefail
SKILL_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
CAD_PY="${CAD_AGENT_PYTHON:-python3}"
"$CAD_PY" -c 'import sys; assert sys.version_info >= (3,11), "Python >= 3.11 required"'
if command -v uv >/dev/null 2>&1; then
  uv venv --quiet --allow-existing --python "$CAD_PY" "$SKILL_DIR/.venv"
  uv pip install --quiet --python "$SKILL_DIR/.venv/bin/python" 'jsonschema>=4,<5' 'numpy>=1.24,<3' 'PyYAML>=6,<7'
else
  "$CAD_PY" -m venv "$SKILL_DIR/.venv"
  "$SKILL_DIR/.venv/bin/python" -m pip install --quiet 'jsonschema>=4,<5' 'numpy>=1.24,<3' 'PyYAML>=6,<7'
fi
"$SKILL_DIR/.venv/bin/python" "$SKILL_DIR/scripts/cad_doctor.py" --json
