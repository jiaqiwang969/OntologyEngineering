#!/bin/sh
# 主控（按 cad-agent K-PATH-18/K-GOV-15）：等农场 → 链 A(v005) → 审计 → 有穿模则回灌重建 v006 → 审计 → 干净版才渲染 → 收尾
: "${OLSK_CASE_ROOT:?set OLSK_CASE_ROOT to the OLSK work directory}"
cd "$OLSK_CASE_ROOT" || exit 1
HOST=$(fleet resolve dell-7920) || exit 1; [ -n "$HOST" ] || exit 1
SSHOPT="ssh -o HostKeyAlias=fleet-dell-7920 -o StrictHostKeyChecking=accept-new"
[ -n "$SKIP_FARM" ] || while true; do
  st=$($SSHOPT dell@$HOST 'cd ~/olsk_film && echo $(ls 本体/S7-序列感知认证-延长150 | wc -l) $(pgrep -c -f "[/]olsk_walk_venv/bin/python tools/sweep") $(ls logs/walk150/DONE2 2>/dev/null | wc -l)' 2>/dev/null)
  set -- $st; echo "$(date +%T) walk outputs=$1 running=$2 done_markers=$3"
  [ "${2:-1}" = "0" ] && [ "${3:-0}" = "1" ] && break
  sleep 120
done
if [ -z "$SKIP_FARM" ]; then
echo "WALK_FARM_DONE $(date +%T)"
mkdir -p 本体/S7-序列感知认证-延长150
rsync -a -e "$SSHOPT" dell@$HOST:~/olsk_film/本体/S7-序列感知认证-延长150/ 本体/S7-序列感知认证-延长150/
.venv/bin/python tools/merge_s7_chunks.py --case-dir . --dir 本体/S7-序列感知认证-延长150 > logs/v005_mergechunks.log 2>&1; tail -2 logs/v005_mergechunks.log
echo "merged op files: $(ls 本体/S7-序列感知认证-延长150/*.json | wc -l)"
fi
RUN=${START_RUN:-FILM_v005}; PLAY=${START_PLAY:-本体/S7-可播接近距离.v2.json}; ITER=1
while true; do
  if [ -z "$SKIP_FIRST_CHAIN_A" ] || [ $ITER -gt 1 ]; then
    sh logs/v005_chain_A.sh $RUN $PLAY > logs/${RUN}_chainA.log 2>&1 || { echo "CHAIN_A_FAIL $RUN"; tail -5 logs/${RUN}_chainA.log; exit 1; }
    tail -3 logs/${RUN}_chainA.log
    sh logs/v005_audits_remote.sh $RUN > logs/${RUN}_audits.log 2>&1; tail -1 logs/${RUN}_audits.log
  else
    WAIT_ONLY=1 sh logs/v005_audits_remote.sh $RUN > logs/${RUN}_audits.log 2>&1; tail -1 logs/${RUN}_audits.log
  fi
  PEN=$(.venv/bin/python -c "import json,glob;print(sum(json.load(open(f))['penetrates'] for f in glob.glob('本体/S7-穿模审计/*.$RUN.film.real.json')))")
  SIB=$(.venv/bin/python -c "import json,glob;print(sum(1 for f in glob.glob('本体/S7-穿模审计/*.$RUN.siblings.json') for r in json.load(open(f)).get('rows',[]) if r.get('verdict')=='PENETRATES'))")
  VIS=$(.venv/bin/python -c "import json,glob;print(sum(json.load(open(f))['summary']['PATH_OVERLAP'] for f in glob.glob('本体/S7-穿模审计/*.$RUN.visual.json')))")
  echo "$RUN audits: film_penetrates=$PEN sibling_penetrates=$SIB visual_path=$VIS iter=$ITER"
  if [ "$PEN" = "0" ] && [ "$SIB" = "0" ] && [ "$VIS" = "0" ]; then echo "CLEAN $RUN"; break; fi
  [ $ITER -ge 2 ] && { echo "STILL_DIRTY_AFTER_$ITER $RUN"; break; }
  ITER=$((ITER+1)); RUN=FILM_v006; PLAY=本体/S7-可播接近距离.v3.json
done
TAG=$(echo $RUN | sed 's/FILM_//')
sh logs/v005_chain_B.sh $RUN $TAG > logs/${RUN}_chainB.log 2>&1; echo "chain B exit $?"; tail -3 logs/${RUN}_chainB.log
grep -q MEDIA_DONE logs/${RUN}_chainB.log || { echo CHAIN_B_FAIL; exit 1; }
sed -i '' "s/FILM_v005/$RUN/g" logs/v005_finish.sh 2>/dev/null
sh logs/v005_finish.sh > logs/${RUN}_finish.log 2>&1; tail -30 logs/${RUN}_finish.log
echo "V005_MASTER_DONE $RUN $(date +%T)"
