#!/usr/bin/env python3
"""按分析线切片：Workbook 各步零件 + S3 绑定 movers + S8 紧固关系 + BOM 行 + S4 机构类 → 功能属性/tracks/<T>/inputs.json"""
import json, csv, re, sys, pathlib
CASE=pathlib.Path(__file__).resolve().parents[1]
TRACKS={
 'A':('机架与底座', ['WB-01.1','WB-01.2','WB-04','WB-05.1','WB-05.2','WB-05.3','WB-06','WB-14','WB-15','WB-41','WB-42','WB-43','WB-47','WB-48.1','WB-48.2','WB-48.3','WB-49','WB-51','WB-52']),
 'B':('Y 轴与肩部', ['WB-02.1','WB-02.2','WB-03','WB-07','WB-08','WB-09','WB-10','WB-11','WB-12','WB-13','WB-16','WB-17','WB-40','WB-44','WB-63']),
 'C':('X 轴、Z 轴与主轴头', ['WB-18','WB-19.1','WB-19.2','WB-19.3','WB-20','WB-21','WB-22','WB-23','WB-24','WB-25','WB-26','WB-27','WB-28','WB-29','WB-30','WB-31','WB-32','WB-33','WB-34','WB-35','WB-36','WB-37','WB-38','WB-39']),
 'D':('床身、刀库与刀具辅助系统', ['WB-56.1','WB-56.2','WB-57','WB-58','WB-71.1','WB-71.2','WB-71.3','WB-71.4']),
 'E':('电气与控制', ['WB-53.1','WB-54','WB-55.1','WB-55.2','WB-55.3','WB-55.4','WB-55.5','WB-55.6','WB-59.4','WB-61','WB-66','WB-67','WB-69.1','WB-69.2','WB-69.3','WB-69.4','WB-70.1','WB-70.2','WB-70.3','WB-70.4']),
 'F':('气动系统', ['WB-53.2','WB-71.1','WB-71.2','WB-71.3','WB-71.4','WB-71.5','WB-71.6']),
 'G':('外壳、门窗与防护', ['WB-45','WB-46.1','WB-46.2','WB-50','WB-59.1','WB-59.2','WB-59.3','WB-60','WB-62','WB-64.1','WB-64.2','WB-64.3','WB-64.4','WB-65.1','WB-65.2','WB-65.3','WB-65.4','WB-68','WB-72','WB-73','WB-74','WB-75','WB-76','WB-77','WB-78']),
 'H':('外购件与标准件家族', []),  # 全机
}
w=json.load(open(CASE/'装配工艺/sop-olsk-workbook.v1.json'))
ops={o['op']:o for o in w['operations']}
s3=json.load(open(CASE/'本体/S3-工序实例绑定.v1.json'))
s3ops={o['op']:o for o in s3['operations']}
s8=json.load(open(CASE/'本体/S8-连接关系图.v1.json'))
node={n['occ']:n for n in s8['nodes']}
fastens=s8['fastens']
s4=json.load(open(CASE/'本体/S4-机构识别.v1.json'))
sc=json.load(open(CASE/'装配动画/FILM_v009/state-chain.json'))
chap={c['op']:i for i,c in enumerate(sc['chapters'])}
bom=list(csv.reader(open(CASE/'功能属性/inputs/BOM_sheet1.csv')))
# BOM 分组：无数量行 = 顶层装配名
groups=[]; cur=None
for r in bom[1:]:
    name=r[0].strip(); qty=r[1].strip() if len(r)>1 else ''
    if name and not qty: cur=name; continue
    if name: groups.append({'group':cur,'part':name,'qty':qty})
bench=json.load(open(CASE/'装配工艺/manual-bench-groups.v1.json'))
def norm(s): return re.sub(r'[^a-z0-9]','',s.lower())
for T,(title,oplist) in TRACKS.items():
    out={'track':T,'title':title,'ops':[], 'bom_groups':{}, 'mechanism_classes':s4['classes'] if T=='H' else [c['class'] for c in s4['classes']], 'bench_groups':s3.get('bench_groups'), 'film_chapter_index':chap}
    names=set()
    for op in (oplist or list(ops)):
        o=ops[op]; b=s3ops.get(op,{})
        movers=[{'occ_uid':m.get('occ_uid'),'body_id':m.get('body_id'),'kind':m.get('kind'),'product':m.get('product')} for m in b.get('movers',[])]
        occs={m.get('occ') for m in b.get('movers',[]) if m.get('occ')}
        fl=[]
        for f,cl in fastens.items():
            if f in occs or any(c in occs for c in cl):
                fl.append({'fastener':node.get(f,{}).get('product'),'clamps':[node.get(c,{}).get('product') for c in cl]})
        for p in o['parts']: names.add(norm(p['name']))
        out['ops'].append({'op':op,'step':o['step'],'title':o['title'],'chapter':o['chapter'],'order_index':b.get('order_index'),'film_chapter':chap.get(op),'bench_group':b.get('bench_group'),'is_wiring':b.get('is_wiring'),'parts':o['parts'],'tools':o['tools'],'how_tos':o['how_tos'],'notes':o.get('notes'),'remarks':o.get('remarks'),'problems':o.get('problems'),'details':o.get('details'),'fasteners':o.get('fasteners'),'movers_in_film':movers,'mover_count':len(movers),'fasten_links':fl[:60]})
    for g in groups:
        if T=='H' or norm(g['part']) in names or any(norm(g['part']) in n or n in norm(g['part']) for n in names if len(n)>6):
            out['bom_groups'].setdefault(g['group'] or '?',[]).append(g)
    d=CASE/'功能属性/tracks'/T; d.mkdir(parents=True,exist_ok=True)
    json.dump(out,open(d/'inputs.json','w'),ensure_ascii=False,indent=1)
    print(T,title,len(out['ops']),'ops', sum(len(v) for v in out['bom_groups'].values()),'bom rows', sum(o['mover_count'] for o in out['ops']),'movers')
