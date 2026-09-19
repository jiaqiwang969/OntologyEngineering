---
name: manufacturing-process-cost
description: Develop and revise discrete-manufacturing process and cost proposals from incomplete customer information. Use for context transfer, tacit-knowledge discovery, make-or-buy comparisons, equipment reuse, proposal feedback, and anonymized manufacturing lessons within ontology-engineering.
---

# 生产制造工艺与成本管理

从制造企业接到询问的时点开始，沿着“用途与要求 → 产品及状态 → 工艺与检验 → 资源与供应链 → 成本与交付”发现缺口，推进方案，把反馈和实际结果接回来。适用于离散机械制造；具体行业、材料、工艺参数和验收值由项目证据提供。

这是方法、脱敏案例和工作记录模板模块，版本 `0.5.3`。八份 JSON 记录模板仍为 `0.2.0`，报告组件与图形模板仍为 `1.0.0`；六张评审卡为 `1.4.0`。本版补强成本参数的来源与派生、批次计费及可同时成立的估算情景。冻结的 Semantica 制造案例仍为 `0.1.1`。先遵守根 [ontology-engineering](../../SKILL.md) 的 source lock、发现、绑定和三联输出规则；正式语义执行仍只走 Semantica。通用方法已有受控语义候选，按 [语义执行与证据范围](references/semantic-use.md) 定位；可执行资产存于 Semantica 工作区，不在 skill 内另建语义正本。记录模板不是 `ProjectOntologyBinding`、`SemanticTaskEnvelope` 或语义校验器。缺少绑定只阻断依赖绑定的执行；继续已授权的资料整理、条件分析和问题准备。

## 每轮怎么做

1. **续接当前决定。** 读取项目入口、当前快照、已采用版本、旧答复和未决项。先说明现在要判断什么，例如能否接单、哪条路线合适、预算能精确到哪一步。
2. **比较情境。** 查旧经验成立的条件与本次差异。产品相似不证明使用、装配、失效后果、数量与验收要求相同；差异也不自动支持取消工序。
3. **顺着关系找缺口。** 用 [对象与成本关系](references/manufacturing-map.md) 定位对象、状态和下游影响，再按 [高价值资料与信息筛选](references/high-value-documents.md) 深读决定相关的原件和对话；合并重复内容，保留条件与反证。只追问会改变本轮决定的未知量。用 [工作闭环](references/operating-loop.md) 把原话、隐含条件、解释和关闭依据分开。
4. **形成条件明确的方案。** 把已有事实、假设和待验证能力分别列出。比较自制、外协、设备复用或改造时，先统一产品版本、数量、交付与费用范围。信息不全时给条件分支或部分核算，不把未知费用当零。 多条路线待选时用[验证排序与改选界限](references/decision-patterns.md#p11-降本排序先验证能改变选择的未知量)，把下一动作的结果接到继续、改选或暂缓的具体条件。
5. **接收反馈并修订。** 先定位反馈针对的方案版本和对象。 产品或订单已变化时，按[变更适用范围](references/decision-patterns.md#p06-证据降级下游结论一起退回)确认哪些实物切换、哪些旧结论仍有依据；对照新旧依赖后核算剩余工作。追加来源及变更事件，追踪工艺、检验、资源、成本和输出依赖；对新输出检查自身快照与结果。使用 [工作规范](references/work-norms.md)，保留旧版及改变理由。需要 PDF 时按 [报告设计](references/report-design.md) 选择本轮问题及视图，冻结输入再生成；文档版号不代替工程状态。
6. **接上责任与下一轮。** 交付“能决定什么、改变了什么、还差什么、谁接什么工作、什么条件下继续”。按 [组织闭环](references/organization-loop.md) 记录接收、复核、决定和关闭证据；未得到接收不能代填已承办。按 [案例演变](references/case-evolution.md) 判断新事实只留在项目，还是需要方法、工具或语义变更；已有规则覆盖时说明 `no_delta`。
7. **让经验进入下一任务。** 方法候选经过试用，采用时绑定精确版本、摘要和范围；在办任务明确切换或保留旧版。下一次实际使用另留结果，经营效果按可比实绩衡量，不能用文档数量证明降本。

工艺顺序中的“检测”要明确产品版本、受检状态、阶段、方法和判据。设备可调配不等于能力已验证，报价编号对应不等于报价范围及版次一致，费用分组和图形位置不证明制造顺序。

## 按需要取用

- 客户重要文档、对话筛选、证据深读与决策影响：[高价值资料与信息筛选](references/high-value-documents.md)。
- 复制到另一台机器、检查断链、重跑冻结案例：[独立分发说明](../../docs/PORTABLE-DISTRIBUTION.md)，分发整个根 skill。
- 沟通、隐性知识、跨次续办：[工作闭环](references/operating-loop.md)。
- 建模、检验覆盖、资源与核算：[对象与成本关系](references/manufacturing-map.md)。
- 报价和成本表已经到手，需要判断估算依据、分批费用及区间能支持什么：[从数字到条件估算](references/estimate-basis.md)。
- 从技术讨论发现精度、焊接与设备功能的驱动条件，比较采购/自研并回写工时预算：[要求怎样影响工艺与成本](references/requirements-process-cost.md)。
- 从客户 STEP 完善特征覆盖、毛坯中间态、工序装夹、刀具与加工成本：[从 STEP 到加工路线](references/step-to-process.md)。
- 规范来源、版本、纠错与脱敏：[工作规范](references/work-norms.md)。
- 承办交接、评审决定、经验采用、工作区结构与效果反馈：[组织闭环](references/organization-loop.md)。
- 理解方案为何反复更新：[连续演变案例](references/case-evolution.md)。
- 从最终方案提炼判断方法，或处理三方理解、设备构造、最小验证、费用选择与修订传播：[十二项判断方法](references/decision-patterns.md)，按本轮决定取用[六张评审卡](assets/templates/decision-review-cards.md)；不要求每次填全套。
- 将记录变成可复核的 XeLaTeX 报告：[报告设计、内容契约与使用方法](references/report-design.md)，以及 [四轮合成演变示例](assets/report-template/example/README.md)。组件只映射已提供记录及费用算术，不出具工程语义通过结论。
- 制作或修改工艺流程图：[五类可编辑图形模板](references/process-flow-templates.md)，附节点、状态、来源及连线对应记录；按实际路线填写，先区分制造顺序与知识关系。
- 方法论依据：[书源锚点](references/theory-anchors.md)，再回相应书源。
- 设计语义增量或评估方法：[16 类案例与 64 个验收意图](references/case-designs.json) 保留当时设计状态；后续可执行实现及精确覆盖见 [案例执行映射](references/case-execution.json)。新增组织关系也在该映射中，不能将局部案例通过称为工厂能力验证。

按本轮任务选择记录模板，不要求每次填完整套表：

| 目的 | 模板 |
| --- | --- |
| 发现缺口、提问、部分关闭或重开 | [discovery-record.json](assets/templates/discovery-record.json) |
| 一次调用的输入、沟通、输出与接续 | [loop-run-record.json](assets/templates/loop-run-record.json) |
| 情境、路线、能力和成本方案 | [solution-record.json](assets/templates/solution-record.json) |
| 新旧方案改变了什么、影响何处 | [revision-record.json](assets/templates/revision-record.json) |
| 规范是谁提出、适用哪里、是否采用 | [norm-register.json](assets/templates/norm-register.json) |
| 谁承办、谁接收、怎样审阅并关闭 | [organization-run.json](assets/templates/organization-run.json) |
| 经验候选、验证、采用、切换及再使用 | [learning-adoption.json](assets/templates/learning-adoption.json) |
| 预算与实绩、可比条件、样本和未测效果 | [outcome-review.json](assets/templates/outcome-review.json) |

模板复制到项目私有工作区后填充；`null` 表示尚未记录，不能自动转成零、否定或批准。实例和原始证据不回写到通用 skill。

## 复用与采用

保留因果关系、提问方法、适用条件和反例；去掉客户、供应商、人员、型号、原图、私有路径及可识别的业务组合。脱敏案例只用于学习决策方法，不能证明另一企业的能力。

方法更改用实际任务验证；语义更改加载 [domain-ontology-loop](../domain-ontology-loop/SKILL.md)，形成完整 PackageDelta 与原生回归。学习层、语义执行层、项目采用和外部发布分别记录。每轮评估学习，不在每轮聊天后静默改写 skill。
