#!/bin/sh
# 链 B：渲染（两机）→ 合成 → 配乐 → faststart → 媒体 QC → 复看请求。用法：v005_chain_B.sh <RUN> <版本标签>
: "${OLSK_CASE_ROOT:?set OLSK_CASE_ROOT to the OLSK work directory}"
cd "$OLSK_CASE_ROOT" || exit 1
export OPENBLAS_NUM_THREADS=1 OMP_NUM_THREADS=1 VECLIB_MAXIMUM_THREADS=1 REBUILD_CASE_DIR=$(pwd)
set -e
RUN=$1; TAG=${2:-v005}
N=$(.venv/bin/python -c "import json;print(json.load(open('装配动画/$RUN/state-chain.json'))['frames'][1])")
# dell 可达才分帧；不可达（内存打满 sshd 无响应）就全部本机渲（K-GOV-16：并发核实包括节点是否还活着）
HOST=$(fleet resolve dell-7920) || exit 1; [ -n "$HOST" ] || exit 1
if timeout 40 ssh -o HostKeyAlias=fleet-dell-7920 -o ConnectTimeout=15 dell@$HOST 'echo ok' 2>/dev/null | grep -q ok; then
  H=$((N*2/5)); echo "frames total $N; local 1..$H; dell $((H+1))..$N"
  sh logs/remote_render.sh $RUN --from $((H+1)) --to $N --samples 96 > logs/${RUN}_render_remote.log 2>&1 &
  /opt/homebrew/bin/blender -b --factory-startup -P tools/render_film_generic.py -- --case-dir . --run $RUN --from 1 --to $H --samples 96 > logs/${RUN}_render_local.log 2>&1
  wait
else
  echo "dell unreachable: rendering all $N frames locally"
  /opt/homebrew/bin/blender -b --factory-startup -P tools/render_film_generic.py -- --case-dir . --run $RUN --from 1 --to $N --samples 96 > logs/${RUN}_render_local.log 2>&1
fi
echo "frames: $(ls 装配动画/$RUN/frames_raw | wc -l)/$N"
.venv/bin/python tools/compose_assembly_film_generic.py --case-dir . --run $RUN --sop 装配工艺/sop-olsk-workbook.v1.json --title "OLSK Large CNC V3 逐零件装配动画（评审候选 ${TAG}）" --legend-extra "　极淡=其他台架件（此刻不在场）" > logs/${RUN}_compose.log 2>&1
MP4=$(ls -t 装配动画/$RUN/*.mp4 | grep -v scored | head -1); echo "mp4 $MP4"
.venv/bin/python tools/build_score_bed.py --cue 配乐/cues/A-machine-room-steady.wav --chain 装配动画/$RUN/state-chain.json --out 配乐/bed_A_$RUN.wav --receipt 配乐/bed_A_$RUN.json > logs/${RUN}_bed.log 2>&1
DEC=$(.venv/bin/python -c "import json;c=json.load(open('装配动画/$RUN/state-chain.json'));print(round((c['chapters'][-1]['start']-1)/30,1))")
OUT="装配动画/$RUN/$(basename "$MP4" .mp4)_scored.mp4"
python3 ~/.codex/skills/narrate-and-score/scripts/assemble.py --video "$MP4" --bed 配乐/bed_A_$RUN.wav --out "$OUT" --decay-from "$DEC" --floor-db -20 --target-lufs -18.2 --receipt 配乐/assemble_$RUN.json > logs/${RUN}_assemble.log 2>&1; tail -4 logs/${RUN}_assemble.log
ffmpeg -v error -y -i "$OUT" -c copy -movflags +faststart "${OUT%.mp4}_fs.mp4" && mv "${OUT%.mp4}_fs.mp4" "$OUT"
.venv/bin/python tools/media_qc_generic.py --case-dir . --run $RUN --mp4 "$OUT" > logs/${RUN}_mediaqc.log 2>&1; tail -3 logs/${RUN}_mediaqc.log
.venv/bin/python tools/write_review_request.py --case-dir . --run $RUN > logs/${RUN}_review.log 2>&1
echo "${RUN}_MEDIA_DONE $(date +%T) $OUT"
