# Semantic Engagement Contract

本合同规定：任何工程 skill、Agent 或项目在调用 `ontology-engineering` 时，怎样让
Semantica 默认介入当前任务，并怎样把经过验证的实践经验炼化为可复用行业本体。
两卷书提供方法与 ISO 本体化推演；它们不是项目事实源，也不是第二套可执行语义。

## 目录

1. [固定职责](#1-固定职责)
2. [每次调用的快速内环](#2-每次调用的快速内环)
3. [跨-skill-输入合同](#3-跨-skill-输入合同)
4. [固定三联输出](#4-固定三联输出)
5. [学习判定与完整-package-delta](#5-学习判定与完整-package-delta)
6. [治理与晋升外环](#6-治理与晋升外环)
7. [自动化与授权矩阵](#7-自动化与授权矩阵)
8. [状态与失败语义](#8-状态与失败语义)
9. [两卷书的使用与更新边界](#9-两卷书的使用与更新边界)
10. [跨-skill-集成约定](#10-跨-skill-集成约定)

## 1. 固定职责

始终区分以下职责：

| 参与者 | 唯一职责 | 不得冒充 |
|---|---|---|
| 两卷书 | 指导怎样观察、提问、建模、解释和审阅 | 项目事实、执行结果或发布授权 |
| `ontology-engineering` Skill | 语义接入、路由、学习编排和结果解释 | 第二套本体实现或后端 |
| Semantica | 唯一可执行语义、package、验证、版本、PROV、receipt 和行业本体记忆 | 事实接受、风险接受或人的决定 |
| 工程工具与受控记录 | 产生项目事实和原生证据 | 通用行业规律 |
| 有权人 | 冲突、删除、风险、合规、晋升和发布决定 | 可复现语义检查 |

“Semantica 默认介入”不等于“每次任务都修改本体”。每次调用必须至少完成一次
锁感知发现和学习判定；只有经过证据、回归和授权的候选才能晋升。

## 2. 每次调用的快速内环

按以下顺序处理每个任务：

```text
接收工程任务
  → 读取项目绑定与 Semantica source identity
  → 从两卷书选择方法镜头和来源锚点
  → 发现现有 package / baseline / capability
  → 形成对象、身份、CQ、证据与权限边界
  → 运行已有 query / shape / rule / oracle（适用时）
  → 执行或审查获授权的工程工作
  → 绑定工程证据、Semantica receipt 与 release 状态
  → 返回工程结果 + 语义结果 + 学习判定
```

问答、解释、审查和诊断默认只读。`build`、`change` 或显式内化任务可以形成候选，
但普通任务不得因为调用了本 skill 就静默改变正式行业本体。

## 3. 跨 Skill 输入合同

调用方应提供一个 `SemanticTaskEnvelope`。用户不必预先知道 package ID；先运行只读
`semantic_engagement.py discover`（不带 `--binding`），从
`corpus_found.packages[]` 取得 package ID/version/digest/required capability，或从
`native_workspace_bootstrap` 取得新 registry 的 version `0` 和 exact empty-package digest。
该入口不接受 backend、任意 package path 或 fallback。选定坐标后才生成 binding。

```json
{
  "$schema": "ontology-engineering.semantic-task-envelope/v1",
  "task_id": "stable-project-task-id",
  "task_kind": "organization-defined-engineering-task-kind",
  "intent": "当前要解决的工程问题",
  "project": "logical-project-id",
  "domain": "logical-industry-domain",
  "requested_decision": "希望支持但不自动代替人作出的决定",
  "actor_id": "engineering-agent-or-operator",
  "requested_actions": ["open", "run", "propose"],
  "required_capabilities": ["declared-semantica-capability"],
  "evidence": [{
    "source_id": "record-001",
    "uri": "evidence:example/record-001",
    "sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
    "media_type": "application/json",
    "captured_at": "2026-08-19T12:00:00Z"
  }],
  "created_at": "2026-08-19T12:01:00Z"
}
```

项目通过 `ProjectOntologyBinding` 说明自己与行业本体的关系。绑定只保存逻辑身份、
哈希和权限，不复制 Semantica 资产，也不把任意路径当 package ID。

```json
{
  "$schema": "ontology-engineering.semantic-project-binding/v1",
  "binding_id": "stable-project-binding-id",
  "project": {
    "project_id": "stable-project-id",
    "domain": "stable-domain-id"
  },
  "semantic_target": {
    "kind": "workspace",
    "workspace_id": "semantica-workspace-example",
    "package_id": "semantica.industry.example"
  },
  "baseline": {
    "version": "v0004",
    "digest": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
  },
  "evidence": {"logical_root": "evidence:example"},
  "authority": {
    "fact": {
      "authority_id": "controlled-engineering-records",
      "scope": ["measurement", "test-result"]
    },
    "decision": {
      "authority_id": "authorized-review-board",
      "scope": ["conflict", "risk", "promotion"]
    }
  },
  "allowed_actions": ["open", "run", "propose"],
  "lifecycle_actions": ["candidate", "proposed"],
  "promotion": {
    "target": "industry-registry",
    "requires_decision_authority": true
  },
  "created_at": "2026-08-19T12:00:00Z",
  "semantic_api": "semantica.ontology.refinery/v1"
}
```

`semantic_target.kind=package` 时改用精确 `package_version`，且必须与 baseline version
一致；`kind=workspace` 时使用逻辑 `workspace_id`。证据引用必须位于绑定的逻辑
evidence root 之下，并带 exact SHA-256、媒体类型和带时区采集时间。`allowed_actions`
控制 OE 命令；`lifecycle_actions` 是六态模型的有序前缀，控制 Semantica 中允许推进到的
最远状态。严格解析器拒绝未知字段、未授权动作、文件路径式 evidence URI、
backend/fallback 选择字段、无哈希证据以及 task/project/domain 不一致。

`actor_id` 是发起本任务的工程 Agent/操作者身份，不等于 fact authority。只有
`PackageDelta.created_by` 才必须命中 binding 中的 fact authority；commit/promote 的
actor/authority 则必须命中 decision authority。`promotion.target` 是 registry channel，
本合同固定为 `industry-registry`，不是 package ID；package 身份只来自
`PackageDelta.package_id`。

`task_kind` 是非空的开放字符串，由行业、组织或上游 skill 定义；行业任务类型
无法用 OE 的固定枚举穷尽。它只用于意图解释与 provenance，不产生权限。
可执行动作仍由 `requested_actions`、binding 的 `allowed_actions/lifecycle_actions`、
内容绑定的 authorization 与 gate evidence 共同约束。

OE 输入名与 native DTO 有意分层，适配器执行以下确定性投影；调用方不得直接把一份 JSON
冒充两个合同，也不得把早先 task 当作未来迁移的授权：

| 当前 OE 调用 | 本次 Native task/context 的 exact action |
|---|---|
| `open` | `engagement`；记录本次 task 与 binding，不是生命周期迁移 |
| `propose` | `candidate`、`proposed`；两个 envelope 均由**当前** OE task 投影，且各自只含一个 action |
| `commit` | `committed` |
| workspace `verify` | `execute_candidate`、`derive_regression_gate`、`regression_passed`、`derive_release_gate`、`release_complete` |
| `promote` | `promoted` |
| `discover`、workspace `run`、`history` | 只读；不制造 transition context |

固定字段同时投影如下：`semantic_api → semantic_api_contract`；
`lifecycle_actions → allowed_actions`（六态有序前缀）；authority
`{authority_id, scope}` 先规范排序，再投影为内容绑定的 native authority token。

OE 先用原始 task 的 `requested_actions` 检查本次 CLI 命令；随后为本次发生的每一个 native
动作生成一份 `requested_actions=[exact-action]` 的 `SemanticTaskEnvelope`。除 `engagement`
外，每份 envelope 都与当前 binding、exact delta SHA 一起进入
`TransitionContextDTO`。Semantica 校验并在 CAS、事件、subject execution suite、gate
evidence 或 promotion descriptor 中保留 context SHA 与 context-object SHA。因而：

- propose 时留下的 candidate/proposed task 只能证明当时的候选与提议意图；
- commit、五个 verify 子动作和 promote 必须各自由其**当前调用**的 OE task 重新投影；
- 一个较早 context 即使 actor、证据或 action 相似，也不能重放成后续迁移权限；
- commit/promote 仍额外要求内容绑定的 decision authorization；verify 的 gate evidence
  只能由 Semantica 对已 commit subject 执行后内部推导，调用方不能提交 pass/fail JSON。

Semantica runtime commit、版本和 wheel SHA-256 必须由 OE source lock 自动注入；不得让
用户手抄，也不得接受环境里碰巧安装的版本。正式调用还要以已校验 wheel 内的
authoritative `RECORD` 逐文件核对实际 import root、package path、文件 SHA/size、symlink
和未记录文件；仅版本号或安装元数据相同不构成同一 runtime。

## 4. 固定三联输出

每次调用都必须分别给出：

1. **工程结果**：当前任务完成了什么、使用了哪些项目事实、仍缺什么。
2. **Semantica 结果**：package/version/scenario、execution、CQ/oracle、regression、
   receipt、PROV、release 及各自 blocker。
3. **本体学习结果**：`no_delta` 及理由，或 candidate ID、适用范围、证据和下一步。

不得用进程退出码、单个 oracle 通过或一次 CQ 通过代替这三类状态。

## 5. 学习判定与完整 Package Delta

只有以下情形才形成候选：

- 现有 package 无法表达一个已复现的重要对象、身份或关系；
- 已编码 CQ、shape、query、rule 或 oracle 对真实失败没有辨识力；
- 同一工程模式在多个受控 attempt 中重复出现；
- 新反例、歧义或旧版本回归暴露了适用范围；
- 既有解释、约束或能力边界被可靠证据证明不完整。

无新知识时返回 `learning.status=passed`、`learning.verdict=no_delta` 和具体原因，不提交
空版本。Native receipt 内部对应 `learning.status=no_delta`；两者由适配器明确分层。

`PackageDelta` 必须覆盖完整 package，而不只是类和属性词表：

```text
ontology       TBox、必要的受控 ABox、身份与版本语义
competency     新增、改写或受影响的 CQ 与 exact acceptance
shapes         闭合交付约束和规范化 violation oracle
queries        named SELECT/ASK/CONSTRUCT/UPDATE 与精确结果类型
rules          Semantica 已声明支持的有界规则及解释
cases          positive / single-fault negative / ambiguity / prior-release
contract       capability、输入、输出、权限和失败合同
provenance     attempt、证据逻辑 ID、哈希、来源与权利状态
book_impact    none / vol1-method / vol2-iso-exemplar / both
```

项目实例事实通常作为 evidence 或运行输入保留，不自动提升为通用 TBox/ABox。

## 6. 治理与晋升外环

对有复用价值的候选执行：

```text
candidate
  → proposed（写入不可变 candidate/proposed ledger；不改变 promoted registry truth）
  → committed（不可变候选版本；不表示可发布）
  → regression_passed（旧 CQ + 新 CQ + 四类案例）
  → release_complete（source/package/input/output/PROV/rights 技术条件闭合）
  → promoted（进入受控 Semantica 行业 package registry）
  → published（仅由外部有权人决定）
```

晋升必须满足：

- 证据可追、可复现且适用范围明确；
- 冲突和删除已有带理由的有权判决；
- 完整 package delta 的资产均有内容哈希；
- 旧 CQ、新 CQ 和四类案例均按合同回归；
- runtime/source/wheel/package/input/output 与 receipt/PROV 绑定；
- 权利、隐私和发布目标明确；
- 目标 ID/version 不覆盖既有不可变版本。

Candidate/propose 会检查 delta 与 binding 的 baseline 坐标，并在 managed workspace 留下
不可变事件；registry current baseline 在 commit 前重新检查，在 promotion 前再次检查，
因此并发期间变 stale 的候选不会进入正式 registry。

Workspace `verify` 不接收 regression/release evidence 文件。Semantica 必须先执行 exact
committed subject，生成内容绑定的 subject execution suite；随后推导 regression evidence
并先写入 `regression_passed`；只有此后才能从同一 suite 和已记录 regression closure
推导 release evidence，再写入 `release_complete`。
执行者 runtime 的结果身份与 subject package/version/SHA 身份分别报告，二者不得混写。
若在任一 checkpoint 后恢复或重放，Semantica 必须从 immutable event/CAS 读取原 context，
并要求调用上下文与原记录完全相等；适配层只报告真正参与写入或经严格重放核验的 context。

Promotion 不原地改写旧 binding。它只返回 `auto_applied=false` 的 successor binding
投影，绑定新 package version/SHA/manifest、predecessor binding SHA 与 promotion-record
SHA；控制面批准并另存该文件后，新 binding 才能用于 `discover/run`。旧 binding 随即因
baseline stale 而 fail closed，但仍可在同一 registry 上读取该 candidate 的 immutable
history。

## 7. 自动化与授权矩阵

| 动作 | 默认 | 条件 |
|---|---|---|
| doctor、source-lock、package/baseline 发现 | 自动 | 只读、fail closed |
| 运行已有 CQ/query/shape/rule/oracle | 自动 | 输入与能力合同满足 |
| 识别 `no_delta` 或生成候选提案 | 自动 | 候选不进入正式 registry |
| 提交纯新增、无冲突候选版本 | 受控 | 用户已授权 `build/change/internalize` 工作区 |
| 同名异义、替换、删除 | 不自动 | 必须有权人逐项给出理由 |
| 接受事实、风险或合规结论 | 不自动 | 由声明的事实/决定权威负责 |
| promotion、书稿修改、push、公开发布 | 不自动 | 各自明确授权和门禁 |

## 8. 状态与失败语义

分别报告实际 JSON section，不能折叠成一个绿色，也不另造平行 alias：

```text
runtime_source.installed_version_matches / installed_wheel_matches
binding + task（严格解析后的对象；失败进入 execution.error_type）
corpus_found.status + packages/selected/native_workspace_bootstrap
execution.status + oracle_checks
regression.status + receipt.status + release.status
learning.status + learning.verdict
learning.promotion.status + learning.publication.status
```

`learning.status` 描述本次学习阶段是否 `passed|blocked|not_run`；`learning.verdict` 才是
`no_delta|candidate`。Publication 使用
`status=not_requested|externally_authorized|published|blocked`；当仍需外部决定时另带
`decision=external_decision_required`，refinery 永不把它改成 published。

`missing`、`unknown`、`unsupported`、`partial`、`placeholder`、`absent`、hash mismatch、
冲突未判决或 release blocked 均须阻断相应阶段。禁止 fallback、静默换后端、空结果
冒充成功，或把 `committed` 解释成 `release_complete`。

## 9. 两卷书的使用与更新边界

第一卷提供通用方法镜头：对象、身份、关系、CQ、OWA/CWA、约束、推理、来源、
PROV 与 Agent 控制。第二卷提供 ISO 本体化推演及十个跨行业观察镜头：主张、身份、
治理、情境危害、需求、测量、变化、依赖、现场和保证。

书不是行业实践日志。只有以下变化才进入书稿：

- 改变普通工程师需要掌握的通用方法或权限边界；
- 揭示稳定、可复用且适合教学的失败模式；
- 修正第二卷 ISO 本体化推演；
- Semantica 的真实能力边界或可演示行为发生实质变化。

更新书稿时读取对应 handbook README，并完成正文、生成 fragment、作者源码锁、
Semantica book binding、wheel/source lock、TeX/PDF 和发布门禁的同一候选收敛。

## 10. 跨 Skill 集成约定

其他 skill 不复制两卷书或 Semantica package。它们只在三个检查点调用本 skill：

1. **任务开始**：提交 task envelope，完成对象/身份/证据/权限和 baseline 发现。
2. **不可逆动作之前**：运行适用的约束、CQ 或 release preflight。
3. **任务结束**：提交原生结果和证据，得到 receipt 与学习判定。

领域 skill 仍负责自己的 CAD、EDA、仿真、质检或现场工具；本 skill 不因语义通过而
获得额外执行权限。若没有绑定或 Semantica 能力不足，返回可解释的 blocker 和候选
建模任务，不绕过 Semantica 建立第二实现。
