# TSC、系统架构与安全分析：从文档名称到可执行闭环

## 1. TSC 不是 TSR 清单

ISO 26262-4:2018 §6.2 把技术安全概念（TSC）界定为一个聚合体：它同时包含技术安全需求、对应的系统架构设计，以及“这个架构为何适合实现安全需求”的理由。因此，只写出一批 TSR，即使每条都有 ASIL 和分配目标，也不等于已形成 TSC。

本书将这个差异转成五类对象：

```text
TechnicalSafetyConcept
  |-- hasTechnicalSafetyRequirementsSpecification
  |      -> TechnicalSafetyRequirementsSpecification
  |            -> documents -> TechnicalSafetyRequirement*
  |-- hasSystemArchitecturalDesignSpecification
  |      -> SystemArchitecturalDesignSpecification
  |            |-- implementsRequirement -> TechnicalSafetyRequirement*
  |            `-- documents -> Element*
  `-- architecturalSuitabilityRationale -> non-empty rationale

SystemArchitecturalSafetyAnalysis
  |-- hasInput -> SystemArchitecturalDesignSpecification
  |-- safetyAnalysisExecutionStatus -> Planned / InProgress / Completed / Cancelled
  `-- produces -> SafetyAnalysisReport
```

这个图里有两种关系不能混用：

- `implementsRequirement` 说明架构规格整体实现了哪条 TSR。
- `allocatedTo` 说明某条 TSR 具体由哪些系统、硬件或软件元素承担。

前者不能替代后者。一份架构可以声称“实现 TSR-1”，但如果 TSR-1 分配给 ECU 和软件组件，而架构规格没有记录这两个元素，该实现声明仍然不闭合。

## 2. 哪些来自标准，哪些是本书模型

| 层次 | 本书采用的结构 | 来源边界 |
|---|---|---|
| 标准直接要求 | TSC 聚合 TSR、对应系统架构和适用性理由 | Part 4 §6.2 |
| 标准直接要求 | 系统架构设计实现 TSR | Part 4 §6.4.3.3 |
| 标准直接要求 | 对系统架构执行安全分析 | Part 4 §6.4.4.1 |
| 标准工作产物 | TSR 规格、TSC、系统架构规格、安全分析报告 | Part 4 §6.5.1/§6.5.2/§6.5.3/§6.5.7 |
| 本书建模构造 | `hasTechnicalSafetyRequirementsSpecification`、`implementsRequirement`、执行状态等 RDF 词汇 | 用于把标准语义转成可查询对象，不冒充 ISO 术语 |
| 本书门禁策略 | 已批准 TSC 不允许仍只有 Planned 分析和 Draft 报告 | 局部封闭的 book house policy |

§6.4.4.1 的主句与四个分析目标分布在 MinerU JSON 的五个文本块中。本书保留了主锚点和四个 `SourceFragment`，以防止只锚主句而丢掉实质目标。中文归纳后，这些目标是：

1. 论证系统设计对安全相关功能和属性的适用性。
2. 识别失效原因及故障影响。
3. 识别或确认安全相关系统元素和接口。
4. 支持设计规格，并根据已识别的原因与影响检查安全机制的有效性。

当前 EPS 报告对象只是空模板，因此上述四项尚无任何一项可以声称完成。

## 3. EPS 教学案例

EPS ABox 中的五个关键对象均为合成教学数据：

| 对象 | 类型 | 当前成熟度 | 当前图可显示或支持检查的内容 |
|---|---|---|---|
| `EPS_TSR_Specification_Draft` | TSR 规格 | Draft | 记录了两条 TSR |
| `EPS_SystemArchitecture_Draft` | 系统架构规格 | Draft | 两条 TSR 都有架构实现边，且分配元素可对应 |
| `EPS_TSC_Draft` | TSC | Draft | 需求、架构与有限适用性理由已聚合 |
| `EPS_SystemSafetyAnalysis_Planned` | 系统架构安全分析 | AnalysisPlanned | 已指定输入和报告模板，尚未执行 |
| `EPS_SafetyAnalysisReport_Template` | 安全分析报告 | Draft | 只是空模板，没有分析结果 |

`CQ-CH05-03` 沿着这条链查询，得到三个分配对应：

| TSR | 架构中已记录的分配元素 | 分析状态 | 报告状态 |
|---|---|---|---|
| `TSR_MotorCurrentLimit` | `AssistMotor` | AnalysisPlanned | Draft |
| `TSR_TorquePlausibility` | `EPS_ECU` | AnalysisPlanned | Draft |
| `TSR_TorquePlausibility` | `EPS_ControlSoftware` | AnalysisPlanned | Draft |

这个查询结果显示当前图已声明计划与对象之间的连接，而不表示安全分析已通过。

## 4. 为什么要把成熟度与对象关系分开

一个常见的错误是：图中已经存在“安全分析活动 -> 安全分析报告”边，于是下游系统把它读成“分析已完成”。这是把对象存在性错当成执行事实。

本书用两条独立状态链阻断这种误读：

- 活动用 `safetyAnalysisExecutionStatus` 表示 Planned、InProgress、Completed 或 Cancelled。
- 工作产物用 `reviewStatus` 表示 Draft、Reviewed 或 Approved。

对应的 fail-closed 规则是：

1. `AnalysisCompleted` 不能只连接 Draft 报告。
2. Approved TSC 不能依赖 Planned 分析或 Draft 报告。
3. Draft TSC 可以连接 Planned 分析，但门禁消息和正文必须明说这只是计划完整性。

这些成熟度规则是本书的工程治理策略。它们用来防止错误声明，不能反向用来证明项目已符合 ISO 26262。

## 5. 本轮边界

当前闭环只覆盖 TSC 的最小聚合结构、TSR 与架构元素的对应、安全分析的计划路径与成熟度防误报。以下内容仍未完成：

- §6.4.3.2 与先前架构的一致性检查。
- §6.4.3.4 至 §6.4.3.6 的可验证性、技术能力、集成可测性、接口隔离和分解约束。
- §6.4.4.1 安全分析的实际执行、Table 1 方法选择和结果证据。
- §6.4.4.2 至 §6.4.4.7 的内外部失效原因处置、系统性故障避免和 HARA 反馈。
- §6.4.9 的 TSR、系统架构、HSI 与 TSC 验证。
- Clause 7 系统与相关项集成测试，以及 Clause 8 安全确认。

因此，这一案例的准确名称是“TSC/架构/安全分析计划的最小对象闭环”，不是“系统层开发完成”。

## 受控来源

- Part 4 §6.2：PDF p14 / MinerU block 1 / bbox `55,126,885,185`。
- Part 4 §6.4.3.3：PDF p17 / MinerU block 9 / bbox `112,438,850,456`。
- Part 4 §6.4.4.1 主句：PDF p17 / MinerU block 17 / bbox `112,737,941,768`。
- §6.4.4.1 四个目标：PDF p17 / MinerU blocks 18-21，已分别建立 `SourceFragment`。
- Part 4 §6.5.1/§6.5.2/§6.5.3/§6.5.7：PDF p22 / MinerU blocks 2/3/4/8。

结构化来源位于 `structured/mineru/ISO-26262-2018/part-04-system-level-product-development/native-full/`，关键散文已与原 PDF 的 `pdftotext -layout` 输出交叉核对。本文使用中文归纳和合成 EPS 对象，不复制标准原表或大段原文。
