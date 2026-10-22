#!/bin/sh
# Fetch, patch, test and build into a NEW private directory. No EDA/service start.
set -eu
[ "$#" -eq 1 ] || { echo 'usage: build-runtime.sh NEW_RUNTIME_DIRECTORY' >&2; exit 2; }
module=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd -P)
output=$1
mkdir -p "$(dirname -- "$output")"
mkdir "$output"
output=$(CDPATH= cd -- "$output" && pwd -P)
mkdir "$output/logs" "$output/bin"
base=f1aab8d2b03ab6a0bd78ce83e7961d838f4d850b
patch="$module/patches/easyeda-agent-runtime-0.23.0-local.4.patch"
version=0.23.0-local.4
export GOTOOLCHAIN=go1.26.3
export PYTHONDONTWRITEBYTECODE=1
git init -q "$output/source"
git -C "$output/source" remote add origin https://github.com/zhoushoujianwork/easyeda-agent.git
git -C "$output/source" fetch --depth 1 origin "$base" > "$output/logs/fetch.stdout" 2> "$output/logs/fetch.stderr"
git -C "$output/source" checkout --detach "$base" > "$output/logs/checkout.stdout" 2> "$output/logs/checkout.stderr"
[ "$(git -C "$output/source" rev-parse HEAD)" = "$base" ]
git -C "$output/source" apply --check "$patch"
git -C "$output/source" apply "$patch"
(
  cd "$output/source"
  go test -p 2 ./internal/app ./internal/protocol ./internal/daemon > "$output/logs/go-tests.stdout" 2> "$output/logs/go-tests.stderr"
  go build -trimpath -ldflags "-X github.com/zhoushoujianwork/easyeda-agent/internal/version.Version=$version" -o "$output/bin/easyeda" ./cmd/easyeda > "$output/logs/go-build.stdout" 2> "$output/logs/go-build.stderr"
  cd extension
  npm ci --ignore-scripts > "$output/logs/npm-ci.stdout" 2> "$output/logs/npm-ci.stderr"
  npm test > "$output/logs/extension-tests.stdout" 2> "$output/logs/extension-tests.stderr"
  npm run typecheck > "$output/logs/typecheck.stdout" 2> "$output/logs/typecheck.stderr"
  npm run build > "$output/logs/connector-build.stdout" 2> "$output/logs/connector-build.stderr"
)
cp "$output/source/extension/build/dist/easyeda-agent-connector_v${version}.eext" "$output/connector.eext"
"$output/bin/easyeda" --version > "$output/logs/version.stdout"
"$output/bin/easyeda" actions > "$output/actions.json"
"$output/bin/easyeda" blocks ls > "$output/blocks.json"
python3 "$module/scripts/record-runtime-build.py" "$output" "$patch"
printf 'Built and tested; no EDA installation or service started: %s\n' "$output"
