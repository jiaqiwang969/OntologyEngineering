# 第9章：生产、运行、服务与报废

本章沿一条合成的 `EPS-CAL-B4` 放行冲突，追问开发前提怎样进入生产控制、一次 B3 读回怎样触发过程与产品处置、单件 PASS 怎样获得身份，以及车辆离厂后服务和现场监控怎样继续守住同一前提。正文采用九个连续问题，不再按条款或文件逐项盘点。

```text
chapter_status: rewritten-nine-question-narrative
chapter_model_status: implemented-with-explicit-execution-report-and-field-analysis-gaps
eps_example_status: TeachingExample; Draft definitions only; no execution asserted
expert_review_status: pending
source_review_status: selected-anchor-review-pending
source_rights_status: not-cleared-for-republication
```

知识门禁通过只表示当前图和反例满足已编码合同，不表示专家评审、量产放行、某颗产品通过、现场过程实施或 ISO 合规。

## 正文合同

`chapter.md` 约 2.9 万汉字，含 9 个 H2 问题、8 张用途明确的表和 3 幅冲突图。问题链为：

```text
字段齐全为何不能签
  → 开发到底交什么
  → 特殊特性为何不是车载安全机制
  → 一行读回怎样成为可执行控制
  → B4/B3 偏差走哪条决策路
  → PASS 怎样绑定这一颗 ECU
  → 服务与报废由谁接手
  → 现场数据何时进入行动
  → 机器和签字人各能批准什么
```

`EPS-CAL-B4`、`EPS-CAL-B3`、V17/E17/E92 和现场工单都是正文桌面走查数据，未被写成主 ABox 的生产、服务或现场执行事实。

## 当前教学图的事实边界

主图当前登记：

- 一份 `Draft` 生产控制计划；
- 一份生产过程定义；
- 两项安全相关特殊特性；
- 两个生产控制步骤定义；
- 三项合成生产资源；
- 一份 `Draft` 现场观测说明及一份由它定义的现场监测过程定义。

主图当前未登记：

- `ProductionExecution` 或 `ProductionControlExecution`；
- 控制措施报告、生产配置、量产放行报告或生产偏差；
- 服务/报废执行与记录；
- 现场观测、独立分析执行、现场问题或行动。

图中的 0 表示“当前交付图没有这类实例”，不表示外部现实世界中没有生产、服务或现场问题。

## 定义、执行、报告与产品身份

```text
ProductionControlPlan --documents--> ProductionProcessDefinition
ProductionProcessDefinition --hasControlStepDefinition--> ProductionControlStepDefinition

ProductionExecution --conformsToProcessDefinition--> ProductionProcessDefinition
ProductionExecution --hasControlExecution--> ProductionControlExecution
ProductionControlExecution --executesControlStepDefinition--> ProductionControlStepDefinition
ProductionControlExecution --reportedIn--> ControlMeasuresReport
```

`Approved` 是定义评审状态，`ProductionCompleted` 是一次执行状态，两者不能互相推出。报告当前只被局部检查日期、受控对象标识、结果和评审状态；自由字符串对象标识及孤立报告仍不能证明“这份报告属于这一颗 ECU”。

## 三层规则身份

| 身份 | ch09 示例 | 不能宣称什么 |
|---|---|---|
| ISO 直接要求 | 控制计划的顺序、方法、必要资源和判据；控制报告三项底线；批准配置/责任人授权偏差；现场问题触发行动；服务/报废按说明执行并留档 | 本地 RDF 字段名、状态机和关系就是标准原文 |
| 本书投影 | 特性有控制或处置理由；Draft/Approved 激活策略；设备/工具不适用理由；定义/执行分离；责任 Agent 关系 | 这些闭世界选择是 ISO 的逐字字段 |
| EPS 项目策略 | B4 目标、单件读回、身份/指纹组合、拒收与 V17 服务走查 | 所有项目都必须使用同一方法或字段 |

Part 10 Clause 14 只作为资料性指南解释“开发识别—生产规划控制—生产中监控”的连续性，不新增规范性义务。

## 已编码门禁与本轮修复

- Approved 控制计划的步骤、顺序、方法、必要设备/工具或不适用理由、判据；
- 定义与执行角色分离、执行状态受控、完成控制有报告；
- 控制报告日期、对象标识、结果和本书要求的评审状态；
- 对执行中或已完成的生产逐条检查配置：每个 `usesConfiguration` 都要有 Approved 放行覆盖，或有针对该配置且由相应责任 Agent 授权的偏差；
- 已登记现场问题指向行动；
- 已登记服务/报废执行同时引用适用说明与执行记录。

逐配置授权已经用 `invalid-production-mixed-approved-unapproved-configurations.ttl` 锁住：同一次执行中，一个配置获批不能遮住另一个未获批配置。`GATE-CH09-04` 保留违规配置绑定，SHACL 节点结果去重。

## 明示的机器缺口

1. §6.4.1.1 的生产准入尚未闭合：执行未被强制引用由 Approved 控制计划文档化的过程定义。
2. `ProductionCompleted` 尚未被要求覆盖全部计划步骤及其完成报告。
3. 报告未强制绑定一次控制执行和序列化产品对象；孤立但字段齐全的报告仍可通过局部 Shape。
4. 现场模型没有独立分析执行与“未识别问题/证据不足”结果；当前仍是 `FieldObservation → FieldIssue → FieldAction` 的粗粒度捷径。
5. 结构门禁不证明设备能力、校准、工具置信、对象真实性、影响分析充分性或行动有效性。

这些缺口是当前声明边界，不是由空集绿灯自动关闭的待办。

## 来源与辅助材料

- `SOURCE-AUDIT.md`：Part 1、Part 7、Part 10 的选定来源坐标、规则身份与待审状态；
- `examples/production-control-plan.md`：Draft 定义模板，不是执行或报告；
- `examples/field-monitoring.txt`：条款三段语义与当前粗粒度映射的对照；
- `ontology/abox-eps-production.ttl`：当前 EPS Draft 定义事实；
- `ontology/source-anchors-part1.ttl`、`source-anchors-part7.ttl`、`source-anchors-part10.ttl`：本章选定坐标锚点；
- `ontology/shapes.shacl.ttl`、`eval/eval-cases.yaml`、`eval/shape-fixtures.yaml`：机器合同及单因反例。

## 运行

```bash
.venv/bin/python eval/check_outline_contract.py
.venv/bin/python eval/check_coverage.py
.venv/bin/python eval/run_eval.py
.venv/bin/python -m unittest eval/test_run_eval.py
.venv/bin/python handbook/render_figures.py --force
python3 handbook/build_handbook.py
```

coverage 当前仍是开发态；selected anchors 的文本、专家、权利与发布处置没有完成，不得把 `anchor_only` 解释成可出版来源审计已完成。
