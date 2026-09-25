**简体中文** · [English](README.en.md)

# Ontology Engineering：让工程知识可读、可查、可验证

说“用工程本体”即可从当前情景开始。助手先续接目标、对象、证据和约束，再选择
需要的内部模块、外部协同、本体建模或 Semantica 路径，按依赖组织本轮工作。
[情景路由说明](references/context-routing.md)区分能力需求、工具可用性和执行权限：
触发根 skill 后默认调用 Jev，助手读取候选来组织工作；失败时明确说明并继续处理。
资料批量判断是另一种按需使用方式，接口可用与实际调度效果分别验证。

**内含完整的 [cad-agent CAD 模块](skills/cad-agent/SKILL.md)，把零件、装配与夹具设计同加工、焊接、检验和成本结合。**
可以直接提出 CAD 设计任务；涉及制造约束时，CAD 与工艺模块共享同版对象和证据，双向调整结构与制造路线。
功能本体明确“为谁实现什么”，第一性原理推导物理约束，工艺本体表达制造过程与状态变化；
共同连接功能、结构、公差、工艺、检验和成本，按已取得的证据持续修订。

也可以从一个尚未成形的工程想法或研究问题开始：保留原始意图，按必要功能与机理比较
候选，围绕证据缺口安排验证、资源、预算和进度，并随反馈修订同一方案。
这些能力进入[根 skill 的工程决定内环](SKILL.md)，历史项目见[记录接续说明](docs/project-method-integration.md)。

**从[制造协作入口](docs/MANUFACTURING-COLLABORATION.md)开始，把群里的生产问题持续跟进下去。**
员工不断提供试制、工艺、设备、供应商和现场问题，助手逐步建立私有项目本体，输出方案，
再随实际反馈深入分析和修订。项目资料与本体留在私有工作区，通用 skill 保存可复用方法。

日常调用与首次准备见[使用说明](docs/USAGE.md)。普通分发包不含凭据，Jev 使用接收者的本地配置。

当前版本 **0.5.9** 将预写工程本体 instruct、默认 Jev 情景路由与项目方法融合纳入完整分发，见[更新说明](docs/releases/manufacturing-method-0.5.9.md)。完整版和核心版都从根 skill 调用，包的区别见[分发说明](docs/PORTABLE-DISTRIBUTION.md)。Jev 的独立质量和真实收益仍需项目实践验证。

<p align="center">
  <a href="references/ontology-engineering-book/handbook/工程本体论-全书.pdf">
    <img src="docs/assets/engineering-ontology-cover.jpg" width="320" alt="《工程本体论》第一卷封面">
  </a>
</p>

<p align="center">
  两卷书提供理论，制造方法连接客户沟通、工艺方案与成本核算，Semantica 承载可执行语义。
</p>

<p align="center">
  <a href="references/ontology-engineering-book/handbook/工程本体论-全书.pdf">阅读第一卷</a> ·
  <a href="references/product-trustworthiness-book/handbook/产品可信工程-全书.pdf">阅读第二卷</a> ·
  <a href="#manufacturing">制造工艺与成本</a> ·
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

随附的[生产制造工艺与成本管理模块](skills/manufacturing-process-cost/SKILL.md)把这套方法
用于离散机械制造：从不完整的询价、图纸、工艺总结、设备台账和报价开始，逐步澄清
隐含条件，形成有依据的方案，再把客户和现场反馈接回下一轮。

并入的 [CAD Agent 模块](skills/cad-agent/SKILL.md)按[通用 CAD 工作流](skills/cad-agent/references/cad-engineering-workflow.md)
处理零件、图纸/BOM、装配、机构与原生回读证据，再经[CAD／工艺双向交接](skills/cad-agent/references/cad-process-integration.md)
把相关对象和版本变化送到制造判断。便携版提供本机 Fusion 守卫及可配置 NX/AutoCAD 远程桥；接收方提供自己的 CAD 软件、许可、主机及 NX sidecar，真实设备另做连接验收。
工艺模块接续必要功能、失效路径、检验、成本和交期；正式语义裁定仍由 Semantica 执行。

## 两卷书，各自回答一个问题

| 卷 | 核心问题 | 你会得到什么 |
|---|---|---|
| [第一卷《工程本体论》](references/ontology-engineering-book/handbook/工程本体论-全书.pdf) | 怎样把模糊的工程语言变成可检验的概念系统？ | 对象与身份、关系、能力问题（CQ）、OWA/CWA、约束、推理、来源、PROV，以及 ontology-guided Agent 的通用方法 |
| [第二卷《产品可信工程》](references/product-trustworthiness-book/handbook/产品可信工程-全书.pdf) | 怎样说明一个产品“为什么值得相信”？ | 以 ISO 26262 本体化推演为贯穿样板，建立主张、身份、治理、情境危害、需求、测量、变化、依赖、现场和保证十类观察镜头 |

第一卷提供通用语法，第二卷展示这套方法怎样进入复杂工程判断。第二卷中的人物、事故、
EPS-RC17、ENV-01 和数值均为合成教学材料；精确 ISO 条款、表格与原文仍须回到用户合法
持有的受控来源核对。本项目不提供官方标准解释、认证或现实产品结论。

## 适合谁

- 需要看懂工单、按当前工艺做事、核对结果和交班的现场员工；
- 接到新客户询问，需要判断能否制造、怎样组织工艺、如何核算成本的制造企业负责人；
- 需要澄清对象、术语、版本、证据和责任边界的工程师与技术负责人；
- 构建企业知识图谱、行业本体、数字线程或工程知识库的团队；
- 希望让 LLM/Agent 在明确语义、证据与权限内工作的开发者；
- 需要审查“检查已通过”是否真的支持风险、合规或发布主张的评审者；
- 想从方法、案例和可复算语义同时学习本体工程的读者。

## 先读什么

| 你的目标 | 推荐入口 |
|---|---|
| 从工程想法或未完成研究走到候选、验证计划和方案 | [工程本体入口](SKILL.md)与[决定、证据和活动的方法](skills/engineering-evidence-methods/references/method-internalization.md)：保留版本、条件和未决项，按依赖组织投入 |
| 跟进群里的试制、工艺、设备、供应商及现场问题 | [制造协作入口](docs/MANUFACTURING-COLLABORATION.md)：用日常语言开始，持续建立项目本体并迭代方案 |
| 第一次接触本体工程 | 第一卷第 1–3 章：为什么需要本体、核心概念、怎样从 CQ 开始 |
| 解决 RDF/OWL、约束或推理问题 | 第一卷第 4–5、7 章，并结合对应 Semantica chapter package 复算 |
| 理解 LLM/Agent 如何受语义约束 | 第一卷第 8 章，再看 [`SKILL.md`](SKILL.md) 的语义接入规则 |
| 建立产品可信或功能安全证据链 | 第二卷前言与第 1–10 章，再按问题阅读第 11–20 章的本体回答 |
| 把方法接入真实工程项目 | 先读 [`Semantic Engagement Contract`](references/semantic-engagement-contract.md) |
| 设计或核对零件、图纸、装配、机构及制造功能 | [CAD Agent 模块](skills/cad-agent/SKILL.md)和[通用 CAD 证据工作流](skills/cad-agent/references/cad-engineering-workflow.md)：核对对象身份、原生来源、关系、主张与变化影响；需要时接入[工艺交接](skills/cad-agent/references/cad-process-integration.md) |
| 从图片、视频或专利反推形态与机构 | [证据驱动的重建方法](skills/cad-agent/references/evidence-driven-reconstruction.md)：先辨认可见事实，再用物理、机构和数学条件推导筛选候选，并说明不能辨识的内部关系 |
| 从客户询问推进制造工艺、成本与反馈迭代 | [制造工艺与成本管理模块](skills/manufacturing-process-cost/SKILL.md)：脱敏演变案例、工作规范、记录模板与 XeLaTeX 报告组件 |

<a id="manufacturing"></a>

## 制造工艺与成本：让沟通、方案和反馈形成闭环

当前方法版本为 **0.5.9**，适用于离散机械制造。行业、材料、工艺参数与验收值由各项目
自己的证据确定；可复用的是发现问题、建立关系、比较方案和验证判断的方法。

```mermaid
flowchart LR
    A["客户询问与高价值资料"] --> B["情境差异与隐含条件"]
    B --> C["工艺、检验与资源方案"]
    C --> D["同口径成本估算"]
    D --> E["客户与现场反馈"]
    E --> B
    E --> F["复盘、脱敏、验证与采用"]
    F --> A
```

| 当前要解决的问题 | 方法与可复用入口 |
|---|---|
| 资料很多，哪些会改变判断？ | [高价值资料与对话筛选](skills/manufacturing-process-cost/references/high-value-documents.md)：保留来源、条件、冲突和反证，围绕本轮决定继续提问 |
| 旧工艺能否移植，精度和设备要求从哪里来？ | [要求怎样影响工艺与成本](skills/manufacturing-process-cost/references/requirements-process-cost.md)、[从 STEP 到加工路线](skills/manufacturing-process-cost/references/step-to-process.md)：连接关键特性、加工状态、装夹、检验和设备功能 |
| 自制、外协、设备复用或自研，哪种方案合适？ | [估算依据与批次费用](skills/manufacturing-process-cost/references/estimate-basis.md)、[十二项判断方法](skills/manufacturing-process-cost/references/decision-patterns.md)：统一数量和费用范围，区分报价、估算、目标与实测，安排能改变选择的验证 |
| 怎样把依据做成可复核的方案？ | [XeLaTeX 报告组件](skills/manufacturing-process-cost/references/report-design.md)、[五类工艺图模板](skills/manufacturing-process-cost/references/process-flow-templates.md)及[图册预览](skills/manufacturing-process-cost/assets/report-template/flowcharts/preview.pdf) |
| 客户纠正或版本变化后，哪些结论需要重查？ | [连续演变案例](skills/manufacturing-process-cost/references/case-evolution.md)、[工作规范](skills/manufacturing-process-cost/references/work-norms.md)与[组织闭环](skills/manufacturing-process-cost/references/organization-loop.md)：保留改变理由，追查依赖，接上责任和下一轮 |

模块配有[八类记录模板](skills/manufacturing-process-cost/SKILL.md#按需要取用)、
[六张可选评审卡](skills/manufacturing-process-cost/assets/templates/decision-review-cards.md)，
以及[四轮报告输入和五个后续决策练习](skills/manufacturing-process-cost/assets/report-template/example/README.md)。
案例展示信息逐步补齐、结论修订和问题重开的过程；报告按当前问题选择视图，
A/B/C 等历史版号不代表固定的制造阶段。

STEP 到工艺目前提供分析与复核方法；报告组件消费已经整理的记录，生成原生 TeX，
并可编译成 PDF。工艺图按项目记录编辑，真实设备能力和实际节约仍由现场证据确认。
通用内容保留因果关系与反例，客户原件、身份、真实报价、模型和对话留在项目私有工作区。

## 五分钟体验

先获取完整仓库，再从根目录运行示例：

```bash
git clone https://github.com/jiaqiwang969/OntologyEngineering.git ontology-engineering
cd ontology-engineering
```

不安装运行时也可以直接阅读两卷 PDF，或按主题检索固定书源：

```bash
python3 scripts/search_ontology_sources.py --scope book \
  "对象身份 identity evidence authority"
```

结果会返回卷、章和仓内来源锚点，便于继续阅读 TeX、Markdown、章节导读或 PDF。

使用 Python 3.10+，从合成输入生成第一份报告的 TeX 与冻结记录，并检查输入到输出的对应：

```bash
python3 scripts/manufacturing_report.py generate \
  --input skills/manufacturing-process-cost/assets/report-template/example/03-resource-cost.json \
  --output ../work/manufacturing-report-001
python3 scripts/manufacturing_report.py verify ../work/manufacturing-report-001
```

输出目录需尚不存在，并位于 skill 之外。生成 PDF 时加 `--compile`，需安装 XeLaTeX
及常规中文字体包、Poppler；依赖与逐页检查步骤见[报告使用说明](skills/manufacturing-process-cost/references/report-design.md)。
上述命令检查记录映射、文件摘要和费用算术；当前报告快照的工程语义须另外通过 Semantica 核验。

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

## 独立分发

制造方法 `0.5.7` 增加证据驱动的形态与机构重建方法和第一性原理推导记录，详见[更新说明](docs/releases/manufacturing-method-0.5.7.md)；`0.5.6` 的远程 CAD、装配与力学能力继续保留。

| 组件 | 当前版本与范围 |
|---|---|
| 制造方法 | `0.5.9`；通用 CAD 证据、形态/机构逆向重建、远程桥、装配/机构有界筛查、制造工艺与成本 |
| 记录模板 / 评审卡 | 两份模板为 `0.2.1`、其余为 `0.2.0` / 评审卡 `1.4.1` |
| 报告组件 / 工艺图 | `1.0.0`；XeLaTeX 与五类可编辑 TikZ 模板 |
| 冻结制造语义包 | `0.1.1`；141 项声明资产、68 个合成场景、23 个 CQ；保留本地技术候选状态 |

分发和使用时保留整个 `ontology-engineering/` 根目录，制造模块的理论引用、案例资产与
运行入口均在目录内；单独复制子模块会缺少这些依赖。
按 [独立分发与新目录验证](docs/PORTABLE-DISTRIBUTION.md) 检查、打包并重跑，
无需访问原客户项目或作者工作区。首次安装 Python 依赖仍可能需要包源。

[0.5.7 公开资产台账](https://github.com/jiaqiwang969/OntologyEngineering/releases/download/manufacturing-v0.5.7/ontology-engineering-core-v0.5.7-assets.json)、上一版制造方法的[冻结清单](https://github.com/jiaqiwang969/OntologyEngineering/blob/b5b148333a39b69aaa8b5b521de8242805cc3838/docs/releases/manufacturing-method-0.5.3.json)、
[本次 README 更新清单](docs/releases/manufacturing-readme-0.5.3-r1.json)与
[语义包 NOTICE](runtime/vendor/MANUFACTURING-NOTICE.md)分别保留。文件和合成案例检查的
范围见分发说明；制造方法的公开授权与[两卷书整体发布状态](docs/PUBLIC-RELEASE-STATUS.md)分开记录。

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

下面的实现与治理内容默认折叠；项目实践从制造协作入口开始，学习理论可读两卷书，维护者按需展开。

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
| Semantica 运行时 | [`0.6.5+oe.6`](runtime/semantica-source-lock.json)，由 source commit 与 wheel SHA-256 精确锁定；doctor 还逐文件核验 wheel `RECORD` 与实际 import root |
| 可执行语义 | ontology、CQ、SHACL、query、受支持 rule、cases、contract、PROV、receipt 与生命周期只在 Semantica 中保留正本；OE 没有第二 backend、fallback 或平行 registry |
| 章节 packages | 共 29 个：第一卷 9 个、第二卷 20 个。第一卷 ch06 为 `absent`，其余 28 个为 `partial`；29 个全部 `release_status=blocked` |
| 规范派生 package | 另有一个 `semantica.chapter_packages.vol2.normative` domain package，当前同为 `partial/blocked`；它不是 ISO 原文副本或合规意见 |
| 冻结制造案例 | `semantica.manufacturing.process-cost-loop@0.1.1` 随根目录传输，由 Semantica 执行；68 个合成场景的回放范围独立于章节包、新项目报告及行业晋升 |
| 两卷 book artifact v1 | 永远只能形成技术 `candidate`。rights/publication 记录只接受 `pending` 或 `blocked`；无签名 JSON、测试绿色或 package receipt 都不能授权公开发布 |

关键合同与状态入口：

- [`Semantic Engagement Contract`](references/semantic-engagement-contract.md)：任务绑定、证据、权限、三联输出与失败语义；
- [`Semantica source lock`](runtime/semantica-source-lock.json)：当前 commit、版本、wheel 与复验基线；
- [`两卷 artifact v1 证据合同`](references/release-evidence/README.md)：candidate-only 技术闭环；
- [`两卷书整体发布状态`](docs/PUBLIC-RELEASE-STATUS.md)：当前为 `BLOCKED`；制造方法的授权上传见上文独立清单；
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
runtime/vendor/                  锁定运行时与冻结制造案例传输包
references/ontology-...-book/    第一卷书源、TeX、图与 PDF
references/product-...-book/     第二卷书源、TeX、图与 PDF
references/                      来源地图、合同与发布证据
skills/domain-ontology-loop/     行业本体治理外环
skills/standard-to-book/         标准到书的受控作者流程
skills/cad-agent/                原生 CAD、Fusion 执行与制造双向证据交接
skills/manufacturing-process-cost/ 制造方法、脱敏案例、记录与报告/工艺图模板
docs/PORTABLE-DISTRIBUTION.md     整根分发与接收方验证
docs/releases/                   按版本保留的更新说明与公开清单
scripts/                         检索、报告生成、案例回放、分发与门禁
tests/                           合同与回归测试
```

</details>

> 两卷书告诉我们怎样看，项目证据告诉我们发生了什么，Semantica 让语义可执行且可记忆，
> 有权人决定什么可以被接受、晋升和发布。
