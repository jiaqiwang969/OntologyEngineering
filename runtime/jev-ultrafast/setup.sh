#!/usr/bin/env bash
set -euo pipefail
JEV_RUNTIME="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
JEV_ROOT="$(cd "$JEV_RUNTIME/../.." && pwd)"
python3 - "$JEV_ROOT" <<'PY'
import sys
sys.path.insert(0, sys.argv[1])
from ontology_engineering.jev_browser import source_identity
source_identity()
print('Pinned browser source and dependency lock verified')
PY
if [ ! -x "$JEV_RUNTIME/.venv/bin/python" ]; then
  python3 -m venv "$JEV_RUNTIME/.venv"
fi
if ! "$JEV_RUNTIME/.venv/bin/python" -m pip --version >/dev/null 2>&1; then
  "$JEV_RUNTIME/.venv/bin/python" -m ensurepip --upgrade
fi
"$JEV_RUNTIME/.venv/bin/python" -m pip install --require-hashes -r "$JEV_RUNTIME/requirements.txt"
exec "$JEV_RUNTIME/.venv/bin/python" "$JEV_ROOT/scripts/jev_browser.py" doctor
