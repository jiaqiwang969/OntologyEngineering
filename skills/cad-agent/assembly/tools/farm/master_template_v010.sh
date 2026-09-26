#!/bin/sh
# v010 主控：准备（合并行走/可播表 v5/单元落成/配置切 S3 v2）→ 链 A（冻结/构建/回读/可读性）→ 预览帧 → 本机审计 → 判定
: "${OLSK_CASE_ROOT:?set OLSK_CASE_ROOT to the OLSK work directory}"
cd "$OLSK_CASE_ROOT" || exit 1
LOG=logs/v010_master.log
echo "V010_MASTER start $(date +%T)" >> $LOG
sh logs/v010_prep.sh > logs/v010_prep.log 2>&1 || { echo "V010_PREP_FAIL" >> $LOG; tail -20 logs/v010_prep.log >> $LOG; exit 1; }
tail -6 logs/v010_prep.log >> $LOG
sh logs/v010_chain_A_seq2.sh FILM_v010 本体/S7-可播接近距离.v5-unit.json > logs/FILM_v010_chainA.log 2>&1 || { echo "V010_CHAIN_A_FAIL" >> $LOG; tail -20 logs/FILM_v010_chainA.log >> $LOG; exit 1; }
tail -3 logs/FILM_v010_chainA.log >> $LOG
.venv/bin/python - >> $LOG 2>&1 <<'PY'
import json
sc=json.load(open('装配动画/FILM_v010/state-chain.json'))
units=[(i+1,c['op'],[m['product'][:30] for m in c.get('movers',[]) if str(m.get('occ','')).startswith('unit:')]) for i,c in enumerate(sc['chapters']) if any(str(m.get('occ','')).startswith('unit:') for m in c.get('movers',[]))]
print('frames',sc['frames'],'chapters',len(sc['chapters']),'unit-join chapters',len(units)); print(units[:20])
want=['WB-02.1','WB-03','WB-05.1','WB-06','WB-07','WB-10','WB-12','WB-13','WB-16','WB-24','WB-25','WB-29','WB-42','WB-49','WB-56.1','WB-56.2','WB-57','WB-59.1','WB-60','WB-63','WB-64.4','WB-65.1','WB-71.2','WB-76','WB-77','整机装配态']
frames=[]; meta=[]
for i,c in enumerate(sc['chapters']):
    if c['op'] not in want: continue
    ms=c.get('movers',[])
    if not ms: fs=[c['start'],(c['start']+c['end'])//2,c['end']-1]
    else:
        m0=ms[0]; ml=ms[-1]; fs=[m0['f0']+1,(m0['f0']+m0['f1'])//2,m0['f1']+3,(ml['f0']+ml['f1'])//2,min(ml['f1']+3,c['end']-1)]
    for f in fs: frames.append(f); meta.append((i+1,c['op'],f))
json.dump({'frames':sorted(set(frames)),'meta':meta},open('logs/v010_preview_frames.json','w')); print('preview frames',len(set(frames)))
PY
/opt/homebrew/bin/blender -b 装配动画/FILM_v010/assembly_source.blend --python tools/render_frames_list.py -- logs/v010_preview_frames.json 装配动画/FILM_v010/preview 48 > logs/v010_preview_render.log 2>&1; echo "preview rendered $(ls 装配动画/FILM_v010/preview | wc -l) $(date +%T)" >> $LOG
sh logs/v005_audits_local.sh FILM_v010 8 > logs/FILM_v010_audits_local.log 2>&1
grep "\[carry\]" logs/FILM_v010_audits_local.log | tail -2 >> $LOG; tail -1 logs/FILM_v010_audits_local.log >> $LOG
PEN=$(.venv/bin/python -c "import json,glob;print(sum(json.load(open(f))['penetrates'] for f in glob.glob('本体/S7-穿模审计/*.FILM_v010.film.real.json')))")
SIB=$(.venv/bin/python -c "import json,glob;print(sum(1 for f in glob.glob('本体/S7-穿模审计/*.FILM_v010.siblings.json') for r in json.load(open(f)).get('rows',[]) if r.get('verdict')=='PENETRATES'))")
VIS=$(.venv/bin/python -c "import json,glob;print(sum(json.load(open(f))['summary']['PATH_OVERLAP'] for f in glob.glob('本体/S7-穿模审计/*.FILM_v010.visual.json')))")
echo "FILM_v010 audits: film_penetrates=$PEN sibling_penetrates=$SIB visual_path=$VIS $(date +%T)" >> $LOG
.venv/bin/python tools/audit_sequence_constraints.py --case-dir . --run FILM_v010 --out 功能属性/order-audit.FILM_v010.v1.json >> $LOG 2>&1
echo "V010_MASTER_DONE $(date +%T)" >> $LOG
