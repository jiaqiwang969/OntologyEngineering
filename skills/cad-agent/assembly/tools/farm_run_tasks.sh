#!/usr/bin/env bash
# 通用任务表执行器（多机分发用）：每行 "OP\tCHUNK"（CHUNK 可空）。
# 用法: farm_run_tasks.sh <case> <tool.py> <并行度> <输出目录相对路径> <任务表文件>
set -u
CASE=$1; TOOL=$2; PAR=$3; ODIR=$4; TF=$5
cd "$(dirname "$0")/.."
PY=.venv/bin/python
mkdir -p /tmp/s7_logs
cat "$TF" | xargs -P "$PAR" -I{} bash -c '
  IFS=$'"'"'\t'"'"' read -r OP CHUNK <<< "{}"
  OUTDIR="'"$CASE"'/'"$ODIR"'"
  if [ -n "$CHUNK" ]; then OUT="$OUTDIR/${OP}.chunk${CHUNK//:/-}.json"; else OUT="$OUTDIR/${OP}.json"; fi
  if [ -s "$OUT" ]; then echo "SKIP $OP $CHUNK"; exit 0; fi
  LOG="/tmp/s7_logs/run_${OP}_${CHUNK//:/-}.log"
  if [ -n "$CHUNK" ]; then
    '"$PY"' '"$TOOL"' --case-dir '"$CASE"' --op "$OP" --chunk "$CHUNK" > "$LOG" 2>&1
  else
    '"$PY"' '"$TOOL"' --case-dir '"$CASE"' --op "$OP" > "$LOG" 2>&1
  fi
  echo "DONE $OP $CHUNK $?"
'
echo "TASKS-COMPLETE"
