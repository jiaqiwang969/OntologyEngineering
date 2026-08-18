"""佐证 demo · 第二卷 ch13 —— 治理本体：独立距离可计算，授权是随组织呼吸的边。

书中论断（references/product-trustworthiness-book/ch13-governance-ontology/
chapter.md）：

  1)「'够不够远'从一句承诺变成一次计算」——查的不是够远的证据，而是太近的
     证据（同队/同上级/同考核线）；书中场景：小林评郑工的归并分析，
     答案表回来**两行**；
  2) 清晨亮灯查询：哪些承诺过的独立与授权已不再成立——书中答案**两行**
     （独立距离被击穿、授权已过期）；
  3)「授权生效必须是授权人本人的记录动作。调用成功不是事实，执行更不是授权」
     ——助手提交的"生效"动作被门禁原样弹回。

执行：两条书中 SPARQL 原样跑出与书相同的行数与原因；生效门禁拒绝助手代签。
"""

import sys
import pyshacl
from rdflib import Graph, Namespace, RDF, Literal

GOV = Namespace("https://product-trustworthiness.local/governance#")

g = Graph()
# 场景一：小林与归并分析作者郑工——同队、同考核线（书中两行）
g.add((GOV.MergeAnalysis_v2, GOV.authoredBy, GOV.ZhengGong))
g.add((GOV.ZhengGong, GOV.memberOf, GOV.Team_Ledger))
g.add((GOV.XiaoLin, GOV.memberOf, GOV.Team_Ledger))          # 同一队伍
g.add((GOV.ZhengGong, GOV.reportsTo, GOV.Mgr_A))
g.add((GOV.XiaoLin, GOV.reportsTo, GOV.Mgr_B))               # 上级不同
g.add((GOV.ZhengGong, GOV.appraisedBy, GOV.Appraiser_X))
g.add((GOV.XiaoLin, GOV.appraisedBy, GOV.Appraiser_X))       # 同一考核线
# 场景二：清晨亮灯——距离被击穿的评审安排 + 过期授权
g.add((GOV.Review_FangGong, GOV.promisedDistance, GOV.OutsideMgmtLine))
g.add((GOV.Review_FangGong, GOV.currentDistance, GOV.SameMgmtLine))
g.add((GOV.LedgerAuthority, GOV.validUntil, Literal("2027-09-30")))
g.add((GOV.PilotAuthority, GOV.validUntil, Literal("2028-06-30")))   # 未过期，不应亮灯

print("【书中论断】独立距离是计算不是承诺（答案两行）；亮灯清单两行；授权生效必须本人")
print("【锚点】ch13 chapter.md 太近查询 · 清晨亮灯查询 · 生效门禁\n")

# 1) 太近查询（书中原样）
rows1 = sorted(str(r[0]) for r in g.query("""
    PREFIX gov: <https://product-trustworthiness.local/governance#>
    SELECT ?tooClose WHERE {
      gov:MergeAnalysis_v2 gov:authoredBy ?a .
      { ?a gov:memberOf ?g .    gov:XiaoLin gov:memberOf ?g .
        BIND("同一队伍" AS ?tooClose) } UNION
      { ?a gov:reportsTo ?m .   gov:XiaoLin gov:reportsTo ?m .
        BIND("同一上级" AS ?tooClose) } UNION
      { ?a gov:appraisedBy ?p . gov:XiaoLin gov:appraisedBy ?p .
        BIND("同一考核线" AS ?tooClose) } }"""))
q1_ok = set(rows1) == {"同一队伍", "同一考核线"}
print(f"1. 太近的证据：{rows1}（书：答案表回来两行）")

# 2) 清晨亮灯（书中原样，判定日 2027-10-08）
rows2 = sorted(str(r[1]) for r in g.query("""
    PREFIX gov: <https://product-trustworthiness.local/governance#>
    SELECT ?item ?why WHERE {
      { ?item gov:promisedDistance gov:OutsideMgmtLine ;
              gov:currentDistance  gov:SameMgmtLine .
        BIND("独立距离被击穿" AS ?why) } UNION
      { ?item gov:validUntil ?end .
        FILTER ( ?end < "2027-10-08" )
        BIND("授权已过期" AS ?why) } }"""))
q2_ok = set(rows2) == {"授权已过期", "独立距离被击穿"}
print(f"2. 清晨亮灯：{rows2}（书：两行，PilotAuthority 未到期不亮）")

# 3) 生效门禁：授权生效必须授权人本人记录
GATE = """
@prefix sh: <http://www.w3.org/ns/shacl#> .
@prefix gov: <https://product-trustworthiness.local/governance#> .
gov:ActivationGate a sh:NodeShape ;
    sh:targetClass gov:ActivationAction ;
    sh:sparql [
        sh:message "授权生效必须是授权人本人的记录动作——调用成功不是事实，执行更不是授权" ;
        sh:select \"\"\"
            PREFIX gov: <https://product-trustworthiness.local/governance#>
            SELECT $this WHERE {
                $this gov:activates ?auth ; gov:recordedBy ?who .
                ?auth gov:grantor ?grantor . FILTER(?who != ?grantor)
            }
        \"\"\" ;
    ] .
"""
act = Graph()
act.add((GOV.NewAuthority, GOV.grantor, GOV.LaoHe))
act.add((GOV.Act_1, RDF.type, GOV.ActivationAction))
act.add((GOV.Act_1, GOV.activates, GOV.NewAuthority))
act.add((GOV.Act_1, GOV.recordedBy, GOV.Assistant))          # 助手代签
conforms, _, text = pyshacl.validate(
    data_graph=act, shacl_graph=Graph().parse(data=GATE, format="turtle"), advanced=True)
print(f"3. 助手提交生效：conforms={conforms}（False=被原样弹回）")

ok = q1_ok and q2_ok and not conforms
print(f"\n【佐证结论】{'成立' if ok else '不成立'}：两条查询行数与书一致、"
      f"授权生效的代签被门禁拒绝——「随组织呼吸的边」每天可问可答。")
sys.exit(0 if ok else 1)
