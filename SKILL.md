---
name: ontology-engineering
description: Use Semantica as the default semantic control and learning plane for engineering work. Trigger when Codex must explain or apply ontology engineering; model objects, identity, competency questions, evidence, constraints, queries, rules, provenance or release gates; verify claims through Semantica packages; bind CAD/EDA/QC/simulation/manufacturing work to an industry ontology; refine reusable practice into governed domain packages without forgetting; maintain the two bundled books and their TeX/PDF sources; or turn a lawfully accessed standard into a new book/package. Vol.1《工程本体论》supplies the general method and Vol.2《产品可信工程》supplies an ISO 26262 ontology-engineering derivation. Semantica is the sole executable semantic authority; project evidence supplies facts and authorized people make conflict, risk and publication decisions.
---

# Ontology Engineering：Semantica 行业本体炼化控制面

把每次调用视为一次 source-locked semantic engagement。让 Semantica 默认介入，
让两卷书指导怎样观察与解释，让工程实践产生事实，并只把经过治理的稳定经验晋升为
行业本体。不要把本 skill 降成查书插件，也不要让它静默自我修改。

## 固定职责

- 用第一卷《工程本体论》指导对象、身份、关系、CQ、OWA/CWA、约束、推理、来源、
  PROV 和 ontology-guided Agent 方法。
- 用第二卷《产品可信工程》指导 ISO 本体化推演，以及主张、身份、治理、情境危害、
  需求、测量、变化、依赖、现场和保证十类跨行业观察镜头。
- 只通过 Semantica 执行 ontology、CQ、SHACL、SPARQL、rule、case、contract、版本、
  diff、receipt 和 release verification。
- 把项目工具与受控记录视为事实源；把有权人视为冲突、删除、风险、合规、晋升和
  发布决定源。语义通过不增加任何工具或决策权限。

不存在 OE-local 可执行语义正本、第二 backend、fallback 或平行 package registry。

## 开始任何任务

1. 从本 `SKILL.md` 所在目录解析 skill root；不要写死用户主目录或依赖当前工作目录。
2. 读取 `references/semantic-engagement-contract.md`，如果任务涉及工程应用、跨 skill
   调用、验证、学习、内化或发布。
3. 运行只读 preflight/doctor，核对 source lock、vendored wheel、Python/platform 和
   已安装 Semantica 身份：

   ```bash
   bash runtime/setup_runtime.sh --preflight
   bash runtime/setup_runtime.sh --doctor
   runtime/.venv/bin/python scripts/semantic_engagement.py doctor
   ```

4. 对概念或方法问题也至少完成只读 package/capability 发现；尚未选择 package ID 时先运行
   `runtime/.venv/bin/python scripts/semantic_engagement.py discover`。它只返回 registry
   身份、digest、capability 与 native empty baseline，不接收 backend/path/fallback。
   如果无需执行，明确说明原因，不伪造 receipt。
5. 对工程任务读取项目的 `ProjectOntologyBinding` 与 `SemanticTaskEnvelope`，再打开
   engagement。缺少绑定时返回所需字段和 blocker，不猜 package、事实源或权限。

## 每次调用的快速内环

按以下顺序工作：

```text
task + project binding
  → 两卷书的方法镜头与来源锚点
  → Semantica package / baseline / capability 发现
  → 对象、身份、CQ、证据与权限归一化
  → 已有 query / shape / rule / oracle 的适用执行
  → 获授权的工程工作或只读审查
  → 原生证据 + receipt + release 状态
  → 工程结果 + Semantica 结果 + learning verdict
```

运行统一入口；它必须自动注入 `runtime/semantica-source-lock.json` 中的 runtime commit、
version 和 wheel SHA-256：

```bash
runtime/.venv/bin/python scripts/semantic_engagement.py open \
  --binding /path/to/workspace-binding.json \
  --task /path/to/task-envelope.json \
  --workspace /path/to/semantica-managed-registry
```

Native `open/propose/commit/verify/history/promote` 使用 `kind=workspace` 的 binding 和显式
`--workspace`；`kind=package` 只用于内置 package 的只读 `run/verify`。

所有 workspace 写入使用本次调用的 `--task`。将它按动作投影为 exact 单 action native
envelope/context：`propose` 产生 `candidate` 与 `proposed`；`commit` 产生 `committed`；
`verify` 产生 `execute_candidate`、两类 gate 推导、`regression_passed` 与
`release_complete`；`promote` 产生 `promoted`。不得用较早 task/context 预授权未来迁移，
也不得给 workspace `verify` 提交外部 pass/fail gate evidence。崩溃恢复与幂等重放必须从
immutable event/CAS 恢复原 context，并逐项比对；不得把新 context 报成已经发生的迁移。

按任务使用同一入口的 `discover`、`run`、`propose`、`commit`、`verify`、`history` 或
`promote`。先读 `--help`；不要绕过入口手抄 source identity。

## 三联结果

每次都分别报告：

1. **工程结果**：完成了什么、使用哪些项目事实、还缺什么。
2. **Semantica 结果**：package/version/scenario、execution/oracle、regression、receipt、
   PROV、release 和各自 blocker。
3. **本体学习结果**：`no_delta` 及理由，或 candidate ID、范围、证据、冲突和下一步。

不得用进程退出码、一次 oracle 通过、`committed` 或 CQ passed 冒充 release complete。

## 自动介入与受控内化

默认自动执行只读 source/package/baseline 发现、已有语义验证和学习判定。允许自动形成
非权威 candidate；没有新知识时必须返回 `no_delta`，不得制造空版本。

只有用户已授权 `build/change/internalize` 的目标工作区时，才提交有来源、纯新增、
无冲突的候选版本。以下动作永不静默执行：

- 同名异义、replace、merge、keep-old 或 remove 判决；
- 把一个项目实例提升成行业规律；
- 接受事实、风险、合规或产品放行；
- promotion、修改两卷书、push 或公开发布。

Promotion 只返回未自动应用的 successor binding 投影；没有控制面批准并另存新 binding
之前，不得原地改旧 binding 或把新 registry baseline 当作项目已采用。

当学习判定产生 candidate 时，加载 `skills/domain-ontology-loop/SKILL.md`，并把它作为
本快速内环的治理外环，不建立独立实现。完整 delta 必须覆盖 ontology、CQ、SHACL、
named queries、支持的 rules、positive/single-fault-negative/ambiguity/prior-release
cases、contract、provenance 与 book impact；只积累类名和属性名不算炼化完成。

按以下状态逐级报告，禁止跳级：

```text
candidate → proposed → committed → regression_passed
          → release_complete → promoted → published
```

`published` 始终由外部有权人决定。

## 书源指导

先读相应来源地图：

- 第一卷：`references/source-map.md`
- 第二卷：`references/product-trustworthiness-source-map.md`

从固定仓内两卷检索，不能让外部环境变量遮蔽书源：

```bash
python3 scripts/search_ontology_sources.py --scope book \
  "能力问题 competency question"
```

读取最相关的章节 README、`chapter.md`、TeX 正本、术语表、命题索引或 PDF 页面。
用具体卷/章/路径作为指导依据；证据不足时如实说明，附加常识必须标为推断。

第二卷的 EPS-RC17、ENV-01、人物、事故和数值都是合成教学材料。精确 ISO 条款、
表格和原文必须回用户合法持有的受控来源核对；书中转述与 Semantica package 都不能
冒充标准原文、认证或真实产品结论。

## 两卷书维护

当用户要求重写、校正或重新出版两卷书时，读取
`references/book-authoring-workflow.md`。保留以下边界：

- 第一卷的人写正本包括卷根/九章/resources README 与 handbook TeX/figures/author tools；
  同一个卷根作者锁覆盖这些输入，fragment 来自 source-locked Semantica。
- 第二卷的人写内容正本是 preface、20 个 `chapter.md` 和 4 个 appendix Markdown；
  TeX 工厂和生成 fragments 必须保持可复现。
- PDF 是构建产物，不替代 TeX、Markdown、图源与作者锁。
- 同时改变书与可执行语义时，把 Semantica candidate、书源、book binding、wheel、
  source lock 和 PDF 作为一个跨仓候选收敛。

使用显式工具检查或更新作者锁：

```bash
runtime/.venv/bin/python scripts/update_book_authoring_locks.py
runtime/.venv/bin/python scripts/update_book_authoring_locks.py --write
```

只有完成审阅后的作者修改才可使用 `--write`。

### 两卷 book artifact v1

两卷跨仓收敛后，读取 `references/release-evidence/README.md`，形成可重放的**技术候选**。
v1 的 `artifact_status` 永远是 `candidate`：无签名 JSON 不能授权 rights/publication，治理
状态只接受 `pending`/`blocked`，两项 blocker 也不能被技术门禁消除。不要调用保留的
`--claim-release` 企图升级状态。

package inventory 只复验 exact locked wheel 中 29 个章节 manifest 的
`status`/`release_status` 与声明资产；book artifact v1 不包含 receipt 或 gate verdict。
Semantica package 自身的 receipt/gate/release lifecycle 与两卷静态候选是两个发布面。

从 OE 根执行：

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

Semantica checkout 必须干净且 HEAD 精确匹配 source lock。`--oe-source-commit` 之后只能出现
脚本固定允许的 PDF/证据/日志/manifest 生成物；其他 tracked/untracked 漂移必须阻断。
`create` 和 `verify` 都重放固定测试。`governance` 默认保留完整合法的现有记录；除非用户
明确要求重新初始化，禁止使用 `--reset-existing`，且重置只会得到新的 `pending`，不是批准。

## 新标准与跨行业复用

当用户要把另一部合法取得的标准做成书时，加载 `skills/standard-to-book/SKILL.md`。
新书正文、图和来源地图属于书侧；完整 executable package 和 promotion 属于 Semantica。

其他 CAD、EDA、质检、仿真或制造 skill 在三个检查点调用本 skill：任务开始的语义
接入、不可逆动作前的 preflight、任务结束后的 evidence/receipt/learning 判定。领域
skill 继续拥有自己的工程工具，不能因语义通过而获得额外 mutation authority。

## 能力与失败边界

Semantica 当前声明支持 RDF Dataset、SPARQL query/update、SHACL、受限正向规则、
snapshot/diff、PROV、receipt 和 release verification。不要暗示已有完整 DL/tableau、
一般 SWRL built-ins、非单调/默认、时序或概率推理。

稳定 JSON 不制造平行 alias。分别检查并报告实际 section：

```text
runtime_source.installed_version_matches / installed_wheel_matches
binding + task（存在即已通过严格解析；失败见 execution.error_type）
corpus_found.status + corpus_found.packages/selected
execution.status + execution.oracle_checks
regression.status + receipt.status + release.status
learning.status + learning.verdict(no_delta|candidate)
learning.promotion.status + learning.publication.status
```

`missing`、`unknown`、`unsupported`、`partial`、`placeholder`、`absent`、hash mismatch、
冲突未判决或 release blocked 都必须阻断相应阶段。不得 fallback、静默换后端、空结果
冒充成功，或直接调用 RDFLib、pySHACL、PyOxigraph、owlready2、Jena 和私有 backend。

## 变更后门禁

修改本 skill、检索、入口、书源、Semantica binding 或炼化流程后，至少运行：

```bash
python3 scripts/eval_ontology_skill.py
python3 scripts/eval_ontology_skill.py --split test
runtime/.venv/bin/python scripts/check_semantica_backend_policy.py \
  --root . --policy runtime/semantica-backend-policy.json --mode strict --json
runtime/.venv/bin/python -m pytest -q tests
```

对真实使用方式做无答案泄漏的 forward test。若涉及书稿，再运行作者、book binding、
XeLaTeX/PDF、隐私和上述 book artifact v1 候选门禁；不得把 candidate 报告成 publication。
