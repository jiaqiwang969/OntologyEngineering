---
name: ontology-engineering-v2
description: Manage engineering projects, CAD with NX/SolidWorks/SolidCraft, professional-tool evidence and source-bound experience in one system. Use for engineering reasoning, project decisions, Codex/Claude history and memory, and maintaining ontology, generated contracts, Rust execution or RRSI methods.
metadata:
  version: "2.0.0-rc.7"
---

# 工程本体 v2

理解用户用自然语言提出的工程目的、工况和取舍，辨清概念与依据关系，在同一 Project 中推进设计、查证、实施和改进。用户不填写 TOML、YAML 或 RDF；agent 根据实际任务维护必要的来源、合同与决定。全体系未闭合时保持“重构中”，当前局部贡献与完整目标分别说明。

## 锚定根目标，防止路线偏移

每一步都严谨，整条路线仍可能离开用户要的对象。按[根目标锚定](references/goal-anchoring.md)执行，以下几条不能省：

- 开始时逐字保存用户原始请求和之后每次纠正，确定**根对象**，把“研究透/分析清楚”转写成可核对的**根交付物与验收项**，记入 Project 目录。实物或产品先列功能，再列承担每项功能的结构，然后才研究机理。
- 每个阶段开始前写明：对象与根对象的关系（根对象本身 / 部件 / 根对象的模型 / 参照案例 / 外部案例）、推进哪项验收；参照和外部案例须写返回条件。
- 连续 2 个阶段离开根对象及其部件、模型，或连续 2 个阶段未推进任何验收项，即为偏移：停止延伸，回到验收清单，确需转向时先征得用户同意。
- 当前安装的程序提供 `project route-audit` 时（以安装回执为准），每个阶段开始前用 Project 和 `route-anchor.json` 运行一次；结果为 drift 或 invalid 时不进入下一阶段。
- 交付前用 `oev2.route-anchor/2`（验收项列出所依据的决定与证据）运行 `project route-audit`。只交付 `delivery_verified`，`delivery_open` 继续调查或补验，`delivery_inconsistent` 先纠正。代码或制品改版后，原决定须补验并重新决定，才能再次交付。
- “继续/按这个思路/深入”指沿根交付物推进未闭合的验收项，不是沿上一轮自己提出的下一步无限延伸。
- 项目执行使用显式 `--project-home` 接入[项目文件体系](docs/PROJECT-WORKSPACE.md)，建议放在该工程项目的 `.engineering/`；安装根与 Project 身份分别核定，不从 cwd 或 profile 猜测。当前安装尚无 `project workspace` 时明确记录能力缺项，不借旧入口声称已启用。
- 上下文压缩、新会话或接续后，先读实际 HEAD 并运行 `project workspace resume`，恢复根请求原文、完整验收、用户纠正、待复核决定和当前动作，再读方法和选择下一步。恢复不完整先补来源；已派发或效果未知的动作先回读，不自动重做。重读方法或旧 RESUME 不能代替这一步。
- 每阶段按当前提交检查 `begin`，先保存执行意图与派发状态，再调用已授权工具；实际结果经原 owner 回读后提交。纠正是否解决须有关联的新复核依据，阶段进展声明本身不足。`ready`、`recorded` 与工程验收分开，待办与失败持续保存。
- 汇报先写根验收进度，再写新增来源与修订号。审查通过只说明本阶段自洽，不说明路线正确。
- “不得把其他型号数值当本产品规格”之类约束限制的是采用方式；应以带状态的假设推进根对象，而不是改去研究别的器件。

## 从当前问题辨析作用

以[同一组工程关系](references/engineering-reasoning.md)连接内层理解与外层行动：根验收要求什么功能，哪个对象或结构承担它，候选机制依赖哪些条件，哪些解释尚未成立，什么观察能区分解释，以及结果影响哪些决定。当前任务从这些关系的缺口产生。Agent 根据工程原件建立和修订关系，软件检查已经表达的关系；不把工具清单当成工程分解。

先明确本轮问题、完整目标和已授权范围。辨认影响判断的对象、表示或模型、方法、实现、主张与证据：它们分别指什么，在当前活动中承担什么作用，为什么与下一步有关。角色由当前问题决定，同一工具或产物可以用于探索、实现或检验；名称相同不保证含义、用途和证据能力相同。

遇到“模型”“正确”“验证”“完成”等词，说明它此刻针对的对象、条件和范围。判断哪些关系已有依据，哪些仍是假设或未知，以及已有依据足以支持什么行动；据此选择能补足缺口的研究、表达、执行或检验。用户明确的依赖关系贯穿后续工作，后取得的依据保留真实取得时间。需要探索才能取得依据时，按探索目的和范围推进。

围绕“必要功能 → 物理机制 → 实现条件 → 候选 → 证据缺口 → 验证 → 有范围的决定”往返工作；选择缺口时以根验收清单为准，不按上一个缺口追向下一个器件。先提出可区分的解释与验证动作，再查原页、推导、计算或调用工具；保留单位、假设、边界和反例。参数优化按原模型比较，机制探索说明新模型前提和扩展范围，共同父目标及全部必要验收保持。说明结果支持哪个主张、还缺什么，条件或来源变化时重审受影响关系及下游决定。

用户追问更深的前提、出现反例或局部调整反复无效时，先辨析问题触及目标、基本约束、模型假设、架构关系还是参数实现，并追查共同前提与下游决定。层次按当前因果依赖划分；必要时修订整个对象的表示和关系。项目本体可以是可修改的工作草稿，稳定采用须限定对象、条件、版本与已核证据。冻结一次试验的输入和判据，不赋予本体工程有效性。

本 Skill 承担可跨项目复用的方法：怎样发现前提、辨析层次、选择验证和接续影响；具体工程内容由各 Project 的来源与证据建立。共享概念或方法的缺陷应回到整个四环检查，不能只在当前实例打补丁；影响闭包仅覆盖已声明依赖，未识别的关系仍需调查。触发、分流及稳定采用条件见[设计方法](references/design-method.md)。这些问题按本轮需要展开，无须每次输出固定表单。

## 以完整场景推进当前工作

跨多轮实现、专业工具接续或恢复的任务，按[连续工程工作](references/continuous-work.md)组织。选择能推进根验收的最小完整使用场景，明确输入、动作、可观察结果和失败情形，尽早贯通相关环节；完整根目标与剩余义务继续保留。

从同一 Project、anchor、work 与 workspace 派生当前工作说明：当前结果、已成立前置、待辨假设、下一检查、授权边界和恢复位置。当前会话在已有授权内连续执行、回读和修正；一般实现选择无需逐项重复确认。目标、关键工程前提、验收或外部动作超出授权时再对齐。每次重试须有输入、诊断、实现或条件上的新依据；失败、partial 与 unknown 进入下一步判断。

按问题选择局部、场景集成与工程验收，关注完整使用行为及条件变化后的影响。派生说明不成为第二份状态正本；已有实例能解决的问题直接修正，共享方法的收益另按 RRSI 比较。

已冻结的探索使用 `project explore reasoning` 核下一动作的验收关联、未决前提、结果分支和受影响决定；`advance` 与探索阶段的 workspace `resume` 接续同一派生视图。`ready` 可以带待调查前提，不表示模型已适用；旧视图复用先 `verify-reasoning`。完整验收、尚无动作覆盖的验收和失败继续保留，Agent 仍须调查未声明的关系。

功能分配、主张和整体验收需要软件承接时，沿[工程关系](references/engineering-reasoning.md#身份功能实现与主张成立条件)绑定实际对象、当前条件及原检查。候选实现与已分配实现分开；局部结果按必要项、替代项和接口条件组合。新关系视图同时呈现原有检查与行动依据；待调查缺口可以继续推进，失败检查不能据此转成采用通过。变更传播保留对象、资料和决定的类别，旧同名依赖的歧义须核原关系。

## 贯通四个环节

**工程本体论 → ggen → Rust 工程模块 → RRSI。** 四环围绕真实任务往返：本体与领域模型先整理为明确规范，实际执行经独立验证和使用反馈，再回到需要修订的环节。各环节保留同一对象、版本、条件、主张和来源；按当前缺口展开，不要求每项任务遍历四环。

| 环节 | 需要明确的关系 |
|---|---|
| 工程本体论 | 说明对象、功能、条件、主张和证据关系；领域模型进一步说明行为、假设与求解依据。模型假设、来源陈述和实际观察保留各自身份。 |
| ggen | 将已明确且受支持的规范生成合同；逐项核对哪些要求由生成校验、手写检查、专业工具或工程评审承担。未表达、仅有注释和未执行分别保留。 |
| Rust 工程模块 | 将合同落实为实际行为，按缺口组织专业能力；将当前结果接回原要求及独立验证。功能实现、检查和验证案例可由不同模块承担，各有明确来源。 |
| RRSI | 用真实反馈区分实例、模型、规范、实现或使用方法的偏差；按证据修正相应层次。共享方法候选在固定目标与判据下比较，试用后接续实际反馈与恢复；前提或判据改变时另建基线。 |

工具可能参与多个环节，其作用随活动改变。按[四环承接](references/design-method.md#在四环之间保持含义和依据)从要求追到承担者与证据，从结果追回原条件与决定；缺口沿既有 Project 或软件维护路径接续。软件维护可用 `design inspect` 的要求关系视图及生成的验证清单核已声明关联。静态映射、生成一致、行为验证和工程采用分别说明；反馈修订按[有证据的改进](references/regularized-evolution.md#反馈如何回到对应层次)。

## 按缺口选择能力并接续

[能力索引](docs/CAPABILITIES.md)是按工程问题选择工具和合同的统一入口。先确定本轮需要，再读相应专门说明；两卷方法书、机械设计/计量/制造资料与供应商信息按其作用查证，相关性和工程适用性分别判断。

用户向 `sources/` 投放资料或要求整理新增内容时，按 [资料接入说明](sources/README.md)清点来源、版本与摘要。PDF 先用 `knowledge library audit-pdf --directory sources/books` 对显式目录做只读审计，区分原件状态、登记关系、重复与逐书文本覆盖；索引不可核时保留未知，零文本不推定扫描件，不自动登记或 OCR。再按资料实际格式选择接入方式，清单由 Agent 准备；PDF 新增或修订先 `--merge --dry-run` 核差异，再按 ID 合并，保留未列旧来源。同 ID、路径和摘要下的描述/导航更新可复用已核文本，仍为 `updated`，旧查询包须重核当前登记。预览不锁定后续输入；`--replace` 仍须完整保留书目，不能当成追加。入库、索引、本体化、当前 Project 证据和工程采用分别记录。新增资料不会自动扩展 `project work` 的既有六书角色。

米思米机构案例、BOM 与 CAD 归入 `sources/misumi/incad/`，按[资料图谱](docs/MISUMI-SOURCE-LIBRARY.md)用 `knowledge library incad` 建立、查找和复核来源关系。命中后核官网条件与原件，再接入已有 Project/work；资料图谱不自动授予工程适用或采用。分片恢复只面向新的资料目录，先核明确交付范围。

涉及米思米零件选型时，沿[三层选型审查](docs/MISUMI-SELECTION.md)区分 inCAD 机构参考、标准目录/技术手册依据和官网当前配置。三层必须绑定同一 Project/work、对象、条件和选择理由；案例 BOM 或历史 CAD 不能替代具体当前型号。现有 `supplier selection-review` 只验证这份离线回执的身份、来源角色、理由与必要检查，不执行网页选型、下载、采购、NX 或工程采用。网页观察必须由当前浏览器合同取得并带日期，结果再经项目 workspace 保存和后续 owner 验收。

复用查询包时，把原件完整性与当前登记分开核。需要当前本地 PDF 依据时，`knowledge verify-packet/feedback` 使用 `--require-current-registration`，整包候选须已绑定且 `current`；旧包无绑定先重新查询，米思米等按各自来源合同核验。状态解释见资料接入说明。登记变化不改写旧包、反馈或决定；当前书目一致性不授予工程采用。

旧包因原件失效而无法验证时，可用 `knowledge audit-packet --packet <包路径>` 只读查看逐候选原件字节状态与登记变化，确定需要恢复原件还是重新查询。诊断不核当前 Project 或原生页内容，不能代替 `verify-packet` 或作为反馈凭证。

专业工具的结果统一经 `project tool-result verify/register` 带来源回到同一 Project，不得用脚本直接改写 project.json；登记不改结论；结论经 `project decide` 由已登记结果更新，采用仍走 `project adopt`。外接专业能力按[作用与结果交接](references/professional-capabilities.md)选择：先确定要回答的问题、输入对象和所需证据，再调用对应 skill/工具。结果携带来源、条件、版本、覆盖与未知回到同一 Project；多工具同意的独立性要核实际来源与计算依赖。性能候选还需原基线与可比测量，经验保存后仍须在新条件下核适用性。

当前会话负责组织任务、读原件、选择工具、回读结果和更新决定；Rust 执行明确的合同、来源与工程检查。八个专业 Skill 随本仓库在 `modules/` 统一维护；运行程序按[安装与接入](docs/TOOL-INTEGRATIONS.md)使用固定上游版本。四份 diff 只用于干净上游副本的重建核验，已整合模块不重复应用；运行程序与数据继续位于 `var/` 等已核位置。engineering-memory 是本系统内置模块，通过 `oev2 memory` 使用；历史经验是否使用由当前任务决定。需要历史时读[记忆方法](modules/engineering-memory/skill/SKILL.md)，还原时间线、失败、纠正、因果未知和适用条件。召回经 `project memory-candidate verify/register` 核来源与绑定，只作不支持决定的待核候选；实际使用效果经 `project memory-feedback` 与 `memory session feedback` 回写。

按当前缺口读取对应模块入口，保留其完整规则与引用资源，不将本表当作工具执行合同：

| 缺口 | 模块入口 |
|---|---|
| 仓库、相关工作与贡献调查 | [GitContribute](modules/gitcontribute/SKILL.md) |
| 本地源码定位与有界上下文 | [LeanToken](modules/leantoken/SKILL.md) |
| 发布制品及授权运行行为调查 | [REA](modules/reverse-engineer-anything/SKILL.md) |
| 精确计算、有限搜索与数学关系 | [Jacobian](modules/jacobian-math/SKILL.md) |
| 复杂度候选与优化证据组织 | [SmokingGun](modules/smokinggun/SKILL.md) |
| 性能原件分析与授权工作负载采集 | [Flameox](modules/flameox/SKILL.md) |
| 电子设计、原理图/PCB 与接线 | [electronics-design](modules/electronics-design/SKILL.md) |
| 工程概念的等距图示、交互 SVG、WebGL/3D 展示与可打印图 | [Anatomy](modules/anatomy/SKILL.md) |

模块入口无需复制到宿主 Skill 顶层；检查本体、memory、八模块的实际路径与同名副本，MCP 连接单独配置和验证。fleet 保持仓库外的独立 Skill。模块可读不代表程序已安装或工程已验收。

电子电路、原理图/PCB、器件模型与接线指导进入 [electronics-design](modules/electronics-design/SKILL.md)。该入口统一选择五个内部模块，各自 `GUIDE.md` 按需展开；保持同一 Project、对象修订、条件与来源，经共同结果合同回交，再保存实际使用反馈。安装、仿真、EDA 原生检查和实物验收分别取证。

工程概念图示、交互 SVG、WebGL/3D 展示与可打印图进入 [Anatomy](modules/anatomy/SKILL.md)。它默认接入既有 `oev2 presentation bind/generate/verify`，上游 kit 只负责渲染与图示检查；Project、Source、Object、Condition、Evidence 和工程采用仍由 v2 维护。

CAD 装配需要可视化时，先用 `oev2 cad presentation project` 将已验证的 NX
装配观察投影为带来源指针的 `projection.json` 和有界 `source.json`，再进入
Anatomy 的 presentation 流程；投影保留 occurrence、变换、离散姿态和损失说明，
不把渲染代理当作原生 CAD 或工程采用结论。

其他专业模块也通过同一条 Project 边界回交：REA、LeanToken 和 GitContribute 使用
`oev2 project source-projection project` 保留源代码/制品图谱及发现；Jacobian 使用
`oev2 project jacobian project` 保留带单位、假设和证书的数学结果；SmokingGun 与
Flameox 使用 `oev2 project performance project` 保留性能观测、基线、环境和清理记录；
电子设计使用 `oev2 project electronics project` 保留原理图、PCB、器件模型和接线卡的
来源与验证。上述命令只做来源核验、边界投影和证据归档，不运行外部工具、不补造测量、
不自动授予工程采用；engineering-memory 仍经 `oev2 memory` 原生接入，fleet 仍是仓库外
的环境能力。

需要执行 v2 程序、维护软件或声明当前能力时，按[使用说明](docs/USAGE.md)核发行 manifest、实际程序、根目录及维护回执；统一源码安装从 `var/install/INSTALLATION.json` 核实际根与八程序身份，初次配置读 [AGENTS.md](AGENTS.md)。旧维护状态只接续对应的历史发行，不覆盖新安装。源码候选版本、已安装程序、历史回执和当前任务各保留身份，能力范围见[发布范围](docs/RELEASE-SCOPE.md)。

CAD 按原生交付物和当前能力选择 NX、SolidWorks 或 SolidCraft，调用前读[独立执行合同](docs/CAD-INDEPENDENT-EXECUTION.md)。NX/SolidWorks 沿 v2 自有 Rust worker 与显式原生 SDK；本机开放源码参数建模、`.sc` 设计及 MCP 交互进入 [SolidCraft 工作流](docs/SOLIDCRAFT-CAD.md)，由固定上游 CLI/MCP 执行、既有 Rust `project tool-result` 接收。用户指定后端或原生文件要求优先，STEP 不能替代 `.prt`/`.sldprt` 的原生特征与装配验收。SolidCraft 是 CAD 执行后端，不另建 Project 或第九套专业模块；当前没有 `oev2 cad solidcraft` 命令或 SolidCraft→NX 专用投影。

读一张已有图纸时，先用任务原话经 `oev2 memory session prepare` 查读图经验（先看不含原文的候选摘要，需要时再 `get`/`verify`），按其条件与未知判断是否适用；再沿[读懂已有图纸](docs/DRAWING-UNDERSTANDING.md)用 `oev2 drawing route`：DWG 只经锁定的 CADCraft fork 提交转换（不给字体），分类只在只收字节的沙箱 worker 中进行，独立复核从源字节重推，检查页经离线浏览器检查。参照答案、手工区域和配对模型不进入输入；零件要素目标保持未知，结果是待复核候选。结束时用 `oev2 memory session feedback` 记录经验实际用、拒绝或未用。正向出图与反向读图的建设、外部工具的固定与保真资格按 [CAD 路线](references/cad-route.md) 进行。

不调用原 cad-agent 的脚本、环境、MCP、Journal runner 或旧默认配置，缺项在当前后端的合同内补齐并验证。Fusion 已退出执行范围。原生工具结果按对应合同回读、保存及独立进程重开；截图、同一内核的 GUI/无窗口结果、STEP 回读与独立工程依据分别记录。浏览器遵守当前用户的不抢焦点规则。

工程采用前，由当前 Rust 内核审查真实 Project、对象、条件与来源，并核必要的独立批准。生成、编译、语义答复、工具完成、审查覆盖和工程采用分别取证，未实现、未执行及部分失败保留具体状态。局部验收说明对父目标的贡献和剩余义务，不扩大为整体完成。

八个专业模块统一由本仓库维护；模块上游原文若提到 setup，按[本布局安装约定](docs/TOOL-INTEGRATIONS.md)只配置 MCP，不能重新向宿主顶层安装同名 Skill。

## 维护与反馈

修改 v2 软件时，`specs/` 为正本，共享类型、接口、错误、状态及可表达约束由锁定的真实 ggen 生成，复杂算法手写 Rust，生成区不可手改。方法文字调整应核对来源、约束迁移和实际使用；涉及规范或程序行为的改动，另执行相应生成、构建、回归与独立接收。[使用说明](docs/USAGE.md)规定当前执行身份与恢复边界，v1 和全局 `oe` 保留原身份。

保存目标、条件、行动、观察、成功/失败、用户纠正与反例，区分项目事实、工作方法、领域候选和软件缺陷。已有表达足够时修正实例或记 `no_delta`。项目内 RRSI 先经 workspace 的 learning-goal / learning-freeze 接续当前完整 Project、根锚定与全部验收；learning-check 可显式诊断，绑定候选的正式 freeze/assess/decide/apply/registry 入口也会重核当前根身份。再沿[有证据的改进](references/regularized-evolution.md)比较固定候选和判据。目标或来源变化重建基线；恢复会话不自动触发方法晋升。来源匹配、规则复述或文字精简本身不证明方法收益，检索与诊断收益也不能批准本体变更。

项目进度和专属验收从相应项目原件接续；历史案例保留适用范围。`var/` 保存私有任务、调查、构建与维护，`sources/` 保存工程资料原件、处理成果及索引。这些私人内容不进入分发，`sources/` 仅发布接入说明 `README.md`。历史命令按原身份阅读，当前执行使用已核入口。每轮报告先写根验收进度，再写实际新增、完成范围、未决问题及下一依赖。
