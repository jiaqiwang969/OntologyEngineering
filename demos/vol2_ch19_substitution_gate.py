"""佐证 demo · 第二卷 ch19 —— 现场本体：代换必须经过设计评估，洞不能粉刷成墙。

书中论断（references/product-trustworthiness-book/ch19-field-ontology/
chapter.md §19.4–§19.5）：

  1)「任何一张代换申请，必须经过一个设计评估对象——评估要覆盖新料触及的
     每一条前提」——SubReq_47 没有 assessedBy 这条边，机器拒绝
     （「对照表不是评估」）；
  2) 评估补上后清单现成一行：低温温度系数「需实测低温曲线后再判」——
     当年三周排查加一个冬天投诉才浮出的那行字；
  3) 台账重建时，助手提议「按时间就近推断补上」缺失记录被门禁退回：
     「推断可以作为标了记号的候选留给人判，不能写成事实」
     ——图不嫌账有洞，图只拒绝把洞粉刷成墙。

执行：A. 无评估的代换单被拒；B. 挂上覆盖全部前提的评估后闭合，且前提清单
里能查到低温那一行；C. 无受控来源的事实写入被拒，标记为候选的不受影响。
"""

import sys
import pyshacl
from rdflib import Graph, Namespace, RDF, Literal

FLD = Namespace("https://product-trustworthiness.local/field#")

GATE = """
@prefix sh: <http://www.w3.org/ns/shacl#> .
@prefix fld: <https://product-trustworthiness.local/field#> .
fld:SubstitutionGate a sh:NodeShape ;
    sh:targetClass fld:SubstitutionRequest ;
    sh:property [ sh:path fld:assessedBy ; sh:minCount 1 ; sh:class fld:DesignAssessment ;
        sh:message "缺设计评估的代换，机器拒绝：对照表不是评估" ] .
fld:FactProvenanceGate a sh:NodeShape ;
    sh:targetClass fld:LedgerFact ;
    sh:property [ sh:path fld:fromRecord ; sh:minCount 1 ;
        sh:message "事实必须挂着受控记录的来源：推断只能作为标记候选留给人判，不能写成事实" ] .
"""
shapes = Graph().parse(data=GATE, format="turtle")

print("【书中论断】代换必须经设计评估闭合；无受控来源的推断不得写成事实")
print("【锚点】ch19 chapter.md §19.4 拒绝六行 · §19.5 台账重建\n")

# A. 无评估的代换单
bad = Graph()
bad.add((FLD.SubReq_47, RDF.type, FLD.SubstitutionRequest))
bad.add((FLD.SubReq_47, FLD.comparisonTable, Literal("十二行对照表")))   # 对照表不是评估
conforms_a, _, text = pyshacl.validate(data_graph=bad, shacl_graph=shapes)
msg = next((l.strip() for l in text.splitlines() if l.strip().startswith("Message")), "")
print(f"A. SubReq_47（只有对照表）：conforms={conforms_a}（False=停在闸口）")
if msg:
    print(f"   {msg[:48]}")

# B. 评估补上：覆盖新料触及的每条前提，低温那行在清单上
good = Graph()
good.add((FLD.SubReq_47, RDF.type, FLD.SubstitutionRequest))
good.add((FLD.SubReq_47, FLD.assessedBy, FLD.Assessment_47))
good.add((FLD.Assessment_47, RDF.type, FLD.DesignAssessment))
for premise, verdict in [
        ("Premise_LowTempTempco", "需实测低温曲线后再判"),
        ("Premise_Footprint", "封装兼容"),
        ("Premise_Derating", "降额裕度不变")]:
    good.add((FLD.Assessment_47, FLD.coversPremise, FLD[premise]))
    good.add((FLD[premise], FLD.verdict, Literal(verdict)))
conforms_b, _, _ = pyshacl.validate(data_graph=good, shacl_graph=shapes)
low = list(good.query("""
    PREFIX fld: <https://product-trustworthiness.local/field#>
    SELECT ?v WHERE { fld:Assessment_47 fld:coversPremise fld:Premise_LowTempTempco .
                      fld:Premise_LowTempTempco fld:verdict ?v }"""))
print(f"B. 评估补上后：conforms={conforms_b}；低温前提在清单上 -> "
      f"“{low[0][0] if low else '?'}”（当年一个冬天才浮出的那行字，现在是现成一行）")

# C. 台账重建：推断不得写成事实
ledger = Graph()
ledger.add((FLD.Fact_Box12, RDF.type, FLD.LedgerFact))
ledger.add((FLD.Fact_Box12, FLD.fromRecord, FLD.PackingSlip_0312))   # 有装箱单，收
ledger.add((FLD.Fact_Box17, RDF.type, FLD.LedgerFact))               # 推断补上，无来源
conforms_c, _, text_c = pyshacl.validate(data_graph=ledger, shacl_graph=shapes)
print(f"C. 台账写入（Box17 为就近推断、无受控来源）：conforms={conforms_c}（False=原样退回）")

ok = (not conforms_a) and conforms_b and bool(low) and not conforms_c
print(f"\n【佐证结论】{'成立' if ok else '不成立'}：缺评估的代换被拒、评估闭合后低温"
      f"前提是清单上现成的一行、推断写事实被退回——规定可以被忙碌绕过，结构绕不过去。")
sys.exit(0 if ok else 1)
