#!/bin/sh
# 本机审计后备（dell 不可达时）：穿模审计 + 兄弟件审计 + 画面口径扫描，xargs -P 并行，跳过已有结果
: "${OLSK_CASE_ROOT:?set OLSK_CASE_ROOT to the OLSK work directory}"
cd "$OLSK_CASE_ROOT" || exit 1
export OPENBLAS_NUM_THREADS=1 OMP_NUM_THREADS=1 VECLIB_MAXIMUM_THREADS=1 REBUILD_CASE_DIR=$(pwd)
RUN=${1:-FILM_v005}; PAR=${2:-6}
mkdir -p logs/audit_$RUN
# 增量承接：与上一版章节输入相同的工序直接承接审计记录（carry_forward_audits.py 会写明 carried_from）
PREV=$(ls -d 装配动画/FILM_v00[0-9] | grep -v "$RUN" | sort | tail -1 | xargs basename)
[ -n "$PREV" ] && .venv/bin/python tools/carry_forward_audits.py --case-dir . --from $PREV --to $RUN
.venv/bin/python -c "import json;print('\n'.join(c['op'] for c in json.load(open('装配动画/$RUN/state-chain.json'))['chapters'] if c.get('movers')))" > logs/audit_ops_$RUN.txt
run_kind() {
  kind=$1; tool=$2; extra=$3; suffix=$4
  for op in $(cat logs/audit_ops_$RUN.txt); do
    [ -f "本体/S7-穿模审计/$op.$RUN.$suffix" ] && continue
    echo "$op"
  done | xargs -P $PAR -I{} sh -c "nice -n 5 .venv/bin/python tools/$tool --op '{}' --run $RUN $extra > 'logs/audit_$RUN/${kind}_{}.log' 2>&1; echo '$kind {} done'"
}
run_kind pen audit_path_penetration_generic.py "--scene film --geom real --step 0.25" film.real.json
run_kind sib audit_sibling_paths_generic.py "--step 0.25" siblings.json
run_kind vis visual_overlap_scan.py "" visual.json
echo "LOCAL_AUDITS_DONE $(ls 本体/S7-穿模审计/*.$RUN.film.real.json | wc -l) pen, $(ls 本体/S7-穿模审计/*.$RUN.siblings.json | wc -l) sib, $(ls 本体/S7-穿模审计/*.$RUN.visual.json | wc -l) vis $(date +%T)"
