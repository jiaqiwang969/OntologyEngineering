# Ontology Engineering：Semantica 行业本体炼化控制面

调用这个 Skill，不只是查两本书，而是开启一次 **source-locked Semantica 语义工作会话**。
Semantica 默认介入工程任务，校验已有行业语义，并判断本次实践是否产生值得治理、回归和晋升的新知识。

## 定位

```text
工程任务与受控证据 ──事实源──┐
两卷书 ───────────方法镜头────┼─→ ontology-engineering Skill
有权人 ───────────决定与授权──┘       │
                                      ▼
                          Semantica 唯一可执行语义层
                package / CQ / SHACL / query / rule / cases
                 receipt / PROV / version / release / refinery
                                      │
                                      ▼
             工程结果 + Semantica 结果 + 本体学习结果
```

- 第一卷《工程本体论》提供通用理论和方法。
- 第二卷《产品可信工程》是 ISO 26262 本体化推演和完整示范。
- 项目原生记录是事实权威；书和本体不能凭空制造项目事实。
- 有权人负责冲突、风险、合规、晋升和发布决定。
- Semantica 是唯一 executable semantics 和行业本体记忆；OE 不维护第二套后端。

“默认介入”不表示每次都改本体。每次至少完成 source lock、语义发现、适用验证和学习判定；没有新知识时明确返回 `no_delta`。

## 本地运行时

先做无写入预检，再安装锁定 wheel，最后审计实际安装身份：

```bash
bash runtime/setup_runtime.sh --preflight
bash runtime/setup_runtime.sh
bash runtime/setup_runtime.sh --doctor
```

安装只接受 [`runtime/semantica-source-lock.json`](runtime/semantica-source-lock.json) 指定的 Semantica 版本、源码 commit 和 wheel SHA-256。缺件、版本不符或哈希不符都会 fail closed；不会改用环境里的其他 RDF/OWL 后端。
Doctor 不只比较版本号：它以已锁定 wheel 的 `RECORD` 为权威，逐文件核对实际 import
root、package path、SHA/size、symlink 和未记录文件，防止同版本 shadow package 或本地篡改。

## 开启一次语义工作会话

还不知道 package ID 时，先做不带 binding 的只读发现：

```bash
runtime/.venv/bin/python scripts/semantic_engagement.py discover
```

输出只列 Semantica registry 中的 package，并给出 `package_id`、`version`、内容 digest、
所需/可用 capability；`native_workspace_bootstrap` 另给出新行业工作区的 version `0` 与
exact empty-package digest。该命令没有 backend、package path 或 fallback 参数。选定目标后，
项目保存两个受控 JSON：

1. `SemanticTaskEnvelope`：这一次要做什么、证据逻辑引用和请求动作。
2. `ProjectOntologyBinding`：项目绑定到哪个 Semantica package/workspace、哪一版 baseline，以及事实与决定权威。

最小任务信封：

```json
{
  "$schema": "ontology-engineering.semantic-task-envelope/v1",
  "task_id": "pump-review-017",
  "task_kind": "review",
  "intent": "审查泵总成释放证据是否闭合",
  "project": "pump-platform-a",
  "domain": "industrial-pump",
  "requested_decision": "是否具备提交人工释放评审的条件",
  "actor_id": "pump-review-agent",
  "requested_actions": ["open", "propose"],
  "required_capabilities": ["semantic.engagement"],
  "evidence": [{
    "source_id": "pump-review-017-record",
    "uri": "evidence:pump-platform-a/review-017",
    "sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
    "media_type": "application/json",
    "captured_at": "2026-08-19T12:00:00Z"
  }],
  "created_at": "2026-08-19T12:01:00Z"
}
```

最小 native workspace 绑定：

```json
{
  "$schema": "ontology-engineering.semantic-project-binding/v1",
  "binding_id": "pump-platform-a-binding",
  "project": {"project_id": "pump-platform-a", "domain": "industrial-pump"},
  "semantic_target": {
    "kind": "workspace",
    "workspace_id": "pump-platform-a-registry",
    "package_id": "semantica.industry.example"
  },
  "baseline": {
    "version": "0",
    "digest": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
  },
  "evidence": {"logical_root": "evidence:pump-platform-a"},
  "authority": {
    "fact": {"authority_id": "controlled-test-system", "scope": ["test-result"]},
    "decision": {"authority_id": "release-board", "scope": ["risk", "promotion"]}
  },
  "allowed_actions": ["open", "propose"],
  "lifecycle_actions": ["candidate", "proposed"],
  "promotion": {"target": "industry-registry", "requires_decision_authority": true},
  "created_at": "2026-08-19T12:00:00Z",
  "semantic_api": "semantica.ontology.refinery/v1"
}
```

真实 ID、版本和 digest 必须来自 Semantica 发现结果，不能复制示例值。新 workspace 的
baseline digest 必须使用 `native_workspace_bootstrap.baseline_digest`。开启 native 会话必须
同时给出绑定中的实际托管工作区路径：

```bash
runtime/.venv/bin/python scripts/semantic_engagement.py doctor
runtime/.venv/bin/python scripts/semantic_engagement.py open \
  --binding ./project-semantic-binding.json \
  --task ./semantic-task.json \
  --workspace ./semantica-industry-registry
```

`kind=package` 是另一种只读绑定，用于已内置 package 的 `run/verify`；它带 exact
`package_version` 和发现所得 digest，不接受 `--workspace`，也不用于 native refinery
`open/propose/commit/promote`。

所有 workspace 写入都必须携带**当前** `--task`。适配器把每次调用拆成 exact 单动作的
Semantica transition context：`propose → candidate + proposed`，`commit → committed`，
`verify → execute_candidate + 两次 gate 推导 + regression_passed + release_complete`，
`promote → promoted`。每个 context 都绑定当前 task、binding 与 delta SHA，并由 Semantica
保留其 context/object SHA；早先 task 不能预授权后续迁移。Workspace `verify` 不接受外部
regression/release pass JSON，门禁只能从 Semantica 实际执行 committed subject 后内部推导。
崩溃恢复或幂等重放也不能伪造“本次发生”的迁移：Semantica 必须从 immutable event/CAS
恢复原 context，并要求调用所给 context 与原记录完全一致；不一致就 fail closed。

稳定 JSON 的进程退出码只说明响应成功送达，**不代表所有语义门禁绿色**。调用方必须
分别读取 `corpus_found.status`、`execution.status`、`regression.status`、`receipt.status`、
`release.status`、`learning.status` 与 `learning.verdict`，以及 learning 内的 promotion /
publication 状态。

完整字段、权限和失败语义见 [`references/semantic-engagement-contract.md`](references/semantic-engagement-contract.md)。

## 每次调用的固定输出

每次调用都必须返回三份结果：

1. **工程结果**：当前工作完成了什么，依据了哪些项目事实，还缺什么。
2. **Semantica 结果**：package/baseline、CQ/shape/query/rule/oracle、receipt/PROV、回归和 release 状态。
3. **本体学习结果**：`no_delta` 及理由，或完整 `PackageDelta` 候选及下一治理步骤。

完整 delta 不只是类和属性，还覆盖 ontology、CQ、SHACL、query、rule、四类案例、能力合同、provenance 和 book impact。

## 双循环

快速内环发生在每个工程任务中：

```text
读取绑定与 source identity → 发现 baseline → 语义验证 → 工程执行/审查
→ 绑定 receipt 与证据 → 三联输出 → no_delta 或 candidate
```

慢速外环只处理有复用价值的候选：

```text
candidate → proposed → committed → regression_passed
→ release_complete → promoted → published（外部授权动作）
```

前五步仍不能自动替代人作风险接受、事实裁决或公开发布。慢速外环见 [`skills/domain-ontology-loop/SKILL.md`](skills/domain-ontology-loop/SKILL.md)。

Promotion 也不会就地改写项目 binding。响应只给出 `auto_applied=false` 的 successor
binding 投影；它同时绑定新 baseline、predecessor binding SHA 与 promotion-record SHA，
必须经控制面批准并另存后才能用于后续 `discover/run`。旧 binding 会因 baseline stale
而阻断新执行，但 immutable candidate history 仍可审计。

## 两卷书

| 卷 | 成书 PDF | 作用 |
|---|---|---|
| 第一卷《工程本体论》 | [工程本体论-全书.pdf](references/ontology-engineering-book/handbook/工程本体论-全书.pdf) | 通用理论、方法和 Agent 语义控制 |
| 第二卷《产品可信工程》 | [产品可信工程-全书.pdf](references/product-trustworthiness-book/handbook/产品可信工程-全书.pdf) | ISO 26262 本体化推演与跨行业观察镜头 |

检索固定使用与脚本同包的两卷书，不允许外部目录遮蔽书源：

```bash
python3 scripts/search_ontology_sources.py --scope book "五张 PASS 为什么拼不成一次放行"
```

书是指导，不是行业实践日志。只有通用方法、稳定教学失败模式、ISO 推演或 Semantica 真实能力边界发生实质变化时，才更新正文。

## TeX/PDF 作者工作流

两卷的 TeX、章节 Markdown、图、生成 fragments、源码锁和 PDF 都是维护对象，不能只提交 PDF。修改书稿后依次执行：

```bash
python3 scripts/update_book_authoring_locks.py
python3 scripts/update_book_authoring_locks.py --write
python3 scripts/update_book_authoring_locks.py
```

再按相应 handbook README 生成 fragments、编译 XeLaTeX/PDF、核对视觉结果，并在 Semantica 中更新 book binding 与相关 package 回归。完整顺序见 [`references/book-authoring-workflow.md`](references/book-authoring-workflow.md)。

## 两卷 book artifact 技术候选

book artifact v1 只生成 `candidate`，不会授权公开发布。rights/publication 的无签名 JSON
只接受 `pending`/`blocked`；即使全部技术门禁通过，也不能把它改写成批准。章节 package
状态只从 source lock 指向的 exact wheel 读取，v1 manifest 不携带 package receipts 或 gates。

从 OE 根收集并复验候选；三个需要 Semantica 源码的步骤必须使用同一个、HEAD 精确匹配
source lock 且 worktree 干净的 checkout：

```bash
runtime/.venv/bin/python scripts/collect_book_release_evidence.py governance
runtime/.venv/bin/python scripts/collect_book_release_evidence.py static
runtime/.venv/bin/python scripts/collect_book_release_evidence.py book-bindings
runtime/.venv/bin/python scripts/collect_book_release_evidence.py regressions \
  --semantica-root /controlled/semantica/checkout
runtime/.venv/bin/python scripts/collect_book_release_evidence.py pdf-qa \
  --visual-review references/release-evidence/pdf-visual-review.json
runtime/.venv/bin/python scripts/book_release_artifacts.py create \
  --oe-source-commit 0123456789abcdef0123456789abcdef01234567 \
  --semantica-root /controlled/semantica/checkout
runtime/.venv/bin/python scripts/book_release_artifacts.py verify \
  --semantica-root /controlled/semantica/checkout
```

`--oe-source-commit` 冻结 OE source closure；该 commit 之后只允许脚本列出的固定生成物。
验证器会重跑固定的 OE/Semantica 测试，而不是相信记录中的 pass。治理初始化默认保留已有
合法记录，只有显式 `governance --reset-existing` 才一起重置为 `pending`。完整合同见
[`references/release-evidence/README.md`](references/release-evidence/README.md)。

## 新标准与跨 Skill 使用

把另一部合法取得的标准做成新书时，使用 [`skills/standard-to-book/SKILL.md`](skills/standard-to-book/SKILL.md)。可读正文和来源地图留在书侧；所有 executable ontology/CQ/shape/query/rule/case/contract/receipt 进入 Semantica package。

CAD、EDA、仿真、质检和制造 skill 在三个检查点接入：任务开始、不可逆动作前、任务结束。领域 skill 继续负责原生工具；ontology-engineering 负责语义验证和炼化，不因此获得额外执行权限。

## 验证

```bash
python3 scripts/eval_ontology_skill.py
python3 scripts/eval_ontology_skill.py --split test
python3 scripts/check_semantica_backend_policy.py --mode strict
python3 scripts/update_book_authoring_locks.py
python3 -m unittest discover -s tests -v
```

严格后端门禁要求零发现：不直连 RDFLib、pySHACL、PyOxigraph、owlready2、Jena，不携带 executable TTL/OWL/SPARQL/SHACL/fixture 副本，也没有 fallback。

## 当前发布边界

本地 source-locked 集成和作者工作流可以继续完善，但“代码可运行”或 book artifact v1
技术候选通过都不等于“内容可公开发布”。当前权利、隐私和章节 release 状态必须分别查看
[`docs/PUBLIC-RELEASE-STATUS.md`](docs/PUBLIC-RELEASE-STATUS.md) 与
[`docs/PRIVACY-AND-RIGHTS.md`](docs/PRIVACY-AND-RIGHTS.md)；默认不得把受限标准原文、企业
证据、个人路径或未清权利资产推入公共仓库。

## 目录

```text
SKILL.md                         默认介入与路由
ontology_engineering/            source-locked Semantica 适配层
runtime/                         wheel/source lock、安装与 doctor
references/                      两卷书、合同与作者源码
demos/                           Semantica package 薄启动器
skills/domain-ontology-loop/     行业本体治理外循环
skills/standard-to-book/         标准到书的作者流程
scripts/                         会话、检索和门禁
tests/                           合同与回归测试
```

> 两卷书告诉我们怎样看，项目证据告诉我们发生了什么，Semantica 让语义可执行且可记忆，有权人决定什么可以被接受、晋升和发布。
