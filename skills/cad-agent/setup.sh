#!/usr/bin/env bash
# Install only the locked Fusion execution wheel and the handoff validator.
set -euo pipefail
SKILL_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
CAD_PYTHON="${FUSION_CAD_PYTHON:-python3}"
"$CAD_PYTHON" -c 'import sys; sys.exit(0 if sys.version_info >= (3, 11) else 1)' ||
  { echo "cad-agent requires Python 3.11+" >&2; exit 1; }
"$CAD_PYTHON" "$SKILL_DIR/scripts/verify_fusion_runtime_wheel.py"
wheel="$SKILL_DIR/dist/oe_cad_fusion_runtime-0.1.0+oe.1-py3-none-any.whl"
if command -v uv >/dev/null 2>&1; then
  uv venv --quiet --allow-existing --python "$CAD_PYTHON" "$SKILL_DIR/.venv"
  uv pip install --quiet --python "$SKILL_DIR/.venv/bin/python" "$wheel" 'websocket-client>=1.8,<2' 'jsonschema>=4,<5'
else
  "$CAD_PYTHON" -m venv "$SKILL_DIR/.venv"
  "$SKILL_DIR/.venv/bin/python" -m pip install --quiet "$wheel" 'websocket-client>=1.8,<2' 'jsonschema>=4,<5'
fi
"$SKILL_DIR/.venv/bin/python" "$SKILL_DIR/scripts/verify_fusion_runtime_wheel.py" --installed
echo "cad-agent core ready: $SKILL_DIR/.venv"
