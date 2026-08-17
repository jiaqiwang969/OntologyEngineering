# ch09 选定来源锚点与门禁边界审计

本文件记录第9章实际使用的 Part 1、Part 7 与 Part 10 证据坐标、规范性身份和机器投影。它不是来源全文复核报告，也不把知识模型通过解释成工程放行。

```text
status: selected-anchor-audit-expanded-review-pending
chapter_status: rewritten-with-draft-eps-definitions
production_execution_evidence_in_main_abox: none
service_or_field_execution_evidence_in_main_abox: none
expert_review_status: pending
source_rights_status: not-cleared-for-republication
```

## 证据面

选定坐标来自本地 ISO 26262:2018 PDF 的 MinerU 结构化提取，并以物理页、block、bbox 登记。MinerU JSON 与 `pdftotext` 是同一 PDF 的两种视图，不是两份独立来源。当前 coverage 状态为 `anchor_only` 或待覆盖，文本哈希、专家复核、权利与最终处置仍待完成。

| 来源 | 本章用途 | 规范性边界 |
|---|---|---|
| Part 1 §3.147 | 安全相关特殊特性的定义、Notes 1–3 与示例边界 | 定义与 Notes/示例分别保留身份；Note 3 支持“不与安全机制混淆” |
| Part 7 Clause 5 | 生产/服务/报废规划、控制定义、软件与标定、开发回流 | 主要规范性来源；适用性和 Notes 不得被改写成无条件 shall |
| Part 7 Clause 6 | 生产前提、实施与维持、偏差、能力、报告、配置和变更 | 主要规范性来源；定义与一次执行必须分开 |
| Part 7 Clause 7 | 现场数据提供—分析—行动、服务/报废按说明执行 | 主要规范性来源；空集不能证明现实实施 |
| Part 10 Clause 14 | 特殊特性从识别到控制与监控的三阶段理解 | 全部作为资料性指南，不新增义务 |

## 已镜像的关键坐标族

`ontology/source-anchors-part1.ttl`：

- `1-3.147`：定义、Notes 1–3 与示例片段；绑定 `SafetyRelatedSpecialCharacteristic`。

`ontology/source-anchors-part7.ttl`：

- `7-5.4.1.1`–`7-5.4.1.8`：生产规划、正确软件/标定程序、可预见过程失效、特殊特性与控制计划、顺序/方法/必要资源/判据、开发回流与变更；
- `7-5.4.3.1`、`.3`–`.7` 的选定坐标：运行/服务/报废规划、服务配置与追溯、用户信息、报废、开发回流及适用的救援信息；
- `7-6.3.1`、`7-6.4.1.1`–`.7`：生产前提、实施/维持、偏差四步、能力、测试设备控制、报告、配置授权与变更；
- `7-7.3.1`、`7-7.4.1.1`–`.3`：Clause 7 前提、现场三段、按说明执行并文档化、变更；
- 相关选定工作产物坐标，包括生产控制计划、控制措施报告、过程能力报告与现场观测说明。

`ontology/source-anchors-part10.ttl`：

- `10-14`、`10-14.1`–`10-14.4`：三阶段、追溯/评估、组织间交换、安全分析可提供输入、软件/标定示例以及可接受参数范围、评估或测量技术、控制策略与接受判据；所有节点均显式标记 `InformativeStatement` 与 `ISOInformativeGuidance`。

Part 7 Annex A 的表格提取存在 `IS0`、碎片单元格等 OCR 问题，只作导航，不直接生成规范门禁事实。

## 关键来源辨析

1. 量产放行报告前提来自 Part 2 §6.5.6；不存在“Part 7 §6.5.6”。
2. §6.4.1.1 的人员培训在 Note 中说明“实施并维持”的含义，不拆成额外独立 shall。
3. checksum 与 read-back 是 §5.4.1.3 的示例，不是所有项目的固定双重方法。
4. §5.4.1.5 的“考虑”被本书投影为“每项特性有控制或处置理由”，该投影是 `BookHousePolicy`。
5. Part 10 Clause 14 可帮助解释连续性，但不能成为新增阻断义务的唯一依据。
6. VIN/序列号、B4/B3、单件读回与指纹组合是 EPS 项目策略；§6.4.1.5 的通用报告底线是日期、受控对象标识与结果。

## 来源直接支持与当前已编码不是一回事

| 语义 | 来源直接支持 | 当前机器状态 |
|---|---|---|
| 控制定义含顺序、方法、必要资源和判据 | 是 | 对 Approved 计划已编码；Approved 激活与 N/A 理由是本书策略 |
| 按计划实施并维持生产 | 是 | §6.4.1.1 准入和全步骤覆盖尚未闭合 |
| 生产偏差按 a–d 分析并验证措施 | 是 | 正文讲解；当前无完整过程失效/影响/措施有效性对象链 |
| 控制报告含日期、对象标识和结果 | 是 | 已编码字段 Shape；报告—执行—序列化产品绑定未编码 |
| 已批准配置或责任人授权偏差 | 是 | 对执行中或已完成生产的每条 `usesConfiguration` 编码，并有混合配置反例 |
| 提供现场数据、分析、对问题触发行动 | 是 | 只编码“已登记问题有行动”；没有独立分析对象 |
| 服务/报废按说明执行并文档化 | 是 | 对已登记执行检查说明与记录；主图无此类执行 |

## 逐配置假绿的修复证据

旧查询在同一执行同时使用配置 A、B 时，只要 A 有 Approved 放行，就可能把整个执行判绿。当前实现对执行中或已完成的生产逐个检查每条 `usesConfiguration`：

- 正常分支：该配置由 `Approved` 的 `ReleaseForProductionReport` 覆盖；
- 例外分支：偏差明确针对该配置，并由对该执行负责的 Agent 授权；
- 无配置：执行中/已完成生产同样报警。

`invalid-production-mixed-approved-unapproved-configurations.ttl` 固定 A 获批、B 未获批的单因反例；SHACL 产生一个去重节点违规，`GATE-CH09-04` 返回执行与 B 的绑定。主 ABox 的 0 执行空集不足以测试这条规则，因此非空 fixture 和独立 gate 单元测试都是合同的一部分。

## 当前必须保留的机器缺口

- `ProductionExecution` 未被强制引用由 Approved 计划文档化的过程定义；
- `ProductionCompleted` 未被强制覆盖全部计划控制步骤；
- `ControlMeasuresReportShape` 允许不绑定控制执行的字段完整报告；
- 受控对象仍可为自由字符串，没有序列化产品同一性门禁；
- 设备能力、校准、工具置信、真实执行、偏差影响与措施有效性不由结构门禁证明；
- 现场 TBox 跳过独立分析执行和无问题/证据不足结果；主图问题数为 0 时，行动 gate 是空集通过。

## 已接线的代表性反例

- Draft 计划不生成执行；定义/执行同一节点被拒绝；
- Approved 控制步骤缺顺序、方法、判据或必要资源被拒绝；
- 完成控制无报告，或报告缺日期、对象标识、结果被拒绝；
- 无配置、未批准配置、无责任关系的偏差、混合批准/未批准配置被拒绝；
- 已登记现场问题无行动被拒绝；
- 服务/报废执行缺说明或记录被拒绝。

这些 fixtures 证明当前规则会咬住各自反例，不证明主 ABox 有真实生产、服务或现场证据，也不证明未建模语义已覆盖。
