"""佐证 demo · 第二卷 ch15 —— 需求追溯本体：重开清单由机器先说。

书中论断（references/product-trustworthiness-book/ch15-requirements-ontology/
chapter.md §15.6）：

  传感器换代、最坏刷新承诺失效时，「把所有理由里引用了那个传感器参数的连线，
  连同它们指向的下游，一次取出来」——重开清单不再依赖会议室辩论或个人记忆，
  也不采用「照改动模块机械地圈」（其盲区正是当年十一天的出处）。

数据：fixtures/ch15_traceability.ttl（三条锚定传感器参数的追溯边 + 一条
锚在电源参数上的干扰边）。
执行：书中 SPARQL 原样执行，应恰好返回三行及其理由，干扰项不入列。
"""

import sys
from _common import load_fixture

g = load_fixture("ch15_traceability")

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

expect = ["FSR_TorqueLimit", "SwReq_PlausibilityCheck", "TSR_WatchdogWindow"]
ok = names == expect and "HwReq_Brownout" not in names
print(f"\n干扰项 HwReq_Brownout（理由锚在电源参数）未入清单 -> {'HwReq_Brownout' not in names}")
print(f"\n【佐证结论】{'成立' if ok else '不成立'}：三条锚定该参数的下游一次取出、"
      f"无关项不入列——重开清单可复制、可复查，不再是谁的记忆。")
sys.exit(0 if ok else 1)
