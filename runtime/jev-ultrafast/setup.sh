#!/usr/bin/env bash
set -euo pipefail
JEV_RUNTIME="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
JEV_ROOT="$(cd "$JEV_RUNTIME/../.." && pwd)"
OE_JEV_PYTHON="${OE_JEV_PYTHON:-python3}"
"$OE_JEV_PYTHON" - "$JEV_ROOT" <<'PY'
import sys
if sys.version_info < (3, 12):
    raise SystemExit('Jev browser setup requires Python >= 3.12; set OE_JEV_PYTHON to a compatible interpreter. Current: ' + sys.version.split()[0])
sys.path.insert(0, sys.argv[1])
from ontology_engineering.jev_browser import source_identity
source_identity()
print('Pinned browser source and dependency lock verified')
PY
if [ ! -x "$JEV_RUNTIME/.venv/bin/python" ]; then
  "$OE_JEV_PYTHON" -m venv "$JEV_RUNTIME/.venv"
fi
"$JEV_RUNTIME/.venv/bin/python" - <<'PY'
import sys
if sys.version_info < (3, 12):
    raise SystemExit('Existing Jev environment requires Python >= 3.12; move runtime/jev-ultrafast/.venv aside and rerun setup with OE_JEV_PYTHON. Current: ' + sys.version.split()[0])
PY
if ! "$JEV_RUNTIME/.venv/bin/python" -m pip --version >/dev/null 2>&1; then
  "$JEV_RUNTIME/.venv/bin/python" -m ensurepip --upgrade
fi
"$JEV_RUNTIME/.venv/bin/python" -m pip install --require-hashes -r "$JEV_RUNTIME/requirements.txt"
exec "$JEV_RUNTIME/.venv/bin/python" "$JEV_ROOT/scripts/jev_browser.py" doctor
