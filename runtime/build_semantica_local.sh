#!/usr/bin/env bash
# Reproduce the Semantica wheel pinned by semantica-source-lock.json.
#
# Usage:
#   runtime/build_semantica_local.sh /path/to/semantica [/path/to/artifacts]
#
# For a network-free Python build, point SEMANTICA_WHEELHOUSE at a directory
# containing the three exact Python build requirements recorded in the lock.
set -euo pipefail

RUNTIME_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LOCK_FILE="$RUNTIME_DIR/semantica-source-lock.json"
SOURCE_ROOT="${1:-${SEMANTICA_SOURCE_ROOT:-}}"

if [ -z "$SOURCE_ROOT" ]; then
  echo "usage: $0 /path/to/semantica [/path/to/artifacts]" >&2
  exit 64
fi

SOURCE_ROOT="$(cd "$SOURCE_ROOT" && pwd)"
if ! git -C "$SOURCE_ROOT" rev-parse --git-dir >/dev/null 2>&1; then
  echo "not a Git checkout: $SOURCE_ROOT" >&2
  exit 65
fi

PYTHON_BIN="${SEMANTICA_BUILD_PYTHON:-python3}"

read_lock() {
  "$PYTHON_BIN" - "$LOCK_FILE" "$1" <<'PY'
import json
import sys

value = json.load(open(sys.argv[1], encoding="utf-8"))
for component in sys.argv[2].split("."):
    value = value[component]
print(value)
PY
}

sha256_file() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  else
    shasum -a 256 "$1" | awk '{print $1}'
  fi
}

LOCKED_COMMIT="$(read_lock source.commit)"
LOCKED_EPOCH="$(read_lock build.source_date_epoch)"
LOCKED_PYPROJECT_HASH="$(read_lock locked_inputs.pyproject_sha256)"
LOCKED_EXPLORER_LOCK_HASH="$(read_lock locked_inputs.explorer_package_lock_sha256)"
EXPECTED_WHEEL="$(read_lock artifact.filename)"
EXPECTED_WHEEL_HASH="$(read_lock artifact.sha256)"
ACTUAL_COMMIT="$(git -C "$SOURCE_ROOT" rev-parse HEAD)"

if [ "$ACTUAL_COMMIT" != "$LOCKED_COMMIT" ]; then
  echo "source commit mismatch: expected $LOCKED_COMMIT, got $ACTUAL_COMMIT" >&2
  exit 66
fi

if [ "$(git -C "$SOURCE_ROOT" rev-parse --is-shallow-repository)" != "false" ]; then
  echo "Semantica checkout is shallow; fetch the complete history first" >&2
  exit 67
fi

if [ -n "$(git -C "$SOURCE_ROOT" status --porcelain --untracked-files=all)" ]; then
  echo "Semantica checkout has uncommitted or untracked source changes" >&2
  exit 68
fi

if [ "$(sha256_file "$SOURCE_ROOT/pyproject.toml")" != "$LOCKED_PYPROJECT_HASH" ]; then
  echo "pyproject.toml does not match the source lock" >&2
  exit 69
fi

if [ "$(sha256_file "$SOURCE_ROOT/explorer/package-lock.json")" != "$LOCKED_EXPLORER_LOCK_HASH" ]; then
  echo "explorer/package-lock.json does not match the source lock" >&2
  exit 69
fi

git -C "$SOURCE_ROOT" fsck --full --no-progress >/dev/null

ARTIFACT_DIR="${2:-$(dirname "$SOURCE_ROOT")/local-builds/$LOCKED_COMMIT}"
mkdir -p "$ARTIFACT_DIR"

BUILD_TMP="$(mktemp -d "${TMPDIR:-/tmp}/semantica-local-build.XXXXXX")"
cleanup() {
  rm -rf "$BUILD_TMP"
}
trap cleanup EXIT

BUILD_SOURCE="$BUILD_TMP/source"
BUILD_OUT_A="$BUILD_TMP/dist-a"
BUILD_OUT_B="$BUILD_TMP/dist-b"
BUILD_VENV="$BUILD_TMP/venv"
mkdir -p "$BUILD_SOURCE" "$BUILD_OUT_A" "$BUILD_OUT_B"
git -C "$SOURCE_ROOT" archive "$LOCKED_COMMIT" | tar -x -C "$BUILD_SOURCE"

(
  cd "$BUILD_SOURCE/explorer"
  npm ci
  npm run test:graph-store
  npm run test:graph-workspace
  npm run test:plugin-registry
  npm run build
)

"$PYTHON_BIN" -m venv "$BUILD_VENV"
if [ -n "${SEMANTICA_WHEELHOUSE:-}" ]; then
  "$BUILD_VENV/bin/python" -m pip install --no-index --find-links "$SEMANTICA_WHEELHOUSE" \
    build==1.3.0 setuptools==84.0.0 wheel==0.48.0
else
  "$BUILD_VENV/bin/python" -m pip install \
    build==1.3.0 setuptools==84.0.0 wheel==0.48.0
fi

for BUILD_OUT in "$BUILD_OUT_A" "$BUILD_OUT_B"; do
  (
    cd "$BUILD_SOURCE"
    SOURCE_DATE_EPOCH="$LOCKED_EPOCH" PYTHONHASHSEED=0 \
      "$BUILD_VENV/bin/python" -m build --wheel --no-isolation --outdir "$BUILD_OUT"
  )
done

BUILT_WHEEL="$BUILD_OUT_A/$EXPECTED_WHEEL"
SECOND_WHEEL="$BUILD_OUT_B/$EXPECTED_WHEEL"
if [ ! -f "$BUILT_WHEEL" ]; then
  echo "expected wheel was not built: $BUILT_WHEEL" >&2
  exit 70
fi
if [ ! -f "$SECOND_WHEEL" ]; then
  echo "second reproducibility wheel was not built: $SECOND_WHEEL" >&2
  exit 70
fi

ACTUAL_WHEEL_HASH="$(sha256_file "$BUILT_WHEEL")"
SECOND_WHEEL_HASH="$(sha256_file "$SECOND_WHEEL")"
if [ "$ACTUAL_WHEEL_HASH" != "$SECOND_WHEEL_HASH" ]; then
  echo "two source-identical wheel builds differ: $ACTUAL_WHEEL_HASH vs $SECOND_WHEEL_HASH" >&2
  exit 71
fi
if [ "$ACTUAL_WHEEL_HASH" != "$EXPECTED_WHEEL_HASH" ]; then
  echo "wheel hash mismatch: expected $EXPECTED_WHEEL_HASH, got $ACTUAL_WHEEL_HASH" >&2
  exit 71
fi

install -m 0644 "$BUILT_WHEEL" "$ARTIFACT_DIR/$EXPECTED_WHEEL"
echo "Semantica source: $LOCKED_COMMIT"
echo "Wheel: $ARTIFACT_DIR/$EXPECTED_WHEEL"
echo "SHA256: $ACTUAL_WHEEL_HASH"
echo "Reproducible builds: 2/2 identical"
