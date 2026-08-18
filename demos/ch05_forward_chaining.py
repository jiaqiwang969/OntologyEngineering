"""佐证 demo · 第一卷 ch05 —— SWRL 前向推理链的机器复算。

书中论断（references/ontology-engineering-book/ch05-reasoning/examples/
swrl-rules.swrl，作者手推的推理链）：

    规则：设备(?x) ∧ 功率(?x,?p) ∧ ?p>10 → 高功率设备(?x)
          高功率设备(?x) → 需要冷却(?x)
    事实：设备(Lathe_003)，功率(Lathe_003, 15)
    手推结论：Lathe_003 需要冷却

执行：把同一组事实与规则交给 Semantica 的 Reasoner 前向链引擎，
机器推导应得出与书中手推完全相同的结论，并给出可解释的推理路径。

注：Semantica 规则语言不支持 SWRL 内建算术（swrlb:greaterThan），
数值比较在进事实前先行判定——这属于「书 ↔ 代码」映射表中声明的差异。
"""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from semantica.reasoning import Reasoner, ExplanationGenerator

POWER = {"Lathe_003": 15.0}   # 书中事实：功率(Lathe_003, 15)

print("【书中论断】事实 设备(Lathe_003)+功率15 经两条规则手推得出：Lathe_003 需要冷却")
print("【锚点】ch05 examples/swrl-rules.swrl「基础推理规则」+ 验证推理链注释\n")

r = Reasoner()
r.add_fact("Equipment(Lathe_003)")
for name, p in POWER.items():
    if p > 10:                      # swrlb:greaterThan(?p,10) 的前置判定
        r.add_fact(f"PowerAbove10({name})")
r.add_rule("IF Equipment(?x) AND PowerAbove10(?x) THEN HighPowerEquipment(?x)")
r.add_rule("IF HighPowerEquipment(?x) THEN RequiresCooling(?x)")

inferred = r.forward_chain()
conclusions = [str(f.conclusion) for f in inferred]
print("机器推理得到：")
for c in conclusions:
    print(f"  + {c}")

target = next((f for f in inferred if "RequiresCooling(Lathe_003)" in str(f.conclusion)), None)
if target:
    exp = ExplanationGenerator().generate_explanation(target)
    print(f"\n推理解释：{getattr(exp, 'natural_language', exp)}")

ok = target is not None and any("HighPowerEquipment(Lathe_003)" in c for c in conclusions)
print(f"\n【佐证结论】{'成立' if ok else '不成立'}："
      f"前向链引擎复算出与书中手推一致的两步结论（高功率设备 → 需要冷却）。")
sys.exit(0 if ok else 1)
