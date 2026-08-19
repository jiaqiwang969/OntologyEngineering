**简体中文** · [English](README.en.md)

# Ontology Engineering：让工程知识可读、可查、可验证

<p align="center">
  <a href="references/ontology-engineering-book/handbook/工程本体论-全书.pdf">
    <img src="docs/assets/engineering-ontology-cover.png" width="320" alt="《工程本体论》第一卷封面">
  </a>
</p>

<p align="center">
  两卷书，一套工程语义方法；从项目证据出发，把可复用知识沉淀为可审计的行业本体。
</p>

<p align="center">
  <a href="references/ontology-engineering-book/handbook/工程本体论-全书.pdf">阅读第一卷</a> ·
  <a href="references/product-trustworthiness-book/handbook/产品可信工程-全书.pdf">阅读第二卷</a> ·
  <a href="https://github.com/jiaqiwang969/semantica">查看 Semantica</a> ·
  <a href="#先读什么">选择阅读路径</a> ·
  <a href="#五分钟体验">五分钟体验</a> ·
  <a href="#technical-governance">技术与治理说明</a>
</p>

工程里真正棘手的，往往不是缺少文件，而是缺少共同语义：讨论的是哪个对象、哪个版本，
证据究竟支持哪项主张，检查结果能说明到哪里，又由谁承担最终决定。

Ontology Engineering 用两卷书讲清观察与建模方法，用项目原生记录守住事实边界，
用 Semantica 让语义可执行、可复算、可记忆，同时把事实接受、风险承担、晋升和发布决定
留给明确的有权人。

## 两卷书，各自回答一个问题

| 卷 | 核心问题 | 你会得到什么 |
|---|---|---|
| [第一卷《工程本体论》](references/ontology-engineering-book/handbook/工程本体论-全书.pdf) | 怎样把模糊的工程语言变成可检验的概念系统？ | 对象与身份、关系、能力问题（CQ）、OWA/CWA、约束、推理、来源、PROV，以及 ontology-guided Agent 的通用方法 |
| [第二卷《产品可信工程》](references/product-trustworthiness-book/handbook/产品可信工程-全书.pdf) | 怎样说明一个产品“为什么值得相信”？ | 以 ISO 26262 本体化推演为贯穿样板，建立主张、身份、治理、情境危害、需求、测量、变化、依赖、现场和保证十类观察镜头 |

第一卷提供通用语法，第二卷展示这套方法怎样进入复杂工程判断。第二卷中的人物、事故、
EPS-RC17、ENV-01 和数值均为合成教学材料；精确 ISO 条款、表格与原文仍须回到用户合法
持有的受控来源核对。本项目不提供官方标准解释、认证或现实产品结论。

## 适合谁

- 需要澄清对象、术语、版本、证据和责任边界的工程师与技术负责人；
- 构建企业知识图谱、行业本体、数字线程或工程知识库的团队；
- 希望让 LLM/Agent 在明确语义、证据与权限内工作的开发者；
- 需要审查“检查已通过”是否真的支持风险、合规或发布主张的评审者；
- 想从方法、案例和可复算语义同时学习本体工程的读者。

## 先读什么

| 你的目标 | 推荐入口 |
|---|---|
| 第一次接触本体工程 | 第一卷第 1–3 章：为什么需要本体、核心概念、怎样从 CQ 开始 |
| 解决 RDF/OWL、约束或推理问题 | 第一卷第 4–5、7 章，并结合对应 Semantica chapter package 复算 |
| 理解 LLM/Agent 如何受语义约束 | 第一卷第 8 章，再看 [`SKILL.md`](SKILL.md) 的语义接入规则 |
| 建立产品可信或功能安全证据链 | 第二卷前言与第 1–10 章，再按问题阅读第 11–20 章的本体回答 |
| 把方法接入真实工程项目 | 先读 [`Semantic Engagement Contract`](references/semantic-engagement-contract.md) |

## 五分钟体验

不安装运行时也可以直接阅读两卷 PDF。要按主题在固定书源中检索，可从仓库根运行：

```bash
python3 scripts/search_ontology_sources.py --scope book \
  "对象身份 identity evidence authority"
```

结果会返回卷、章和仓内来源锚点，便于继续阅读 TeX、Markdown、章节导读或 PDF。

<details>
<summary>体验 source-locked Semantica 发现</summary>

先预检，再安装并审计锁定运行时，最后只读发现可用 package：

```bash
bash runtime/setup_runtime.sh --preflight
bash runtime/setup_runtime.sh
bash runtime/setup_runtime.sh --doctor
runtime/.venv/bin/python scripts/semantic_engagement.py discover
```

缺件、版本不符或哈希不符会 fail closed，不会悄悄切换到另一个 RDF/OWL 后端。

</details>

## 一个极简关系图

```text
两卷书：告诉我们怎样观察、提问和建模 ──┐
项目证据：告诉我们实际发生了什么 ─────┼─→ 一次语义工作会话
有权人：决定什么可以接受、晋升和发布 ──┘          │
                                                   ▼
                                      Semantica：唯一可执行语义
                                                   │
                                                   ▼
                               工程结果 + 语义结果 + 学习判定
```

这四类角色不能互相冒充：书不是事实源，Semantica 不替人承担风险决定，项目记录不会自动
成为行业规律，有权人也不能用口头批准替代可复现的语义检查。

## 深入了解

下面的实现与治理内容默认折叠；普通读者可以先从两卷书开始，需要接入项目或维护仓库时再展开。

<details>
<summary><strong>每次任务的快慢双循环</strong></summary>

### 快速内环

快速内环发生在每一次工程调用中：

```text
任务与项目绑定
  → 从两卷书选择方法镜头
  → 发现已有语义与能力
  → 对齐对象、身份、证据和权限
  → 执行适用检查与获授权的工程工作
  → 返回工程结果、Semantica 结果和学习判定
```

“默认介入”不等于“每次都修改本体”。没有新知识时明确返回 `no_delta`；只有稳定、可复用、
有来源的经验才进入慢速治理外环：

```text
candidate → proposed → committed → regression_passed
          → release_complete → promoted → published
```

这些状态不可跳级。`candidate`、`committed`，甚至技术上的 `release_complete` 都不是公开
发布；`published` 始终需要外部有权人决定。完整外环见
[`domain-ontology-loop`](skills/domain-ontology-loop/SKILL.md)。

</details>

<details>
<summary><strong>书稿、TeX 与 PDF</strong></summary>

两卷 PDF 是正式构建产物，但不是唯一内容正本：

- 第一卷由卷根/章节导读、XeLaTeX 正文、图源和作者工具共同构成；从 Semantica 生成的
  fragments 是受控出版快照，不应手工改成第二套语义真相。
- 第二卷以 preface、20 个 `chapter.md`、4 个 appendix Markdown 和 TeX 装配源为内容正本；
  fragments 由确定性构建生成。
- 作者锁记录每次出版实际消费的源码与资产；PDF 不能替代 Markdown、TeX、图源和锁。

构建入口分别位于
[`第一卷 handbook`](references/ontology-engineering-book/handbook/README.md) 和
[`第二卷 handbook`](references/product-trustworthiness-book/handbook/README.md)；跨书稿、
Semantica 与 PDF 的完整顺序见
[`两卷书作者与 Semantica 收敛工作流`](references/book-authoring-workflow.md)。

</details>

<a id="technical-governance"></a>
<details>
<summary><strong>技术与治理说明</strong>：source lock、29 个章节 package 与 candidate-only 边界</summary>

以下是当前锁定字节所证明的状态，不是对未来分支或公开发布的承诺：

| 项目 | 当前事实 |
|---|---|
| Semantica 运行时 | [`0.6.5+oe.3`](runtime/semantica-source-lock.json)，由 source commit 与 wheel SHA-256 精确锁定；doctor 还逐文件核验 wheel `RECORD` 与实际 import root |
| 可执行语义 | ontology、CQ、SHACL、query、受支持 rule、cases、contract、PROV、receipt 与生命周期只在 Semantica 中保留正本；OE 没有第二 backend、fallback 或平行 registry |
| 章节 packages | 共 29 个：第一卷 9 个、第二卷 20 个。第一卷 ch06 为 `absent`，其余 28 个为 `partial`；29 个全部 `release_status=blocked` |
| 规范派生 package | 另有一个 `semantica.chapter_packages.vol2.normative` domain package，当前同为 `partial/blocked`；它不是 ISO 原文副本或合规意见 |
| 两卷 book artifact v1 | 永远只能形成技术 `candidate`。rights/publication 记录只接受 `pending` 或 `blocked`；无签名 JSON、测试绿色或 package receipt 都不能授权公开发布 |

关键合同与状态入口：

- [`Semantic Engagement Contract`](references/semantic-engagement-contract.md)：任务绑定、证据、权限、三联输出与失败语义；
- [`Semantica source lock`](runtime/semantica-source-lock.json)：当前 commit、版本、wheel 与复验基线；
- [`两卷 artifact v1 证据合同`](references/release-evidence/README.md)：candidate-only 技术闭环；
- [`公共发布状态`](docs/PUBLIC-RELEASE-STATUS.md)：当前整体状态为 `BLOCKED`；
- [`隐私、来源与公开发布`](docs/PRIVACY-AND-RIGHTS.md)：default deny + allowlist 边界；
- [`新增一本书`](docs/ADDING-A-BOOK.md)：把合法取得的标准转化为书与 Semantica package 的流程。

### 维护者最小门禁

```bash
runtime/.venv/bin/python scripts/check_semantica_backend_policy.py \
  --root . --policy runtime/semantica-backend-policy.json --mode strict --json
runtime/.venv/bin/python -m pytest -q tests
```

### 仓库导航

```text
SKILL.md                         默认语义接入与路由
ontology_engineering/            source-locked Semantica 适配层
runtime/                         wheel/source lock、安装与 doctor
references/ontology-...-book/    第一卷书源、TeX、图与 PDF
references/product-...-book/     第二卷书源、TeX、图与 PDF
references/                      来源地图、合同与发布证据
skills/domain-ontology-loop/     行业本体治理外环
skills/standard-to-book/         标准到书的受控作者流程
scripts/                         检索、语义会话与门禁
tests/                           合同与回归测试
```

</details>

> 两卷书告诉我们怎样看，项目证据告诉我们发生了什么，Semantica 让语义可执行且可记忆，
> 有权人决定什么可以被接受、晋升和发布。
