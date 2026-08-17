# ch04 HARA 本体模块规格说明

本文件是写 OWL 前必须存在的专家可评审中间产物。方法采用 METHONTOLOGY 的“规格说明—知识获取—概念化—形式化—实现—评估—维护”主线，并结合 Ontology Development 101 的范围与能力问题步骤。

## 1. 目的与使用者

- 目的：把 Part 3 Clause 5/6 的相关项、危害事件、S/E/C、ASIL、安全目标、假设和工作产物组织成可追溯知识模型。
- 使用者：功能安全经理、HARA 主持人、系统工程师、本体工程师、书稿审校与构建工具。
- 决策边界：本体支持知识检查，不自动批准 HARA，不替代工程判断或独立验证。

## 2. 范围与非目标

范围：3-5.4、3-5.5.1、3-6.3.1、3-6.4.1 至 3-6.4.6、3-6.5，以及 Table 1-4 中 ch04 所需结构。当前来源账本对象化 37 条 shall/shall-not、6 条 may 和 3 条 no-ASIL 适用性边界；仍有明确登记的 partial、pending 和出版权利复核项。

非目标：本模块不完成 FSC/FSR、系统设计、ASIL 分解、量产验证或安全案例批准；不把 EPS 教学数值当作标准事实。

## 3. 概念化产物

### 3.1 术语表

`Item`、`Hazard`、`OperationalSituation`、`HazardousEvent`、`Severity`、`Exposure`、`Controllability`、`ASIL`、`SafetyGoal`、`HARAAssumption`、`HARAClassification`、`HARAReport`、`VerificationActivity`。

### 3.2 概念分类树

```text
StandardDocument -> Part -> Clause -> StandardProvision
WorkProduct -> HARAReport / HARAVerificationReport
SafetyRequirement -> FunctionalSafetyRequirement / TechnicalSafetyRequirement
HazardousEvent -> S/E/C projection + HARAClassification
HARAClassification -> assumptions + status + report + ASIL result
```

`NormativeRequirement` 属于标准知识层；`SafetyRequirement` 属于项目工程层，二者不合并。

### 3.3 二元关系表

| 主体 | 关系 | 客体 | 含义 |
|---|---|---|---|
| HazardousEvent | `hasHazard` | Hazard | 事件中的危害 |
| HazardousEvent | `inSituation` | OperationalSituation | 事件中的场景 |
| HazardousEvent | `leadsToSafetyGoal` | SafetyGoal | HARA 到目标追溯 |
| HARAClassification | `assessesEvent` | HazardousEvent | 一次分级决策 |
| HARAClassification | `basedOnAssumption` | HARAAssumption | 判定前提 |
| HARAClassification | `recordedIn` | HARAReport | 工作产物归档 |
| Knowledge entity | `hasSourceAnchor` | Clause | Part/Page/Block/BBox 来源 |
| Safety requirement | `derivedFrom` | goal/requirement | 项目纵向派生，不表示来源 |

### 3.4 概念字典与属性

- HazardousEvent：唯一 hazard、situation、item、S/E/C；每个维度有 rationale；非边界组合有一个 ASIL/QM。
- SafetyGoal：功能目标表述、唯一 ASIL、至少一个来源危害事件；合并时采用最高 ASIL。
- HARAAssumption：明确陈述和验证状态。
- Clause：稳定 clause ID、PDF page、MinerU block、bbox、source artifact。

### 3.5 实例与规则表

- EPS ABox：2 个危害事件、1 个安全目标、5 个假设、2 个分级决策、HARA 草稿和验证模板。
- Table 4：36 个 `ASILMapping` 实例。
- 边界规则：S0/E0/C0 不要求 ASIL assignment；QM 是 Table 4 结果。

## 4. 能力问题

- CQ-CH04-01：指定 EPS 危害事件的 ASIL 是什么？
- CQ-CH04-02：哪些 ASIL D 事件导出了哪些安全目标？
- CQ-CH04-03：Table 4 的页码、block 和 bbox 是什么？
- CQ-CH04-04：哪些 HARA 假设尚未完成验证？
- CQ-CH04-05：跨 EPS/BMS/AEB 三个相关项，各 ASIL 等级有多少个危害事件？

能力问题界定本体用途和验收能力，不等同于安全需求。

## 5. 验收合同

- RDF/Turtle 全部可加载；
- 主数据集通过 SHACL；
- CQ 使用精确 binding、空集或行数 oracle；
- S0 无 ASIL 正例通过；
- 错误 Table 4 映射和缺失安全目标反例命中指定 Shape；
- 来源锚点可反查 PDF page/block/bbox；
- 来源账本中的规范模态计数与 MinerU 源文扫描一致，且 Permission 与 ApplicabilityBoundary 不混类；
- `expert_review_status=pending` 时不得声称技术批准或 ISO 合规。

## 6. 建模决策

- 直接 `hasSeverity/hasExposure/hasControllability/hasASIL` 用于高频查询；`HARAClassification` 保存决策上下文、假设和状态。
- 来源追溯使用 `hasSourceAnchor`；项目需求派生使用 `derivedFrom`。
- SHACL 采用封闭检查；OWL/RDFS 用于开放世界分类，两者职责分开。
- 教学案例显式类型化为 `TeachingExample`，避免与标准事实和项目证据混淆。
