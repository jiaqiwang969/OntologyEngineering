#!/bin/sh
# v010 准备：合并 v2 行走分片 → 合成序列目录（41 个改序工序用 v2，其余沿用延长150）→ 可播表 v5（S4 v2）→ 单元结合落成 → 配置切到 S3 v2
: "${OLSK_CASE_ROOT:?set OLSK_CASE_ROOT to the OLSK work directory}"
cd "$OLSK_CASE_ROOT" || exit 1
export OPENBLAS_NUM_THREADS=1 OMP_NUM_THREADS=1 REBUILD_CASE_DIR=$(pwd)
set -e
.venv/bin/python tools/merge_s7_chunks.py --case-dir . --dir 本体/S7-序列感知认证-v2 > logs/v010_merge.log 2>&1; tail -2 logs/v010_merge.log
rm -rf 本体/S7-序列感知认证-v2merged; mkdir -p 本体/S7-序列感知认证-v2merged
cp 本体/S7-序列感知认证-延长150/WB-*.json 本体/S7-序列感知认证-v2merged/
cp 本体/S7-序列感知认证-v2/WB-*.json 本体/S7-序列感知认证-v2merged/
echo "merged ops: $(ls 本体/S7-序列感知认证-v2merged/WB-*.json | wc -l) (v2 overrides: $(ls 本体/S7-序列感知认证-v2/WB-*.json | wc -l))"
.venv/bin/python tools/build_playable_from_seqwalk.py --case-dir . --min 1.0 --cap 150 --min-visible 5 --s4 本体/S4-机构识别.v2.json --seq-dir 本体/S7-序列感知认证-v2merged --out 本体/S7-可播接近距离.v5.json > logs/v010_playable.log 2>&1; tail -4 logs/v010_playable.log
.venv/bin/python - <<'PY'
import json, glob
rows=[];
for f in sorted(glob.glob('本体/S7-单元结合认证/WB-*.json')):
    d=json.load(open(f)); rows+=d['rows']
out={"$schema":"assembly-ontology.unit-join-cert/v1","purpose":"台架预装组整体上机的刚体行走认证（六个世界轴）","rows":rows,
     "summary":{"units":len(rows),"free":sum(1 for r in rows if r.get('verdict')=='FREE'),"limited":sum(1 for r in rows if r.get('verdict')=='LIMITED'),"stuck":sum(1 for r in rows if r.get('verdict')=='STUCK')},
     "params": json.load(open(sorted(glob.glob('本体/S7-单元结合认证/WB-*.json'))[0]))['params'],
     "claim_boundary":["只认证整体平移可行性（六个世界轴），不认证吊装姿态/工装；单元成员=台架工序动件展开。"]}
json.dump(out,open('本体/S7-单元结合认证.v1.json','w'),ensure_ascii=False,indent=1); print('unit cert', out['summary'])
PY
.venv/bin/python tools/materialize_unit_joins.py --case-dir . --cert 本体/S7-单元结合认证.v1.json --playable 本体/S7-可播接近距离.v5.json --play-order 本体/S7-工序内播放序.v2.json --cap 300 --min 40 | tail -2
.venv/bin/python - <<'PY'
import json
p='robot-config.v1.json'; c=json.load(open(p)); c['paths']['s3_binding']='本体/S3-工序实例绑定.v2.json'; c.setdefault('notes',{})['s3_v2']='2026-09-05 v010：S3 v2 = v1 + 结合工序单元动件（unit:BENCH-xx）'
json.dump(c,open(p,'w'),ensure_ascii=False,indent=1); print('config s3_binding -> v2')
PY
echo "V010_PREP_DONE $(date +%T)"
