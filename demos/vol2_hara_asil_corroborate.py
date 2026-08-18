"""佐证 demo · 第二卷 ch04/ch14 —— HARA 的 S×E×C→ASIL 判定链可机器复算、可门禁。

书中论断（references/product-trustworthiness-book/ch04-concept-hara/examples/
asil-determination.txt 与 hara-worksheet.csv；ch14 情境危害本体章）：

  1)「Table 4 是一个有限映射」——代表性映射单元如 S3E4C3→D、S2E4C3→C、
     S1E3C1→QM（完整 36 行在受控正本 ontology/asil-table4.ttl，不随书分发）；
  2) EPS 教学判定链：HE_UnintendedAssist_Highway = S3×E4×C3 → ASIL D，
     HE_LossOfAssist_Parking = S1×E3×C1 → QM；
  3)「实际 ASIL 与期望 ASIL 不同即产生 violation」——反例把 S2E4C3 写成 B，
     门禁必须拒绝（正确为 C）；
  4) ch14 的落点：“S3、E4、C3，D 级，这是机器背书过的结论。”

执行：
  A. 把 asil-determination.txt 的代表性映射编码为前向链规则，
     逐行复算 hara-worksheet.csv（书作者手填的 EPS 工作表）——结论应全部一致；
  B. 把「查表一致性」写成 SHACL 形状，重演书中反例（S2E4C3 标成 ASIL B）——
     门禁应当场拒绝。
"""

import _common  # noqa: F401 — 静默 Semantica 进度输出

import csv
import sys
from pathlib import Path

SKILL_ROOT = Path(__file__).resolve().parents[1]
CH04 = SKILL_ROOT / "references/product-trustworthiness-book/ch04-concept-hara/examples"

# asil-determination.txt 中列出的代表性映射单元（Table 4 子集，锚 3-6.4.3.10）
TABLE4 = {
    ("S3", "E4", "C3"): "ASIL D", ("S3", "E4", "C2"): "ASIL C", ("S3", "E4", "C1"): "ASIL B",
    ("S3", "E3", "C3"): "ASIL C", ("S3", "E3", "C2"): "ASIL B", ("S3", "E3", "C1"): "ASIL A",
    ("S2", "E4", "C3"): "ASIL C", ("S2", "E4", "C2"): "ASIL B", ("S2", "E4", "C1"): "ASIL A",
    ("S1", "E3", "C3"): "ASIL A", ("S1", "E3", "C2"): "QM", ("S1", "E3", "C1"): "QM",
}

print("【书中论断】Table 4 是有限映射；EPS 判定链 S3E4C3→D、S1E3C1→QM；"
      "实际 ASIL 偏离查表结果必须产生 violation")
print("【锚点】ch04 examples/asil-determination.txt · hara-worksheet.csv · ch14 chapter.md\n")

# A. 前向链复算书作者手填的 HARA 工作表
from semantica.reasoning import Reasoner

r = Reasoner()
rows = list(csv.DictReader(open(CH04 / "hara-worksheet.csv")))
for row in rows:
    hid = row["hazard_id"]
    r.add_fact(f"Sev_{row['severity']}({hid})")
    r.add_fact(f"Exp_{row['exposure']}({hid})")
    r.add_fact(f"Ctl_{row['controllability']}({hid})")
for (s, e, c), asil in TABLE4.items():
    tag = asil.replace("ASIL ", "ASIL_").replace(" ", "_")
    r.add_rule(f"IF Sev_{s}(?h) AND Exp_{e}(?h) AND Ctl_{c}(?h) THEN Assigned_{tag}(?h)")
inferred = {str(f.conclusion) for f in r.forward_chain()}

print("A. 前向链复算 hara-worksheet.csv（书作者手填 vs 机器复算）：")
all_match = True
for row in rows:
    hid, book_asil = row["hazard_id"], row["asil"]
    tag = book_asil.replace("ASIL ", "ASIL_").replace(" ", "_")
    hit = f"Assigned_{tag}({hid})" in inferred
    all_match &= hit
    print(f"   {hid}: {row['severity']}×{row['exposure']}×{row['controllability']}"
          f" | 书填 {book_asil} | 机器复算 {'一致' if hit else '不一致!'}")

# B. 查表一致性门禁：重演书中反例（S2E4C3 误标 ASIL B）
import pyshacl
from rdflib import Graph, Namespace, RDF, Literal

HAZ = Namespace("https://product-trustworthiness.local/hazard#")
gate = """
@prefix sh: <http://www.w3.org/ns/shacl#> .
@prefix haz: <https://product-trustworthiness.local/hazard#> .
haz:ASILTable4Shape a sh:NodeShape ;
    sh:targetClass haz:HazardousEvent ;
    sh:sparql [
        sh:message "实际 ASIL 与 Table 4 查表结果不同（书中反例：S2E4C3 应为 ASIL C）" ;
        sh:select \"\"\"
            PREFIX haz: <https://product-trustworthiness.local/hazard#>
            SELECT $this WHERE {
                $this haz:hasSeverity "S2" ; haz:hasExposure "E4" ;
                      haz:hasControllability "C3" ; haz:hasASIL ?a .
                FILTER(?a != "ASIL C")
            }
        \"\"\" ;
    ] .
"""
bad = Graph()
bad.add((HAZ.HE_Bad, RDF.type, HAZ.HazardousEvent))
bad.add((HAZ.HE_Bad, HAZ.hasSeverity, Literal("S2")))
bad.add((HAZ.HE_Bad, HAZ.hasExposure, Literal("E4")))
bad.add((HAZ.HE_Bad, HAZ.hasControllability, Literal("C3")))
bad.add((HAZ.HE_Bad, HAZ.hasASIL, Literal("ASIL B")))   # 书中反例：故意写错
conforms, _, text = pyshacl.validate(
    data_graph=bad, shacl_graph=Graph().parse(data=gate, format="turtle"),
    advanced=True)
msg = next((l.strip() for l in text.splitlines() if l.strip().startswith("Message")), "")
print(f"\nB. 门禁重演书中反例（S2E4C3 误标 ASIL B）：conforms={conforms}（False=拒绝成功）")
if msg:
    print(f"   {msg[:70]}")

ok = all_match and not conforms
print(f"\n【佐证结论】{'成立' if ok else '不成立'}：EPS 判定链机器复算与书作者手填全部一致，"
      f"查表一致性门禁拒绝了书中反例——ch14 那句“机器背书过的结论”成立。")
sys.exit(0 if ok else 1)
