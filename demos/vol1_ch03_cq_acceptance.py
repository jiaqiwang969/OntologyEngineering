"""佐证 demo · 第一卷 ch03/ch04 —— 能力问题是本体的验收测试，CQ→SPARQL 可执行。

书中论断（references/ontology-engineering-book/ch03-ontology-methodology/
examples/competency-questions.txt）：

  「能力问题（CQ）是本体的'需求规格说明书'与'验收测试用例'：本体建成后，
    必须能通过查询或推理回答全部CQ」「CQ → SPARQL查询，结果正确即通过验收」

执行：从 ch04 examples/sparql-queries.sparql 原样取出 CQ1
（哪些设备可以加工铝合金？），在按 ch04 制造本体词汇构造的最小教学 ABox 上执行：
  A. 正例：能加工铝合金的设备被查出（验收通过）；
  B. 反例：删除 canProcess 能力后 CQ1 返回空（验收测试真的能挡住不合格本体）。
"""

import _common  # noqa: F401 — 静默 Semantica 进度输出

import re
import sys
from pathlib import Path

SKILL_ROOT = Path(__file__).resolve().parents[1]
SPARQL_FILE = (SKILL_ROOT / "references/ontology-engineering-book"
               / "ch04-ontology-languages/examples/sparql-queries.sparql")

from rdflib import Graph, Namespace, RDF, RDFS, Literal, XSD

MFG = Namespace("http://example.org/manufacturing#")

# 原样提取书中 CQ1 查询（PREFIX 块 + CQ1 的 SELECT）
text = SPARQL_FILE.read_text()
prefixes = "\n".join(l for l in text.splitlines() if l.startswith("PREFIX"))
m = re.search(r"# CQ1.*?\n(SELECT.*?ORDER BY[^\n]*)", text, re.S)
cq1 = prefixes + "\n" + m.group(1)

print("【书中论断】CQ 是本体的验收测试用例：CQ→SPARQL，结果正确即通过验收")
print("【锚点】ch03 examples/competency-questions.txt §1 · ch04 examples/sparql-queries.sparql CQ1\n")

def make_abox(with_capability: bool) -> Graph:
    g = Graph()
    g.bind("", MFG)
    g.add((MFG.Aluminum, RDF.type, MFG.Material))
    for eq, zh, power in [(MFG.Lathe_001, "数控车床一号", 15.0),
                          (MFG.Mill_002, "立式铣床二号", 11.0)]:
        g.add((eq, RDF.type, MFG.Equipment))
        g.add((eq, RDFS.label, Literal(zh, lang="zh")))
        g.add((eq, MFG.power, Literal(power, datatype=XSD.float)))
        if with_capability:
            g.add((eq, MFG.canProcess, MFG.Aluminum))
    g.add((MFG.Press_003, RDF.type, MFG.Equipment))       # 不能加工铝合金的设备
    g.add((MFG.Press_003, RDFS.label, Literal("液压机三号", lang="zh")))
    return g

# A. 正例：验收通过
rows = list(make_abox(True).query(cq1))
names = sorted(str(r[1]) for r in rows)
pass_ok = len(rows) == 2 and names == ["数控车床一号", "立式铣床二号"]
print(f"A. CQ1 原样执行：{len(rows)} 行 -> {names}（应恰好两台，Press_003 不入选）")

# B. 反例：能力缺失时验收测试挡住
rows_bad = list(make_abox(False).query(cq1))
fail_ok = len(rows_bad) == 0
print(f"B. 删除 canProcess 后 CQ1：{len(rows_bad)} 行（0=验收测试成功挡住不合格本体）")

ok = pass_ok and fail_ok
print(f"\n【佐证结论】{'成立' if ok else '不成立'}：书中 CQ1 查询原文可直接执行为验收测试，"
      f"合格本体通过、缺能力的本体被挡——「CQ 即验收」不是比喻。")
sys.exit(0 if ok else 1)
