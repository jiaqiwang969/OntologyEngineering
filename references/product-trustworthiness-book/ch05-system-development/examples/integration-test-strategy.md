# Clause 7 集成与测试策略：把三个子阶段建成可审计的计划闭环

## 1. 三个子阶段，不等于三套孤立文档

ISO 26262-4:2018 Clause 7 把系统与相关项集成测试分成三个子阶段：先将每个元素的硬件和软件集成，再将系统元素集成为完整相关项，最后把相关项集成到整车中。这是三个不同的验证层次，不是把同一个“测试完成”状态复制三次。

实务中容易出现两个相反的错误。一种是只有一张“系统测试计划”，看不出硬件-软件、系统和整车三层各自验证什么；另一种是三个团队各建一套文档，但没有一个可查询的共同范围，于是 FSR 或 TSR 在三份计划之间漏掉也无从发现。

本书采用一个明确的聚合模型：

```text
IntegrationAndTestStrategy                       一份逻辑策略
  |-- considersWorkProduct -> HARA / FSC / TSC / Architecture / HSI
  |-- hasInput <- HardwareSoftwareIntegrationActivity
  |-- hasInput <- SystemIntegrationActivity
  `-- hasInput <- VehicleIntegrationActivity
                         |
                         `-- produces -> IntegrationAndTestReport
                                          一份逻辑汇总报告
```

“一份”指可统一查询、统一版本治理的**逻辑工作产物**。项目完全可以使用多个物理文件、分卷、测试数据包和附件，只要它们受控地归入同一逻辑工作产物。这是本书为防止范围分裂而采用的 book house policy，不是对 ISO “禁止多份物理报告”的额外解释。

## 2. 把标准语义与本书治理规则分开

| 层次 | 当前模型中的含义 | 边界 |
|---|---|---|
| 标准直接语义 | Clause 7 包含硬件-软件、系统和整车集成三个子阶段 | Part 4 §7.1/§7.4.1.3 |
| 标准直接语义 | 集成前应有安全目标、FSC、TSC、系统架构与 HSI 等先决信息 | Part 4 §7.3.1 |
| 标准直接语义 | 完整集成过程中，每条 FSR 和 TSR 至少验证一次 | Part 4 §7.4.1.5 |
| 标准工作产物 | integration and test strategy 与 integration and test report | Part 4 §7.5.1/§7.5.2 |
| 本书建模构造 | `atIntegrationSubPhase`、`considersWorkProduct`、`verificationExecutionStatus` 等 RDF 关系 | 用于把标准语义转为可查询对象，不冒充 ISO 术语 |
| 本书治理规则 | 三个子阶段共用一份逻辑报告；若未取消活动全部 Completed，报告至少应为 Reviewed | fail-closed 的 book house policy，不是 ISO 符合性结论 |
| EPS 项目特定选择 | 三项活动具体覆盖哪 9 条需求 | `ProjectSpecificStrategy`，不能冒充 ISO 固定映射或通用最佳答案 |

这个区分很重要：SHACL 可以检查“当前图是否违反本书的局部封闭规则”，但不能仅凭绿灯证明项目已执行标准要求的测试。

机器层也保持同样的分离：`IntegrationAndTestStrategyShape` 只标记 `DirectISORequirement`，检查策略明确考虑 FSC、TSC 及对应架构；§7.3.1 的条款级“信息可用”在当前缺少项目阶段上下文节点的图中，被 `IntegrationClausePrerequisiteAvailabilityShape` 投影到策略节点，并明确标记为 `ISORequirementOperationalization`。成熟度、聚合和身份门禁标记为 `BookHousePolicy`，三项 EPS 活动则标记为 `ProjectSpecificStrategy`。单个 Shape 不混写这些依据。

## 3. 先决输入：有连接不等于可准入

EPS 教学策略 `EPS_IntegrationAndTestStrategy_Draft` 通过 `considersWorkProduct` 连接五类工作产物：

| 输入 | EPS 对象 | 当前图可显示或支持检查的内容 |
|---|---|---|
| HARA/安全目标 | `HARA_Report_Draft` | 存在可追溯的安全目标输入 |
| FSC | `EPS_FunctionalSafetyConcept_Draft` | 两条合成 FSR 被聚合在 FSC 草稿中 |
| TSC | `EPS_TSC_Draft` | 策略已连接当前 TSC 草稿 |
| 系统架构 | `EPS_SystemArchitecture_Draft` | 连接的架构正是该 TSC 所引用的架构规格 |
| HSI | `HSI_TorqueInterface_Draft` | 存在两条具有硬件/软件端点的 HSI 接口需求 |

SHACL 门禁不只查 URI 是否存在：它还要求 HARA 真的记录 SafetyGoal，FSC 真的记录 FSR，TSC 真的指向当前架构，并且这些工作产物均有受控的评审状态。五类输入及逻辑报告还通过 `appliesToItem` 唯一绑定 `EPS_Item`；活动必须恰好引用一份策略，范围查询中的 HSI、FSC 与 TSC 也必须由这同一策略解析，不能从两份策略各取一半。

但当前这五类输入仍是教学草稿。因此，这些连接只显示“Clause 7 计划已声明从哪里取输入”，不表示输入内容完整、已批准或已满足项目准入条件。“输入路径存在”和“输入可用”必须分成两个判定。`GATE-CH05-04` 的空结果也只表示当前图在已编码的 Item 身份规则下未报告缺失或冲突，不表示基线版本彼此兼容。

## 4. EPS 的 9 条计划范围映射

当前 ABox 有 6 个唯一需求对象，因为部分需求在多个子阶段重复纳入计划，形成 9 条 `verifies` 范围边：

| 子阶段 | 计划活动 | 纳入计划的需求 | 映射数 |
|---|---|---|---:|
| 硬件-软件集成 | `EPS_HardwareSoftwareIntegration_Planned` | `HSI_TorqueSignal`、`HSI_TorqueDiagnosticUse`、`TSR_TorquePlausibility` | 3 |
| 系统集成 | `EPS_SystemIntegration_Planned` | `FSR_LimitAssistTorque`、`FSR_DetectUnintendedAssist`、`TSR_TorquePlausibility`、`TSR_MotorCurrentLimit` | 4 |
| 整车集成 | `EPS_VehicleIntegration_Planned` | `FSR_LimitAssistTorque`、`FSR_DetectUnintendedAssist` | 2 |
| **合计** | 3 个活动 | 6 个唯一需求对象 | **9** |

这 9 条边是 EPS 教学案例的项目特定范围选择，不是 ISO 为所有项目预先规定的需求到子阶段映射。这个分层表达了三种不同的计划意图：

1. 硬件-软件层同时看 HSI 接口需求和 TSR，防止“只测信号通不通，不看技术安全行为”。
2. 系统层同时看 FSR 和 TSR，将元素交互与系统级功能/技术安全需求放在同一范围中。
3. 整车层将 FSR 纳入计划，检查相关项进入整车后的实现与接口行为。

`TSR_TorquePlausibility` 同时出现在硬件-软件和系统子阶段，两条 FSR 同时出现在系统和整车子阶段。这不是重复计数错误：一条需求可在不同集成层次接受不同视角的验证。

同时也要看清当前门禁的封闭范围：`IntegrationRequirementPlanCoverageShape` 系统性检查的是 FSC 中每条 FSR，以及 TSC 所连接的 TSR 规格中每条 TSR，是否被某个未取消活动纳入计划。硬件-软件子阶段的范围门禁要求至少同时存在**当前策略所考虑 HSI 规格中**的需求和**当前 TSC 中**的 TSR，不允许用另一份未入策略的 HSI 来凑类型。但它尚未实现“每条 HSI 需求均须覆盖”的逐条全封闭规则。当前两条 HSI 都被列入，是 EPS 基线事实，不应被误写成通用门禁已证明所有 HSI 完整。

## 5. `VerificationPlanned` 和 `Draft` 为什么必须保留

`CQ-CH05-04` 查询三个子阶段、活动执行状态及报告成熟度，当前精确返回三行：

| 活动 | 执行状态 | 共享报告 | 报告成熟度 |
|---|---|---|---|
| `EPS_HardwareSoftwareIntegration_Planned` | `VerificationPlanned` | `EPS_IntegrationAndTestReport_Template` | `Draft` |
| `EPS_SystemIntegration_Planned` | `VerificationPlanned` | `EPS_IntegrationAndTestReport_Template` | `Draft` |
| `EPS_VehicleIntegration_Planned` | `VerificationPlanned` | `EPS_IntegrationAndTestReport_Template` | `Draft` |

除上表中的查询列外，策略的 `reviewStatus` 为 `Draft`，三项活动对象的 `activityStatus` 也均为 `Draft`。这是“活动定义本身的成熟度”，不能取代 `verificationExecutionStatus` 所表达的执行事实。若活动进入 `VerificationInProgress` 或 `VerificationCompleted`，本书门禁要求策略和活动定义都先达到 `Approved`；批准仍不证明执行有效。

`CQ-CH05-05` 则返回上一节的 9 条范围边，每一行的状态仍然都是 `VerificationPlanned`。`GATE-CH05-03` 的空结果只表示：当前 FSC/TSC 中不存在尚未被任何未取消 Clause 7 活动纳入**计划范围**的 FSR/TSR；`GATE-CH05-04` 检查五类输入和逻辑报告是否都绑定同一 Item。

这三个结果共同显示当前计划骨架可按已登记查询取回，却不表示任何一次测试已经发生。当前 Clause 7 ABox 没有测试环境、受测配置、测试用例、刺激与预期结果、执行日志、实际结果、通过/失败判定或异常闭环。因此：

```text
Draft strategy/activity + verifies + VerificationPlanned + Draft report
    = 计划范围已记录
    != 测试已执行
    != 需求已验证
    != Clause 7 已完成
```

正因为如此，活动执行状态与工作产物评审状态被分成两条独立轴：活动可以是 Planned、InProgress、Completed 或 Cancelled；策略与报告可以是 Draft、Reviewed 或 Approved。对象间有边，不会自动把两条成熟度轴推到“完成”。当前轻量模型仍把活动定义成熟度和执行生命周期放在同一个活动身份上；真实项目还应像第 9 章那样，把版本化定义、每次执行和结果证据拆成不同个体。

## 6. 用反例理解 fail-closed 门禁

当前 SHACL 不试图替代测试评审，而是先拦住几种明显不可靠的知识状态：

| 反例变更 | 应被拦下的原因 |
|---|---|
| 策略不再考虑 TSC，或 TSC 与所连架构不对应 | 先决输入链不闭合 |
| 取消唯一的整车集成活动 | 策略不再覆盖三个子阶段 |
| 硬件-软件活动中只保留 TSR，删掉所有 HSI 需求 | 不满足该子阶段的最小范围类型约束 |
| 删掉 `TSR_MotorCurrentLimit` 唯一的范围边 | TSC 中出现了无任何未取消活动覆盖的 TSR |
| 活动没有连接逻辑报告 | 无法从活动追溯到预期结果工作产物 |
| 三项活动全部声称 `VerificationCompleted`，共享报告仍是 `Draft` | 完成声明与报告成熟度矛盾 |
| 把其他 validation 生命周期的 `Validated` 写入 Clause 7 执行状态，或新造第四个子阶段 | 显式 `sh:in` 枚举拒绝 RDFS `range` 推理造成的假合法性 |
| 用另一份未被策略考虑的 HSI 规格中的需求替换当前 HSI 需求 | 类型相同但范围身份不同，不能满足当前策略 |
| 同一 Item 建第二份逻辑策略，或让活动同时引用两份策略 | 违反逻辑聚合和唯一策略输入；不能跨策略拼接 HSI/FSR/TSR |
| 五类先决输入或逻辑报告同时绑定另一 Item | 类型正确仍属于跨项目拼接，Item 身份不闭合 |
| `Draft` 策略/活动定义直接声明 InProgress 或 Completed | 执行前的批准门槛未满足；状态跳跃会被拒绝 |
| 子阶段指向第二份逻辑报告 | 违反本书的报告聚合治理；物理分卷应归入既有逻辑对象 |
| 一份逻辑报告同时标记 `Draft` 和 `Reviewed` | 当前成熟度不唯一，不能用任一有利状态绕过门禁 |

最后一条只是完成态的**必要治理条件**：未取消活动全部 Completed 时，关联的逻辑报告至少要 Reviewed。它仍不是充分条件。一份 Reviewed 报告是否真的含有完整测试证据、是否使用合适方法、是否解决异常，当前门禁尚未判定。

反向也要避免误报：计划活动可以在逻辑集成报告之外声明预期的执行日志、原始数据包或异常单。报告聚合与完成态查询都显式限定 `IntegrationAndTestReport`，普通 `WorkProduct` 输出不会被误当成第二份报告，也不被强制套用报告成熟度；该正例保持 `VerificationPlanned + Draft`，不再用空模板伪造完成态。

## 7. 整车集成验证不是 Clause 8 安全确认

“已经把 EPS 装到车上测试”不能自动改写为“已完成安全确认”。当前 `EPS_VehicleIntegration_Planned` 是 Part 4 Clause 7 下的 `IntegrationAndTestActivity`，其计划范围是两条 FSR，状态是 `VerificationPlanned`。它不是 Clause 8 的 safety validation 活动，也没有 Clause 8 的验证规格、环境和报告对象。

两者可以使用同一辆车、部分相同设备或底层原始数据，但审计问题不同，活动身份、输入、接受准则和工作产物也不能混用。正确的复用方式是让两类活动分别引用共享证据，而不是用一个“vehicle test passed”标签合并它们的结论。

## 8. 从计划骨架走向完成证据

对真实项目，当前图只适合作为建档起点。要从 `VerificationPlanned` 走向可评审的 Completed，至少还要对象化并审查：

1. 每个子阶段的集成顺序、受测对象与基线配置。
2. 需求到测试规格、测试用例和接受准则的追溯。
3. 测试环境、刺激、仪器、软硬件版本及校准/适用性证据。
4. 实际执行记录、预期/实际结果、判定、异常与再测闭环。
5. 物理分卷到逻辑报告的版本、基线和批准链。

当前原型尚未为每次测试运行建立独立执行个体，也没有把报告中的结果字段、测试用例和异常闭环做成完成态充分条件。因此，即使未来某个合成 fixture 能通过 `Completed` 的必要门禁，也不能把它当作真实测试证据。

Tables 3–16 的矩阵对象已作为后续批次登记：Table 3 的测试用例导出方法，以及 Tables 4–16 在硬件-软件、系统和整车层的方法/推荐单元，现可在 `system-integration-method-tables.ttl` 中查询。矩阵门禁检查当次加载图中的已登记数量、维度与关系，不能自证分母完整或转录忠实；发布仍须对照受控原 PDF 复核。项目方法选择、组合与偏离理由，以及任何实际执行证据仍未建模，表格对象本身也不会产生测试证据。

表格之外，§7.4.1.3(b)–(d) 的硬件-软件开放问题闭环、车辆系统/环境接口和 SEooC 假设，§7.4.1.4 的配置变体，以及各子阶段其余测试目标与方法应用也仍是 `planned`。来源锚点已建立不等于这些内容已被教材吸收或被门禁实现。

所以，当前成果的准确名称是“Clause 7 三子阶段的计划范围与成熟度骨架”，不是“Clause 7 集成测试完成”，更不是“Part 4 已符合”。

## 9. 本案例的本体化价值

这个案例的重点不是把测试计划换成 Turtle 语法，而是把问题、查询、门禁和失败后的工程动作连起来。仅就 Clause 7 计划骨架的下列五个问题而言，当前准确状态是 **4 个已有可执行检查，1 个能力问题仍待建模**：

| 工程问题 | 当前状态与机制 | 责任角色与失败动作 |
|---|---|---|
| 哪条需求在哪个子阶段由哪项活动计划验证？ | 已实现：`CQ-CH05-05` 返回 9 条项目特定范围边 | 集成测试负责人修订活动范围和选择理由 |
| FSC/TSC 是否有 FSR/TSR 未被未取消活动覆盖？ | 已实现：`GATE-CH05-03` 期望空集 | 需求负责人补计划边，或记录经批准的不适用理由 |
| 五类输入和逻辑报告是否属于同一 Item？ | 已实现：`GATE-CH05-04`、`IntegrationInputItemIdentityShape` 与反例 fixture | 配置/集成负责人停止跨项目拼接并校正身份、版本和基线 |
| 计划、执行和报告成熟度是否被折成一个“完成”？ | 已实现必要条件：`CQ-CH05-04`、显式状态枚举、执行前批准及汇总报告门禁 | 验证负责人撤回越级状态并补审批/报告；仍须人工审查证据充分性 |
| 某次整车测试究竟属于 Clause 7 verification、Clause 8 safety validation，还是共享证据？ | **planned CQ**：当前尚无 Clause 8 活动、规格和报告对象，不能假装查询已实现 | 安全确认负责人先建立两类活动身份、各自准则和共享证据引用，再增加 CQ/反例 |

这才是“本体化”的工程价值：不仅能查，还能知道当前查不到什么、由谁补、补完后怎样验收。图的绿灯只表示它通过了当前已声明的知识约束；测试是否做过、结果是否充分，仍必须回到受控的工程证据。

## 受控来源

- Part 4 §7/§7.1：PDF p22 / MinerU blocks 9/11；§7.1 的 bbox 为 `55,706,884,768`。
- Part 4 §7.3.1：PDF p23 / MinerU block 5 / bbox `112,300,477,317`；安全目标、FSC、TSC、系统架构和 HSI 分别保留在 blocks 6–10 的 `SourceFragment`。
- Part 4 §7.4.1.2/§7.4.1.3：PDF p23 block 23 与 p24 block 1；§7.4.1.3 的四个子项保留在 p24 blocks 2–5。
- Part 4 §7.4.1.5：PDF p24 / MinerU block 9 / bbox `55,488,885,520`。
- Part 4 §7.4.2.1.1/§7.4.2.1.2/§7.4.2.2.2：PDF p25 blocks 3/4 与 p26 block 0。
- Part 4 §7.4.3.1.1/§7.4.3.2.2：PDF p27 block 6 与 p28 block 0。
- Part 4 §7.4.4.1.1/§7.4.4.1.2/§7.4.4.2.2：PDF p29 blocks 6/8 与 p30 block 4。
- Part 4 §7.5.1/§7.5.2：PDF p32 / MinerU blocks 3/4，bbox 分别为 `57,497,660,513` 与 `57,529,768,546`。

结构化来源位于 `structured/mineru/ISO-26262-2018/part-04-system-level-product-development/native-full/`。本文使用中文归纳和合成 EPS 教学对象，不复制标准原表或大段原文。

## 附节（R1-p4-tables3-16）：十四张方法表的层次画像

三个集成子阶段各带自己的方法表（当前账本登记在 `system-integration-method-tables.ttl` 中：
14 表/53 方法/212 单元，并由三层闭世界门禁检查当次图）。读法与 Part 6 相同（§4.3 备选条目选组合给理由），
但这批表多了一个维度值得凝视——**同一方法沿"硬件-软件→系统→整车"层次爬升时画像在变**：

**用例推导先行（Table 3，7.4.1.6）**：九种推导方法是三个层次共用的入口，其中 1g
"公共极限条件/时序/相关失效来源分析"显式指向 Part 9 Clause 7——DFA 不只是安全分析章的事，
它直接喂集成测试用例。1h/1i（环境条件/现场经验）B 起即 ++，集成测试从一开始就被推向真实世界。

**层次爬升的三条画像线**：

- **故障注入**：硬件-软件层机制有效性表（T7）A/B 仅 +；系统层实现表（T9）C 起 ++；
  到整车层实现表（T13）**A 起全 ++**。越接近真实使用环境，故障注入越被强推（`CQ-CH05-07` 固化）。
- **背靠背对比**：硬件-软件层（T4 1c/T5 1a）+ + ++ ++；到系统层（T9 1c/T10 1a）滑落为
  **o + + ++**。一种可用的工程解读是：随着系统行为与环境交互增多，单一比较模型的保真边界更需显式评估；这是对推荐矩阵的教学解释，不是 ISO 给出的因果结论。
- **稳健性验证**：硬件-软件层（T8）资源/压力测试 D 才 ++，且其要求条款 7.4.2.2.6 的括号
  语义是 (A),(B),(C),D——**稳健性证明对 A/B/C 只是建议，对 D 才是要求**；系统层（T12）
  抗干扰/EMC/ESD 全 ASIL ++；整车层（T16）四法 C 起 ++。三张表连读就是"稳健性义务
  随层次与 ASIL 的双向阶梯"。

**括号语义的完整清单**（§4.4：括号内 ASIL 为建议而非要求）：7.4.2.2.3/.4 →(A)；
7.4.2.2.5 →(A)(B)；7.4.2.2.6 与 7.4.3.2.3 →(A)(B)(C)；7.4.4.2.3/.4/.5 →(A)(B)。
对应表个体的 `rdfs:comment` 保存当前转录文本；逐字忠实仍须回到受控原文复核。`o` 单元在当前图中集中于系统层 A（T9/T10/T12 共 6 格）
与整车层性能表 A（T14 三格）。它们只表示 ISO 对该 ASIL 下使用该方法没有倾向，不是"允许减免"；项目仍须按 §4.3 论证所选组合能满足对应要求。

> 覆盖边界：与 Part 6 批次同理，脚注条件细则 OCR 受损，本批次只转录推荐矩阵与括号语义；
> 项目方法组合的选择理由与执行证据仍是真实工程活动，本仓库不代填。

## 附节（R1-p4-clause7-remaining）：集成执行的七条容易漏掉的语义

方法表之外，Clause 7 还有一组执行级要求、总体组织语义和支持信息，工程上最容易被"我们有测试计划"一句话带过：

1. **标准定义的总体集成顺序（7.2）**：硬件-软件→系统→整车的自底向上顺序不是项目习惯，而是 §7.2 对 Clause 7 的总体组织描述；该段用的是陈述式 `is carried out`，不应单独改写成 `shall`。具体子阶段的规范性动作分别见 7.4.2.1.1、7.4.3.1.1 和 7.4.4.1.1。
2. **可考虑的支持信息有外部来源（7.3.2）**：整车架构、**其他车辆系统的技术安全概念**、安全分析报告。前两者可来自相关项之外，说明集成策划常是跨团队/跨供应商的信息汇集点。但原文是 `can be considered`，不是 §7.3.1 的 `shall be available`；不得仅因未获取就自动判为 ISO 输入违规。
3. **测试活动挂在 Part 8 Clause 9 框架下（7.4.1.1）**：三个检查目标（需求正确实现/安全机制的
   功能性能、精度与时序/内外接口一致正确）不是自由发挥，验证的计划-规格-执行-评价遵循
   ISO 26262-8:2018 Clause 9 的通用验证框架。
4. **开放问题必须闭环（7.4.1.3 b）**：系统/整车级测试规格要**确保硬件-软件验证中的
   open issues 被处理**——上一层"遗留问题清单"是下一层测试策略的显式输入，不是附录。
5. **接口与环境入策略（7.4.1.3 c）+ SEooC 假设核销（7.4.1.3 d）**：策略要覆盖相关项内外的
   车辆系统接口与环境；用了 SEooC 开发的元素，就要核对当初开发假设是否需要在集成中验证——
   这与第 2 章 SEooC 的"假设-核销"闭环直接呼应。
6. **配置变体要证据（7.4.1.4）**：可配置系统（元素变体/标定数据）须对**量产预期配置**给出
   合规证据；NOTE 允许"有论证的配置子集"——子集是论证出来的，不是默认的。
7. **测试目标→表格的机制（7.4.2.2.1/7.4.3.2.1/7.4.4.2.1）**：每个层次先立测试目标，
   再由对应表格的适当方法组合落实；NOTE 2 说明，在功能、复杂度或分布式特性使其合理时，某些测试可在其他集成子阶段执行，**前提是给出充分理由**。这是测试执行位置的调整，不等于可以取消任一集成子阶段。

> 与本体的连接：7.4.1.6/7.4.2.2.2-6/7.4.3.2.2-5/7.4.4.2.2-5 的条款锚点已随方法表批次入
> `system-integration-method-tables.ttl` 与 `source-anchors-part4.ttl`；本附节的 7.2/7.3.2/
> 7.4.1.1/7.4.1.4 与三条 x.2.1 锚点随本条目补入 source-anchors-part4.ttl。执行级证据
> （开放问题清单、配置论证、SEooC 核销记录）是真实工程活动，仓库只建结构不代填。
