#!/bin/sh
# 主控（本机审计版）：等本机审计 → 判定 → 干净则链 B 渲染；否则回灌重建 v006（链 A）→ 本机审计 → 链 B → 收尾
: "${OLSK_CASE_ROOT:?set OLSK_CASE_ROOT to the OLSK work directory}"
cd "$OLSK_CASE_ROOT" || exit 1
RUN=${START_RUN:-FILM_v005}; PLAY=${START_PLAY:-本体/S7-可播接近距离.v2.json}; ITER=1
while true; do
  if [ $ITER -gt 1 ]; then
    sh logs/v005_chain_A.sh $RUN $PLAY > logs/${RUN}_chainA.log 2>&1 || { echo "CHAIN_A_FAIL $RUN"; tail -5 logs/${RUN}_chainA.log; exit 1; }
    tail -3 logs/${RUN}_chainA.log
    sh logs/v005_audits_local.sh $RUN 6 > logs/${RUN}_audits_local.log 2>&1
  fi
  until grep -q "LOCAL_AUDITS_DONE" logs/${RUN}_audits_local.log 2>/dev/null; do sleep 120; done
  tail -1 logs/${RUN}_audits_local.log
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
sed -i '' "s/FILM_v00[0-9]/$RUN/g" logs/v005_finish.sh 2>/dev/null
sh logs/v005_finish.sh > logs/${RUN}_finish.log 2>&1; tail -30 logs/${RUN}_finish.log
echo "V005_MASTER_DONE $RUN $(date +%T)"
