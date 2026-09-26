#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
skill_dir="$(cd "$script_dir/../.." && pwd -P)"
python_bin="${WIPER_KINEMATICS_PYTHON:-$skill_dir/.venv/bin/python}"

if [ ! -x "$python_bin" ]; then
  echo "wiper kinematics: Python runtime missing; run $skill_dir/setup.sh" >&2
  exit 1
fi

result_file="$(mktemp)"
trap 'rm -f "$result_file"' EXIT
PYTHONDONTWRITEBYTECODE=1 PYTHONPATH="$script_dir" \
  "$python_bin" -m unittest discover -s "$script_dir/tests" -v
PYTHONDONTWRITEBYTECODE=1 PYTHONPATH="$script_dir" \
  "$python_bin" -m wiper_kinematics --output "$result_file" >/dev/null
echo "wiper kinematics: PASS"
