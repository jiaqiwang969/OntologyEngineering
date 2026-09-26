#!/usr/bin/env bash
# 任务表执行器（BSD/macOS 兼容版：不用 xargs -I 的替换缓冲，round-robin 分桶+后台 worker）
# 用法: farm_run_tasks_portable.sh <case> <tool.py> <并行度> <输出目录相对路径> <任务表文件>
set -u
CASE=$1; TOOL=$2; PAR=$3; ODIR=$4; TF=$5
cd "$(dirname "$0")/.."
PY=.venv/bin/python
LOGD="${TMPDIR:-/tmp}/s7_logs"
mkdir -p "$LOGD"
BUCKET_PREFIX="${TMPDIR:-/tmp}/s7_bucket.$$"
rm -f "${BUCKET_PREFIX}".*
i=0
while IFS= read -r line; do
  [ -z "$line" ] && continue
  printf '%s\n' "$line" >> "${BUCKET_PREFIX}.$((i % PAR))"
  i=$((i + 1))
done < "$TF"

run_bucket() {
  local bf=$1
  while IFS=$'\t' read -r OP CHUNK; do
    local OUT LOG
    if [ -n "${CHUNK:-}" ]; then OUT="$CASE/$ODIR/${OP}.chunk${CHUNK//:/-}.json"
    else OUT="$CASE/$ODIR/${OP}.json"; fi
    if [ -s "$OUT" ]; then echo "SKIP $OP ${CHUNK:-}"; continue; fi
    LOG="$LOGD/run_${OP}_${CHUNK//:/-}.log"
    if [ -n "${CHUNK:-}" ]; then
      "$PY" "$TOOL" --case-dir "$CASE" --op "$OP" --chunk "$CHUNK" > "$LOG" 2>&1
    else
      "$PY" "$TOOL" --case-dir "$CASE" --op "$OP" > "$LOG" 2>&1
    fi
    echo "DONE $OP ${CHUNK:-} $?"
  done < "$bf"
}

for b in "${BUCKET_PREFIX}".*; do
  [ -e "$b" ] && run_bucket "$b" &
done
wait
rm -f "${BUCKET_PREFIX}".*
echo "TASKS-COMPLETE"
