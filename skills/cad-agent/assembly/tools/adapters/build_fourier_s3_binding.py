#!/usr/bin/env python3
"""Fourier-N1 S3：S5 单元折叠 + SOP 台架绑定 + 唯一消耗闭合。

折叠（K-ID-02 采购件/预装模块=求解单元；SOP 原文以整体安装）：
  ① 轴承子装配（NSK/GE8C/619/688/BEAR）→ 单元
  ② 执行器整装/FSA 替代模型 → 单元
  ③ PCBA：子树叶中电子件占比>=0.6 且叶数>=5 → 该子树根为单元
  ④ 显式单元：T5M电池、gr2-skey、急停模块、按键支架内板卡
折叠后动件 = 单元(1 个 mover，成员逐一列账) + 未折叠叶（结构件/螺钉逐颗）。

台架（SOP QP-Fourier-N1-001 A01 分区；左右由世界 X 裁定，+X=左待几何验证）：
  LEG-L/R（1.x）、ARMS-L/R（2.x）、WAIST（3.x）、TORSO-HEAD（4.x）、
  SHELLS（5.3）、BATTERY（5.5）；结合章 5.1/5.2/5.4 presence。
"""
import argparse, json, re, collections
from pathlib import Path

ELEC = re.compile(r'LQFP|SOT|0402|0603|0805|^R\d+|^C\d+|^L\d+|^U\d+|LED|USB|HDMI|RJ45|XT30|XT60|18650|PIN|pogo|POGO|CONN|插座|排针|贴片|电容|电阻|芯片|Battery|BMS', re.I)
BEARING = re.compile(r'NSK|GE8C|GE6|619|688ZZ|6806|BEAR|轴承', re.I)
ACTUATOR = re.compile(r'FSA\d+|执行器整装|数据替代', re.I)
EXPLICIT_UNIT = re.compile(r'T5M电池$|gr2-skey|急停\^|急停板|^POWER-TOP\^|按键支架', re.I)

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--case-dir", required=True, type=Path)
    a = ap.parse_args()
    s1 = json.loads((a.case_dir/"本体/S1-occurrence清单.v1.json").read_text())
    occ = s1['occurrences']; asm = s1['assembly_occurrences']

    def subtree_leaves(prefix):
        return [o for o in occ if o['nauo_path'].startswith(prefix+'/')]

    # ---- 折叠根探测（自顶向下，先到先得，不嵌套折叠）----
    fold_roots = []
    claimed = set()
    for ao in sorted(asm, key=lambda x: x['depth']):   # 浅层优先
        path = ao['nauo_path']
        if any(path.startswith(c+'/') or path==c for c in claimed):
            continue
        lv = subtree_leaves(path)
        if not lv: continue
        prod = ao['product']
        reason = None
        if EXPLICIT_UNIT.search(prod): reason='EXPLICIT_MODULE'
        elif BEARING.search(prod): reason='BEARING_UNIT'
        elif ACTUATOR.search(prod): reason='ACTUATOR_UNIT'
        else:
            ne = sum(1 for o in lv if ELEC.search(o['product']))
            if len(lv)>=5 and ne/len(lv)>=0.6: reason='PCBA'
        if reason:
            fold_roots.append({"unit_id":ao['occ_id'],"product":prod,"path":path,
                               "reason":reason,"member_leaves":len(lv)})
            claimed.add(path)
    folded_leaf_uids = {}
    for fr in fold_roots:
        for o in subtree_leaves(fr['path']):
            folded_leaf_uids[o['occ_uid']] = fr['unit_id']

    # ---- 台架划分 ----
    tops = {a2['occ_id']:a2 for a2 in asm if a2['depth']==1}
    def top_prefix(pid): return tops[pid]['nauo_path']
    # 坐标语义（聚类+肘关节-L/-R 锚点实测）：Z=上，Y=侧向，左=+Y；
    # 主簇外游离件（源 CAD 停放/错位件）单独入 OUTLIER-PARKED。
    def yof(o): return o['world'][1][3]
    def zof(o): return o['world'][2][3]
    MAIN = lambda o: (-500 < o['world'][0][3] < 500 and -600 < yof(o) < 300 and -800 < zof(o) < 700)

    benches = collections.defaultdict(list)   # bench -> [leaf occ dict]
    def assign(oid_list, bench):
        for o in oid_list: benches[bench].append(o)

    legs_p = top_prefix('asm_0005'); arms_p = top_prefix('asm_1198')
    waist_p = top_prefix('asm_0000'); rob_p = top_prefix('asm_0024'); shell_p = top_prefix('asm_1199')
    # 腿：二层单元实例按质心 X 分左右；下肢直挂叶给 LEGS-JOIN
    # 左右分侧：S2 连通分量整体定侧。单件质心 Y 中线分侧踩过坑——中线附近的
    # 螺钉与原点异常件会被分错（实测"左大腿"件落入 LEG-R 台架，S8 报 75 例
    # 跨侧夹持异常）。左右腿各是一个 S2 连通体，按分量 Y 均值整体归侧。
    uid2idx = {o['occ_uid']: i for i, o in enumerate(occ)}
    adj_s2 = collections.defaultdict(set)
    for gname in ("本体/S2-共轴界面图.v1.json", "本体/S2-平面界面图.v1.json"):
        gp = a.case_dir / gname
        if gp.exists():
            g2 = json.loads(gp.read_text())
            for p2 in g2.get("occurrence_pairs", []):
                adj_s2[p2["a_occ"]].add(p2["b_occ"])
                adj_s2[p2["b_occ"]].add(p2["a_occ"])

    def side_split(lv):
        idxs = {uid2idx[o['occ_uid']] for o in lv}
        by_i = {uid2idx[o['occ_uid']]: o for o in lv}
        comp_of, comps = {}, []
        for i in idxs:
            if i in comp_of:
                continue
            stack, comp = [i], []
            comp_of[i] = len(comps)
            while stack:
                cur = stack.pop()
                comp.append(cur)
                for j in adj_s2.get(cur, ()):
                    if j in idxs and j not in comp_of:
                        comp_of[j] = len(comps)
                        stack.append(j)
            comps.append(comp)
        cy_all = sum(yof(o) for o in lv) / max(len(lv), 1)
        out = {}
        for comp in comps:
            ys = [yof(by_i[i]) for i in comp]
            cy_c = sum(ys) / len(ys)
            side = 'L' if cy_c >= cy_all else 'R'
            for i in comp:
                out[by_i[i]['occ_uid']] = side
        return out

    leg_units = [a2 for a2 in asm if a2['nauo_path'].startswith(legs_p+'/') and a2['depth']==2]
    for lu in leg_units:
        lv = subtree_leaves(lu['nauo_path'])
        kind = 'THIGH' if '大腿' in lu['product'] else ('SHIN' if '小腿' in lu['product'] else 'LEGPART')
        sides = side_split(lv)
        for o in lv:
            benches[f"LEG-{sides[o['occ_uid']]}-{kind}"].append(o)
    leg_direct = [o for o in occ if o['nauo_path'].startswith(legs_p+'/') and o['occ_uid'] not in {x['occ_uid'] for b in benches.values() for x in b}]
    assign(leg_direct, "LEGS-JOIN")
    # 臂：扁平叶按 X 分左右（|x|<1mm 的且名含 -L/-R 按名，其余按 X 就近）
    arm_lv = subtree_leaves(arms_p)
    cy_arm = sum(yof(o) for o in arm_lv)/max(len(arm_lv),1)
    for o in arm_lv:
        nm = o['product']
        if '-L' in nm and '-LR' not in nm: side='L'
        elif '-R' in nm and '-LR' not in nm: side='R'
        else: side = 'L' if yof(o) >= cy_arm else 'R'
        benches[f"ARM-{side}"].append(o)
    assign(subtree_leaves(waist_p), "WAIST")
    assign(subtree_leaves(shell_p), "SHELLS")
    # Robben：电池单元叶→BATTERY；其余→TORSO-HEAD
    bat_root = next(fr for fr in fold_roots if fr['reason']=='EXPLICIT_MODULE' and 'T5M电池' in fr['product'])
    for o in subtree_leaves(rob_p):
        if not MAIN(o):
            benches['OUTLIER-PARKED'].append(o)
        elif o['occ_uid'] in folded_leaf_uids and folded_leaf_uids[o['occ_uid']]==bat_root['unit_id']:
            benches['BATTERY'].append(o)
        else:
            benches['TORSO-HEAD'].append(o)

    # ---- 台架边界紧固件重指派：跨台架夹持的螺钉物理上在结合工序才装 ----
    # 膝关节界面螺钉按子树落进 THIGH/SHIN 台架，但被夹件在另一台架——那一刻
    # 对方台架的件根本不在场。此类紧固件移入 LEGS-JOIN（前驱含全部四个腿台架）。
    cfg2 = json.loads((a.case_dir / "robot-config.v1.json").read_text())
    FASTP = [re.compile(p) for p in cfg2["fastener_lexicon"]["patterns"]]
    coax_g = json.loads((a.case_dir / "本体/S2-共轴界面图.v1.json").read_text())
    coax_adj = collections.defaultdict(set)
    for p2 in coax_g["occurrence_pairs"]:
        gap2 = p2.get("min_radial_gap_mm")   # 0.0 是合法完美贴合，不能用 or 兜底
        if (p2.get("max_overlap_mm") or 0) >= 0.5 and gap2 is not None and gap2 <= 1.0:
            coax_adj[p2["a_occ"]].add(p2["b_occ"])
            coax_adj[p2["b_occ"]].add(p2["a_occ"])
    LEGB = {"LEG-L-THIGH", "LEG-L-SHIN", "LEG-R-THIGH", "LEG-R-SHIN"}
    bench_of = {}
    for bn, lvs in benches.items():
        for o in lvs:
            bench_of[o['occ_uid']] = bn
    moved_join = 0
    for bn in sorted(LEGB):
        keep = []
        for o in benches[bn]:
            is_f = any(rp.search(o['product']) for rp in FASTP)
            cross = False
            if is_f and o['occ_uid'] not in folded_leaf_uids:
                i0 = uid2idx[o['occ_uid']]
                for j in coax_adj.get(i0, ()):
                    ouid = occ[j]['occ_uid']
                    ob = bench_of.get(ouid)
                    if (ob in LEGB and ob != bn
                            and not any(rp.search(occ[j]['product']) for rp in FASTP)):
                        cross = True
                        break
            if cross:
                benches['LEGS-JOIN'].append(o)
                bench_of[o['occ_uid']] = 'LEGS-JOIN'
                moved_join += 1
            else:
                keep.append(o)
        benches[bn] = keep
    if moved_join:
        print(f"  膝界面紧固件移入 LEGS-JOIN: {moved_join}")

    # ---- 展平为 movers（折叠单元 1 mover + 未折叠叶逐个），唯一消耗校验 ----
    PRED = {"LEG-L-THIGH":[],"LEG-L-SHIN":[],"LEG-R-THIGH":[],"LEG-R-SHIN":[],
            "LEGS-JOIN":["LEG-L-THIGH","LEG-L-SHIN","LEG-R-THIGH","LEG-R-SHIN"],
            "ARM-L":[],"ARM-R":[],"WAIST":[],"TORSO-HEAD":[],
            "SHELLS":["TORSO-HEAD"],"BATTERY":["TORSO-HEAD"],
            "JOIN-TORSO-WAIST":["TORSO-HEAD","WAIST"],
            "JOIN-ARMS":["JOIN-TORSO-WAIST","ARM-L","ARM-R"],
            "JOIN-SHELLS":["JOIN-ARMS","SHELLS"],
            "JOIN-LEGS":["JOIN-SHELLS","LEGS-JOIN"],
            "JOIN-BATTERY":["JOIN-LEGS","BATTERY"],
            "OUTLIER-PARKED":[]}
    ORDER = ["LEG-L-THIGH","LEG-L-SHIN","LEG-R-THIGH","LEG-R-SHIN","LEGS-JOIN",
             "ARM-L","ARM-R","WAIST","TORSO-HEAD","SHELLS","BATTERY",
             "JOIN-TORSO-WAIST","JOIN-ARMS","JOIN-SHELLS","JOIN-LEGS","JOIN-BATTERY",
             "OUTLIER-PARKED"]
    seen=set(); ops_out=[]; unit_index={fr['unit_id']:fr for fr in fold_roots}
    for op in ORDER:
        lvs = benches.get(op, [])
        movers=[]; units_here=collections.OrderedDict()
        for o in lvs:
            uid=o['occ_uid']
            if uid in seen: raise SystemExit(f"双重消耗 {uid} @ {op}")
            seen.add(uid)
            # 游离停放件是源 CAD 垃圾位姿，不并入任何求解单元（否则单元刚体网格
            # 会横跨米级空间，且同一 unit_id 在两个台架出现破坏实体唯一性）
            fu = None if op=="OUTLIER-PARKED" else folded_leaf_uids.get(uid)
            if fu:
                units_here.setdefault(fu, []).append(o)
            else:
                movers.append({"occ_id":o['occ_id'],"occ_uid":uid,"product":o['product'],"kind":"leaf"})
        for fu, members in units_here.items():
            fr=unit_index[fu]
            movers.append({"occ_id":fu,"occ_uid":"unit:"+fr['path'],"product":fr['product'],
                           "kind":"solver_unit","reason":fr['reason'],
                           "member_count":len(members),
                           "member_occ_ids":[m['occ_id'] for m in members]})
        ops_out.append({"op":op,"predecessors":PRED[op],
                        "presence_only": op.startswith("JOIN-"),
                        "sop_anchor":{"LEG":"SOP 1.x","ARM":"SOP 2.x","WAIST":"SOP 3.x",
                                      "TORSO":"SOP 4.x","SHELLS":"SOP 5.3","BATTERY":"SOP 5.5",
                                      "JOIN":"SOP 5.x","LEGS":"SOP 1.x/5.4"}.get(op.split('-')[0],""),
                        "consumes_leaves":len(lvs),
                        "movers":movers})
    n_le=sum(o['consumes_leaves'] for o in ops_out)
    doc={"$schema":"assembly-ontology.s3-op-binding/v2","record_id":"FOURIER-S3-BINDING-REBUILD-V001",
         "purpose":"S5 折叠+SOP 台架绑定；唯一消耗闭合。",
         "coordinate_semantics":{"+Y":"左（肘关节-L/-R 锚点实测）","+Z":"上（主簇 1.17m 高）","X":"前后",
                                  "basis":"200mm 网格连通聚类 + 命名锚点；主簇 2671 叶，7 个游离件入 OUTLIER-PARKED"},
         "fold_summary":{"units":len(fold_roots),
                         "folded_leaves":len(folded_leaf_uids),
                         "by_reason":dict(collections.Counter(fr['reason'] for fr in fold_roots))},
         "fold_roots":fold_roots,
         "closure":{"leaf_occurrences":len(occ),"consumed":n_le,
                    "status":"PASS" if n_le==len(occ) else "FAIL"},
         "claim_boundary":[
           "折叠单元=SOP 整体安装语义（执行器/板卡/轴承/电池模块）；成员逐一列账（X1+ A4）。",
           "左右以世界 X 分侧，+X=左 为假设待已知左右件验证。",
           "SOP 46 工序为台架内子序证据，工序内粒度由 S7 行走裁定。"],
         "operations":ops_out}
    out=a.case_dir/"本体/S3-工序实例绑定.v1.json"
    out.write_text(json.dumps(doc,ensure_ascii=False,indent=1),encoding='utf-8')
    print(f"写出 {out}")
    print(f"  台架 {len(ops_out)}  叶消耗 {n_le}/{len(occ)}  闭合 {doc['closure']['status']}")
    print(f"  折叠单元 {len(fold_roots)}（{doc['fold_summary']['by_reason']}）折叠叶 {len(folded_leaf_uids)}")
    for o in ops_out:
        nm=sum(1 for m in o['movers'] if m['kind']=='leaf'); nu=sum(1 for m in o['movers'] if m['kind']=='solver_unit')
        print(f"   {o['op']:18s} 叶动件 {nm:4d} + 单元 {nu:3d}  (叶消耗 {o['consumes_leaves']})")
    return 0

if __name__=="__main__":
    raise SystemExit(main())
