#!/usr/bin/env bash
# wjq-C02 S7 扫掠农场：按绑定自动生成 (op, chunk) 任务表，xargs 并行。
# 用法: farm_s7_launch.sh <case-dir> <tool.py> <并行度> [chunk大小]
set -u
CASE=$1; TOOL=$2; PAR=${3:-24}; CH=${4:-45}; ODIR=${5:-本体/S7-runs}
cd "$(dirname "$0")/.."
PY=.venv/bin/python
$PY - "$CASE" "$CH" <<'EOF' > /tmp/s7_tasks.txt
import json,sys
case,ch=sys.argv[1],int(sys.argv[2])
b=json.load(open(f"{case}/本体/S3-工序实例绑定.v1.json"))
for o in b['operations']:
    n=len(o['movers'])
    if n==0:
        print(f"{o['op']}\t")
    elif n<=ch:
        print(f"{o['op']}\t")
    else:
        s=0
        while s<n:
            e=min(n,s+ch); print(f"{o['op']}\t{s}:{e}"); s=e
EOF
echo "任务表 $(wc -l < /tmp/s7_tasks.txt) 项，并行 $PAR"
mkdir -p /tmp/s7_logs
cat /tmp/s7_tasks.txt | xargs -P "$PAR" -I{} bash -c '
  IFS=$'"'"'\t'"'"' read -r OP CHUNK <<< "{}"
  OUTDIR="'"$CASE"'/'"$ODIR"'"
  if [ -n "$CHUNK" ]; then OUT="$OUTDIR/${OP}.chunk${CHUNK//:/-}.json"; else OUT="$OUTDIR/${OP}.json"; fi
  if [ -s "$OUT" ]; then echo "SKIP $OP $CHUNK"; exit 0; fi
  LOG="/tmp/s7_logs/${OP}_${CHUNK//:/-}.log"
  if [ -n "$CHUNK" ]; then
    '"$PY $TOOL"' --case-dir '"$CASE"' --op "$OP" --chunk "$CHUNK" > "$LOG" 2>&1
  else
    '"$PY $TOOL"' --case-dir '"$CASE"' --op "$OP" > "$LOG" 2>&1
  fi
  echo "DONE $OP $CHUNK $?"
'
echo "全部完成"
