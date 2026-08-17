#!/usr/bin/env python3
"""ISO 26262 本体化刻录（形式 B）。

从本地受控提取件（ISO_SOURCE_ROOT）生成条款骨架，叠加 glosses/ 转述，
输出 TTL 正本与 Markdown 卡片视图。公开产物不含标准原文。

用法：python3 scripts/engrave_iso.py [--part 1] [--part 3]
"""
from __future__ import annotations
import json, os, re, sys
from pathlib import Path

SKILL = Path(__file__).resolve().parent.parent
NORM = SKILL / "references" / "iso-normative-ontology"
BOOK2 = SKILL / "references" / "product-trustworthiness-book"
SRC = Path(os.environ.get(
    "ISO_SOURCE_ROOT",
    "/Users/jqwang/143-工程规范/structured/mineru/ISO-26262-2018"))

PART_DIRS = {
    1: "part-01-vocabulary/native-full/ISO 26262-1-2018/auto/ISO 26262-1-2018_content_list_v2.json",
    3: "part-03-concept-phase/native-full/ISO 26262-3-2018/auto/ISO 26262-3-2018_content_list_v2.json",
}
PREFIX = "@prefix isoN: <https://ontology-engineering.local/iso26262/normative#> .\n" \
         "@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\n"


def blocks(path: Path):
    data = json.loads(path.read_text())
    for pi, page in enumerate(data):
        for bi, b in enumerate(page):
            parts = []
            c = b.get("content") or {}
            for arr in c.values():
                if isinstance(arr, list):
                    for t in arr:
                        if isinstance(t, dict) and t.get("content"):
                            parts.append(str(t["content"]))
            txt = " ".join(parts).strip()
            if txt:
                yield pi, bi, txt


def modality_of(text: str) -> str:
    t = f" {text} "
    if text.startswith("NOTE"): return "Note"
    if text.startswith("EXAMPLE"): return "Example"
    if " shall " in t or " shall." in t: return "Shall"
    if " should " in t: return "Should"
    if " may " in t or " can be " in t: return "May"
    return "Structural"


def esc(s: str) -> str:
    return s.replace("\\", "\\\\").replace('"', '\\"')


def load_glosses(part: int) -> dict:
    p = NORM / "glosses" / f"part{part}-glosses.yaml"
    if not p.exists(): return {}
    # 轻量 YAML 子集解析（无依赖）：顶层 "key": 后接缩进字段
    out, key, cur = {}, None, {}
    for ln in p.read_text().splitlines():
        if not ln.strip() or ln.strip().startswith("#"): continue
        m = re.match(r'^"([^"]+)":\s*$', ln)
        if m:
            if key: out[key] = cur
            key, cur = m.group(1), {}
            continue
        m = re.match(r"^\s+(zh|keywords|book):\s*(.*)$", ln)
        if m and key:
            k, v = m.group(1), m.group(2).strip()
            if v.startswith("["):
                cur[k] = [x.strip() for x in v.strip("[]").split(",") if x.strip()]
            else:
                cur[k] = v
    if key: out[key] = cur
    return out


def mine_part1_from_glossary() -> list[dict]:
    """从附录 C 挖 Part 1 术语转述（本 skill 自带，无需外部件）。"""
    text = (BOOK2 / "appendices" / "appendix-c-glossary.md").read_text()
    units = []
    pat = re.compile(
        r"^- \*\*(?P<zh>[^（*]+)（(?P<en>[^）]+)）\*\*"
        r"（1-3\.(?P<num>\d+)(?P<meta>[^）]*)）——(?P<gloss>.+)$", re.M)
    for m in pat.finditer(text):
        gloss = re.sub(r"\*\*", "", m.group("gloss")).strip()
        serves = ",".join(re.findall(r"ch\d+", m.group("meta")))
        units.append(dict(num=int(m.group("num")), zh=m.group("zh").strip(),
                          en=m.group("en").strip(), gloss=gloss, serves=serves))
    return units


def find_part1_anchor(units, extract_path: Path):
    if not extract_path.exists(): return
    index = {}
    for pi, bi, txt in blocks(extract_path):
        m = re.match(r"^3\.(\d+)(?:\s|$)", txt)
        if m:
            index.setdefault(int(m.group(1)), (pi, bi))
    for u in units:
        if u["num"] in index:
            u["anchor"] = index[u["num"]]


def load_cases():
    """教学案例注册表：一等对象。返回 {case_id: {ch, teaches[], summary}}"""
    p = NORM / "glosses" / "teaching-cases.yaml"
    out, key, cur = {}, None, {}
    for ln in p.read_text().splitlines():
        if not ln.strip() or ln.strip().startswith("#"): continue
        m = re.match(r"^([a-z0-9-]+):\s*$", ln)
        if m:
            if key: out[key] = cur
            key, cur = m.group(1), {}
            continue
        m = re.match(r"^\s+(ch|teaches|summary):\s*(.*)$", ln)
        if m and key:
            k, v = m.group(1), m.group(2).strip()
            cur[k] = [x.strip() for x in v.strip("[]").split(",")] if v.startswith("[") else v
    if key: out[key] = cur
    return out


def link_case(name, gloss, cases, threshold=4):
    """泛化连接（加权+门槛）：名称命中权重 3×len(kw)，转述命中 1×len(kw)；
    总分低于门槛不连（宁缺毋滥，兜底走 词条→章→案例 遍历）。"""
    best, score = None, 0
    for cid, c in cases.items():
        s = 0
        for kw in c.get("teaches", []):
            if not kw: continue
            if kw in name: s += 3 * len(kw)
            elif kw in gloss: s += len(kw)
        if s > score:
            best, score = cid, s
    return best if score >= threshold else None


def emit_cases_ttl(cases):
    ttl = [PREFIX, "# 教学案例层：书中场景/事故的一等对象（讲法关系的目标端）\n"]
    for cid, c in cases.items():
        iri = "isoN:Case_" + cid.replace("-", "_")
        ttl.append(f'{iri} a isoN:TeachingCase ;')
        ttl.append(f'    isoN:inChapter "{c.get("ch","")}" ;')
        kws = "、".join(k for k in c.get("teaches", []) if k)
        ttl.append(f'    isoN:teachesConcepts "{esc(kws)}" ;')
        ttl.append(f'    isoN:caseSummary "{esc(c.get("summary",""))}" .\n')
    (NORM / "teaching-cases.ttl").write_text("\n".join(ttl))


def engrave_part1():
    units = mine_part1_from_glossary()
    find_part1_anchor(units, SRC / PART_DIRS[1])
    cases = load_cases()
    emit_cases_ttl(cases)
    for u in units:
        cid = link_case(u["zh"] + " " + u["en"], u["gloss"], cases)
        if cid:
            u["case_id"] = cid
            u["taught"] = "[" + cases[cid].get("ch","") + "] " + cases[cid].get("summary","")
    ttl = [PREFIX, "# Part 1 术语刻录（转述来源：本书附录 C；模态=Definition）\n"]
    cards = ["# ISO 26262 Part 1 术语卡（本体化刻录 · 卡片视图）\n",
             "> 自动生成自 part1-vocabulary.ttl；转述为本书作者综合，非标准原文。\n"]
    for u in sorted(units, key=lambda x: x["num"]):
        iri = f"isoN:P1_T{u['num']}"
        ttl.append(f'{iri} a isoN:TermDefinition ;')
        ttl.append(f'    isoN:clauseId "1-3.{u["num"]}" ; isoN:partNumber 1 ;')
        ttl.append(f'    rdfs:label "{esc(u["zh"])}"@zh ; isoN:enLabel "{esc(u["en"])}" ;')
        ttl.append(f'    isoN:modality isoN:Definition ; isoN:glossStatus "glossed" ;')
        if u.get("serves"):
            ttl.append(f'    isoN:servesChapter "{u["serves"]}" ;')
        if u.get("anchor"):
            ttl.append(f'    isoN:pageIndex {u["anchor"][0]} ; isoN:blockIndex {u["anchor"][1]} ;')
        if u.get("case_id"):
            ttl.append(f'    isoN:taughtBy isoN:Case_{u["case_id"].replace("-","_")} ;')
        ttl.append(f'    isoN:zhGloss "{esc(u["gloss"])}" .\n')
        anchor = f'p{u["anchor"][0]}/b{u["anchor"][1]}' if u.get("anchor") else "待钉"
        cards.append(f'### 1-3.{u["num"]} {u["zh"]}（{u["en"]}）｜术语')
        cards.append(f'转述：{u["gloss"]}')
        if u.get("taught"):
            cards.append(f'书中讲法：{u["taught"]}')
        cards.append(f'映射：{u.get("serves") or "—"} ｜ 提取件锚点：{anchor}\n')
    nums = sorted(u["num"] for u in units)
    ttl.append(f'isoN:Coverage_P1 a isoN:CoverageDeclaration ; isoN:partNumber 1 ;')
    ttl.append(f'    isoN:coversRange "1-3.{nums[0]} 至 1-3.{nums[-1]}" ; isoN:unitCount {len(units)} ;')
    ttl.append(f'    rdfs:comment "本层 Part 1 收录以上词条；范围外词条号未收录，请回标准原文核对。"@zh .')
    cards.insert(2, f'> **覆盖声明**：本层收录词条 1-3.{nums[0]} 至 1-3.{nums[-1]} 共 {len(units)} 条；此范围之外的词条号本层未收录（判缺以此声明为界）。\n')
    (NORM / "part1-vocabulary.ttl").write_text("\n".join(ttl))
    (NORM / "part1-cards.md").write_text("\n".join(cards))
    return len(units)


def engrave_part3():
    path = SRC / PART_DIRS[3]
    glosses = load_glosses(3)
    units = []
    for pi, bi, txt in blocks(path):
        m = re.match(r"^(\d+(?:\.\d+)+)[\s\t]+(\S.*)$", txt)
        if not m: continue
        cid = m.group(1)
        if len(cid.split(".")) < 2: continue
        units.append(dict(cid=cid, pi=pi, bi=bi, mod=modality_of(txt)))
    ttl = [PREFIX, "# Part 3 概念阶段刻录（骨架=提取件自动生成；转述=glosses 叠加）\n"]
    cards = ["# ISO 26262 Part 3 条款卡（本体化刻录 · 卡片视图）\n",
             "> 骨架自动生成；已刻转述条目标 glossed，其余 pending，慢慢刻。\n"]
    n_glossed = 0
    for u in units:
        g = glosses.get(u["cid"])
        iri = "isoN:P3_" + u["cid"].replace(".", "_")
        ttl.append(f'{iri} a isoN:NormativeUnit ;')
        ttl.append(f'    isoN:clauseId "3-{u["cid"]}" ; isoN:partNumber 3 ;')
        ttl.append(f'    isoN:modality isoN:{u["mod"]} ;')
        ttl.append(f'    isoN:pageIndex {u["pi"]} ; isoN:blockIndex {u["bi"]} ;')
        if g:
            n_glossed += 1
            kw = "、".join(g.get("keywords", []))
            bk = "、".join(g.get("book", []))
            ttl.append(f'    isoN:zhGloss "{esc(g["zh"])}" ;')
            if kw: ttl.append(f'    isoN:keywords "{esc(kw)}" ;')
            if bk: ttl.append(f'    isoN:servesChapter "{esc(bk)}" ;')
            ttl.append('    isoN:glossStatus "glossed" .\n')
            cards.append(f'### 3-{u["cid"]}｜{u["mod"]}｜glossed')
            cards.append(f'转述：{g["zh"]}')
            cards.append(f'关键词：{kw or "—"} ｜ 映射：{bk or "—"} ｜ 锚点：p{u["pi"]}/b{u["bi"]}\n')
        else:
            ttl.append('    isoN:glossStatus "pending" .\n')
    pend = len(units) - n_glossed
    cards.append(f'---\n骨架总数：{len(units)} ｜ 已刻转述：{n_glossed} ｜ 待刻：{pend}')
    (NORM / "part3-concept-phase.ttl").write_text("\n".join(ttl))
    (NORM / "part3-cards.md").write_text("\n".join(cards))
    return len(units), n_glossed


if __name__ == "__main__":
    parts = [int(a.split("=")[-1]) for a in sys.argv[1:] if a.startswith("--part")] \
            or ([int(x) for x in sys.argv[2::2]] if "--part" in sys.argv else [1, 3])
    if 1 in parts or not parts:
        n = engrave_part1()
        print(f"Part1: {n} 条术语已刻（含转述）")
    if 3 in parts or not parts:
        total, g = engrave_part3()
        print(f"Part3: 骨架 {total} 条，已刻转述 {g} 条，待刻 {total-g} 条")
