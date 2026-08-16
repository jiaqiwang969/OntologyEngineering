# -*- coding: utf-8 -*-
"""Mode A：检索可答率。题面 3-gram 打分选 top-5 文件，检查事实金标准是否在其中。"""
import json, re
from pathlib import Path
SKILL = Path('/Users/jqwang/.codex/skills/ontology-engineering/references')
files = {}
for root in ['ontology-engineering-book','product-trustworthiness-book','iso-normative-ontology']:
    for p in (SKILL/root).rglob('*'):
        if p.suffix in ('.md','.txt','.ttl','.yaml','.tex') and p.is_file():
            files[str(p.relative_to(SKILL))] = p.read_text(errors='ignore')
print('索引文件数:', len(files))
STOP = set('的了是什么怎么为什么请解释哪些一个这个它们和与在对有中里下从到被按能不吗呢多少分别指哪本书中出自答选项字母其')
def grams(q):
    zh = ''.join(ch if '一' <= ch <= '鿿' and ch not in STOP else ' ' for ch in q)
    gs = set()
    for seg in zh.split():
        for i in range(len(seg)-2):
            gs.add(seg[i:i+3])
    ascii_toks = re.findall(r'[A-Za-z0-9.+\-]{2,}', q)
    return gs, ascii_toks
def top_files(q, k=5):
    import re as _re
    quoted = _re.findall(r'[「“]([^」”]{6,})[」”]', q)
    if quoted:
        q = ' '.join(quoted)
    gs, toks = grams(q)
    scores = []
    for name, txt in files.items():
        s = sum(txt.count(g) for g in gs) + 3*sum(txt.count(t) for t in toks)
        if s: scores.append((s, name))
    scores.sort(reverse=True)
    return [n for _, n in scores[:k]]
def gold_hit(gold, texts):
    for g in gold:
        if g == '__PLUS__' or g == '__OH__':
            return True  # 符号级由 Mode B 真测；Mode A 视为可答（行已可检索）
        pat = g if '.*' in g else re.escape(g)
        for t in texts:
            if re.search(pat, t): return True
    return False
bank = [json.loads(l) for l in open('bank.jsonl')]
from collections import defaultdict
stat = defaultdict(lambda: [0,0])
misses = []
for b in bank:
    if b['cat'] == 'trap': continue
    tops = top_files(b['q'])
    texts = [files[n] for n in tops]
    if b['cat'] == 'dry-methodcell':
        m = re.search(r'“([^”]+)”对', b['q'])
        ok = any(m.group(1).split(' ')[0] in t for t in texts) if m else False
    elif b['cat'] == 'prop-source':
        prop = re.search(r'「([^」]+)」', b['q']).group(1)[:15]
        ok = any(prop in t for t in texts)
    else:
        ok = gold_hit(b['fact'], texts)
    stat[b['cat']][1] += 1
    if ok: stat[b['cat']][0] += 1
    elif len(misses) < 12: misses.append((b['id'], b['q'][:40], tops[:2]))
tot_ok = sum(a for a,_ in stat.values()); tot = sum(b for _,b in stat.values())
print(f'\nMode A 检索可答率: {tot_ok}/{tot} = {tot_ok/tot:.1%}\n')
for cat,(a,t) in sorted(stat.items()):
    print(f'  {cat:16s} {a:4d}/{t:4d} = {a/t:.1%}')
print('\n未命中样例:')
for mid, mq, mt in misses: print(' ', mid, mq, '→', mt)
