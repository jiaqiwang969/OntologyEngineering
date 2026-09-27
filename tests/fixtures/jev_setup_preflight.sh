#!/usr/bin/env bash
# Run only the copied Jev setup in a caller-owned installation-test fixture.
# Its OE_JEV_PYTHON fixture rejects pip and every non-preflight operation.
set -euo pipefail
JEV_TEST_ROOT="${1:?installation fixture root required}"
exec bash "$JEV_TEST_ROOT/runtime/jev-ultrafast/setup.sh"
