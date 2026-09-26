#!/bin/sh
# 链 A（v010 表达层：台架章隐藏非本组件、剖切=隐藏、遮挡虚化到章末、就位停顿、动件最小占比、视线不平行插入轴、终章环绕）
: "${OLSK_CASE_ROOT:?set OLSK_CASE_ROOT to the OLSK work directory}"
cd "$OLSK_CASE_ROOT" || exit 1
export OPENBLAS_NUM_THREADS=1 OMP_NUM_THREADS=1 VECLIB_MAXIMUM_THREADS=1 REBUILD_CASE_DIR=$(pwd)
set -e
RUN=$1; PLAY=$2; SEQ=${3:-本体/S7-序列感知认证-v2merged}; SKIP_PLAYABLE=1
.venv/bin/python tools/freeze_film_state_generic.py --case-dir . --run $RUN --playable "$PLAY" --play-order 本体/S7-工序内播放序.v2-unit.json --s4 本体/S4-机构识别.v2.json --s7 本体/S7-扫掠认证.v3.json > logs/${RUN}_freeze.log 2>&1; tail -2 logs/${RUN}_freeze.log
/opt/homebrew/bin/blender -b --factory-startup -P tools/build_assembly_animation_generic.py -- --case-dir . --out $RUN --state 装配动画/$RUN/pipeline-state.v1.json --allow-existing-output --playable "$PLAY" --rx 1280 --ry 900 --cap 150 --fpm-max 30 --shots-max 10 --vis-select 6 --vis-probes 3 --ghost-alpha 0.08 --ghost-max 8 --mate-ghost-alpha 0.15 --bench-ghost 0 --bench-ghost-radius 0 --cutaway-hide 1 --seat-hold 10 --min-mover-frac 0.16 --axis-view-max 0.85 --finale-orbit-deg 50 --frame-margin 1.22 --metal-cap 0.35 --rough-floor 0.45 --world 0.2 --albedo-cap 0.72 --emit-gain 1.2 --overlay-top 104 --overlay-bottom 118 > logs/${RUN}_build.log 2>&1
grep -q "\[anim\] 完成" logs/${RUN}_build.log || { echo BUILD_FAIL; tail -20 logs/${RUN}_build.log; exit 1; }
/opt/homebrew/bin/blender -b --factory-startup -P tools/reopen_readback_generic.py -- --case-dir . --run $RUN > logs/${RUN}_readback.log 2>&1
grep -q PASS logs/${RUN}_readback.log || { echo READBACK_FAIL; tail -10 logs/${RUN}_readback.log; exit 1; }
echo "${RUN}_READBACK_PASS $(date +%T)"
/opt/homebrew/bin/blender -b --factory-startup -P tools/measure_readability.py -- --case-dir . --run $RUN > logs/${RUN}_readability.log 2>&1; grep "readability\]" logs/${RUN}_readability.log | tail -1
