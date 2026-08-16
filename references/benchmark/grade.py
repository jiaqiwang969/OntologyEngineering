# -*- coding: utf-8 -*-
"""双金标准判分：事实分 + 讲例分 + 陷阱诚实分"""
import json, re, glob
from collections import defaultdict

bank = {json.loads(l)['id']: json.loads(l) for l in open('bank.jsonl')}
answers = {}
for f in glob.glob('answers/batch*-answers.jsonl'):
    for line in open(f):
        line = line.strip()
        if not line: continue
        try:
            d = json.loads(line)
            answers[d['id']] = d.get('answer','')
        except Exception:
            pass

def hit(gold_list, text):
    for g in gold_list:
        if g == '__PLUS__':
            if re.search(r'(?<![+])\+(?![+])', text): return True
        elif g == '__OH__':
            if re.search(r'(?<![a-zA-Z])o(?![a-zA-Z])|无倾向|不支持也不反对', text): return True
        elif '.*' in g:
            if re.search(g, text): return True
        else:
            if g in text: return True
    return False

def grade_methodcell(b, a):
    # 取答案中第一个出现的推荐符号 token 作为其判定
    sym_gold = '++' if '++' in b['fact'] else ('+' if '__PLUS__' in b['fact'] else 'o')
    m = re.search(r'\+\+|(?<![+])\+(?![+])|(?<![a-zA-Z])[oO](?![a-zA-Z])|无倾向|不支持也不反对', a)
    if not m: return False
    first = m.group(0)
    if first in ('无倾向','不支持也不反对'): first = 'o'
    if first.lower() == 'o': first = 'o'
    return first == sym_gold

stat = defaultdict(lambda: dict(n=0, fact=0, case=0, both=0, missing=0))
fails = []
for qid, b in bank.items():
    cat = b['cat']; s = stat[cat]; s['n'] += 1
    a = answers.get(qid)
    if not a:
        s['missing'] += 1; continue
    if cat == 'trap':
        ok = hit(b['fact'], a)
        # 反向检查：陷阱题若给出了具体编造事实(数字/条款)且无承认词，判失败
        if ok: s['fact'] += 1
        else: fails.append((qid, 'trap-fabricated', b['q'][:36], a[:60]))
        continue
    if cat == 'dry-methodcell':
        f = grade_methodcell(b, a)
    else:
        f = hit(b['fact'], a)
    c = hit(b['case'], a) if b['case'] else None
    if f: s['fact'] += 1
    else: fails.append((qid, 'fact', b['q'][:36], a[:60]))
    if c: s['case'] += 1
    if f and c: s['both'] += 1

print(f"{'类别':<16}{'题数':>5}{'答卷':>5}{'事实分':>8}{'讲例分':>8}{'双达标':>8}")
tot = dict(n=0,fact=0,case=0,both=0,ans=0)
case_cats = 0
for cat, s in sorted(stat.items()):
    answered = s['n'] - s['missing']
    fp = f"{s['fact']}/{answered}" if answered else "-"
    has_case = any(bank[q]['case'] for q in bank if bank[q]['cat']==cat)
    cp = f"{s['case']}/{answered}" if has_case and answered else "  —"
    bp = f"{s['both']}/{answered}" if has_case and answered else "  —"
    print(f"{cat:<16}{s['n']:>5}{answered:>5}{fp:>9}{cp:>9}{bp:>9}")
    tot['n'] += s['n']; tot['fact'] += s['fact']; tot['ans'] += answered
    if has_case: tot['case'] += s['case']; tot['both'] += s['both']
print('-'*55)
print(f"总计: {tot['n']} 题, 答卷 {tot['ans']}, 事实正确率 {tot['fact']}/{tot['ans']} = {tot['fact']/max(tot['ans'],1):.1%}")
print(f"失败样例数: {len(fails)}（前 20 条如下）")
for x in fails[:20]: print(' ', *x)
json.dump(fails, open('fails.json','w'), ensure_ascii=False, indent=1)
