#!/usr/bin/env bash
# 工程本体论 skill 的可执行佐证运行时。
# 用途：为 demos/ 里的「书中论断 → 代码执行 → 佐证结论」示例准备 Python 环境。
# 依赖锁定版本，避免 semantica 0.6.x API 变动破坏 demo。
set -euo pipefail

RUNTIME_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SKILL_ROOT="$(cd "$RUNTIME_DIR/.." && pwd)"
VENV="$RUNTIME_DIR/.venv"
OE_RUNTIME_PYTHON="${OE_RUNTIME_PYTHON:-python3}"

case "${1:-}" in
  --doctor)
    if [ -x "$VENV/bin/python" ]; then
      exec "$VENV/bin/python" "$RUNTIME_DIR/doctor_runtime.py"
    fi
    exec "$OE_RUNTIME_PYTHON" "$RUNTIME_DIR/doctor_runtime.py"
    ;;
  --preflight)
    exec "$OE_RUNTIME_PYTHON" "$RUNTIME_DIR/doctor_runtime.py" --preflight
    ;;
  --help|-h)
    echo "usage: $0 [--preflight|--doctor]"
    echo "  no option    verify inputs, create/update the venv, then verify the installation"
    echo "  --preflight  non-mutating lock, wheel and Python compatibility checks"
    echo "  --doctor     non-mutating preflight plus installed-venv provenance checks"
    exit 0
    ;;
  "")
    ;;
  *)
    echo "unknown option: $1" >&2
    echo "usage: $0 [--preflight|--doctor]" >&2
    exit 2
    ;;
esac

# Fail before creating or updating the environment when the lock, vendored wheel,
# or bootstrap Python is incompatible. This check does not claim dependencies are
# available offline; a first install can still require the configured package index.
"$OE_RUNTIME_PYTHON" "$RUNTIME_DIR/doctor_runtime.py" --preflight

if [ ! -d "$VENV" ]; then
  "$OE_RUNTIME_PYTHON" -m venv "$VENV"
elif [ ! -x "$VENV/bin/python" ]; then
  echo "runtime venv is incomplete: $VENV/bin/python is unavailable" >&2
  echo "move the stale .venv aside, then run setup_runtime.sh again" >&2
  exit 1
fi
"$VENV/bin/python" -m pip install --quiet --upgrade pip
(
  cd "$SKILL_ROOT"
  "$VENV/bin/python" -m pip install --quiet -r "$RUNTIME_DIR/requirements.txt"
)
VENDORED_WHEEL="$("$VENV/bin/python" - "$RUNTIME_DIR/semantica-source-lock.json" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as stream:
    print(json.load(stream)["artifact"]["filename"])
PY
)"
# The OE build metadata may stay constant while its source-bound package payload
# changes.  Reinstall the exact locked wheel even when pip already sees the same
# public version, otherwise a previous local build can survive an atomic lock update.
"$VENV/bin/python" -m pip install --quiet --force-reinstall --no-deps \
  "$RUNTIME_DIR/vendor/$VENDORED_WHEEL"
PYTHONPATH="$SKILL_ROOT" "$VENV/bin/python" - <<'PY'
from ontology_engineering.semantica_runtime import read_runtime_source_lock

read_runtime_source_lock(verify_vendored_artifact=True)
PY
"$VENV/bin/python" "$RUNTIME_DIR/doctor_runtime.py"

echo "runtime ready: $VENV"
echo "run demos with: $VENV/bin/python demos/<demo>.py"
