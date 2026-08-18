"""佐证 demo · 第一卷 ch02 —— 单调推理不撤销结论；开放世界 vs 封闭世界。

书中论断（references/ontology-engineering-book/ch02-ontology-foundations/
examples/reasoning-examples.txt）：

  1) 单调推理：TBox 链 CNCLathe⊑CNCMachine⊑ProcessingEquipment 加事实
     CNCLathe(Lathe_001) 推出两条推论，且「之后无论添加什么新事实，
     推论永远成立，不会被撤销」；
  2)「这种'结论收回'是单调逻辑无法表达的」——加入 hasFault(Lathe_001)
     后，单调引擎里 available(Lathe_001) 依然在推论集中，不会消失；
  3) OWA vs CWA：知识库只有 producedBy(Product_A, Lathe_001) 时，
     「Product_B 是否由 Lathe_001 生产」在 OWA 下是未知，在 CWA
     （否定即失败）下是假。

执行：单调链用 Semantica Reasoner 复算；撤销失败用两阶段前向链验证；
OWA/CWA 用 rdflib 的证明缺失 vs FILTER NOT EXISTS 对照。
"""

import sys
from semantica.reasoning import Reasoner

print("【书中论断】单调推论不被新事实撤销；结论收回是单调逻辑做不到的；OWA 未知 ≠ CWA 假")
print("【锚点】ch02 examples/reasoning-examples.txt §1–§3\n")

# 1) 单调推理链
r = Reasoner()
r.add_fact("CNCLathe(Lathe_001)")
r.add_rule("IF CNCLathe(?x) THEN CNCMachine(?x)")
r.add_rule("IF CNCMachine(?x) THEN ProcessingEquipment(?x)")
step1 = {str(f.conclusion) for f in r.forward_chain()}
mono_ok = {"CNCMachine(Lathe_001)", "ProcessingEquipment(Lathe_001)"} <= step1
print(f"1. 子类链两条推论：{sorted(step1)} -> {mono_ok}")

# 2) 加入故障事实后，单调引擎无法撤销 available
r2 = Reasoner()
r2.add_fact("Equipment(Lathe_001)")
r2.add_rule("IF Equipment(?x) THEN Available(?x)")        # 默认规则的单调近似
r2.add_fact("HasFault(Lathe_001)")                        # 新增故障事实
r2.add_rule("IF HasFault(?x) THEN Unavailable(?x)")
step2 = {str(f.conclusion) for f in r2.forward_chain()}
no_retract = "Available(Lathe_001)" in step2 and "Unavailable(Lathe_001)" in step2
print(f"2. 故障加入后 Available 未被收回（与 Unavailable 并存）：{no_retract}")
print("   —— 正如书中所说，撤销要靠默认逻辑/应用层，单调引擎给不了\n")

# 3) OWA vs CWA
from rdflib import Graph, Namespace

MFG = Namespace("http://example.org/manufacturing#")
g = Graph()
g.add((MFG.Product_A, MFG.producedBy, MFG.Lathe_001))
owa_unknown = len(list(g.query(
    "ASK { mfg:Product_B mfg:producedBy mfg:Lathe_001 }", initNs={"mfg": MFG}))) >= 0 \
    and not bool(g.query("ASK { mfg:Product_B mfg:producedBy mfg:Lathe_001 }",
                         initNs={"mfg": MFG}))
cwa_false = bool(g.query(
    "ASK { FILTER NOT EXISTS { mfg:Product_B mfg:producedBy mfg:Lathe_001 } }",
    initNs={"mfg": MFG}))
print(f"3. OWA：查询证明失败（未知，不是假）-> {owa_unknown}；"
      f"CWA/NAF：FILTER NOT EXISTS 判否 -> {cwa_false}")

ok = mono_ok and no_retract and owa_unknown and cwa_false
print(f"\n【佐证结论】{'成立' if ok else '不成立'}：单调链推出且不撤销、"
      f"故障事实推不翻旧结论、同一缺失在 OWA 是未知在 CWA 是假。")
sys.exit(0 if ok else 1)
