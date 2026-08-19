# Semantica Industry Ontology Refinery Contract

本合同定义根 `ontology-engineering` 快速语义内环与 `domain-ontology-loop` 慢速治理外环
之间的唯一交接方式。实现合同是 `semantica.ontology.refinery/v1`；本目录不维护第二套
本体、版本器、CAS 或 registry。

## 目录

1. [内环与外环](#1-内环与外环)
2. [可移植根与 source identity](#2-可移植根与-source-identity)
3. [Semantica 托管工作区](#3-semantica-托管工作区)
4. [输入 DTO](#4-输入-dto)
5. [完整 PackageDelta](#5-完整-packagedelta)
6. [状态与门禁](#6-状态与门禁)
7. [Python API](#7-python-api)
8. [CLI](#8-cli)
9. [退出码与分项报告](#9-退出码与分项报告)
10. [授权与事实边界](#10-授权与事实边界)

## 1. 内环与外环

根 skill 对每次任务执行快速内环：

```text
task + project binding + source lock
  → package/baseline/capability discovery
  → 现有 query/shape/rule/oracle
  → 工程证据 + engagement receipt
  → 根响应 learning.status = passed|blocked|not_run
  → 根响应 learning.verdict = no_delta|candidate
```

`no_delta` 结束本轮，不产生版本。只有 exact `candidate` 进入慢速外环：

```text
candidate → proposed → committed → regression_passed
          → release_complete → promoted
```

外环负责复用性、冲突、回归、release 和 promotion，不重新执行工程工具，也不把书中
例子当项目事实。一个任务成功不必产生 candidate；一个 candidate 存在也不意味着应当
commit 或 promote。

## 2. 可移植根与 source identity

Agent 必须从 `skills/domain-ontology-loop/SKILL.md` 的真实位置按
`Path(skill_file).resolve().parents[2]` 解析 `OE_SKILL_ROOT`，再使用：

```bash
"$OE_SKILL_ROOT/runtime/.venv/bin/python"
```

不要依赖调用时 cwd、用户主目录、某个 Semantica 源码 checkout 或绝对仓库路径。
refinery workspace 是用户明确授权的目标路径；package ID、workspace ID 和 `evidence:`
URI 是逻辑/opaque identity，不能被解释为文件路径。Promotion target 是固定 registry
channel `industry-registry`，不是 package ID。

根 OE source lock 必须自动注入：

```text
runtime_commit
runtime_artifact_sha256
runtime_version
```

这些字段进入 `SemanticEngagementReceipt.runtime_source`，并继续绑定 subject execution
suite、regression/release gate evidence 与 promotion record。调用方不得手抄 source
identity；本外环只消费根 semantic engagement 生成并校验过的 envelope、binding、
receipt，以及 Semantica 从 committed subject 内部执行后推导的 gate evidence。source
lock 缺失、wheel provenance 不可得、实际 import/安装文件与 locked wheel `RECORD`
不一致，或身份不完整时，当前阶段 fail closed。

## 3. Semantica 托管工作区

只能通过 `IndustryOntologyRegistry.create()` 或 CLI `init` 创建。当前托管布局为：

```text
<workspace>/
  workspace.json
  registry.json
  objects/sha256/<prefix>/<sha256>
  engagements/<engagement-record-sha256>.json
  refinements/<delta-sha256>/
    delta.json
    envelope.json
    binding.json
    engagement.json
    current.json
    events/<sequence>-<event-sha256>.json
  packages/<sha256(package-id)>/versions/<sha256(version)>/record.json
  registry-events/<sequence>-<event-sha256>.json
```

语义资产、manifest、授权和 gate evidence 进入 immutable CAS。candidate 与 registry 各自
使用连续 hash-chained events；`current.json` 和 `registry.json` 只是受校验的原子指针，
不是可手改正本。package/version 目录只使用身份哈希，外部 package ID 永不拼成路径。

禁止：

- 手改、删除或重建 objects、events、refinements、package records、current/registry 指针；
- 用一个“当前 ontology 文件”覆盖历史；
- 让 `init` 覆盖已存在 workspace；
- 用未知 package ID、移动 branch、文件路径或未校验安装版本代替 registry/source lock。

CAS hash mismatch、事件断链、stale baseline、重复不可变 version、未知 ID、symlink 或
workspace lock 冲突均须阻断。

## 4. 输入 DTO

### SemanticTaskEnvelope

由根快速内环建立并绑定：task ID/kind、project/domain、intent、requested decision、
actor、requested actions、required capabilities、至少一个带逻辑 URI 与 SHA-256 的
source evidence，以及创建时间。任务证据 URI 只作身份记录；refinery 不据此任意打开路径。

### ProjectOntologyBinding

绑定 project/domain、opaque `package_id`、`workspace_id`、baseline version/package SHA、
evidence root、fact authorities、decision authorities、allowed lifecycle actions、exact
`semantic_api_contract` 和 promotion target。初始 baseline 使用 version `0` 与根 unbound
`discover` 返回的 `native_workspace_bootstrap.baseline_digest`；不得手抄或读取 workspace
内部文件。其余 baseline 必须精确命中 registry current version 与 digest。

Native `SemanticTaskEnvelope.actor_id` 是任务 Agent/操作者，不要求等于 fact authority。
只有 delta `created_by` 才必须命中 fact authority；commit/promote authorization 的
actor/authority 必须命中 decision authority。OE 的 command verbs、`semantic_api` 与
`lifecycle_actions` 由根适配器确定性投影成 native state verbs、
`semantic_api_contract` 与 `allowed_actions`，详见根 engagement contract。

### TransitionContextDTO

每个 native 写动作必须使用当前 OE 调用重新投影的 exact 单 action envelope，并与当前
binding、exact `delta_sha256` 一起生成 `TransitionContextDTO`。合法 action 固定为：

```text
candidate · proposed · committed · execute_candidate
derive_regression_gate · regression_passed
derive_release_gate · release_complete · promoted
```

Candidate context 由 `register_candidate` 从 exact `candidate` envelope 建立；其余 context
显式传给对应原生 API。Semantica 保留 `context_sha256` 与 context object SHA，并把它们
继续绑定事件、subject execution suite、gate evidence、provenance closure 和 promotion
descriptor。上下文必须来自本次调用；过去的 envelope/context 不能充当未来迁移授权。

### SemanticEngagementReceipt

绑定 envelope SHA、binding SHA、自动注入的 runtime source identity，并分别保留
execution、regression、receipt、release 和 learning。任一 required capability、证据哈希、
execution receipt 或 runtime identity 缺失，receipt 即为 `blocked`。

`learning.status=no_delta` 不带 delta SHA；`candidate` 必须绑定 exact `delta_sha256`。

## 5. 完整 PackageDelta

`PackageDelta` 的顶层合同为：

```json
{
  "schema_version": "1.0",
  "package_id": "opaque.industry.package",
  "base_version": "v0004",
  "base_package_sha256": "<64 lowercase hex>",
  "target_version": "v0005",
  "rationale": "为什么这项实践可复用，边界是什么",
  "created_by": "bound-fact-authority",
  "created_at": "<ISO-8601 with timezone>",
  "required_capabilities": ["declared-capability"],
  "source_evidence": [{"source_id": "...", "uri": "evidence:project/record", "sha256": "...", "media_type": "...", "captured_at": "..."}],
  "book_impact": "none",
  "ontology": [],
  "competency_questions": [],
  "shapes": [],
  "queries": [],
  "rules": [],
  "cases": [],
  "contract": [],
  "provenance": [],
  "delta_sha256": "<canonical-content SHA-256>"
}
```

八类可执行资产与一个顶层书稿影响字段不可互相替代：

| 部分 | 最低含义 |
|---|---|
| `ontology` | TBox、必要受控 ABox、身份与版本语义 |
| `competency_questions` | 新增/改写/受影响 CQ 与 exact acceptance |
| `shapes` | 闭合交付约束及规范化 violation oracle |
| `queries` | named query/update 与精确结果类型 |
| `rules` | Semantica 已声明支持的有界规则与解释 |
| `cases` | positive、negative、ambiguity、prior_release；negative 尽量采用单故障反例 |
| `contract` | capability、输入、输出、权限、失败和 release 合同 |
| `provenance` | attempt、证据逻辑 ID/哈希、来源、权利和派生关系 |
| `book_impact` | 顶层枚举：`none / vol1-method / vol2-iso-exemplar / both` |

`book_impact` 是独立顶层字段，不能塞入 `provenance`。它参与 `delta_sha256`，并继续写入
materialized manifest 与 promotion record。影响为 `none` 也必须显式填写，并由
`PackageDelta.rationale` 说明原因；非 none 时另行建立带书中锚点的作者候选。是否真正
改书仍需作者授权，并走两卷书作者门禁。

每个数组元素是 content-addressed asset delta：

```json
{
  "category": "ontology|competency_questions|shapes|queries|rules|cases|contract|provenance",
  "asset_id": "opaque-asset-id",
  "operation": "add|replace|remove",
  "media_type": "application/json",
  "sha256": "<exact content sha256>",
  "content_base64": "<add/replace bytes; null for remove>",
  "replaces_sha256": "<required for replace/remove>",
  "role": "optional semantic role",
  "case_kind": "positive|negative|ambiguity|prior_release|null"
}
```

只有 `cases` 可声明 `case_kind`。`add` 不得覆盖同 ID 资产；`replace/remove` 必须绑定
当前 exact SHA。字段存在不等于 coverage 通过：release gate 检查 materialized package
拥有全部八类资产及四类 case；`book_impact` 则由 DTO enum、delta/manifest/promotion
哈希链独立验证。基线已有且未改变的资产可以继续满足 coverage；初始 package 必须在
release 前补齐全部八类资产。

## 6. 状态与门禁

| 状态 | 根命令 / exact native context | 必备门禁 | 权威含义 |
|---|---|---|---|
| `candidate` | `propose` / `candidate` | 当前 task/context、binding 与 registry current baseline、fact authority、learning delta、capability、receipt 完整 | 仅留存候选 |
| `proposed` | `propose` / `proposed` | 当前 task/context、engagement complete、binding 允许 proposed；写不可变事件但不改 promoted truth | 等待治理判决 |
| `committed` | `commit` / `committed` | 当前 task/context、exact commit authorization、baseline 仍 current、冲突/replace/remove 有理由 | 不可变候选 package |
| `regression_passed` | `verify` / `execute_candidate → derive_regression_gate → regression_passed` | exact subject suite、Semantica 内部推导的固定 regression checks、package/receipt/runtime/capability 绑定 | 回归通过 |
| `release_complete` | `verify` / `derive_release_gate → release_complete` | 与 regression 相同 suite、内部推导的固定 release checks、八类 package coverage、四类 case、book impact | 技术 release 条件闭合 |
| `promoted` | `promote` / `promoted` | 当前 task/context、独立 exact promote authorization、baseline 未 stale、version 未占用 | 加入受控 registry |

必须按次序前进。`propose_candidate()` 会连续留下 `candidate` 和 `proposed` 两个事件并
返回 proposed state；它没有跳过 candidate。根 workspace `verify` 先以
`execute_candidate` context 运行 exact committed subject，再执行
`derive_regression_gate → regression_passed → derive_release_gate → release_complete`；release
closure 因而包含已经记录的 regression transition。它不接收调用方 gate JSON。若 release
失败而停在 `regression_passed`，重试只有在重新
导出的 suite/evidence 与已记录 semantic hash 和 CAS object hash 一致时才可继续，否则
阻断。已经写入 `proposed` 或 `release_complete` 的幂等重放也必须从 event/CAS 读取原
context，并要求本次 context 与之完全一致；不得把未参与历史的 context 作为迁移证据返回。

`published` 故意不在状态表中。不得增加、假设或调用 refinery publish 动作。

## 7. Python API

先 feature-detect exact native contract：

```python
from semantica.ontology.refinery import refinery_capabilities

capabilities = refinery_capabilities()
assert capabilities["contract"] == "semantica.ontology.refinery/v1"
assert "book_impact" in capabilities["delta_categories"]
assert "none" in capabilities["book_impacts"]
```

根 OE 适配器内部调用的 exact 原生工作流 API（调用方通常只用下一节统一入口）：

```text
IndustryOntologyRegistry.create(workspace, registry_id=...)
open_engagement(workspace, envelope=..., binding=..., receipt=...)
TransitionContextDTO.create(action=..., delta_sha256=..., envelope=..., binding=...)
propose_candidate(workspace, delta=..., envelope=<candidate>, binding=...,
                  engagement=..., context=<proposed>)
commit_candidate(workspace, delta_sha256=..., authorization=...,
                 context=<committed>)
execute_candidate(workspace, delta_sha256=..., context=<execute_candidate>,
                  runtime_source=...)
derive_gate_evidence(workspace, delta_sha256=...,
                     context=<derive_regression_gate|derive_release_gate>,
                     gate=..., execution_suite_sha256=...)
verify_candidate(workspace, delta_sha256=...,
                 execution_suite_sha256=...,
                 regression_evidence=<Semantica-derived>,
                 regression_context=<regression_passed>,
                 release_derivation_context=<derive_release_gate>,
                 release_context=<release_complete>)
promote_candidate(workspace, delta_sha256=..., authorization=...,
                  context=<promoted>)
history(workspace, delta_sha256=...)
```

需要分步或只读操作时，使用 `IndustryOntologyRegistry` 的 `register_candidate`、`propose`、
`commit`、`record_regression`、`record_release`、`promote`、`status`、`history`、
`list_packages`、`resolve_package` 和 `read_asset`。不要通过私有方法或直接文件 I/O 绕过
门禁。

## 8. CLI

外环统一使用根 `semantic_engagement.py`；它负责 source identity、OE→native DTO 投影、
当前 task context、binding/registry 核对和分项响应。不要从本合同复制 JSON 后绕过根入口
直接写 native workspace。

```bash
OE_PYTHON="$OE_SKILL_ROOT/runtime/.venv/bin/python"
OE_ENTRY="$OE_SKILL_ROOT/scripts/semantic_engagement.py"

"$OE_PYTHON" "$OE_ENTRY" open \
  --binding "$PROJECT_BINDING_JSON" --task "$CURRENT_TASK_JSON" \
  --workspace "$REFINERY_WORKSPACE"

"$OE_PYTHON" "$OE_ENTRY" propose \
  --binding "$PROJECT_BINDING_JSON" --task "$CURRENT_TASK_JSON" \
  --workspace "$REFINERY_WORKSPACE" --delta "$PACKAGE_DELTA_JSON" \
  --engagement "$ENGAGEMENT_RECEIPT_JSON"

"$OE_PYTHON" "$OE_ENTRY" commit \
  --binding "$PROJECT_BINDING_JSON" --task "$CURRENT_TASK_JSON" \
  --workspace "$REFINERY_WORKSPACE" --candidate "$DELTA_SHA256" \
  --authorization "$COMMIT_AUTHORIZATION_JSON"

"$OE_PYTHON" "$OE_ENTRY" verify \
  --binding "$PROJECT_BINDING_JSON" --task "$CURRENT_TASK_JSON" \
  --workspace "$REFINERY_WORKSPACE" --candidate "$DELTA_SHA256"

"$OE_PYTHON" "$OE_ENTRY" promote \
  --binding "$PROJECT_BINDING_JSON" --task "$CURRENT_TASK_JSON" \
  --workspace "$REFINERY_WORKSPACE" --candidate "$DELTA_SHA256" \
  --authorization "$PROMOTE_AUTHORIZATION_JSON"

"$OE_PYTHON" "$OE_ENTRY" history \
  --binding "$PROJECT_BINDING_JSON" --workspace "$REFINERY_WORKSPACE" \
  --candidate "$DELTA_SHA256"
```

`open/propose/commit/workspace verify/promote` 都是工作区写入，必须带当前 task；
`discover/run/history` 只读且不会在缺失路径上创建 workspace。`verify` 故意没有
`--regression-evidence` 或 `--release-evidence`。Promotion 响应中的 successor binding
`auto_applied=false`；控制面批准并另存该 document 后，才用新 binding 做 workspace
`discover/run`。旧 binding 不原地修改。

## 9. 退出码与分项报告

统一入口总以 exit 0 表示“稳定 JSON 已送达”，包括参数、输入、门禁和 workspace 被阻断的
情况；它从不表示 release 绿色。必须检查 `command_verdict` 和每个独立 section：

| 字段 | 建议值 | 依据 |
|---|---|---|
| `execution.status` | `passed|blocked|not_run` | DTO、context hash、binding、baseline、subject execution |
| `regression.status` | `not_run|passed|blocked` | Semantica-derived regression evidence 与 event |
| `receipt.status` | `missing|bound|verified|invalid` | source/wheel/package/input/output/PROV 绑定 |
| `release.status` | `not_checked|complete|blocked` | Semantica-derived release evidence、coverage 与 event |
| `learning.promotion.status` | `not_requested|promoted|blocked` | promotion authorization、descriptor、registry record/event |
| `learning.publication.status` | `not_requested|externally_authorized|published|blocked` | 外部权利/发布记录；refinery 不生成；另以 `decision=external_decision_required` 表明仍需决定 |

同时报告当前 state、delta/package/version SHA、event chain、receipt/PROV/evidence SHA、
缺失 capability、失败 check、stale baseline 和授权 blocker。只有状态与证据都满足时才可
描述相应阶段为绿色。

## 10. 授权与事实边界

| 决定 | 负责人 | refinery 的作用 |
|---|---|---|
| 实践记录是否为事实 | binding 中的 fact authority | 验证 creator、source ID/hash 与 receipt 绑定 |
| 同名冲突、merge/replace/remove | decision authority | 锁定 exact delta 与带理由授权，不替人裁决 |
| 风险、合规、产品放行 | 有资格的工程/合规角色 | 验证 release evidence，不签署结论 |
| package commit | decision authority | 要求 action=commit 的内容绑定授权 |
| industry promotion | promotion authority | 要求独立 action=promote 授权并追加 registry |
| 两卷书修改 | 作者/出版权利人 | 记录 `book_impact`，不自动改书 |
| push / published | 仓库、权利或发布负责人 | 完全位于 refinery 之外 |

项目实例、一次成功 attempt、视频观察、Agent 推断或合成书例都不能自动成为行业规律。
只有作用域明确、证据可追、冲突已判、回归与 release 分别闭合并获 promotion 授权的 exact
package version 才能进入 Semantica registry。
