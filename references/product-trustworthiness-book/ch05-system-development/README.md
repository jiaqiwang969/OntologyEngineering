# 第5章 系统层开发：让安全目标落到架构与接口 / Chapter 5: System-Level Development

本章回答“**安全目标定了以后，怎样把责任、接口、时间和证据分到正确对象上，并且只声称当前证据真正支持的那一步**”。候选正文已按 ch04 的问题链写法完成一次结构性重写：从台架因 HSI 无判据而停下的现场起步，沿同一条 EPS 链经过 FSR/TSR/TSC、架构与安全分析、HSI、时间分摊、Clause 7/8，最后回到同一台架。领域内容来自 ISO 26262-3:2018 Clause 7 与 ISO 26262-4:2018 Clause 6/7/8；工程对象仍只是计划态教学骨架。

> 案例边界：EPS 的 FSR、TSR、TSC、架构、安全分析、HSI、集成策略与安全确认对象均为合成教学数据，不是 ISO 标准自带案例，也不是量产结论。`AnalysisPlanned`/`VerificationPlanned`/`SafetyValidationPlanned`、`NotRun`/`NotPerformed` 与 `Draft` 只显示当前图中已声明的对象、计划范围和诚实缺口，不表示安全分析、§6.4.9 验证、Clause 7 测试或 Clause 8 安全确认已经执行。

## 本章交付物

| 产物 | 作用 |
|---|---|
| `chapter.md` | **候选连续正文**（以问题链、工程判断和证据回查为验收，不以字数为完成判据）：台架停测→技术翻译→有向追溯→架构反问→HSI 合同→最坏时间账→四条证据轴→机械后备单因素变式→CQ/Shape/fixture→回到台架与硬件/软件分叉 |
| `examples/ftti-timing.md` | **R4b**：FTTI/FDTI/FRTI/FHTI/EOTI/EOTTI/DTTI 七时间量归属 + Item 层 FTTI 向安全机制最大 FHTI 的架构分摊 + 阀门控制四场景走查 + EPS 时间预算未闭合边界 + 配图 FIG-P10-6 |
| `../../ontology/system-integration-method-tables.ttl` | **R1-p4-tables3-16**：Part 4 集成测试方法表全量转录——Table 3（用例推导 9 法）+ 硬件-软件（4-8）/系统（9-12）/整车（13-16）三层次 14 表/53 方法/212 单元；要求条款括号语义逐条转录（如 7.4.2.2.6 稳健性对 (A)(B)(C) 仅建议、对 D 才要求）；三层闭世界门禁 |
| `examples/tsc-architecture-analysis.md` | TSC 不是 TSR 清单：三份受控工作产物、架构实现关系、安全分析计划和成熟度 fail-closed 走查 |
| `examples/integration-test-strategy.md` | Clause 7 三子阶段：一份逻辑策略、一份逻辑汇总报告、需求计划覆盖与完成态门禁 |
| `../../ontology/abox-eps-system.ttl` | EPS 追溯链：SG1→2 条 FSR→2 条 TSR；另建 TSC/TSR 规格/系统架构/安全分析计划以及 HSI 双端点草稿 |
| `../../ontology/abox-eps-integration.ttl` | EPS 集成与测试策略、三个 `VerificationPlanned` 活动、六个唯一需求对象的九条范围边、共享 Draft 报告模板，以及 Clause 7/8 可分别引用的底层 Evidence 身份；尚无机械后备、车载通信或供电接口的真实范围对象 |
| `../../ontology/abox-eps-validation.ttl` | EPS Clause 8 活动、环境、规范、用例、方法、结果、评价与报告的 `Planned`/`NotRun`/`Draft` 教学骨架 |
| `../../ontology/source-anchors-part1.ttl` / `source-anchors-part4.ttl` / `source-anchors-part10.ttl` | Part 1 时间词条、Part 4 §6/§7/§8 与 Part 10 §4.4 的已登记局部证据坐标锚点（page/block/bbox 从 MinerU 提取并与 PDF 文本交叉核对；锚点存在不等于条款全覆盖或来源复核已关闭） |
| `../../ontology/shapes.shacl.ttl` | 追溯、TSC/架构/安全分析成熟度、HSI、Clause 7 计划覆盖，以及 Clause 8 环境/规范/逐 SG 用例/方法/逐面评价/状态/报告闭包门禁；规则身份区分 ISO 直接要求、本地操作化、本书治理与项目特定选择 |
| `../../eval/eval-cases.yaml` | `CQ-CH05-01–15` 与 ch05 门禁：前七题覆盖 TSR/HSI/TSC、时间与集成，`CQ-CH05-08–15` 精确查询 Clause 8 环境、规范、用例、四评估面、方法理由、状态、报告闭包和 Clause 7/8 证据身份 |
| `../../eval/fixtures/invalid-asil-downgrade.ttl` | 反例：ASIL 被降级的派生需求，须被拦截 |

## 追溯链（本章主线）

```
SafetyGoal(SG1, ASIL D)
   │ derivedFrom
   ├─ FSR1 限制助力转矩 ──allocatedTo→ EPS_System
   └─ FSR2 检测非预期助力 ─allocatedTo→ EPS_ECU
        │ derivedFrom
        └─ TSR1 转矩合理性校验 ─allocatedTo→ EPS_ECU / EPS_ControlSoftware
   FSR1 → TSR2 电机电流硬限幅 ─allocatedTo→ AssistMotor

EPS_TSC_Draft (TechnicalSafetyConcept, Draft)
   ├─hasTechnicalSafetyRequirementsSpecification→ EPS_TSR_Specification_Draft
   ├─hasSystemArchitecturalDesignSpecification→ EPS_SystemArchitecture_Draft
   │       ├─implementsRequirement→ TSR1 / TSR2
   │       └─documents→ TSR 分配元素
   └─architecturalSuitabilityRationale→ 有限草稿理由
           分析路径为 AnalysisPlanned→Draft report template

HSI_TorqueInterface_Draft (HSISpecification, Draft)
   ├─documents→ HSI_TorqueSignal (HSIRequirement)
   └─documents→ HSI_TorqueDiagnosticUse (HSIRequirement)
          两条需求均 derivedFrom→ TSR1
          两条需求均 allocatedTo→ TorqueSensor + EPS_ControlSoftware

EPS_IntegrationAndTestStrategy_Draft (Draft)
   ├─documents→ EPS_Item
   ├─considersWorkProduct→ HARA / FSC / TSC / Architecture / HSI (均 appliesToItem EPS_Item, Draft)
   ├─HardwareSoftwareIntegrationSubPhase → VerificationPlanned
   ├─SystemIntegrationSubPhase           → VerificationPlanned
   └─VehicleIntegrationSubPhase          → VerificationPlanned
          三项活动 produces→ EPS_IntegrationAndTestReport_Template (Draft)

EPS_SafetyValidationSpecification_Draft (Draft)
   ├─代表性环境 + 标定配置 + SG1 用例 + 方法选择
   ├─四个评估面：3×Applicable + 1×NotApplicable，均 NotPerformed
   └─SafetyValidationPlanned → NotRun result → NotPerformed evaluation
          produces→ EPS_SafetyValidationReport_Template (Draft)
```

ASIL D 沿派生需求链继承（Part 8 §6.4.2.2；FSR 分配另见 Part 3 §7.4.2.8(a)）。若某派生需求 ASIL 低于上游且未走显式分解，门禁会拦下（分解见 ch08）。HSI 规格本身是工作产物，不承接需求专属的 `hasASIL`、`derivedFrom` 或 `allocatedTo`。

## 标准依据 / 权威锚点

- Part 3 §7.4.2.1/§7.4.2.2：FSR 从安全目标派生，每个安全目标至少有一条 FSR。
- Part 8 §6.4.2.2/§6.4.2.3：派生安全需求继承上游 ASIL，并分配给实现它的相关项或元素。
- Part 4 §6.2/§6.4.3.3：TSC 聚合 TSR、对应系统架构与架构适用性理由；系统架构实现 TSR。
- Part 4 §6.4.4.1/§6.5.7：对系统架构执行安全分析，并形成独立安全分析报告工作产物。
- Part 4 §6.4.7.1–§6.4.7.4：HSI 规格描述硬件与软件交互、相关特性和诊断使用，并在系统架构设计阶段形成后继续细化。
- Part 4 §6.5.4：HSI specification 是独立工作产物，不是一条 TSR 的别名。
- Part 4 §6.4.9.1/§6.4.9.2/§6.5.6：TSR、系统架构、HSI、生命周期相关需求和 TSC 需要形成系统层设计验证证据及报告；该活动与 Clause 7 实物集成测试分开。
- Part 4 §7.1/§7.4.1.3：集成与测试分为硬件-软件、系统和整车三个子阶段，策略须覆盖这些层次。
- Part 4 §7.4.1.5：每条 FSR/TSR 在完整集成子阶段中至少验证一次；本章先把它实现为计划覆盖缺口查询，不能据此声称已验证。
- Part 4 §7.4.1.2、§7.4.4.1.2：策略还要考虑对安全概念有贡献的其他技术元素，整车集成还要验证车载通信和供电网络接口；当前 EPS ABox 在这两处仍有明示范围缺口。
- Part 4 §7.5.1/§7.5.2：标准列出一项 integration and test strategy 与一项 integration and test report 工作产物；本书将其建成逻辑聚合对象，不限制项目采用多个物理分卷。
- Part 4 §8.4.1–§8.4.4：安全确认需要代表性整车语境、受控规范、逐 SG 用例、适当的方法组合、四类评估面及对结果的独立评价；当前仅实现合成计划态对象和结构合同。
- Part 4 §8.4.3.1：用于安全确认的测试可以沿用 Part 8 §9.4.2/§9.4.3 的验证测试要求；这允许复用测试纪律，不允许复用 Clause 7 的活动身份或通过结论。
- Part 4 §8.5.1/§8.5.2：安全确认规范与安全确认报告是独立工作产物；当前报告仍为挂接 `NotRun`/`NotPerformed` 的 Draft 模板。

## 本体化实践：追溯是关系，不是文档

Part 8 明确要求安全需求向上游、实现与验证对象保持追溯。本章用可查询关系实现其中一部分：

1. **`derivedFrom` 显化上游**：FSR→SG、TSR→FSR、HSI 接口需求→TSR，`derivedFrom+` 可查询需求链。
2. **工作产物与内容分层**：TSR 规格记录 TSR，TSC 连接 TSR 规格和系统架构规格，HSI 规格记录 HSI 接口需求。
3. **架构实现与需求分配对齐**：`implementsRequirement` 建立整体实现边，`allocatedTo` 建立元素承载边；门禁要求两者在同一 TSC 中对齐（CQ-CH05-03）。
4. **活动执行与报告成熟度分开**：`AnalysisPlanned` 与 `Draft` 是两个维度；有活动和报告 URI 不等于已执行或已批准。
5. **HSI 双端点结构门禁**：通用门禁拒绝悬空需求；HSI 专属门禁还要求不同的硬件端点和软件端点，但不据此声称接口内容或验证证据完整。
6. **ASIL 继承一致**：SHACL 用 `asilRank` 比较派生需求与上游 ASIL；没有已登记且匹配的 ASIL 分解关系时，降级会被报告（GATE-CH05-02 + 反例 fixture）。分解方案与独立性是否工程充分仍须另行审查。
7. **集成计划覆盖与完成事实分离**：`verifies` 在 `VerificationPlanned` 活动上表示范围；九条边的画像包含 FSR、TSR 与 HSI 需求，但当前覆盖门禁的局部闭包分母严格只有 FSC 明示的 FSR 和 TSC/TSR 规格明示的 TSR。HSI 另有双端点与归属等结构门，不能据此声称 HSI 集成范围完备；机械后备和车载通信/供电接口也尚未进入真实范围。执行状态与报告成熟度共同满足必要治理条件后，仍不自动证明范围完整或测试充分。
8. **逻辑工作产物与物理文件分离**：三个子阶段共享一个逻辑报告节点，用来统一查询和治理；这不等于 ISO 禁止分阶段报告或附件。
9. **状态、范围与规则身份均显式封闭**：`sh:in` 防止 RDFS `range` 把其他 validation 状态或自定义阶段推成合法值；每项活动只引用一份策略，HSI/FSR/TSR 必须由该策略解析；ISO 直接要求、`ISORequirementOperationalization`、`BookHousePolicy` 与 `ProjectSpecificStrategy` 分层记录。
10. **Item 身份不靠类型猜测**：五类 Clause 7 输入和逻辑报告显式 `appliesToItem EPS_Item`，`GATE-CH05-04` 与 SHACL 共同拒绝跨项目拼接；这仍不替代版本/基线兼容性审查。
11. **Clause 8 不与 Clause 7 共用结论身份**：两类活动和报告可以引用同一候选底层数据，但必须保持各自的目的、规范、评价和状态；`CQ-CH05-15` 精确返回这条身份边界。
12. **计划骨架不冒充执行证据**：`CQ-CH05-08–14` 把环境、规范、逐 SG 用例、四评估面、方法理由、结果与评价状态展开；当前答案仍明确包含合成/TBD、`NotRun`、`NotPerformed` 和 `Draft`。
13. **方法理由进入精确 oracle**：`CQ-CH05-12` 不只列出四个已选方法，还精确返回选择理由与“未证明充分性，也未执行”的 `exampleStatus`；这能防理由被静默替换，仍不能代替专家判断方法组合是否适当。

一个可复现的建模反例：若安全需求误用安全目标专属的 `goalStatement` 属性，由于该属性声明了 `rdfs:domain SafetyGoal`，RDFS 推理会把 FSR 归类为安全目标，并可能触发面向安全目标的 ASIL 一致性门禁。当前模型为安全需求使用专属的 `requirementStatement`。这个反例说明**属性的 domain 会参与类型推断**——建模时属性归属要与类层次严格对齐。

## 落地建议 / 快速开始

```bash
.venv/bin/python eval/run_eval.py    # 追溯门禁随主门禁一起跑
```

- **系统工程师**：先读 `examples/tsc-architecture-analysis.md`，再对照 `abox-eps-system.ttl` 检查 TSR、架构实现与分配元素是否在同一 TSC 中闭合。
- **安全分析负责人**：把 `AnalysisPlanned` 视为未执行待办，不要将 Draft 报告模板当成分析结论。
- **接口责任人**：检查 HSI 草稿是否把工作产物、接口需求和硬件/软件责任端分开记录；不要把门禁通过写成 HSI 已验证。
- **集成测试负责人**：用 `CQ-CH05-04/05` 审查三子阶段、计划范围和共享报告状态，再用 `GATE-CH05-03/04` 查 FSR/TSR 覆盖缺口与 Item 身份冲突；任何绿灯都不能替代测试记录。
- **安全确认负责人**：用 `CQ-CH05-08–15` 审查 Clause 8 的计划边界、方法理由和状态一致性；当前查询结果只支持“计划骨架可审计”，不支持“安全目标已达成”。
- **想学本体化的读者**：改一条 TSR 的 ASIL 为 A（低于上游 D），重跑门禁看 `TraceASILInheritanceShape` 如何拦下。

## 尚未完成的系统层范围

当前已形成结构性重写后的候选正文，知识门禁覆盖追溯、TSC/架构/安全分析计划、HSI 最小结构、Clause 7 已声明需求的三子阶段计划范围，以及 Clause 8 的合成环境、规范、逐 SG 用例、方法选择、四个实体化评估面、结果/评价状态和报告集合闭包。仍未完成的是：完整 TSR/TSC 与系统架构规格，安全分析与 §6.4.9 验证的实际执行，Clause 7 中机械后备及车载通信/供电网络的范围对象和全部执行证据，完整 HSI 项目参数，项目级方法选择与偏离理由，以及 Clause 8 的真实代表性车辆、受控项目参数、测试执行、逐面证据、方法充分性专家裁决和报告批准。Part 4 的来源哈希、技术复核、claim/rights、成书视觉复核和用户明确验收也仍开放。结构门禁全绿、CQ 精确命中和富正文存在都不等于系统层工程完成、安全确认通过或出版放行。
