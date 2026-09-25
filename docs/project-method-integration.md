# 工程项目方法的融合与历史记录接续

`build-engineering-project` 的通用方法已进入根工程决定内环、共用工程证据方法和既有
组织记录。没有新增同名子 skill，没有复制其 `projectctl.py`、OWL/SHACL 或领域 profile。
旧入口撤出发现目录后，直接使用 `ontology-engineering` 续接任务。

本轮是方法与输入记录的融合：沿用现有身份、主张、范围、功能、依赖与证据检查，未新增
Semantica package 或宣称 27 个旧 shape 已自动等价迁移。书源、runtime lock 和项目采用
绑定不变；本地修改不自动成为公开分发版本。

## 从哪里继续工作

| 需要的能力 | 当前落点 |
| --- | --- |
| 想法、需求、候选、下一次验证与持续反馈 | [根入口](../SKILL.md)及[共用方法](../skills/engineering-evidence-methods/references/method-internalization.md) |
| 对象与条件一致、因果对照、模型与验证数据 | [工程证据模块](../skills/engineering-evidence-methods/SKILL.md)及锁定的 Semantica 输入合同 |
| 需求来源、范围和未决问题 | 既有项目记录；无记录时选用 `discovery-record` 的决定/缺口字段 |
| 候选在某个决定与范围中的评价 | 既有设计/路线及评价记录；可用 `solution-record.candidate_evaluations` 引用同版对象与证据 |
| 验证活动、人员资源、依赖、费用和接续 | [组织工作项](../skills/manufacturing-process-cost/references/organization-loop.md)，按需填写 `engineering_activity`、资源和计划依据 |
| 预算与提案、图表或对外文件 | 原成本记录与 `solution-record.document_views`；快照、过滤与产物 manifest 各有依据 |
| 独立记录/发布包审计 | 已安装时使用 `audit-engineering-project`；其报告不修改状态、不批准发布、不代替 Semantica |

这些 JSON 是可裁剪的记录模板，不是新的强制 schema。已有台账直接映射职责，不再复制一套
事实正本；字段未填保持未知。跨行业时复用关系，不套用制造业的参数、工艺或固定格式。

## 历史 case.json 的接续

1. 找到原 `case.json`、项目说明、来源、`.engineering` 事件和已发布快照。先保存精确字节
   与 SHA-256；记录原始位置和 schema，不覆盖旧发布物，不批量重编号。
2. 从 `project`、需求/CQ/约束、候选/主张/证据中识别当前决定、对象版本、条件、来源与
   未决项。原有 `preferred/frozen/RELEASED` 等值作为历史声明保留；需按原决定范围与证据
   核实，不能直接赋给新的整体状态。
3. 实验、仿真、过程、资源和 gate 仍是原来源记录。计划与执行、校准与验证、声明与实测
   分别读取；同名概念不能只改字段名就声称语义等价。原 profile 的材料、设备、阈值和
   等级保持项目实例，不能成为通用默认值。
4. 继续使用项目原记录；如需整理为既有模板，建立显式 `旧文件摘要 + JSON Pointer →
   原实体 ID/版本 → 目标字段` 对照，保留未映射字段、歧义和排除理由。映射是来源整理，
   不会自动接受事实、补上权限或制造缺失证据。RDF 只通过现有来源投影进入正式 review。
5. 将新增记录与修改理由追加到该项目的原事件/修订体系，另存新快照；把未完成事项接入
   原工作项。工程支持、活动结果、文档发布和语义生命周期分别报告，不再执行旧的
   `cycle/automate/release` 全局阶段推进。
6. 原 schema `build-engineering-project/1.0` 仍可直接交独立 auditor 读取，无需先转换。
   其兼容检查反映旧合同（含原来的阶段/领域限制），不成为新项目的通用语义裁决。
   已改用其他记录结构时不能再套用旧 schema 自称审计通过。

需要字节审计时先发现已安装的 auditor 位置，再运行：

```bash
python3 <audit-skill-dir>/scripts/audit_engineering_project.py /project/case.json \
  --json-out /project/reviews/audit-current.json \
  --markdown-out /project/reviews/audit-current.md
```

复用该项目确实采用的 policy；不要把样例预算、期限或页数带入新项目。输出选择新路径，
不能覆盖被审计资料。工具未安装时明确该项未运行，仍可继续其他已授权工作。

## 旧能力问题逐项处置

“现有可用检查”仅指相应输入范围，不承诺整条业务问题已自动求解。来源、原始活动与
未编码的关系仍须工程审阅；只有实际的项目 native review 才能填语义执行结果。

| 旧 CQ | 融合后的回答途径与边界 |
| --- | --- |
| 1 谁的哪次反馈创建/改变需求 | 原话、来源和修订 → 同版要求；Jev 候选与断言采用分开 |
| 2 正在评价哪个候选版本 | 范围化评价引用设计/路线 ID 与版本；适用时接 `pattern-identity` |
| 3 哪些假设失效，哪些仍开放 | 保留挑战的目标和范围；`pattern-change` 处理声明的变化，依赖缺口继续开放 |
| 4 什么证据支持/反驳/限制主张 | 原来源与条件 → 主张支持关系；`pattern-scope` 及适用的共用证据检查 |
| 5 已列资源、能力与使用权限有何区别 | 资源条目、能力证据、预约与授权分别关联工作项；没有通用设备资格自动判定 |
| 6 预测是否在比较前冻结 | 对实际版本/事件/数据做 `validation-lineage`；不从文件名推断顺序 |
| 7 结果能否追到原始数据和分析版本 | 来源摘要、原始/派生数据与分析记录；重现须实际运行，哈希相同不证明计算正确 |
| 8 当前决定还缺哪些必需证据 | 列出适用的功能/证据义务及完整范围；`pattern-realization`、`review-coverage` 不自行补齐义务清单 |
| 9 谁批准什么范围、版本与动作 | 原授权/决定记录与 exact binding/task；语义运行授权不替代物理操作授权 |
| 10 发布视图来自哪个快照 | 输入与产物 manifest、生成版本及受众过滤；发布计划文件不放行产品 |
| 11 变更后哪些对象要重审 | 对照新旧支持依赖与工作项；现有 `impact` 提示重审，未查清关系不能报无影响 |
| 12 示意图是否冒充实验事实 | 核对生成活动、来源及目标主张；区分说明用途与证据角色，不按文件用途整体互斥 |

## 27 个旧 shape 的义务归属

以下保留检查意图并说明去向，不移植旧约束或声明逐个等价。旧的本地 RDF/SHACL 执行器
随入口退役；新增正式检查只能在 Semantica 中提出完整 delta、回归并按范围采用。

| 原 shape（省略共同后缀 Shape） | 处置 |
| --- | --- |
| EngineeringProject、CompetencyQuestion | 意图、需求与 CQ 来源进入现有记录；去掉项目必须已有候选等过早完备要求 |
| DialogueChangeEvent | 来源事件与解释/采用分层；无需每条消息创建本体类或变更 |
| Candidate、CandidateState、CandidateEvaluation | 保留对象身份与版本，评价绑定决定/范围/判据；候选状态不作对象本质类型 |
| Claim、Evidence、EvidenceBoundary | 对齐现有主张/证据/范围合同，逐字段核对语义与来源，不做同名即等价映射 |
| EvidenceScheme | 保留原分级体系和 namespace；同号的可靠度、成熟度和几何等级不能合并 |
| CommunicationArtifact | 退役整体禁止证据角色的约束；按生成活动与有界支持关系审阅 |
| ProcessRun、ManufacturingRun、TestRun、SimulationRun | 保留过程输入输出、对象/批次/设备/模型/版本；接现有专业工具事实，单独审查未编码的执行义务 |
| CalibrationRecord、RawDataArtifact、DerivedDataArtifact、MeasurementRecord | 保留校准有效条件、原始字节、派生血缘、单位与删失；既有数量/条件/验证方法按适用输入检查，未测或低于检出限不填零 |
| Gate、GateCriterion、DecisionOutcome、GateDecision | 决定绑定版本化判据、必需义务、结果和理由；用有依据的依赖取代固定阶段次序 |
| HumanAuthorization | 继续查授权来源、主体、动作、版本和范围；历史字符串不构成新的授权 |
| VersionRecord、ReleaseManifest、ReleaseView | 保留独立版本轴、原始快照和哈希视图；旧“发布即整体 RELEASED”的副作用退役 |

## 撤下旧入口的验收

- 根入口独立处理未成形任务，产生有条件的候选比较、取证活动、资源/预算/计划和下一触发。
- 续接新反馈，能定位对象版本和支持边；依赖范围未知时不宣称全部影响已查清。
- 单故障反例：记录数量相同但活动 ID 不匹配、资源重复占用、前驱/后继倒置、有向环、
  校准数据充作验证、旧版本证据用于新配置，都不能被误报完成或适用。
- 保留模糊结果与无效试验，分清候选反证与数据无效；一份提案发布不触发产品放行。
- 历史 case 可直接做独立审计，原文件与发布快照不变；旧入口不再在活动 skill 目录内。
- 运行根检索、held-out、strict backend policy 和测试，并对真实使用方式做无答案泄漏的
  独立 forward test。记录既有失败与本次引入的失败，不能用方法说明冒充测试回执。

方法成熟度以独立试用结果为准，真实项目收益仍需可比实绩。本轮语义学习为 `no_delta`：
改动连接工作方法与记录，未提交、晋升或发布新语义包；这不表示所有旧检查已经自动覆盖。
