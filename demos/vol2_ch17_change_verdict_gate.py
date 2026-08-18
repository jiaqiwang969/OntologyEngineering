"""佐证 demo · 第二卷 ch17 —— 变化本体：旧值牵出"通过"，判决必须有理由。

书中论断（references/product-trustworthiness-book/ch17-change-ontology/
chapter.md）：

  1) 第四条路：「软件维的旧值被换掉之后，哪些'通过'绑定着它？」——
     书中 SPARQL 一分钟返回，替代逐卡辩论与六层清单；
  2)「宣布一张卡作废，却说不出作废了谁，这不是判断，是挥手——门禁拒收」；
  3)「保留是判断，不是默认：请写明该卡为何不受本次变更波及」——
     没有理由的"保留"同样拒收（书中 GateReport 记法）。

数据：fixtures/ch17_change.ttl、ch17_verdicts_bad.ttl、ch17_verdicts_good.ttl。
执行：A. 书中查询原样跑——绑定旧值的三张通过被牵出（冲振卡不返回）；
     B. 空波及面的作废与缺理由的保留被拒；C. 理由写入后放行。
"""

import sys
from _common import load_fixture

import pyshacl
from rdflib import Graph

g = load_fixture("ch17_change")

print("【书中论断】旧值牵出绑定它的通过；空波及面的作废是挥手；无理由的保留被拒")
print("【锚点】ch17 chapter.md 第四条路查询 · GateReport 记法\n")

# A. 书中查询原样执行
rows = sorted(str(r[0]).split("#")[-1] for r in g.query("""
    PREFIX chg: <https://product-trustworthiness.local/change#>
    SELECT ?pass WHERE {
      chg:Change_0091 chg:changedDimension ?dim ;
                      chg:changedFrom      ?oldValue .
      ?pass a chg:PassRecord ;
            ?binding ?oldValue .
      ?binding chg:bindsDimension ?dim .
    }"""))
q_ok = rows == ["Pass_FaultInjection", "Pass_HilRun", "Pass_SwReg"]
print(f"A. 绑定旧值的通过：{rows}（冲振卡不在内 -> {'Pass_Vibration' not in rows}）")

# B/C. 判决门禁
GATE = """
@prefix sh: <http://www.w3.org/ns/shacl#> .
@prefix chg: <https://product-trustworthiness.local/change#> .
chg:RevokeNeedsScope a sh:NodeShape ;
    sh:targetClass chg:RevokeVerdict ;
    sh:property [ sh:path chg:revokes ; sh:minCount 1 ;
        sh:message "作废判决的波及面为空：宣布作废却说不出作废了谁，这不是判断，是挥手" ] .
chg:RetainNeedsReason a sh:NodeShape ;
    sh:targetClass chg:RetainVerdict ;
    sh:property [ sh:path chg:retainReason ; sh:minCount 1 ;
        sh:message "保留是判断，不是默认：请写明该卡为何不受本次变更波及" ] .
"""
shapes = Graph().parse(data=GATE, format="turtle")

v = load_fixture("ch17_verdicts_bad")
conforms_bad, _, text = pyshacl.validate(data_graph=v, shacl_graph=shapes)
msgs = [l.strip()[9:] for l in text.splitlines() if l.strip().startswith("Message")]
print(f"\nB. 两条违规判决：conforms={conforms_bad}，门禁消息 {len(msgs)} 条")
for m in sorted(msgs):
    print(f"   - {m[:46]}")

v2 = load_fixture("ch17_verdicts_good")
conforms_ok, _, _ = pyshacl.validate(data_graph=v2, shacl_graph=shapes)
print(f"C. 理由写入判决后：conforms={conforms_ok}（成为下次变更可复查、可推翻的东西）")

ok = q_ok and "Pass_Vibration" not in rows and not conforms_bad and len(msgs) == 2 and conforms_ok
print(f"\n【佐证结论】{'成立' if ok else '不成立'}：三张通过被旧值牵出、挥手式作废与"
      f"默认式保留都被拒、写明理由后放行——差异分析成为可复制的程序。")
sys.exit(0 if ok else 1)
