---
name: engineering-evidence-methods
description: Check whether engineering measurements, models, causal comparisons, observations, validation data, proofs and cost bases support a stated claim. Project source-bound facts to Semantica; distinguish applicability failures from missing evidence and physical failures. Use when evidence is being transferred into a CAD, manufacturing or research decision.
---

# 工程证据与方法

先写清本轮主张，再检查支持它的证据。这个共用模块连接 CAD、工艺成本、仿真、实验与测量；求解、采集和材料操作仍由专业工具完成。正式 TBox、CQ、SHACL、SPARQL 和规则只由根 [ontology-engineering](../../SKILL.md) 锁定的 Semantica 执行。

资料量大且问题重复时，按[共用批量判断入口](../../docs/judgment-intake.md)使用受控问题目录。
需采用项目断言或审阅模式拆合时，按[受控采用与迁移](../../docs/judgment-governance.md)
绑定确切来源、审阅和决定；普通只读审查与模型候选不触发采用。
`judgment_batch.py` 保留来源、候选、未知与请求失败；`judgment_review.py` 从原始响应
核对候选身份，并逐项调用原生 review。功能审查覆盖从本次实际执行汇总；
导入 PASS 文件不能替代执行。`impact` 模式只沿已记录的支持依赖列出重审对象，
物理因果假说另存，不把依赖闭包当作已证明的物理机制。

## 选择与主张有关的检查

| 主张依赖什么 | 要核对什么 |
| --- | --- |
| 数值或参数 | 量的种类、观测算子、参考态、区域、单位、适用条件和使用目的 |
| CAD／仿真表示 | 决定结论的特征是否保留；误差界是否适合所需判断 |
| 因果或驱动能力 | 干预／对照是否可比；强制运动、外载、支撑和功由谁提供 |
| 没观察到 | 当时是否有观察机会、所需分辨率和范围；遮挡不能当不存在 |
| 独立验证 | 数据血缘、实验单位、冻结和解盲；改文件名不产生新样本 |
| 工具检查 | 工具确实执行、输入版本一致，能力与已执行项目覆盖所需义务 |
| 物料和成本 | 物理有效量、实际投料、损耗及费用使用同一操作、版本和口径 |
| 数学证明 | 精确命题、假设与实际变量的映射；已证定理不自动证明实物适用 |
| 主动补信息 | 哪个缺口、执行了什么、前后用同一判据比较；计划不等于采集完成 |

无需把全部检查施加给简单任务。记录为什么某项主张需要这些义务；选择的完整性仍需工程判断。只影响相关主张，不停止独立的探索工作。

## 从 ABox 到检查结果

1. 按 [证据合同与调用](references/evidence-contract.md) 把专业工具或受控审阅的事实放入 JSON 快照；记录每个字段的来源文件摘要与 JSON Pointer。候选、假设和已观察事实分别保存，不能先把假设写成实测事实再检查。
2. 用 `scripts/method_evidence.py list` 查看当前锁定 Semantica 包的字段与适用边界。`project` 只核对来源完整性并生成一个主张的 ABox，不作工程判决。
3. 经根 source-locked `semantic_engagement.py review` 调用精确晋升版本的命名 query/shape。没有项目绑定时仍可整理事实；作者案例回归不替代该项目的 review。
4. `satisfied` 表示该项证据义务在给定记录下满足；`violated` 表示存在明确不适用或不一致；`unknown` 表示尚不足以判断。后两者均不能继续为所申请的结论背书，但不自动断定实物失效。
5. 沿主张依赖回写 CAD／工艺方案。模型删掉必要界面时，只能撤回它对界面功能的支持；若源几何本身已有相反证据，另立有来源的挑战。参数、配置或原件变更后重新检查受影响的支持边，保留旧回执。

`analysis-record.v1` 仍可作为来源记录。其自由文字假设不会自动成为本模块已核对的条件；新增字段必须由有来源的提取／审阅产生，不能自动补成有利值。

## 从经验到 TBox

按 [方法提炼与成熟度](references/method-internalization.md) 接收未完善但有价值的想法：可先形成假说、适用域和反例，再用合成跨域案例检验检查逻辑。Semantica package 的技术回归通过，不等于方法已在真实工程中证明收益，更不能把项目参数提升为通用规律。

冻结案例通过 `scripts/run_methodology_cases.py --run --output <新目录>` 重放。语义包身份和内容来自根 `runtime/semantic-bundles.json`；分发的 ZIP 是 Semantica 资产运输件，不是第二套本体正本。
