#!/bin/sh
# Public synthetic continuity example. All decisions remain in the native CLI.
set -eu
if [ "$#" -ne 2 ]; then
  echo 'Usage: sh examples/project-workspace/demo.sh /absolute/installation /absolute/new-project-home' >&2
  exit 2
fi
OE_DEMO_ROOT="$1"
OE_DEMO_HOME="$2"
OE_DEMO_BIN="${OEV2_BIN:-$OE_DEMO_ROOT/var/install/bin/oev2}"
python3 - "$OE_DEMO_ROOT" "$OE_DEMO_HOME" <<'PY'
from pathlib import Path
import sys
root, home = map(Path, sys.argv[1:])
if not root.is_absolute() or root.resolve(strict=True) != root or not root.is_dir():
    raise SystemExit('Installation must be an existing absolute real directory')
if not home.is_absolute() or '..' in home.parts or home.resolve() != home:
    raise SystemExit('Project home must be an absolute real path without symlinks')
if home.exists() or home == root or home in root.parents or root in home.parents:
    raise SystemExit('Use a new project home outside the software installation')
home.mkdir(mode=0o700)
PY
cp -R "$OE_DEMO_ROOT/examples/project-workspace/fixtures/." "$OE_DEMO_HOME/"
oe_demo() {
  "$OE_DEMO_BIN" --root "$OE_DEMO_ROOT" --project-home "$OE_DEMO_HOME" project workspace "$@"
}
oe_demo init --workspace-id bracket-demo --project-id bracket-continuation-demo > "$OE_DEMO_HOME/init.result.json"
oe_demo commit --input initial-commit.json > "$OE_DEMO_HOME/initial.result.json"
oe_demo resume --output views/session-one.md > "$OE_DEMO_HOME/resume-one.json"
# Each call is a new process; no in-memory session state supplies the binding.
oe_demo resume --output views/session-two.md > "$OE_DEMO_HOME/resume-two.json"
oe_demo learning-goal --output learning/old-goal.json > "$OE_DEMO_HOME/learning-goal.result.json"
python3 - "$OE_DEMO_HOME" <<'PY'
from pathlib import Path
import hashlib, json, sys
home = Path(sys.argv[1])
head = json.loads((home/'HEAD.json').read_text())['commit']
(home/'expected-head.json').write_text(json.dumps(head, indent=2)+'\n')
proposal = json.loads((home/'corrected-commit.template.json').read_text())
proposal['expected_parent'] = head
(home/'corrected-commit.json').write_text(json.dumps(proposal, indent=2)+'\n')
goal = (home/'learning/old-goal.json').read_bytes()
request = {'goal': {'path':'learning/old-goal.json','sha256':hashlib.sha256(goal).hexdigest()}}
(home/'stale-goal-freeze.json').write_text(json.dumps(request, indent=2)+'\n')
PY
oe_demo begin --expected-head expected-head.json --stage S1 > "$OE_DEMO_HOME/begin.result.json"
oe_demo commit --input corrected-commit.json > "$OE_DEMO_HOME/correction.result.json"
oe_demo resume --output views/after-correction.md > "$OE_DEMO_HOME/resume-corrected.json"
if oe_demo learning-freeze --input stale-goal-freeze.json --output learning/stale-candidate > "$OE_DEMO_HOME/runs/stale-freeze.stdout" 2> "$OE_DEMO_HOME/runs/stale-freeze.stderr"; then
  echo 'Unexpected success: an old root goal was accepted' >&2
  exit 1
fi
oe_demo learning-goal --output learning/current-goal.json > "$OE_DEMO_HOME/current-goal.result.json"
python3 - "$OE_DEMO_HOME" <<'PY'
from pathlib import Path
import json, sys
home = Path(sys.argv[1])
read = lambda name: json.loads((home/name).read_text())
first, second = read('resume-one.json'), read('resume-two.json')
assert first == second and first['status'] == 'ready'
after = read('resume-corrected.json')
assert after['corrections'][0]['status'] == 'acknowledged'
assert after['acceptance'][0]['status'] == 'open'
assert not after['automatic_dispatch'] and after['engineering_adoption'] == 'not_granted'
assert read('project.json')['decisions'][0]['conclusion'] == 'unknown'
assert not read('project.json')['decisions'][0]['adopted']
failure = (home/'runs/stale-freeze.stderr').read_text() + (home/'runs/stale-freeze.stdout').read_text()
assert 'workspace_learning_root_changed' in failure
assert not (home/'learning/stale-candidate').exists()
assert read('learning/old-goal.json')['root_anchor'] != read('learning/current-goal.json')['root_anchor']
print('Synthetic continuity checks passed; engineering acceptance remains open.')
print('Retained example: ' + str(home))
PY
