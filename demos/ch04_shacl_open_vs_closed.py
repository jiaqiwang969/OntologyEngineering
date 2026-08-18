"""佐证 demo · 第一卷 ch04/ch07 —— OWL 开放世界 vs SHACL 封闭校验。

书中论断（references/ontology-engineering-book/ch07-knowledge-graph/examples/
kg-quality-shacl.ttl 文件头，与 ch04 语言章呼应）：

    「OWL公理面向"推理"（开放世界），SHACL面向"校验"（封闭检查）：
      缺序列号在OWL下只是"未知"，在SHACL下就是一条违规报告」

执行：构造一台缺序列号的设备实例——
  A. 在 OWL/RDFS 语义下查询：得不到任何矛盾，只是查不到（开放世界）；
  B. 用书中手写的 kg-quality-shacl.ttl 经 pyshacl 校验：产出违规报告；
  C. 用 Semantica OntologyEngine.to_shacl 从同一概念模型自动派生形状，
     再校验同一违例：同样被拦（书的方法论与工具实现一致）。

三条都成立 → 佐证成立，退出码 0；任何一条不成立 → 退出码 1。
"""

import sys
from pathlib import Path

SKILL_ROOT = Path(__file__).resolve().parents[1]
BOOK_SHACL = (SKILL_ROOT / "references/ontology-engineering-book"
              / "ch07-knowledge-graph/examples/kg-quality-shacl.ttl")

import pyshacl
from rdflib import Graph, Namespace, RDF, Literal, XSD

MFG = Namespace("http://example.org/manufacturing#")

# 一台合规设备、一台缺序列号的设备
data = Graph()
data.bind("mfg", MFG)
data.add((MFG.Lathe_OK, RDF.type, MFG.Equipment))
data.add((MFG.Lathe_OK, MFG.hasSerialNumber, Literal("EQ-2026-0001")))
data.add((MFG.Lathe_OK, MFG.hasPower, Literal(15.0, datatype=XSD.float)))
data.add((MFG.Lathe_BAD, RDF.type, MFG.Equipment))   # 故意不给序列号
data.add((MFG.Lathe_BAD, MFG.hasPower, Literal(15.0, datatype=XSD.float)))

print("【书中论断】缺序列号在 OWL 下只是“未知”，在 SHACL 下就是一条违规报告")
print(f"【锚点】ch07 kg-quality-shacl.ttl 文件头；ch04 语言章 OWL 开放世界语义\n")

# A. 开放世界：SPARQL 查询查不到，但图中没有任何矛盾
missing = list(data.query(
    "SELECT ?e WHERE { ?e a mfg:Equipment . "
    "FILTER NOT EXISTS { ?e mfg:hasSerialNumber ?s } }",
    initNs={"mfg": MFG}))
open_world_ok = len(missing) == 1 and missing[0][0] == MFG.Lathe_BAD
print(f"A. OWL/RDFS 开放世界：图谱无矛盾，仅能查出缺值实体 -> {open_world_ok}")

# B. 书中手写 SHACL：违规报告
shapes = Graph().parse(BOOK_SHACL, format="turtle")
conforms, _, text = pyshacl.validate(data_graph=data, shacl_graph=shapes)
book_msgs = [l.strip() for l in text.splitlines()
             if l.strip().startswith("Message") and "序列号" in l]
book_catches = (not conforms) and bool(book_msgs)
print(f"B. 书中手写 SHACL（pyshacl 执行）：conforms={conforms}")
if book_msgs:
    print(f"   违规消息：{book_msgs[0][:70]}")

# C. Semantica 自动派生形状，拦同一违例
from semantica.ontology.engine import OntologyEngine

ontology = {
    "uri": "http://example.org/manufacturing",
    "name": "MfgMini", "version": "demo",
    "classes": [{"name": "Equipment", "uri": str(MFG.Equipment), "label": "Equipment"}],
    "properties": [{"name": "hasSerialNumber", "uri": str(MFG.hasSerialNumber),
                    "label": "hasSerialNumber", "type": "data",
                    "domain": "Equipment", "range": "string",
                    "required": True, "cardinality": {"min": 1, "max": 1}}],
}
auto_shacl = OntologyEngine(base_uri=str(MFG)).to_shacl(
    ontology, base_uri=str(MFG), severity="Violation")
# 上游 quirk：semantica 0.6.5 的 SHACLGenerator.__init__ 强制在 base_uri 尾部
# 追加 "/"，把 "…#" 变成 "…#/"，导致 targetClass 命名空间错位。此处修正。
auto_shacl = auto_shacl.replace(str(MFG) + "/", str(MFG))
conforms2, _, _ = pyshacl.validate(
    data_graph=data, shacl_graph=Graph().parse(data=auto_shacl, format="turtle"))
print(f"C. Semantica to_shacl 自动派生形状：conforms={conforms2}（False=同样拦截）")

ok = open_world_ok and book_catches and not conforms2
print(f"\n【佐证结论】{'成立' if ok else '不成立'}：开放世界不报错、封闭校验两条路线都拦截缺序列号。")
sys.exit(0 if ok else 1)
