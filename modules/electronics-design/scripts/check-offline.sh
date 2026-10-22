#!/bin/sh
# Offline test orchestration only. Never starts EDA, a bridge, or a daemon.
set -eu
[ "$#" -eq 1 ] || { echo 'usage: check-offline.sh NEW_OUTPUT_DIRECTORY' >&2; exit 2; }
module=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd -P)
output=$1
mkdir -p "$(dirname -- "$output")"
mkdir "$output"
output=$(CDPATH= cd -- "$output" && pwd -P)
export PYTHONDONTWRITEBYTECODE=1
python3 "$module/modules/easyeda-agent/scripts/tests/run.py" > "$output/easyeda-agent.stdout" 2> "$output/easyeda-agent.stderr"
node --check "$module/modules/easyeda-api/scripts/bridge-server.mjs" > "$output/api-syntax.stdout" 2> "$output/api-syntax.stderr"
audit="$module/modules/lceda-jlc-3d-model-extraction/scripts/audit_model_package.py"
python3 "$audit" "$module/examples/offline/model" --pretty > "$output/model-positive.json" 2> "$output/model-positive.stderr"
mkdir "$output/tampered" "$output/no-model"
cp "$module/examples/offline/model/triangle.obj" "$output/tampered/triangle.obj"
cp "$module/examples/offline/model/SHA256SUMS" "$output/tampered/SHA256SUMS"
printf '\n# deliberately changed bytes\n' >> "$output/tampered/triangle.obj"
if python3 "$audit" "$output/tampered" --pretty > "$output/model-negative.json" 2> "$output/model-negative.stderr"; then
  echo 'tampered source unexpectedly accepted' >&2; exit 1
fi
if python3 "$audit" "$output/no-model" --pretty > "$output/missing-negative.json" 2> "$output/missing-negative.stderr"; then
  echo 'missing model unexpectedly accepted' >&2; exit 1
fi
python3 - "$output" <<'PY'
import json,sys
from pathlib import Path
p=Path(sys.argv[1])
read=lambda name:json.loads((p/name).read_text())
a=read('model-positive.json');b=read('model-negative.json');c=read('missing-negative.json')
assert a['status']=='ok'
assert a['folders'][0]['obj_summaries'][0]['vertices']==3
assert a['folders'][0]['obj_summaries'][0]['faces']==1
assert a['folders'][0]['obj_summaries'][0]['bbox_size']==[1.0,1.0,0.0]
assert any(x.startswith('sha256_mismatch:') for x in b['folders'][0]['errors'])
assert 'no_obj_step_or_stp_model' in c['folders'][0]['errors']
assert 'missing_SHA256SUMS' in c['folders'][0]['errors']
PY
printf '%s\n' 'matched: offline helper fixtures and explicit negative cases' \
  'not run: EDA/connector/API server/network extraction/CAD alignment/fabrication/wiring/power' \
  'upstream connector TypeScript cross-check: skipped when external source is absent' > "$output/RESULT.txt"
printf 'Offline records: %s\n' "$output"
