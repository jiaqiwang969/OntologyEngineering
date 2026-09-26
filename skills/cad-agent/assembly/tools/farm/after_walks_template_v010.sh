#!/bin/sh
: "${OLSK_CASE_ROOT:?set OLSK_CASE_ROOT to the OLSK work directory}"
cd "$OLSK_CASE_ROOT" || exit 1
until [ "$(ps -eo args | grep -c '[s]weep_sequence_reverse_generic.py --case-dir')" = "0" ] && [ "$(ps -eo args | grep -c '[w]alk_unit_joins.py --case-dir')" = "0" ]; do sleep 60; done
echo "farms done $(date +%T)" >> logs/v010_master.log
# 重跑：多台架组同章（WB-24/WB-29）、任何分量 STUCK/LIMITED<40、或成员未按连通分量拆分（无 component 字段）的结合工序
.venv/bin/python - > logs/unit_rerun_jobs.txt <<'PY'
import json, glob, os
s3=json.load(open('本体/S3-工序实例绑定.v1.json'))
multi={o['op'] for o in s3['operations'] if len(o.get('unit_joins') or [])>1}
rerun=set(multi)
for f in glob.glob('本体/S7-单元结合认证/WB-*.json'):
    d=json.load(open(f)); op=os.path.basename(f)[:-5]
    for r in d['rows']:
        if r.get('verdict') in ('STUCK','NO_GEOMETRY') or (r.get('best',{}).get('clear_run_mm',0)<40): rerun.add(op)
allops=[o['op'] for o in s3['operations'] if o.get('unit_joins')]
for op in allops:
    if not os.path.exists(f'本体/S7-单元结合认证/{op}.json'): rerun.add(op)
print('\n'.join(sorted(rerun)))
PY
echo "unit reruns: $(tr '\n' ' ' < logs/unit_rerun_jobs.txt)" >> logs/v010_master.log
mkdir -p logs/unit_walk2
.venv/bin/python ~/.codex/skills/cad-agent/assembly/tools/farm/farm_launcher.py --case-dir . --jobs logs/unit_rerun_jobs.txt --n 6 --log-dir logs/unit_walk2 --python .venv/bin/python --cmd "tools/walk_unit_joins.py --case-dir {case} --only {job} --cap 300 --coarse-step 2.0 --out 本体/S7-单元结合认证/{job}.json" > logs/unit_walk2/launcher.log 2>&1
grep -h "best=" logs/unit_walk2/*.log >> logs/v010_master.log
sh logs/v010_master.sh
