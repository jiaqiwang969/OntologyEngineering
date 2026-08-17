# 第3章 安全管理：谁能为安全签字

本章用一场“全部指标为绿、量产放行却无人能签”的 EPS 会议，回答谁负责功能安全、怎样让计划真正发生、如何选择确认措施与独立性，以及凭什么接受一个安全论断。正文已按 ch04 的问题链方法全文重写；领域内容主要来自 ISO 26262-2:2018，本体化重点是把 Safety Case、Claim、Argument、Evidence、确认措施及其状态建成可查询、可校验但不越权批准的关系网。

> 案例边界：EPS Safety Case 是 `Draft`，SG1 Claim 是 `ClaimOpen`，七项引用对象均为 `EvidenceCandidate`，确认评审是 `MeasurePlanned`。新增的 Clause 8 安全确认规范与报告仍为 `Draft`，活动、结果和评价分别停在 `SafetyValidationPlanned`、`ValidationCaseNotRun`、`ValidationEvaluationNotPerformed`；候选件数量增加不会推动 Claim 状态。计划 I3 只表示安全计划的配置达到 Table 1，不表示评审已经执行，更不表示 SG1 已获接受。

## 本章交付物

| 产物 | 作用 |
|---|---|
| `chapter.md` | 本章连续教材正文：绿表为何不能签→责任为何消失→角色与能力双门槛→活计划与 DIA→确认措施与真实独立性→Safety Case 的 CAE 论证→FSA 建议与放行决定分离→机器拒绝边界→倒带进入第一次 HARA |
| `examples/safety-plan-template.md` | 确认措施计划字段与状态转换模板 |
| `examples/safety-culture.md` | **R5b**：§5.4.2.1 shall 义务 + Annex B Table B.1 全部 18 判据/9 对对照 + 9 组本书机制映射 + "结构进本体，判断留证据"边界 |
| `../../ontology/safety-culture-catalog.ttl` | **R5b+**：安全文化受控目录——规范/资料模态分层、18/9/5/9 闭世界清单、`contrastsWith` 配对、`mechanismEcho` 映射与自动/人审机制边界 |
| `examples/confirmation-measures.txt` | Table 1 独立性等级矩阵 11 行/55 单元、`—` 与 I0-I3 的语义区别 |
| `examples/safety-case-skeleton.txt` | 带 Claim/Evidence/确认状态的 CAE 骨架 |
| `../../ontology/confirmation-independence.ttl` | Table 1 全部 11 行在 QM/ASIL A-D 下的 55 个独立性等级映射单元 |
| `../../ontology/abox-eps-safetycase.ttl` | EPS 教学 Safety Case 与候选证据关系 |
| `../../ontology/source-anchors-part1.ttl` | Safety Case 术语 1-3.136 的真实锚点 |
| `../../ontology/source-anchors-part2.ttl` | Part 2 §6 与三页 Table 1 的真实锚点 |
| `../../ontology/shapes.shacl.ttl` | 独立性、执行状态、Claim/Evidence 治理，以及安全文化目录的七层闭世界门禁 |

## 核心对照表：确认措施与独立性

以下完整重构 Table 1 的**独立性等级矩阵**：9 类确认评审、功能安全审核和功能安全评估，共 11 行×5 列。

| 确认措施 | QM | ASIL A | ASIL B | ASIL C | ASIL D |
|---|---|---|---|---|---|
| 相关项层面影响分析确认评审 | I3 | I3 | I3 | I3 | I3 |
| 危害分析和风险评估确认评审 | I3 | I3 | I3 | I3 | I3 |
| 安全计划确认评审 | — | I1 | I1 | I2 | I3 |
| 功能安全概念确认评审 | — | I1 | I1 | I2 | I3 |
| 技术安全概念确认评审 | — | I1 | I1 | I2 | I3 |
| 集成与测试策略确认评审 | — | I0 | I1 | I2 | I2 |
| 安全确认规范确认评审 | — | I0 | I1 | I2 | I2 |
| 安全分析与相关失效分析确认评审 | — | I1 | I1 | I2 | I3 |
| Safety Case 确认评审 | — | I1 | I1 | I2 | **I3** |
| 功能安全审核 | — | — | I0 | I2 | **I3** |
| 功能安全评估 | — | — | I0 | I2 | **I3** |

`—` 表示对是否开展该确认措施既无要求也无正反建议。I0 不是“无要求”：它表示建议开展；一旦开展，应由不同于相关工作产物创建者的人员执行。I1-I3 的组织独立性逐级提高。模型因此用 `NoConfirmationMeasureRequirement` 表示 `—`，而不把它伪装成 rank 更低的 I0。

> 覆盖边界：原表每行还有不同的 `Scope` 和 `Independence with regard to`。当前只完整对象化 55 个等级单元，尚不能声称 Table 1 所有行语义已结构化。

## 来源边界

- Part 1 3.136 定义 Safety Case 是针对 item 或 element 的功能安全论证，并由开发活动工作产物汇集的证据支撑。
- Part 2 6.4.6 NOTE 3 资料性说明，执行确认措施人员的独立性等级在安全计划中规定。本书据此设计 `plannedAtIndependence` 字段，但不把 NOTE 冒充为独立 SHALL。
- Part 2 6.4.8 要求形成 Safety Case，并建议随安全生命周期逐步汇集工作产物。
- Part 2 6.4.9 与 Table 1 给出确认措施及组织独立性；6.4.10-6.4.12分别约束评审、审核和评估。
- Part 2 6.5.4 将 Safety Case 列为工作产物，6.5.5 列出确认措施报告。
- Part 2 6.4.13 规定量产放行决定，6.5.6 把 `ReleaseForProductionReport` 列为其工作产物；Part 7 随后把该报告作为生产及运行等阶段的输入，并用其限定获批配置。报告类存在不证明放行决定已发生。

Table 1 跨物理 PDF 第 29-31 页。MinerU 对第 30、31 页保留了表格几何坐标，但逐页 HTML 为空；表值已与原 PDF 的 `pdftotext -layout` 输出交叉确认。来源锚点明确记录这一提取限制。

## 本体化实践

### 1. 论证关系与治理状态同时建模

`SafetyCase` 是 `WorkProduct`，通过 `containsClaim` 和 `containsArgument` 明确论证边界。`Argument` 再以 `backedByEvidence` 引用工程对象。连边只表示“被引用”，不会自动把草稿、教学值或占位 DFA 提升为已接受证据。

状态单独记录：Safety Case 用 `reviewStatus`，Claim 用 `claimStatus`，Evidence 用 `evidenceStatus`。本书门禁还要求：已批准 Safety Case 通过 `confirmedBy` 指向已完成且有报告的 Safety Case 确认评审，该评审再以 `appliesToWorkProduct` 指回同一 Safety Case；案例内所有 Claim 均为 `ClaimAccepted`，而其实际支撑证据全部处于 `EvidenceAccepted`。这是 CAE 治理契约，不是 ISO 原文字段。

### 2. 受控词表替代字符串判别键

Table 1 映射和项目确认措施都通过 `hasConfirmationMeasureKind` 指向受控个体，例如 `SafetyCaseConfirmationReviewKind`。这避免了字符串拼写漂移，也避免旧 `measureKind` 的 domain 把 `ConfirmationReview` 误推断为 `IndependenceRequirement`。

### 3. 计划配置与实际执行分开

确认措施具有 `MeasurePlanned`、`MeasureInProgress`、`MeasureCompleted`、`MeasureCancelled` 状态。`plannedAtIndependence` 是安全计划配置；`performedAtIndependence` 只允许用于已完成措施。完成状态必须关联 `ConfirmationMeasureReport`，而未完成措施不得用该属性预先挂接报告。

### 4. 缺字段必须失败

`ConfirmationIndependenceShape` 只负责把已记录的计划/实际独立性与 Table 1 比较，并显式标为 `ISORequirementOperationalization`。`ConfirmationMeasureGovernanceShape` 另行负责本书闭世界的字段完整性、受控 kind、唯一映射和计划/执行事实分离，并要求每个措施精确归入与 kind 一致的一种评审/审核/评估专门类型，不得用双重类型绕过对齐。这种拆分防止将项目数据契约冒充为 ISO 原句。

### 5. CAE 边界与成熟度联动

`SafetyCaseArgumentShape` 禁止一个案例内的 Argument 跨边界支持案例外 Claim；`SafetyCaseConfirmationGovernanceShape` 禁止用功能安全审核或针对别案的评审替代本 Safety Case 确认评审，也禁止已批准案例仍包含未接受 Claim；`ClaimGovernanceShape` 禁止已接受 Claim 依赖候选、仅验证或已否决证据。这些 Shape 均标为 `BookHousePolicy`，只能证明记录结构与状态一致，不能证明论证充分或人员真实独立。

### 6. 文化的概念结构可执行，组织状态必须留给证据

`NR_2_5_4_2_1` 对象化§5.4.2.1 的 shall 义务；Table B.1 的 18 条判据则属于共享 `InformativeExample`。两类在 TBox 中互斥，避免把 Annex 校准示例提升为规范要求。`SafetyCultureCatalog_2_B1` 声明 18 判据、9 对、5 主题和 9 机制的精确范围；可执行机制必须指向真实 Shape，人类治理机制则禁止挂 Shape。`GATE-CH03-03` 从该目录的 `hasCultureIndicator` 显式限定计数范围，合法外部目录不会污染 18/9/5/9 基线。这些门禁检查知识目录，不检查任何组织是否文化达标。

## 快速验证

```bash
.venv/bin/python eval/run_eval.py
```

`CQ-CH03-01` 必须精确返回当前七项 `EvidenceCandidate`；`CQ-CH03-03` 应返回 `Draft / ClaimOpen / MeasurePlanned`；`CQ-CH03-04` 必须精确返回 11 行/55 个独立性等级单元；`CQ-CH03-05` 精确返回“制衡判据→独立性机制→`ConfirmationIndependenceShape`”；`GATE-CH03-03` 返回 9 组精确机制映射并携带 18/9/5/9 计数。安全文化 17 个负例各只命中一个责任 Shape，基线目录与一个外部来源扩展示例两个正例均为零违规；这是当前回归证据，表明现有 Shape 没有把该扩展示例误纳入 Table B.1 基线目录的闭世界范围，但不能单凭一个扩展示例证明所有未来扩展都不会受影响。所有 CQ、SHACL 和反例结果的专家评审状态仍为 `pending`；门禁通过是知识模型验收结果，不是 ISO 26262 符合性结论、确认评审、功能安全评估或量产放行。
