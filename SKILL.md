---
name: ontology-engineering
metadata:
  version: "0.7.0"
description: Interpret engineering requests in context and coordinate the needed CAD, manufacturing, evidence, ontology modeling and Semantica workflows. Develop ideas, compare candidates and revise decisions from source-bound evidence and feedback. Use for engineering-ontology requests, cross-skill engineering collaboration, domain knowledge modeling or Semantica integration. Source-locked Semantica remains the sole executable semantic authority; the two engineering books guide the method.
---

# 工程本体：按情景组织工程推理与协同

从功能和物理约束推导设计与制造方案，再用原生证据和现场反馈校正：

- **功能本体**：服务对象、使用情境、必要功能、输入输出、功能之间的依赖与失效后果；
  明确由哪个结构、界面或系统实现，以及怎样判断功能成立。
- **第一性原理**：按问题选用守恒关系、受力与变形、传热流动、材料行为和几何约束，
  显式写出边界条件、假设、量纲与可检验的推导，再确定需要的精度、裕量和验证。
- **工艺本体**：毛坯与中间态、工序前后条件、基准与装夹、设备刀具、连接与表面处理、
  检验及成本；关联每道工序实现或影响的功能，追踪顺序、余量和变形的后果。

主线是“功能要求 → 物理机理与约束 → 结构与公差 → 工艺与装夹 → 检验与成本”，
现场结果沿依赖关系反馈到上游。按本轮问题选择必要的分析、仿真或试验；已明确停止的
仿真不因方法讨论重新启动。上述是建模与推理方法；具体可执行本体、规则和覆盖范围
须经 Semantica 发现、绑定和验证，缺失项保留为候选，不能宣称全部已实现。

## 先理解情景，再组织工作

用户说“调用工程本体”时，先续接当前目标、待决问题、对象版本、已有结果、资料、
约束和已授权范围，再判断本轮需要什么产出与能力。显式点名的 skill 纳入工作，
其他模块按实际需要选择；保留此前的停止指令及尚未解除的条件。

按[本轮目标与工程迭代](references/engineering-iteration.md)，默认由本体根据背景提出
本轮要解决的问题、交付物与验收条件，与用户对齐后推进。已有明确任务即作为已对齐目标，
普通执行步骤自主继续；实质改变目标、验收或范围时再对齐。

按[情景分析与能力协同](references/context-routing.md)匹配内部模块、可用外部 skill
和工具。可单项执行，也可围绕共同对象和证据联动；用输入、输出和前置依赖解释
为什么需要它们。概念解释、项目记录、本体建模、现有语义审查、Semantica 接入和
行业本体演化分别判断，不把一次调用展开成全部流程。小任务可由根入口直接完成。

网页子任务可使用内置的 [Jev Ultrafast 工具](references/jev-browser-tool.md)：固定源码、
独立运行环境和可调用入口，由 Jev 多轮选择观察到的网页动作。按用户指定的浏览器方式
和账号范围接入，结果回到当前工程目标验收；不以网页循环代替整体工程循环。
供应商资料取得时，把当前用途、已有证据、剩余缺口和页面实际选项交给 Jev；由它选择
下一项模型、图纸、规格或采购资料。不得在适配器里把目标固定成 STEP/AP203。
文件与内容核验后再反馈下一轮选择，执行详情见 CAD 模块的 MISUMI 指南。

需要供应商目录里的原理、选型、计算边界、装配或失效知识时，走
[供应商工程知识入口](references/supplier-knowledge.md)：由当前项目生成对象、功能、
条件和缺口快照，调用统一入口 `scripts/jev_knowledge.py`。来源适配与筛选实现内置于
本 skill，米思米是其中的供应商资料源，无需独立 MISUMI 应用。原件经 Drive 单独传输后，归回当前 skill 的 `sources/books/` 与 `sources/misumi/`；
索引在 `sources/.indexes/`。代码、方法和下载后的资料组成同一本地目录；Git 与核心
ZIP 排除 `sources/` 和 `var/`。按[下载位置与接入](docs/PORTABLE-DISTRIBUTION.md)校验和登记，
查询只恢复所需原页。
Jev 按本轮决定选择主题，
返回带原页、片段和条件线索的候选；主 agent 读入后核适用性，把采用理由与下一验证
动作写回原项目。知识检索也可独立服务概念讨论，不因查目录而启动 CAD。
查询历史用于追溯，项目台账继续是当前状态正本；来源陈述不自动变成项目事实或 TBox。
知识链的 Jev 判断共用根 instruct 与[书源方法解释](references/supplier-knowledge-interpretation.md)，
原页筛选之后再依据返回内容提出核查与交接候选；读入该结果，保留条件、反例和未知，
把采用范围接回当前决定。

资料融合按[统一架构与升级计划](docs/releases/mechanical-design-next.md)组织：第一卷指导建模，
第二卷提供工程本体化的方法示范，机械教材补充领域机理与方法，供应商目录提供具体
实现与配置边界。先保持整书领域覆盖地图，再按工程问题跨书凝练；复用既有
[五组工程设计模式](references/judgment-pattern-source-map.json)，把对象、关系、条件、反例和
CQ 接到同一项目的决定、计算、CAD、工艺及成本依据。Jev 协助模式匹配、领域细化和
概念对齐；生成候选与正式 Semantica 语义分别验证。来源材料不直接成为项目事实。
每项采用可回到原文档、版次、物理 PDF 页及图表公式；原页图片按需生成，局部图保留
整页上下文。相同陈述合并时保留来源和条件，精简不删掉反例或查证能力。全文、原图、
索引与项目记录分离保存；[两页示例](examples/knowledge-distillation/README.md)只演示方法，
不限定教材覆盖或用户项目。整书索引、候选映射与完成本体化分别报告。
同一知识入口现可登记本地教材 PDF，与米思米共用查询、Jev 审阅和证据包；原页按需
渲染，工程交接可核对当前决定与对象身份。扫描正文仍需提取／看图；目录地图、文字
检索和身份一致不等于完成教材本体化或知识到 CAD、工艺及成本的适用性采用。

用户纠正、重复失败或任务检查点出现有价值的新经验时，按
[经验复盘与做梦式整理](references/practice-consolidation.md)回看证据、请 Jev 提出处置、
由主 agent 复核后维护对应指南，并验证下一任务能否实际采用。操作经验更新与正式
本体晋升分别报告；当前是任务内执行方法，没有自动启用夜间常驻调度。

Jev 的判断必须使用预先维护的[工程本体 instruct](references/context-routing-instructions.json)：
意图与 CQ、对象身份、功能机理、条件证据、ABox/TBox 和交接依赖共同约束能力选择。
每次用户触发本 skill，都先按[情景路由调用](references/context-routing.md#默认调用)
运行 `scripts/route_engineering_task.py`，让 Jev 对本轮能力需求作出候选判断，再实际
读取结果来选择模块和组织协同。输入由 agent 从上下文整理，用户无需填写；保留先前
状态、最新请求和明确约束，只发送本轮已授权向服务提供的必要情景。简单任务也运行
这一步，不因为已有主观判断而省略。Jev 不能用分数增加权限或替代正式语义审查。

主 agent 核对候选与实际输入、来源、能力和授权后继续执行。分歧或模型漏选须按证据
修正并简记理由；不能只保存路由报告而不使用。服务失败、用户禁止外发或必要上下文
不可提供时，记录 `unavailable`／未调用的原因并由主 agent 接续，不能冒充 Jev 已判断。
这是默认的 Jev 辅助编排；[资料批量判断](docs/judgment-intake.md)按实际材料规模另行使用。

新反馈到来时，重审受影响的选择和交接，续用项目身份与历史；只有任务本身不明确时
才补最小澄清，不让用户先挑内部模块或填写技术合同。

本 skill 内含完整的 [cad-agent CAD 模块](skills/cad-agent/SKILL.md)，负责零件、装配、
机构、夹具、STEP、图纸及 NX 原生建模与回读。CAD 默认通过 NXOpen/Journal 直接执行，
不使用 CAD MCP；标准件首选米思米中国。机械工程、装配、仿真及证据方法保留。用户仍可直接说
“用 cad-agent 设计夹具”或“检查 STEP”；从本入口进入 CAD 模块执行。
Fusion 已退出当前工具范围，不作为备用执行器；历史教材只用于机械方法与来源追溯。

融合让 CAD 与[制造工艺和成本](skills/manufacturing-process-cost/SKILL.md)共享对象、
版本、功能要求和证据：设计中的壁厚、公差、孔位与连接决定装夹、加工、焊接和检验；
制造能力、变形、刀具可达性及现场反馈反过来推动结构调整。涉及这些相互影响时，按
[CAD／工艺双向交接](skills/cad-agent/references/cad-process-integration.md)联动两个模块。
单项 CAD 任务直接用 CAD 工具；Semantica 负责正式语义检查，原生模型和加工验证各自取证。

下一版以机械设计完整链路为升级主线，范围和有界案例验收见
[机械设计链路发布草案](docs/releases/mechanical-design-next.md)。供应商知识、选型计算、
NX、制造检验与反馈继续由本入口统筹；已接通的接口和仍待验证的交接分别标明，
不以资料取得或单项工具完成代替整条设计链的验收。

把每次调用视为一次 source-locked semantic engagement。Semantica 默认以锁感知发现介入，
正式执行取决于本轮问题、证据及绑定是否适用；发现、接入配置和正式审查分别报告。
让两卷书指导怎样观察与解释，让工程实践产生事实，并只把经过治理的稳定经验晋升为
行业本体。不要把本 skill 降成查书插件，也不要让它静默自我修改。

## 从想法持续推进工程决定

收到工程设想、研究课题或尚未定型的方案时，先续接用户原始意图、当前待决问题、
对象版本与已有记录。沿“必要功能 → 物理机制 → 实现条件 → 候选 → 证据缺口 →
验证活动 → 有范围的决定”展开；根据已有证据往返修订，活动顺序由依赖决定。
按[共用方法中的决定与验证关系](skills/engineering-evidence-methods/references/method-internalization.md)
把功能要求、反例和下一步投入连接起来：

- 对每个候选说明它在什么条件下实现哪些功能，哪些主张已有支持、受反驳或仍未知。
  优选、暂缓和排除都绑定决定、对象版本、适用范围与理由，保留替代路线。
- 下一次查资料、计算、仿真或试验应回答会改变决定的问题；明确输入、对照、输出、
  有效条件及不同结果后的动作。资源已列出、能预约、能力已验证和获准使用分别取证。
- 用这些活动的前置条件、人员设备占用、等待时间和费用形成计划；需求、预算或资源
  变化后只重审有关系的工作，同时列出依赖尚未查清的范围。
- 把方案书、试验计划、预算和进度作为同一项目记录的视图。工程支持状态、活动完成、
  文件发布与 Semantica 生命周期分别记录，不用一个项目总状态代替。

已有项目使用原来的台账、来源、事件和稳定 ID；尚无记录时，按需要复用
[缺口、方案与组织记录](skills/manufacturing-process-cost/references/organization-loop.md)，
无需先建立整套数据库。用户提供日常描述，agent 负责记录与来源关联。新消息先区分
需求、观察、建议、假说、决定或表达偏好；只有有依据的变化才更新相应对象。
小任务只处理当前决定，不强制生成预算、排期、正式报告或新本体版本。

历史 `build-engineering-project/1.0` 项目按[记录接续与迁移说明](docs/project-method-integration.md)
保留来源和快照；当前工作直接从本入口继续，不调用旧状态机或旧独立本体检查器。

## 从生产问题持续协作

微信群或其他渠道持续讨论新品试制、工艺/流程选择、设备、供应商、质量、成本及现场问题时，
先读[制造协作入口](docs/MANUFACTURING-COLLABORATION.md)。续接当前问题与方案，沿新证据
完善项目本体，再更新有依据的方案与下一验证动作。项目本体、客户资料、对话和实际参数
只留在本 skill 的 `var/projects/` 私有工作区，不写入可分发方法与示例。
员工提供日常描述及手头资料即可；agent 负责来源、记录、绑定和工具调用，不要求员工
先学本体或手填 JSON。技术回执与学习记录附后可查；现场任务卡只是按需输出的一种视图。

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

CAD 操作按 [NX 直连执行](skills/cad-agent/references/nx-execution.md)，不启动旧 CAD MCP。
正式语义仍只转交本 skill 的 source-locked Semantica 入口；不恢复已退役的独立
`cad-agent-semantic` 后端。

批量对话、报告或多版工程资料需要归类和支持关系初判时，按
[共用批量判断与证据审查](docs/judgment-intake.md)接入 Jev。两卷书指导模式与提问；
Jev 只产生候选，Semantica 执行已实现的身份、主张、范围、功能覆盖及依赖检查。
模型的支持与反驳都不能直接成为已采用断言；未知、失败及必查义务须保留。
小任务直接使用专业工具与既有语义审查，无须为了调用模型而拆成批次。

## 开始任何任务

1. 从本 `SKILL.md` 所在目录解析 skill root；不要写死用户主目录或依赖当前工作目录。
   所有本 skill 自有的原件、索引、项目、状态、缓存和维护产物都留在该根目录内；
   原件与资料索引用 `sources/`，项目用 `var/projects/`，状态与缓存用 `var/state/`、
   `var/cache/`，维护与构建用 `var/maintenance/`、`var/builds/`。不在 `~/.local`
   建第二套数据目录，不用指向外部的符号链接替代归位。Git 和发布包排除两个本地子树；
   共享系统程序、NX 安装和账号认证由环境提供，不复制到技能的可分发文件。
2. 续接情景并运行上述默认 Jev 路由，读取 `routing.json` 后安排本轮工作；随后读取
   `references/semantic-engagement-contract.md`，如果任务涉及工程应用、跨 skill
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
   engagement。缺少绑定时记录所需字段和 blocker，不猜 package、事实源或权限；
   由 agent 根据已有授权和证据准备，确需补充的项目设置交给维护者。只阻断依赖该绑定的
   正式执行，继续已授权的资料整理、条件分析和问题准备，不把技术字段交给现场员工填写。

## 每次调用的快速内环

沿“当前决定 → 所需能力 → 有源输入 → 适用检查 → 工程工作 → 证据与反馈”推进。
正式执行使用统一入口，自动注入 runtime source lock：

```bash
runtime/.venv/bin/python scripts/semantic_engagement.py open \
  --binding var/projects/<project>/workspace-binding.json \
  --task var/projects/<project>/task-envelope.json \
  --workspace var/projects/<project>/semantica-registry
```

先读[统一合同](references/semantic-engagement-contract.md)的对应操作；其中维护
package/workspace、fresh task、exact action、证据绑定、幂等恢复和生命周期权限。
项目 `review` 使用已晋升 package 与本轮有界 ABox，候选作者测试不能替代它。
查询和语义通过不能替代原生回读、数值校核或实物验收。

## 三联结果

每次都分别保留以下结果。现场任务先用日常语言交付动作、检查与未决项，
将完整技术身份和回执放在同一次任务的附后记录或链接，不省略未运行和未知状态：

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

仅在用户要求修改、校正或重新出版两卷书时，取得作者源码并读取
`references/book-authoring-workflow.md`；技术候选证据见作者源码中的
`references/release-evidence/README.md`。作者锁、生成片段、跨仓收敛和出版命令均由
该流程维护。下载的 PDF 可用于查证，不能替代 TeX／Markdown 作者正本。
书稿、可执行语义、技术候选和公开发布分别验证；不通过编辑无签名记录取得发布权限。

## 新标准与跨行业复用

当任务涉及从不完整客户询问形成制造工艺与成本方案、情境迁移、隐性知识发现、
设备复用、自制外协比较或方案反馈时，加载
[manufacturing-process-cost](skills/manufacturing-process-cost/SKILL.md)。该模块提供脱敏
演变案例、工作规范、项目记录模板和从冻结记录生成的 XeLaTeX 报告组件；实例留在
私有项目，执行语义仍由 Semantica 负责。报告组件的输入与视图检查不能代替工程语义核验。

制造方法与冻结案例的独立分发见 [分发说明](docs/PORTABLE-DISTRIBUTION.md)。冻结的
Semantica 传输包随根目录携带，由运行时解释；它不是 OE-local 作者正本或平行 registry。
不得把仅有候选索引、原机路径可用或文件检查通过称为接收方语义执行通过。

当用户要把另一部合法取得的标准做成书时，加载 `skills/standard-to-book/SKILL.md`。
新书正文、图和来源地图属于书侧；完整 executable package 和 promotion 属于 Semantica。

CAD 建模、修复、图纸/BOM、装配、机构或几何核验加载
[CAD Agent 模块](skills/cad-agent/SKILL.md)。先按其[通用 CAD 工程工作流](skills/cad-agent/references/cad-engineering-workflow.md)
区分定义、实例、特征、界面、基准、配合、机构状态与来源，再用通用 CAD 证据合同形成
带来源的项目 ABox 和变更影响；它不拥有第二套正式语义执行权。工艺选择依赖 CAD
对象，或候选工艺反过来要求 CAD 核查可达性、公差、配合、运动包络等条件时，按
[CAD／工艺双向证据交接](skills/cad-agent/references/cad-process-integration.md)
把确切对象链到功能、路线、主张与问题；制造模块接续功能覆盖、检验、成本与交期。
证据转换器只核对身份与来源，不出具 Semantica 或实物放行结论。

装配及相关力学任务另读[装配与力学通用判断内核](skills/cad-agent/references/assembly-and-physics-kernel.md)：区分实例、界面、逐动作状态、外部支撑、装入路径、载荷需求、接头容量和实物证据。具体项目的搭建步骤、材料数值与仿真算法保留在项目 ABox；可迁移的是主张之间的因果关系及反证边界。

从照片、视频、专利或残缺 CAD 反推外形、零件关系和工作原理时，另读[证据驱动的形态与机构重建](skills/cad-agent/references/evidence-driven-reconstruction.md)。可见内容、机理候选、第一性原理推导、本方 CAD 方案和原物/实物结论分层；条件推导用于收窄缺口，不把欠定的隐藏结构伪装成观察事实。

EDA、质检、仿真和其他制造 skill 在三个检查点调用本 skill：任务开始的语义
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

## 工程证据适用性

CAD、工艺、仿真、测量或成本结论依赖来源证据时，使用[工程证据与方法模块](skills/engineering-evidence-methods/SKILL.md)。它把数量参考态、模型保真、因果对照、观测机会、独立验证、检查覆盖、物料口径及证明适用桥交给 Semantica 检查；来源完整、支持义务满足和实物成立分别报告。
