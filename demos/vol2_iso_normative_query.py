"""佐证 demo · 第二卷《产品可信工程》—— 工程规范本体化后可查询、刻录纪律可校验。

书中论断（第二卷后十章主题 + references/iso-normative-ontology/normative-tbox.ttl
文件头的刻录纪律）：

    「标准条款可以本体化为带模态（shall/should/may/NOTE）的可引用个体；
      个体只承载条款坐标/模态/中文转述/锚点，模态必须保真」

执行：
  A. SPARQL 查询本体化刻录层：按模态统计条款个体、按刻录状态统计进度、
     取一条已刻录术语卡（含中文转述）——证明「规范变成了可查询的数据」；
  B. 把刻录纪律写成 SHACL 形状（每个条款个体必须有且仅有 clauseId、模态、
     刻录状态），pyshacl 校验全部 TTL —— 证明「纪律可被机器强制」；
  C. 反例对照：注入一个缺模态的坏条款个体，形状应当场拦截。
"""

import _common  # noqa: F401 — 静默 Semantica 进度输出

import sys
from pathlib import Path

SKILL_ROOT = Path(__file__).resolve().parents[1]
ISO_DIR = SKILL_ROOT / "references/iso-normative-ontology"

import pyshacl
from rdflib import Graph, Namespace, RDF

ISON = Namespace("https://ontology-engineering.local/iso26262/normative#")

g = Graph()
for ttl in sorted(ISO_DIR.glob("*.ttl")):
    g.parse(ttl, format="turtle")
print(f"加载 {len(list(ISO_DIR.glob('*.ttl')))} 个 TTL，共 {len(g)} 三元组\n")

print("【书中论断】规范条款可本体化为带模态的可查询个体，且刻录纪律可被机器校验")
print("【锚点】第二卷后十章；iso-normative-ontology/normative-tbox.ttl 刻录纪律 1)–3)\n")

# A. 可查询：模态分布 + 刻录进度 + 一条术语卡
rows = list(g.query("""
    SELECT ?m (COUNT(?u) AS ?n) WHERE {
        ?u a ?t ; isoN:modality ?m .
        FILTER(?t IN (isoN:NormativeUnit, isoN:TermDefinition))
    } GROUP BY ?m ORDER BY DESC(?n)""", initNs={"isoN": ISON}))
print("A. 模态分布（SPARQL）：")
for m, n in rows:
    print(f"   {str(m).split('#')[-1]:12s} {n}")
status = list(g.query("""
    SELECT ?s (COUNT(?u) AS ?n) WHERE { ?u isoN:glossStatus ?s }
    GROUP BY ?s""", initNs={"isoN": ISON}))
print("   刻录进度：", ", ".join(f"{s}={n}" for s, n in status))
card = next(iter(g.query("""
    SELECT ?label ?gloss WHERE {
        ?u a isoN:TermDefinition ; rdfs:label ?label ; isoN:zhGloss ?gloss .
    } LIMIT 1""", initNs={"isoN": ISON})), None)
if card:
    print(f"   术语卡样例：{card[0]} — {str(card[1])[:60]}…")

# B. 刻录纪律 → SHACL 形状
DISCIPLINE = """
@prefix sh: <http://www.w3.org/ns/shacl#> .
@prefix isoN: <https://ontology-engineering.local/iso26262/normative#> .
isoN:NormativeUnitShape a sh:NodeShape ;
    sh:targetClass isoN:NormativeUnit ;
    sh:property [ sh:path isoN:clauseId ; sh:minCount 1 ; sh:maxCount 1 ;
                  sh:message "条款个体必须有且仅有一个条款坐标 clauseId" ] ;
    sh:property [ sh:path isoN:modality ; sh:minCount 1 ; sh:maxCount 1 ;
                  sh:message "条款个体必须有且仅有一个模态（模态保真纪律）" ] ;
    sh:property [ sh:path isoN:glossStatus ; sh:minCount 1 ;
                  sh:in ("glossed" "pending") ;
                  sh:message "刻录状态只能是 glossed 或 pending" ] .
"""
shapes = Graph().parse(data=DISCIPLINE, format="turtle")
conforms, _, _ = pyshacl.validate(data_graph=g, shacl_graph=shapes)
print(f"\nB. 全库刻录纪律校验（pyshacl）：conforms={conforms}")

# C. 反例：缺模态的坏个体必须被拦
bad = Graph()
bad += g
bad.add((ISON.P_BAD, RDF.type, ISON.NormativeUnit))
from rdflib import Literal
bad.add((ISON.P_BAD, ISON.clauseId, Literal("9-9.9")))
bad.add((ISON.P_BAD, ISON.glossStatus, Literal("pending")))
conforms_bad, _, text = pyshacl.validate(data_graph=bad, shacl_graph=shapes)
msg = next((l.strip() for l in text.splitlines() if "模态" in l), "")
print(f"C. 注入缺模态坏个体：conforms={conforms_bad}（False=拦截成功）")
if msg:
    print(f"   违规消息：{msg[:60]}")

ok = bool(rows) and card is not None and conforms and not conforms_bad
print(f"\n【佐证结论】{'成立' if ok else '不成立'}："
      f"条款已是可查询数据（{sum(int(n) for _, n in rows)} 个带模态个体），"
      f"刻录纪律全库合规且能拦截违例。")
sys.exit(0 if ok else 1)
