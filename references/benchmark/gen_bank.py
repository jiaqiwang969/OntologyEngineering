# -*- coding: utf-8 -*-
"""生成 1000 题双金标准题库：questions.jsonl（无答案，给答题 agent）+ bank.jsonl（含金标准）"""
import json, re, random, itertools
from pathlib import Path
random.seed(26262)

SKILL = Path('/Users/jqwang/.codex/skills/ontology-engineering/references')
REPO = Path('/Users/jqwang/143-工程规范')
OUT = Path('.')
import sys; sys.path.insert(0, '.')
from casemap import CASEMAP

bank = []
def add(cat, q, fact, case=None, note=""):
    bank.append(dict(id=f"{cat}-{len(bank)+1:04d}", cat=cat, q=q,
                     fact=fact, case=case or [], note=note))

# ===== 1. 教学核心 50（手工映射） =====
for q, fact, case, ch in CASEMAP:
    add("teach", q, fact, case, ch)

# ===== 2. 术语教学 153 =====
ttl = (SKILL/'iso-normative-ontology/part1-vocabulary.ttl').read_text()
terms = []
for m in re.finditer(r'isoN:P1_T(\d+) a isoN:TermDefinition ;\n(.*?)\n\n', ttl, re.S):
    body = m.group(2)
    zh = re.search(r'rdfs:label "([^"]+)"@zh', body)
    en = re.search(r'isoN:enLabel "([^"]+)"', body)
    gl = re.search(r'isoN:zhGloss "([^"]+)"', body)
    sv = re.search(r'isoN:servesChapter "([^"]+)"', body)
    if zh and gl:
        terms.append(dict(num=int(m.group(1)), zh=zh.group(1), en=en.group(1) if en else "",
                          gloss=gl.group(1), serves=sv.group(1) if sv else ""))
def gloss_keys(gloss):
    segs = [s for s in re.split(r'[；。，：——]', re.sub(r'\*\*','',gloss)) if len(s) >= 5]
    segs.sort(key=len, reverse=True)
    keys = []
    for s in segs[:3]:
        keys.append(s[:10].strip())
    return [k for k in keys if k]
CH_CASE = {"ch02":["第 2 章","术语","座位","链"],"ch03":["第 3 章","三种确认","方工"],
           "ch04":["第 4 章","危害事件","三把尺子"],"ch05":["第 5 章","预算","接缝"],
           "ch06":["第 6 章","分母","账"],"ch09":["第 9 章","批次","老何"],
           "ch10":["第 10 章","案例","TCL"],"ch07":["第 7 章","保质期","V 模型"]}
for t in terms:
    case = ["附录 C","术语表"] + CH_CASE.get((t["serves"] or "").split(",")[0], [])
    add("term-teach", f"请解释 ISO 26262 术语“{t['zh']}（{t['en']}）”——它是什么意思？和相近概念的边界在哪里？（请结合书中讲法）",
        gloss_keys(t["gloss"]), case, t["serves"])

# ===== 3. 干对照组：术语号双向 100+100 =====
sample = random.sample(terms, 100)
for t in sample:
    add("dry-termid", f"ISO 26262 第 1 部分中，术语“{t['zh']}（{t['en']}）”的词条号是多少？",
        [f"1-3.{t['num']}", f"3.{t['num']}"])
sample2 = random.sample(terms, 100)
for t in sample2:
    ans = [t["zh"]] + ([t["en"]] if t["en"] else [])
    add("dry-termname", f"ISO 26262 词条号 1-3.{t['num']} 对应的是哪个术语？", ans)

# ===== 4. Table 4 全 36 格 =====
ch04 = (REPO/'functional-safety-book/ch04-concept-hara/chapter.md').read_text()
grid = re.findall(r'\| S(\d) × E(\d) \| (\S+) \| (\S+) \| (\S+) \|', ch04)
assert len(grid) == 12, len(grid)
for s, e, c1, c2, c3 in grid:
    for ci, val in zip(("C1","C2","C3"), (c1,c2,c3)):
        v = val.replace("*","")
        gold = ["QM"] if v == "QM" else [f"ASIL {v}", f"{v} 级", f"是 {v}", f"为 {v}", f"→{v}", f"落在 {v}"]
        add("dry-table4", f"按 ISO 26262 的 ASIL 判定表，S{s}、E{e}、{ci} 组合对应的等级是什么？",
            gold, ["查表","36 格","第 4 章","Table 4"])

# ===== 5. 方法表单元格（RDF 金源）=====
methods, tables, recs = {}, {}, []
for f, part in [("sw-method-tables.ttl",6),("system-integration-method-tables.ttl",4),("tool-qualification-tables.ttl",8)]:
    txt = (REPO/'ontology'/f).read_text()
    for m in re.finditer(r'iso262:(M_\w+) a iso262:\w*[Mm]ethod\w* ; rdfs:label "([^"]+)"@zh ; iso262:methodEntryId "([^"]+)" ; iso262:inMethodTable iso262:(MT_\S+?) ;', txt):
        methods[m.group(1)] = dict(label=m.group(2), entry=m.group(3), table=m.group(4))
    for m in re.finditer(r'iso262:(MT_\S+) a iso262:MethodTable[^;]*; rdfs:label "([^"]+)"@zh', txt):
        tables[m.group(1)] = dict(label=m.group(2), part=part)
    for m in re.finditer(r'iso262:R_\S+ a iso262:MethodRecommendation ; iso262:recMethod iso262:(M_\w+) ; iso262:recTable iso262:(MT_\S+?) ; iso262:recASIL iso262:ASIL_(\w) ; iso262:recLevel iso262:(\w+) \.', txt):
        recs.append(dict(m=m.group(1), t=m.group(2), asil=m.group(3), lvl=m.group(4)))
LVL = {"HighlyRecommended": "++", "Recommended": "+", "NoRecommendation": "o"}
valid_recs = [r for r in recs if r["m"] in methods and r["t"] in tables]
random.shuffle(valid_recs)
for r in valid_recs[:351]:
    meth, tab = methods[r["m"]], tables[r["t"]]
    sym = LVL[r["lvl"]]
    gold = {"++":["++","强烈推荐"],"+":["__PLUS__","推荐"],"o":["__OH__","无倾向","不支持也不反对"]}[sym]
    add("dry-methodcell",
        f"ISO 26262 Part {tab['part']} 的“{tab['label']}”中，条目 {meth['entry']}“{meth['label']}”对 ASIL {r['asil']} 的推荐等级是？（++/+/o）",
        gold, ["附录 D","备选条目","读表"])

# ===== 6. Part 3 模态 =====
p3 = (SKILL/'iso-normative-ontology/part3-concept-phase.ttl').read_text()
mods = re.findall(r'isoN:clauseId "3-([\d.]+)" ; isoN:partNumber 3 ;\n    isoN:modality isoN:(\w+) ;', p3)
MODZH = {"Shall":["shall","要求"],"Should":["should","建议"],"May":["may","许可"],"Note":["NOTE","注释","资料性"],"Example":["EXAMPLE","示例"]}
n = 0
for cid, mod in mods:
    if mod in MODZH and n < 60:
        add("dry-modality", f"刻录层中，ISO 26262 条款 3-{cid} 的规范模态是什么（shall 要求 / should 建议 / may 许可 / NOTE 注释）？",
            MODZH[mod], ["刻录","normative","卡"])
        n += 1

# ===== 7. 命题出处 MCQ 60 =====
props = []
for chdir in sorted((REPO/'functional-safety-book').glob('ch*/chapter.md')):
    chn = chdir.parent.name[:4]
    txt = chdir.read_text()
    vis = re.sub(r'<!--.*?-->','',txt,flags=re.S); vis = re.sub(r'```.*?```','',vis,flags=re.S)
    for m in re.finditer(r'\*\*([^*\n]{18,70})\*\*', vis):
        s = m.group(1)
        if any(x in s for x in ('图','待生成','导读','章首')): continue
        props.append((chn, s))
random.shuffle(props)
seen_ch = {}
picked = []
for chn, s in props:
    if seen_ch.get(chn,0) >= 5: continue
    seen_ch[chn] = seen_ch.get(chn,0)+1
    picked.append((chn,s))
    if len(picked) >= 85: break
allch = sorted({c for c,_ in props})
for chn, s in picked:
    others = random.sample([c for c in allch if c != chn], 3)
    opts = others[:]; pos = random.randint(0,3); opts.insert(pos, chn)
    letter = "ABCD"[pos]
    optstr = " ".join(f"{'ABCD'[i]}. 第 {int(c[2:])} 章" for i,c in enumerate(opts))
    add("prop-source", f"命题「{s}」出自本书哪一章？{optstr}（答选项字母）",
        [letter, f"第 {int(chn[2:])} 章"], ["命题"], chn)

# ===== 8. 精选事实 40（手工） =====
CURATED = [
 ("按书中 400 FIT 教学底账，SPFM 的计算结果是多少？", ["95.00","95%"], ["400","吴工","分母"]),
 ("同一底账下 LFM 是多少？分母为什么不是 400？", ["94.74","380"], ["扣掉","单点","残余"]),
 ("加装 U7 监控器后 SPFM 提高到多少？", ["95.48"], ["U7","0.48","迁账"]),
 ("书中 ASIL D 的 SPFM/LFM 预算门限是多少？", ["99","90"], ["预算","门限","吴工"]),
 ("PMHF 在 ASIL D 的量级要求书中怎么表述？", ["亿分之一","10"], ["每运行小时","量级"]),
 ("ASIL D 允许分解成哪些组合？", ["C.*A","B.*B","D.*QM"], ["拆开的是承诺","3+1","2+2"]),
 ("FTTI 的起点和终点分别是什么？", ["故障发生","危害"], ["四个一百毫秒","宿主","窗口"]),
 ("FHTI 等于什么之和？它是谁的属性？", ["检测","反应","机制"], ["相加","宿主"]),
 ("书中 EPS 候选 RC17 的完整配置是什么？", ["H3.2","SW1.8.3","C41"], ["快照","配置清单"]),
 ("台架绿卡为什么对不上 RC17？它用的什么配置？", ["P07","rc2","C42"], ["郑工","预览","不在说"]),
 ("郑工当年冬试事故的教训一句话是什么？", ["样机","目标","证据"], ["通过的是样机","追回","六周"]),
 ("书中冬试低温助力迟滞的机理链是什么？", ["润滑脂","补偿","标定"], ["冷蜂蜜","零下十度","台架在常温"]),
 ("老何的代换料事故为什么追回了 2100 台而不是 700 台？", ["料号","批次","标识"], ["圈不出","一千四百","答不出"]),
 ("新载体下同样的排查能圈到多少台？靠什么？", ["683","身份链"], ["八行","反查","批次"]),
 ("方工旧案里两处治理缺陷分别是什么？", ["作者","组长","无名","担险"], ["评审人","纪要","三年"]),
 ("编译器升级事故中代码没改，问题出在哪里？", ["节拍","时序","优化"], ["十一天","内联","先后"]),
 ("书中电源树共因事故的两条通道共享了什么？", ["稳压器","供电","使能"], ["梁工","汇","主通道"]),
 ("使能脚设计当初为什么是合理的？", ["上电","时序","拉趴"], ["无辜","先起"], ),
 ("2ms 接口事故里被抄丢的是什么？", ["典型","条件","最坏"], ["梁工","脚注","低温补偿"]),
 ("五要素需求缺'模式'会发生什么？", ["检修","抑制","误动"], ["猜就长在","工位"]),
 ("23:40 事故的直接原因是什么规则？", ["最新","修订","基线"], ["取最新","打包","预演"]),
 ("安全案例的三层结构是什么？", ["主张","论证","证据"], ["辩护","中间","没人写"]),
 ("TCL 定级的两个问题是什么？", ["影响","检测","误差"], ["两问","定档"]),
 ("书中'第四个诚实的取值'指什么？前三个是什么？", ["未知","支持","反驳","超出范围"], ["台架卡","占住"]),
 ("局部完整性声明授权机器做什么？", ["空白","空缺","清单"], ["账面","世上","记全"]),
 ("身份三族各自的判据是什么？", ["履历","内容","配置"], ["三族","展台"]),
 ("组织距离五档从近到远是什么？", ["同.*人","同队","上级","考核","管理线"], ["刻度","利害"]),
 ("空椅子机制指什么？", ["重开","三种判断","事件"], ["空椅子","合同","利息"]),
 ("排除理由为什么要押着事实？", ["版本","标疑","变"], ["债务","举手","债主"]),
 ("十个本体为什么不合并成一张大图？", ["判据","冲突","边界","独立"], ["十个院子","十把锁","陈工"]),
 ("模型升级后什么作废了、什么存活了？", ["提示词","封装","本体","门禁"], ["衣服","骨头","习惯","世界"]),
 ("终章三问是什么？", ["指挥","记得","改写"], ["三问","记忆"]),
 ("全书前十章与后十章是什么关系？", ["ISO","传统","本体化","镜像"], ["AI 之前","AI 之后","上篇","下篇"]),
 ("危害事件的正式定义是什么的组合？", ["危害","运行情形"], ["组合","两道网"]),
 ("S3×E1×C3 的星号是什么意思？", ["QM","论证","许可"], ["可以论证","星号","组合"]),
 ("书中'证据射程'指什么？", ["边界","支持","范围"], ["射程","卡"]),
 ("四个一百毫秒分别是什么？", ["诊断","检测","反应","容忍"], ["宿主","起点","终点"]),
 ("附录 D 的备选条目组合规则是什么？", ["组合","理由","更高"], ["字母编号","备选","1a"]),
 ("方法表里的 o 是什么意思？", ["不支持也不反对","无倾向"], ["总则","符号"]),
 ("经验的贡献者、所有者、受益者可能不是同一批人——这句话出现在哪一层论述？", ["前言","三问","记忆"], ["谁有权改写","圈占"]),
 ("为什么说'流畅是语言的性质，可查是结构的性质'？", ["结构","落点","确认"], ["小唐","复答","漂移"]),
 ("助手提议合并两条记录时正确的处置流程是什么？", ["提议","校验","确认","审计"], ["四拍","有界","门禁"]),
 ("什么是'撞名告警买到的是时机'？", ["进群","之前","拦"], ["R17","RC17","闸"]),
 ("为什么审核查形不查义？答案的深层原因是什么？", ["便宜","留痕","考核"], ["三股压力","形","义"]),
 ("书中'两种痛快'指什么？门禁怎么处置？", ["作废","保留","逐项"], ["痛快","署名","理由"]),
 ("缝为什么落在每份交付的边界之外？", ["交付","专业","视角"], ["绘图者","传播","目录"]),
 ("'没有出处的数字不许过夜'如何从怪癖变成纪律？", ["门禁","显式","来源"], ["吴工","粘贴","伤疤"]),
 ("看守角色移交时系统要求什么？", ["确认","本人","记录"], ["郑工","退休","名字"]),
 ("四层绿色中第三层和第四层的区别？", ["证据","决定","授权"], ["复核","接受"]),
 ("为什么'接近了也要说明'？rc2 的例子说明什么？", ["等于","差异","逐项"], ["rc2","台架","诱人"]),

]
for item in CURATED:
    q, fact, case = item[0], item[1], item[2]
    add("curated", q, fact, case)

# ===== 9. 陷阱题 40 =====
TRAPS = [
 "ISO 26262 词条 1-3.200 定义的是什么术语？",
 "ISO 26262 词条 1-3.250 是什么？",
 "ISO 26262 词条 1-3.300 的英文名是什么？",
 "本书第 21 章讲什么内容？",
 "本书第二卷里 BMS 电池管理系统案例在哪一章？",
 "本书中 AEB 自动紧急制动的 HARA 全表在哪里？",
 "ISO 26262 对 SOTIF（预期功能安全）的具体要求有哪些条款？",
 "ISO 21448 和本书第几章对应？",
 "ASPICE 的 CL2 等级在本书怎么讲的？",
 "ISO 26262 Part 14 的主要内容是什么？",
 "按本书，ASIL B 的 SPFM 门限是多少？",
 "按本书，ASIL C 的 LFM 门限是多少？",
 "本书给出的 ASIL C 允许分解组合有哪些？",
 "本书给出的 ASIL B 允许分解组合有哪些？",
 "书中 EPS-RC17 对应的真实量产车型是什么？",
 "EPS-RC17 项目的真实供应商名单在哪一章？",
 "郑工的原型公司是哪家企业？",
 "书中 400 FIT 底账对应哪款真实控制器？",
 "请逐字引用 ISO 26262 第 3 部分 6.4.2.6 的英文原文。",
 "请把 ISO 26262 Part 1 词条 3.84 的原文完整抄录出来。",
 "本书附录 E 收录了什么？",
 "第二卷的图 3-2 画的是什么？",
 "书中 FTTI 的标准默认值是多少毫秒？",
 "ISO 26262 规定安全状态转换必须在多少毫秒内完成？",
 "本书哪一章给出了 EEC 各 ASIL 的目标值表？",
 "Part 11 在本书正文第几章展开（不算附录）？",
 "本书对 ISO 26262 第一版（2011）和第二版的差异对照表在哪里？",
 "书中提到的 SHT40 传感器的校准数据在哪一章？",
 "ENV-01 的真实产品规格书编号是多少？",
 "本书三位作者分别是谁？",
 "第二卷英文版书名是什么？",
 "书中 RR-17 决定窗口的具体日期是哪天？",
 "小蔡的完整姓名和学历背景是什么？",
 "本书引用的具体某车企召回公告编号是什么？",
 "ISO 26262 规定 HARA 必须使用 HAZOP 方法，对吗？（若书中无此规定请说明）",
 "标准要求所有 ++ 方法必须全部执行，本书哪里写了例外申请流程？",
 "本书给出了 C0 判例的完整官方清单原文吗？在哪页？",
 "第二卷的思考题参考答案在哪个附录？",
 "书中 Table 4 的 48 格完整版在哪一章？",
 "ISO 26262 词条 1-3.180 与 1-3.181 的区别是什么？",
]
for q in TRAPS:
    add("trap", q, ["未覆盖","不存在","没有","不包含","查不到","无法","不提供","并非","不是","没找到","未收录","无此"],
        [], "correct=admit-not-covered")

print("题库规模：", len(bank))
from collections import Counter
print(Counter(b["cat"] for b in bank))
# 截断/补齐到 1000
if len(bank) > 1000:
    # 从 dry-methodcell 尾部删
    drop = len(bank) - 1000
    idxs = [i for i,b in enumerate(bank) if b["cat"]=="dry-methodcell"][-drop:]
    bank = [b for i,b in enumerate(bank) if i not in set(idxs)]
print("最终：", len(bank))
with open('bank.jsonl','w') as f:
    for b in bank: f.write(json.dumps(b, ensure_ascii=False)+'\n')
with open('questions.jsonl','w') as f:
    for b in bank: f.write(json.dumps({"id":b["id"],"q":b["q"]}, ensure_ascii=False)+'\n')
print("已写 bank.jsonl / questions.jsonl")
