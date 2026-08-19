# OntologyEngineering

## 两卷书是石头，Semantica 是唯一语义运行层

OntologyEngineering 面向进入陌生专业领域的工程师、中小制造企业和
**AI × 制造**团队。它把合法获得的标准与工程知识，在专业复核下重构为普通
工程师能读、能查、能验证的中文知识产品。

本仓库现在有一条清楚的责任边界：

```text
OntologyEngineering
  ├─ 第一卷《工程本体论》（9 章）
  ├─ 第二卷《产品可信工程》（20 章）
  ├─ 两卷来源地图、检索与教学薄入口
  └─ 锁定 Semantica 源码/wheel 的构建证据

Semantica fork
  ├─ 29 个 built-in chapter packages + normative domain package
  ├─ ontology / CQ / SHACL / SPARQL / case / rule / contract
  ├─ snapshot / diff / version / PROV / receipt / release gate
  └─ 同一核心之上的 Python / CLI / MCP
```

换句话说，**外部“石头”只有两卷书**。本体、能力问题、形状、查询、案例、
工程规则和生命周期制品不再是本仓库的第二套资产；它们全部由
[Semantica fork](https://github.com/jiaqiwang969/semantica) 的内建包拥有并执行。
运行时只有 Semantica；任何失败都直接阻断。

## 60 秒上手

### 先读书

| 卷 | 书名 | 成书 PDF | 作用 |
|---|---|---|---|
| 第一卷 | 《工程本体论》 | [工程本体论-全书.pdf](references/ontology-engineering-book/handbook/工程本体论-全书.pdf) | 理论与方法：怎样把对象、身份、关系、规则和证据说清楚 |
| 第二卷 | 《产品可信工程》 | [产品可信工程-全书.pdf](references/product-trustworthiness-book/handbook/产品可信工程-全书.pdf) | 实战示范：以功能安全为样板，走完传统工程与规范本体化 |

第一卷教“怎样建”；第二卷证明“怎样在一个严苛领域完整走一遍”。两卷共同坚持：

> 工程本体是语义之根，受控活动是事实之源，有权的人是决定之源。

### 查一个问题

```bash
git clone https://github.com/jiaqiwang969/OntologyEngineering.git
cd OntologyEngineering
python3 scripts/search_ontology_sources.py --scope book \
  "五张 PASS 为什么拼不成一次放行"
```

检索返回书中章节、案例和上下文。第一卷地图见
[`references/source-map.md`](references/source-map.md)，第二卷地图见
[`references/product-trustworthiness-source-map.md`](references/product-trustworthiness-source-map.md)。

### 运行书中论断

```bash
bash runtime/setup_runtime.sh
runtime/.venv/bin/python demos/vol2_hara_asil_corroborate.py
runtime/.venv/bin/semantica package list --json
```

`demos/` 只是薄启动器：它选择 Semantica package/scenario，并输出 exact oracle、
PROV/receipt 与发布状态。数据、本体、查询、形状、规则和案例均不在 demo 中复制。
完整映射见 [`demos/README.md`](demos/README.md)。

本地安装使用 `runtime/semantica-source-lock.json` 锁定的完整源码构建与 wheel；文档
故意不写会过时的 commit 或哈希。以 lock 文件和实际 receipt 为准，不以环境里碰巧
安装的版本为准。

### 验证 Skill 与唯一后端边界

```bash
python3 scripts/eval_ontology_skill.py
python3 scripts/eval_ontology_skill.py --split test
python3 scripts/check_semantica_backend_policy.py --strict
```

严格门禁要求 zero exception：OE 不直连 RDFLib、pySHACL、PyOxigraph、owlready2、
Jena 或动态旁路，不保留 executable TTL/OWL/SPARQL/SHACL/fixture 副本，也没有
fallback。未知包、缺失资产、不支持能力或哈希不一致必须 fail closed。

## 为什么这样分层

书和代码有不同职责：

- 书负责让人理解来源、概念、判断链、例外与责任边界；
- Semantica package 负责稳定语义和可复现执行；
- query、SHACL、rules 与 exact oracle 负责暴露不一致；
- snapshot、version、PROV 与 receipt 负责说明“哪一版、用什么输入、得到什么结果”；
- Agent 只在已有权限内执行，语义通过不等于授权通过。

这使 29 章各自拥有可审计的执行身份，同时避免书仓库和运行库各维护一份会漂移的
“真相”。详细契约见
[`references/semantica-deep-binding.md`](references/semantica-deep-binding.md)。

## 当前两卷的内容边界

第二卷前十章按 ISO 26262 生命周期讲传统功能安全实践；后十章把同一批困难逐项
转成主张、身份、治理、情境危害、需求追溯、测量证据、版本变化、依赖独立、
制造现场和发布保证十个本体化视角。

EPS-RC17、ENV-01、人物、事故与数值都是合成教学材料，不对应真实企业、产品或
个人，不能作为真实产品结论、合规意见或认证依据。书中条款内容是原创转述；精确
ISO 条款、表格和原文必须回到用户合法持有、受控管理的来源核对。

## 把另一部标准做成书

用 `standard-to-book` 建立私有友好的作者工作区：

```bash
python3 skills/standard-to-book/scripts/init_book.py \
  --slug quality-management \
  --title "质量管理工程导读" \
  --standard "ISO 9001" \
  --output ./workbooks
python3 skills/standard-to-book/scripts/validate_book.py \
  ./workbooks/quality-management --stage structure
```

新书的可读正文、图与来源地图进入书侧；其 executable ontology、CQ、query、shape、
case、rule、contract 与 receipt 必须进入 Semantica 的新 built-in package。作者工作区
里的候选文件不是发布正本。完整合同见
[`docs/ADDING-A-BOOK.md`](docs/ADDING-A-BOOK.md)。

## 把实践长成行业本体

`skills/domain-ontology-loop/` 提供受治理的内化流程：基线快照 → delta → 带理由的
冲突判决 → 版本/PROV commit → 旧 CQ 防遗忘回归。OE 入口只委托 Semantica 的治理
实现，不维护第二套生命周期代码。

## 目录结构

```text
SKILL.md                              # 来源优先的 Agent 工作流
README.md                             # 项目与边界说明
references/
├── ontology-engineering-book/        # 第一卷：唯一书源之一
├── product-trustworthiness-book/     # 第二卷：唯一书源之一
├── source-map.md                     # 第一卷来源地图
├── product-trustworthiness-source-map.md
└── semantica-deep-binding.md         # 唯一运行时契约
demos/                                # Semantica package 教学薄入口
ontology_engineering/                 # 受门禁约束的唯一 bootstrap
runtime/                              # 源码/wheel 锁与本地安装
scripts/                              # 书源检索、评测、隐私与架构门禁
skills/                               # 新书与领域内化工作流
docs/                                 # 接入、权利与发布合同
```

这里刻意没有 OE-local normative package、本体、shape、query 或 fixture 目录；它们的
稳定身份是 Semantica package ID。

## 隐私、来源与发布

公共仓库不接收标准原始 PDF/受限抽取物、企业内部资料、真实项目数据、私人会话、
密钥、个人绝对路径、未清权利图或含身份信息的元数据。发布采用 default deny +
allowlist：

```bash
python3 scripts/check_public_privacy.py --root . --include-ignored
```

扫描通过不等于版权、技术或出版审批通过。当前发布状态见
[`docs/PUBLIC-RELEASE-STATUS.md`](docs/PUBLIC-RELEASE-STATUS.md)，详细边界见
[`docs/PRIVACY-AND-RIGHTS.md`](docs/PRIVACY-AND-RIGHTS.md)。

## 最后一句

> 标准平民化，不是把严谨性删掉；
> 而是把严谨性讲明白，并做成普通工程师能够验证的步骤。

仓库中的代码与文档只能按各自明确登记的许可和权利状态使用。Semantica 源码、
两卷书、图像、标准派生表达和受控来源的权利边界彼此独立；Git 可见或技术上可构建
都不自动等于获得再许可、再发布或标准原文复制权。
