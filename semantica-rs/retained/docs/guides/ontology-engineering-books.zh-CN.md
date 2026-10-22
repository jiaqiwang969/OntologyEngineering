[English (canonical)](ontology-engineering-books.md) | [简体中文](ontology-engineering-books.zh-CN.md)

# 从两卷书到可执行工程语义

本指南是 Semantica `0.6.5+oe.6` 所带 OntologyEngineering 集成的简体中文
说明；英文版为正本。这个 fork 将中文书系《工程本体论》和《产品可信工程》两卷
推导出的可执行语义直接集成到 Semantica。

**先读两卷书：** [中文导读与 PDF](https://github.com/jiaqiwang969/OntologyEngineering)
· [English overview](https://github.com/jiaqiwang969/OntologyEngineering/blob/main/README.en.md)

## 为什么值得使用这套集成

两卷书让工程师能够以人类可读的方式学习本体工程方法并理解面向 ISO 的推演；
Semantica 则让团队把其中可复用的部分作为确定、可检查的 package 用于真实工程流程。
这种分工既让人容易学习和审阅方法，也让机器与评审者能够审计执行、证据和治理过程。

## 职责边界

- 两卷书是外部的人类可读方法层和面向 ISO 的推演层；它们既不是项目事实源，也不是
  可执行 runtime。
- Semantica 是唯一的可执行语义。可执行 ontology、能力问题、SPARQL、SHACL、案例、
  支持的规则、版本快照、PROV、receipt 和 release verification 均由 Semantica 管理。
- 项目工具和受控记录提供项目事实及原生证据。语义检查通过并不等于接受这些事实，
  也不会扩大工具权限。
- 冲突、删除、风险、合规、promotion、权利和 publication 由有权人决定。任何执行结果
  或 gate 都不会授予这些权限。

不存在第二套 ontology runtime，也不会 fallback 到书籍仓库中的本地资产。不受支持的
SWRL built-in、描述逻辑 profile 或不完整的章节 contract 都返回 `blocked`，绝不会
报告为成功。

## Package 清单

Semantica `0.6.5+oe.6` 包含 29 个章节 package（第一卷 9 个、第二卷 20 个），
以及 1 个规范性领域 package。这份清单本身并不表示这些 package 已经
release-complete。

```python
from semantica.chapter_packages import (
    list_chapter_packages,
    list_domain_packages,
    validate_chapter_registry,
    verify_book_source_bindings,
)

assert len(list_chapter_packages()) == 29
assert len(list_domain_packages()) == 1
assert validate_chapter_registry() == ()

# Run from a checkout that contains both maintained book projects.
assert verify_book_source_bindings("/path/to/ontology-engineering").passed
```

章节 ID 从 `semantica.chapter_packages.vol1.ch01` 到 `vol1.ch09`，以及从
`vol2.ch01` 到 `vol2.ch20`。额外的领域 package 是
`semantica.chapter_packages.vol2.normative`。

## 执行精确场景

```python
import os

from semantica.chapter_packages import SemanticPackageRunner

runner = SemanticPackageRunner()
result = runner.run(
    "semantica.chapter_packages.vol1.ch03",
    runtime_commit=os.environ["SEMANTICA_RUNTIME_COMMIT"],
    runtime_artifact_sha256=os.environ["SEMANTICA_WHEEL_SHA256"],
)

print(result.status)                    # scenario execution status
print(result.release_verdict.complete)  # independent release decision
print(result.to_json())                 # backend-neutral evidence DTO
```

场景成功与 release readiness 被有意区分。即使精确场景的 oracle 通过，只要已声明的
领域缺口仍然存在，当前 package manifest 就会保留 `release_status: blocked`。
因此，场景通过不代表 package 已经 release-complete。

## CLI 与 MCP

```bash
semantica package list --json
semantica package show semantica.chapter_packages.vol2.ch12 --json
semantica package verify-books \
  --book-root /path/to/ontology-engineering \
  --json
: "${SEMANTICA_RUNTIME_COMMIT:?set the reviewed 40-character source commit}"
: "${SEMANTICA_WHEEL_SHA256:?set the reviewed 64-character wheel SHA-256}"
semantica package run semantica.chapter_packages.vol2.ch12 \
  --runtime-commit "$SEMANTICA_RUNTIME_COMMIT" \
  --runtime-artifact-sha256 "$SEMANTICA_WHEEL_SHA256" \
  --json
```

正式 MCP server 通过 `list_chapter_packages`、`get_chapter_package`、
`verify_book_sources`、`run_chapter_package` 和 `verify_chapter_package`
提供同一组 allowlist 控制面。顶层 `mcp` package 只是该 adapter 的兼容视图，
不是第二套实现。

`verify-books` 检查全部 29 份作为绑定正本的章节源、维护者指南、受维护的 TeX
源或生成的 TeX 快照、匹配的章节 contract，以及声明由相应章节派生的每一项
package 资产。任何文件缺失或 hash 漂移都会阻断命令。书籍仓库中的
`scripts/rebind_semantica_books.py` 是有意修改书稿后，经评审执行的显式绑定刷新
步骤；runtime 执行绝不会重写这些绑定，也不会 fallback 到书籍仓库中的本地语义。

## 迁移 PROV 与书籍构建

`read_migration_map(volume)` 和 `resolve_migration_successor(old_path)`
保存精确的 source-to-package ledger。`package_asset_text(package_id, asset_id)`
只有在 registry 和 SHA-256 校验通过后才加载文本资产。有歧义的旧路径会保留全部候选，
未知路径则保持未解析状态。

打包资产不包含受控的 ISO 原文。规范性 engraver 只接受显式、合法受控的 source root
和显式的 book root；它仅输出坐标、模态、允许释放的释义及 hash，并在原子替换生成的
artifact 集合前检查完整输出。该构建操作不等于外部 publication，也不是权利判定。

## 行业本体炼化：快速接入环与慢速真值环

两卷书是外部方法说明：第一卷定义本体工程方法，第二卷演示面向 ISO 的推演。
Semantica 是唯一的可执行语义。项目不得执行来自书籍仓库的第二份 ontology、CQ、
SHACL、query、rule 或 case 资产副本。

快速环在每个工程任务中运行。它绑定 task envelope、项目 baseline、runtime source
identity、五部分 engagement result 和 source hash。`no_delta` 结果仍然是接入证据。
只有完整的 engagement 才能创建 candidate。慢速环在显式授权下，让该不可变 candidate
依次经过 `candidate -> proposed -> committed -> regression_passed ->
release_complete -> promoted`。

每个 `PackageDelta` 都声明以下八个数组，即使某个数组为空也不例外：
`ontology`、`competency_questions`、`shapes`、`queries`、`rules`、
`cases`、`contract` 和 `provenance`。replace/remove 变更在 commit 时必须
带有逐资产匹配的决定。contract 资产是严格 JSON：它把 runner 角色分配给精确的 CAS
资产，并把固定回归套件绑定到真实场景、CQ ID，以及 positive、negative、ambiguity
和 prior-release case 资产。

commit 后，调用方不能提交绿色布尔值或合成 receipt。`execute_candidate` 使用
`SemanticPackageRunner` 执行 contract 绑定的场景，并保存每次完整运行结果及其原生
receipt；gate evidence 从这个 suite 推导。Regression 始终包含六项
`cq.*`/`case.*` 检查；release 始终包含六项 package/capability/receipt/
provenance/rights/I/O 检查。对于非 bootstrap 版本，prior coverage 从不可变的
base descriptor、manifest、projection、CQ registry 和 case object 推导。目标
package 不能只把新的 CQ 或 case 标记为 `prior` 就满足该要求。

每次受治理的 candidate/lifecycle 写入都带有 `TransitionContextDTO`：一个精确
action、精确 delta、当前的单 action task envelope，以及保留的项目 binding。
candidate 注册从 candidate envelope 推导 context；之后的所有写入都必须显式提供
context。candidate 之前的 `record_engagement` 操作是唯一例外。Semantica 把每个
context 存入 CAS，将其绑定到 event chain，并在重启时重放。`actor_id` 仍是可审计
的 actor assertion；authority 只来自 binding 以及显式的 commit 或 promotion
authorization。只有在 regression 已被记录后才能推导 release。重试同样 fail closed：
调用方不能用新的 proposal 或 release context 替换真正产生不可变 event 的 context。

Execution、regression、release 和 promotion 各自带有确定性的 PROV closure。
该 closure 绑定 source evidence、candidate envelope、项目 binding、engagement
receipt、delta、manifest/projection、有序 transition context、runtime identity、
具名 provenance 资产、场景输入和输出 hash、完整 runner 结果、原生 receipt 和原生
PROV bundle。具名 rights-evidence 资产只证明相应字节已被绑定，并不构成法律判定。

```python
from semantica.chapter_packages import SemanticPackageRunner
from semantica.ontology import (
    IndustryOntologyRegistry,
    TransitionContextDTO,
    commit_candidate,
    derive_gate_evidence,
    execute_candidate,
    promote_candidate,
    propose_candidate,
    verify_candidate,
)

# envelope is the retained candidate envelope. binding, engagement, delta,
# commit_authorization, promotion_authorization, and runtime_source are also
# validated refinery DTOs. make_task_envelope(action) is application-owned and
# must return a current envelope whose requested_actions is exactly (action,).
registry = IndustryOntologyRegistry.create(
    "/var/lib/semantica/industry", registry_id="factory-ontology"
)

def context(action):
    return TransitionContextDTO.create(
        action=action,
        delta_sha256=delta.delta_sha256,
        envelope=make_task_envelope(action),
        binding=binding,
    )

propose_candidate(
    registry,
    delta=delta,
    envelope=envelope,
    binding=binding,
    engagement=engagement,
    context=context("proposed"),
)
commit_candidate(
    registry,
    delta_sha256=delta.delta_sha256,
    authorization=commit_authorization,
    context=context("committed"),
)
suite = execute_candidate(
    registry,
    delta_sha256=delta.delta_sha256,
    context=context("execute_candidate"),
    runtime_source=runtime_source,
)
regression = derive_gate_evidence(
    registry,
    delta_sha256=delta.delta_sha256,
    context=context("derive_regression_gate"),
    gate="regression",
    execution_suite_sha256=suite.suite_sha256,
)
verification = verify_candidate(
    registry,
    delta_sha256=delta.delta_sha256,
    execution_suite_sha256=suite.suite_sha256,
    regression_evidence=regression,
    regression_context=context("regression_passed"),
    release_derivation_context=context("derive_release_gate"),
    release_context=context("release_complete"),
)
assert verification.state.state == "release_complete"
promote_candidate(
    registry,
    delta_sha256=delta.delta_sha256,
    authorization=promotion_authorization,
    context=context("promoted"),
)

# A later task discovers and executes the promoted package from the registry;
# no package path or book-local fallback is accepted.
reopened = IndustryOntologyRegistry("/var/lib/semantica/industry")
runner = SemanticPackageRunner()
result = runner.run_registry(
    reopened,
    delta.package_id,
    "the-contract-scenario-id",
    runtime_commit=runtime_source.runtime_commit,
    runtime_artifact_sha256=runtime_source.runtime_artifact_sha256,
    runtime_version=runtime_source.runtime_version,
)
assert runner.verify(result).complete
```

如果进程在记录 regression 后、记录 release 前停止，重新打开 registry，并以
`regression_evidence=None` 和 `regression_context=None` 调用
`verify_candidate`。Semantica 会重新加载并验证不可变的 regression evidence；
两个 release context 仍然必需。release 完成后，重试必须提供已绑定到 event 的
同一组精确 release context。

`build_refinery_acceptance_delta(...)` 是由 Semantica 提供的公开 fixture，用于
adapter 和安装验收。它构造完整、可执行的八类资产表面，包括严格 projection 和
source-evidence binding。它不能代替领域炼化流程中由证据支撑的 package delta。

Promotion 只表示进入本地行业 registry，不等于 publication。分发、标准许可和任何
外部 publication 始终由相关权利人或 release authority 显式决定。

关于打包衍生资产的 source 和权利边界，请参见
`semantica/chapter_packages/NOTICE.md`。
