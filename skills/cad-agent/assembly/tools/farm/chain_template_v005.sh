#!/bin/sh
# v005：延长行程（行走 cap 150）+ 表达层重做（分组机位/材质/虚化/HUD）→ 合并/可播 v2/冻结/构建/回读/两机渲染/拼片/配乐/QC
: "${OLSK_CASE_ROOT:?set OLSK_CASE_ROOT to the OLSK work directory}"
cd "$OLSK_CASE_ROOT" || exit 1
export OPENBLAS_NUM_THREADS=1 OMP_NUM_THREADS=1 VECLIB_MAXIMUM_THREADS=1 REBUILD_CASE_DIR=$(pwd)
set -e
HOST=$(fleet resolve dell-7920) || exit 1; [ -n "$HOST" ] || exit 1
SSHOPT="ssh -o HostKeyAlias=fleet-dell-7920 -o StrictHostKeyChecking=accept-new"
mkdir -p 本体/S7-序列感知认证-延长150
rsync -a -e "$SSHOPT" dell@$HOST:~/olsk_film/本体/S7-序列感知认证-延长150/ 本体/S7-序列感知认证-延长150/
echo "walk files: $(ls 本体/S7-序列感知认证-延长150 | wc -l)"
.venv/bin/python tools/merge_s7_chunks.py --case-dir . --dir 本体/S7-序列感知认证-延长150 > logs/v005_mergechunks.log 2>&1; tail -3 logs/v005_mergechunks.log
echo "merged op files: $(ls 本体/S7-序列感知认证-延长150/*.json | wc -l)"
.venv/bin/python tools/build_playable_from_seqwalk.py --case-dir . --min 1.0 --cap 150 --seq-dir 本体/S7-序列感知认证-延长150 --out 本体/S7-可播接近距离.v2.json > logs/v005_playable.log 2>&1; tail -6 logs/v005_playable.log
.venv/bin/python tools/freeze_film_state_generic.py --case-dir . --run FILM_v005 --playable 本体/S7-可播接近距离.v2.json > logs/v005_freeze.log 2>&1; tail -2 logs/v005_freeze.log
/opt/homebrew/bin/blender -b --factory-startup -P tools/build_assembly_animation_generic.py -- --case-dir . --out FILM_v005 --state 装配动画/FILM_v005/pipeline-state.v1.json --allow-existing-output --playable 本体/S7-可播接近距离.v2.json --rx 1280 --ry 900 --cap 150 --fpm-max 30 --shots-max 4 --ghost-alpha 0.05 --ghost-max 8 --mate-ghost-alpha 0.3 --bench-ghost 0.03 --metal-cap 0.55 --rough-floor 0.4 --world 0.30 --overlay-top 104 --overlay-bottom 118 > logs/v005_build.log 2>&1
grep -q "\[anim\] 完成" logs/v005_build.log || { echo BUILD_FAIL; tail -20 logs/v005_build.log; exit 1; }
/opt/homebrew/bin/blender -b --factory-startup -P tools/reopen_readback_generic.py -- --case-dir . --run FILM_v005 > logs/v005_readback.log 2>&1
grep -q PASS logs/v005_readback.log || { echo READBACK_FAIL; tail -10 logs/v005_readback.log; exit 1; }
echo "V005_READBACK_PASS $(date +%T)"
N=$(.venv/bin/python -c "import json;print(json.load(open('装配动画/FILM_v005/state-chain.json'))['frames'][1])"); H=$((N*2/5))
echo "frames total $N; local 1..$H; dell $((H+1))..$N"
sh logs/remote_render.sh FILM_v005 --from $((H+1)) --to $N --samples 64 > logs/v005_render_remote.log 2>&1 &
/opt/homebrew/bin/blender -b --factory-startup -P tools/render_film_generic.py -- --case-dir . --run FILM_v005 --from 1 --to $H --samples 64 > logs/v005_render_local.log 2>&1
wait
echo "frames: $(ls 装配动画/FILM_v005/frames_raw | wc -l)/$N"
echo "V005_RENDER_DONE $(date +%T)"
.venv/bin/python tools/compose_assembly_film_generic.py --case-dir . --run FILM_v005 --sop 装配工艺/sop-olsk-workbook.v1.json --title "OLSK Large CNC V3 逐零件装配动画（评审候选 v005）" --legend-extra "　极淡=其他台架件（此刻不在场）" > logs/v005_compose.log 2>&1
MP4=$(ls -t 装配动画/FILM_v005/*.mp4 | grep -v scored | head -1); echo "mp4 $MP4"
.venv/bin/python tools/build_score_bed.py --cue 配乐/cues/A-machine-room-steady.wav --chain 装配动画/FILM_v005/state-chain.json --out 配乐/bed_A_v005.wav --receipt 配乐/bed_A_v005.json > logs/v005_bed.log 2>&1
DEC=$(.venv/bin/python -c "import json;c=json.load(open('装配动画/FILM_v005/state-chain.json'));print(round((c['chapters'][-1]['start']-1)/30,1))")
OUT="装配动画/FILM_v005/$(basename "$MP4" .mp4)_scored.mp4"
python3 ~/.codex/skills/narrate-and-score/scripts/assemble.py --video "$MP4" --bed 配乐/bed_A_v005.wav --out "$OUT" --decay-from "$DEC" --floor-db -20 --target-lufs -18.2 --receipt 配乐/assemble_v005.json > logs/v005_assemble.log 2>&1; tail -6 logs/v005_assemble.log
ffmpeg -v error -y -i "$OUT" -c copy -movflags +faststart "${OUT%.mp4}_fs.mp4" && mv "${OUT%.mp4}_fs.mp4" "$OUT"
.venv/bin/python tools/media_qc_generic.py --case-dir . --run FILM_v005 --mp4 "$OUT" > logs/v005_mediaqc.log 2>&1; tail -3 logs/v005_mediaqc.log
.venv/bin/python tools/write_review_request.py --case-dir . --run FILM_v005 > logs/v005_review.log 2>&1
echo "V005_MEDIA_DONE $(date +%T)"
