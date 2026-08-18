"""佐证 demo · 第二卷 ch15 —— 需求追溯本体：重开清单由机器先说。

书中论断（references/product-trustworthiness-book/ch15-requirements-ontology/
chapter.md §15.6）：

  传感器换代、最坏刷新承诺失效时，「把所有理由里引用了那个传感器参数的连线，
  连同它们指向的下游，一次取出来」——重开清单不再依赖会议室辩论或个人记忆，
  也不采用「照改动模块机械地圈」（其盲区正是当年十一天的出处）。

执行：按书中记法建图——三条 rationale 锚在 req:Param_SensorUpdate 上的
追溯边（应入清单），一条锚在其他参数上的边（机械圈选会漏、按理由查不受影响）；
书中 SPARQL 原样执行，应恰好返回三行及其理由。
"""

import sys
from rdflib import Graph, Namespace, Literal

REQ = Namespace("https://product-trustworthiness.local/requirements#")

g = Graph()
affected = [
    ("FSR_TorqueLimit", "最坏刷新时间参与其超时预算推导"),
    ("TSR_WatchdogWindow", "看门狗窗口以传感器刷新为下界"),
    ("SwReq_PlausibilityCheck", "似真性检查周期引用了该刷新承诺"),
]
for i, (req_name, why) in enumerate(affected):
    edge, rat = REQ[f"edge_{i}"], REQ[f"rationale_{i}"]
    g.add((edge, REQ.rationale, rat))
    g.add((rat, REQ.dependsOnAspect, REQ.Param_SensorUpdate))
    g.add((edge, REQ.toReq, REQ[req_name]))
    g.add((rat, REQ.justification, Literal(why)))
# 干扰项：理由锚在电源参数上——与传感器换代无关，不应入清单
edge_x, rat_x = REQ.edge_power, REQ.rationale_power
g.add((edge_x, REQ.rationale, rat_x))
g.add((rat_x, REQ.dependsOnAspect, REQ.Param_SupplyRail))
g.add((edge_x, REQ.toReq, REQ.HwReq_Brownout))
g.add((rat_x, REQ.justification, Literal("欠压阈值与传感器刷新无关")))

print("【书中论断】理由锚在参数上，重开清单是一次查询取出的下游连线，不靠会议与记忆")
print("【锚点】ch15 chapter.md §15.6 三段式（问题/查询/答案）\n")

rows = list(g.query("""
    PREFIX req: <https://product-trustworthiness.local/requirements#>
    SELECT ?affected ?why
    WHERE {
      ?edge  req:rationale       ?r .
      ?r     req:dependsOnAspect req:Param_SensorUpdate .
      ?edge  req:toReq           ?affected .
      ?r     req:justification   ?why .
    }"""))
names = sorted(str(r[0]).split("#")[-1] for r in rows)
print("重开清单（机器先说）：")
for a, w in rows:
    print(f"  {str(a).split('#')[-1]:24s} | {w}")

expect = sorted(n for n, _ in affected)
ok = names == expect and "HwReq_Brownout" not in names
print(f"\n干扰项 HwReq_Brownout（理由锚在电源参数）未入清单 -> {'HwReq_Brownout' not in names}")
print(f"\n【佐证结论】{'成立' if ok else '不成立'}：三条锚定该参数的下游一次取出、"
      f"无关项不入列——重开清单可复制、可复查，不再是谁的记忆。")
sys.exit(0 if ok else 1)
