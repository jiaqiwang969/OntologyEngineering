#!/bin/sh
# 链 A：可播表 → 冻结 → 构建 → 回读。用法：v005_chain_A.sh <RUN> <可播表相对路径> [walk 目录]
: "${OLSK_CASE_ROOT:?set OLSK_CASE_ROOT to the OLSK work directory}"
cd "$OLSK_CASE_ROOT" || exit 1
export OPENBLAS_NUM_THREADS=1 OMP_NUM_THREADS=1 VECLIB_MAXIMUM_THREADS=1 REBUILD_CASE_DIR=$(pwd)
set -e
RUN=$1; PLAY=$2; SEQ=${3:-本体/S7-序列感知认证-延长150}
.venv/bin/python tools/build_playable_from_seqwalk.py --case-dir . --min 1.0 --cap 150 --min-visible 5 --seq-dir "$SEQ" --out "$PLAY" > logs/${RUN}_playable.log 2>&1; tail -6 logs/${RUN}_playable.log
.venv/bin/python tools/freeze_film_state_generic.py --case-dir . --run $RUN --playable "$PLAY" > logs/${RUN}_freeze.log 2>&1; tail -2 logs/${RUN}_freeze.log
/opt/homebrew/bin/blender -b --factory-startup -P tools/build_assembly_animation_generic.py -- --case-dir . --out $RUN --state 装配动画/$RUN/pipeline-state.v1.json --allow-existing-output --playable "$PLAY" --rx 1280 --ry 900 --cap 150 --fpm-max 30 --shots-max 10 --vis-select 6 --vis-probes 3 --ghost-alpha 0.25 --ghost-max 8 --mate-ghost-alpha 0.3 --bench-ghost 0.02 --bench-ghost-radius 3 --frame-margin 1.22 --metal-cap 0.55 --rough-floor 0.4 --world 0.26 --albedo-cap 0.78 --emit-gain 1.2 --overlay-top 104 --overlay-bottom 118 > logs/${RUN}_build.log 2>&1
grep -q "\[anim\] 完成" logs/${RUN}_build.log || { echo BUILD_FAIL; tail -20 logs/${RUN}_build.log; exit 1; }
/opt/homebrew/bin/blender -b --factory-startup -P tools/reopen_readback_generic.py -- --case-dir . --run $RUN > logs/${RUN}_readback.log 2>&1
grep -q PASS logs/${RUN}_readback.log || { echo READBACK_FAIL; tail -10 logs/${RUN}_readback.log; exit 1; }
echo "${RUN}_READBACK_PASS $(date +%T)"
/opt/homebrew/bin/blender -b --factory-startup -P tools/measure_readability.py -- --case-dir . --run $RUN > logs/${RUN}_readability.log 2>&1; grep "readability\]" logs/${RUN}_readability.log | tail -1
