"""佐证 demo · 第二卷 ch18 —— 依赖独立本体：向上闭包找汇合，事实换版理由举手。

书中论断（references/product-trustworthiness-book/ch18-dependency-ontology/
chapter.md）：

  1) 汇合查询：主通道与监控通道「沿供电边逐级上溯，直到再无上游」，
     两份名单出现同一个名字即相遇——「只追一步不算数」，书中答案表一行：
     dep:Reg5V_U3（降成本变更把两路稳压合并到同一颗上游稳压器）；
  2) 门禁三条硬规矩之三（活的）：「押着过期事实的排除，自动标疑」——
     传感器配对事实升版后，靠它成立的排除记为 Stale，
     所属独立性主张记为 NeedsReopen。

数据：fixtures/ch18_dependency.ttl。
执行：A. SPARQL 属性路径 feedsFrom+ 复算两级上溯的汇合点；
     B. 押 v3 过期事实的排除被标疑，押现行事实的不受影响。
"""

import sys
from _common import load_fixture

g = load_fixture("ch18_dependency")

print("【书中论断】沿供电边闭包上溯，两通道在 Reg5V_U3 相遇（一行）；过期事实上的排除自动举手")
print("【锚点】ch18 chapter.md 汇合查询 · 门禁三条硬规矩\n")

# A. 汇合查询（书中 feedsFromPlus 即传递闭包，此处用 SPARQL 属性路径实现）
rows = list(g.query("""
    PREFIX dep: <https://product-trustworthiness.local/dependency#>
    SELECT DISTINCT ?node WHERE {
      dep:MainChain dep:feedsFrom+ ?node .
      dep:MonChain  dep:feedsFrom+ ?node .
    }"""))
meet = sorted(str(r[0]).split("#")[-1] for r in rows)
one_hop_rows = list(g.query("""
    PREFIX dep: <https://product-trustworthiness.local/dependency#>
    SELECT DISTINCT ?node WHERE {
      dep:MainChain dep:feedsFrom ?node . dep:MonChain dep:feedsFrom ?node . }"""))
print(f"A. 闭包汇合：{meet}（Batt 在 Reg 之上也相遇，首个汇合 Reg5V_U3）")
print(f"   只追一步：{len(one_hop_rows)} 行（书：只追一步不算数——一步查不到汇合）")
meet_ok = "Reg5V_U3" in meet and len(one_hop_rows) == 0

# B. 事实换版，理由举手（书中门禁伪码的直译）
stale = list(g.query("""
    PREFIX dep: <https://product-trustworthiness.local/dependency#>
    SELECT ?excl ?claim WHERE {
      ?excl a dep:Exclusion ; dep:restsOn ?fact .
      ?fact dep:supersededBy ?new .
      OPTIONAL { ?claim dep:hasExclusion ?excl }
    }"""))
flagged = [(str(e).split("#")[-1], str(c).split("#")[-1] if c else "-") for e, c in stale]
print(f"\nB. 事实升版扫描：{flagged}")
print("   -> Excl_NoSharedSensor 记 Stale（理由自动举手），Claim_ChannelIndep 记 NeedsReopen")
stale_ok = flagged == [("Excl_NoSharedSensor", "Claim_ChannelIndep")]

ok = meet_ok and stale_ok
print(f"\n【佐证结论】{'成立' if ok else '不成立'}：闭包上溯在第二级找到 Reg5V_U3"
      f"（登记看得见改了什么，这条查询补上波及谁）；押过期事实的排除自动标疑，"
      f"押现行事实的不受牵连。")
sys.exit(0 if ok else 1)
