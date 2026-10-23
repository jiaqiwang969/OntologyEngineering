---
name: ontology-engineering-v2
description: Manage engineering projects, professional-tool evidence and source-bound experience in one system. Use for engineering reasoning, project decisions, Codex/Claude history and memory, and maintaining ontology, generated contracts, Rust execution or RRSI methods.
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

先明确本轮问题、完整目标和已授权范围。辨认影响判断的对象、表示或模型、方法、实现、主张与证据：它们分别指什么，在当前活动中承担什么作用，为什么与下一步有关。角色由当前问题决定，同一工具或产物可以用于探索、实现或检验；名称相同不保证含义、用途和证据能力相同。

遇到“模型”“正确”“验证”“完成”等词，说明它此刻针对的对象、条件和范围。判断哪些关系已有依据，哪些仍是假设或未知，以及已有依据足以支持什么行动；据此选择能补足缺口的研究、表达、执行或检验。用户明确的依赖关系贯穿后续工作，后取得的依据保留真实取得时间。需要探索才能取得依据时，按探索目的和范围推进。

围绕“必要功能 → 物理机制 → 实现条件 → 候选 → 证据缺口 → 验证 → 有范围的决定”往返工作；选择缺口时以根验收清单为准，不按上一个缺口追向下一个器件。先提出可区分的解释与验证动作，再查原页、推导、计算或调用工具；保留单位、假设、边界和反例。参数优化按原模型比较，机制探索说明新模型前提和扩展范围，共同父目标及全部必要验收保持。说明结果支持哪个主张、还缺什么，条件或来源变化时重审受影响关系及下游决定。

这些问题用于组织判断，按本轮需要展开，无须每次输出固定表单。角色变化及工程到软件的对应例子见[设计方法](references/design-method.md)。

## 贯通四个环节

**工程本体论 → ggen → Rust 工程模块 → RRSI。** 各环节围绕同一对象、版本、条件和来源承接工作；根据任务选择所需环节和阅读深度。

| 环节 | 需要明确的关系 |
|---|---|
| 工程本体论 | 当前讨论什么，其表示、功能、条件与主张分别是什么，哪些依据支持哪些判断。模型假设、来源陈述和实际观察保留各自身份。 |
| ggen | 将需要软件承接的含义表达为规范与合同，保留身份、条件、依赖、未知及验收要求；核对生成能力实际覆盖什么。生成成功只支持相应的一致性结论。 |
| Rust 工程模块 | 将合同落实为实际行为，按当前缺口组织专业能力；说明执行和检查实际完成了什么，其结果能支持到哪里。Rust Semantica、工程合同、领域规则和编排各承担明确职责。 |
| RRSI | 从真实反馈定位理解、表达、实现或使用中的偏差，提出有限候选，在固定目标与判据下独立比较，再决定使用或保留基线。 |

工具可能参与多个环节，其作用随活动改变。工程对象与软件结构之间要说明谁承担功能、谁执行约束、什么独立依据检验行为。ggen 保持受支持的规范到生成物的一致性，Rust 执行明确规则；规则和结果的正确性仍需相应工程来源与独立验证。尚未表达或检验的关系保留未决状态。

## 按缺口选择能力并接续

[能力索引](docs/CAPABILITIES.md)是按工程问题选择工具和合同的统一入口。先确定本轮需要，再读相应专门说明；两卷方法书、机械设计/计量/制造资料与供应商信息按其作用查证，相关性和工程适用性分别判断。

用户向 `sources/` 投放资料或要求整理新增内容时，按 [资料接入说明](sources/README.md)清点来源、版本与摘要。PDF 先用 `knowledge library audit-pdf --directory sources/books` 对显式目录做只读审计，区分原件状态、登记关系、重复与逐书文本覆盖；索引不可核时保留未知，零文本不推定扫描件，不自动登记或 OCR。再按资料实际格式选择接入方式，清单由 Agent 准备；PDF 新增或修订先 `--merge --dry-run` 核差异，再按 ID 合并，保留未列旧来源。同 ID、路径和摘要下的描述/导航更新可复用已核文本，仍为 `updated`，旧查询包须重核当前登记。预览不锁定后续输入；`--replace` 仍须完整保留书目，不能当成追加。入库、索引、本体化、当前 Project 证据和工程采用分别记录。新增资料不会自动扩展 `project work` 的既有六书角色。

米思米机构案例、BOM 与 CAD 归入 `sources/misumi/incad/`，按[资料图谱](docs/MISUMI-SOURCE-LIBRARY.md)用 `knowledge library incad` 建立、查找和复核来源关系。命中后核官网条件与原件，再接入已有 Project/work；资料图谱不自动授予工程适用或采用。分片恢复只面向新的资料目录，先核明确交付范围。

复用查询包时，把原件完整性与当前登记分开核。需要当前本地 PDF 依据时，`knowledge verify-packet/feedback` 使用 `--require-current-registration`，整包候选须已绑定且 `current`；旧包无绑定先重新查询，米思米等按各自来源合同核验。状态解释见资料接入说明。登记变化不改写旧包、反馈或决定；当前书目一致性不授予工程采用。

旧包因原件失效而无法验证时，可用 `knowledge audit-packet --packet <包路径>` 只读查看逐候选原件字节状态与登记变化，确定需要恢复原件还是重新查询。诊断不核当前 Project 或原生页内容，不能代替 `verify-packet` 或作为反馈凭证。

专业工具的结果统一经 `project tool-result verify/register` 带来源回到同一 Project，不得用脚本直接改写 project.json；登记不改结论；结论经 `project decide` 由已登记结果更新，采用仍走 `project adopt`。外接专业能力按[作用与结果交接](references/professional-capabilities.md)选择：先确定要回答的问题、输入对象和所需证据，再调用对应 skill/工具。结果携带来源、条件、版本、覆盖与未知回到同一 Project；多工具同意的独立性要核实际来源与计算依赖。性能候选还需原基线与可比测量，经验保存后仍须在新条件下核适用性。

当前会话负责组织任务、读原件、选择工具、回读结果和更新决定；Rust 执行明确的合同、来源与工程检查。七个专业 Skill 随本仓库在 `modules/` 统一维护；运行程序按[安装与接入](docs/TOOL-INTEGRATIONS.md)使用固定上游版本。四份 diff 只用于干净上游副本的重建核验，已整合模块不重复应用；运行程序与数据继续位于 `var/` 等已核位置。engineering-memory 是本系统内置模块，通过 `oev2 memory` 使用；历史经验是否使用由当前任务决定。需要历史时读[记忆方法](modules/engineering-memory/skill/SKILL.md)，还原时间线、失败、纠正、因果未知和适用条件。召回经 `project memory-candidate verify/register` 核来源与绑定，只作不支持决定的待核候选；实际使用效果经 `project memory-feedback` 与 `memory session feedback` 回写。

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

模块入口无需复制到宿主 Skill 顶层；检查本体、memory、七模块的实际路径与同名副本，MCP 连接单独配置和验证。fleet 保持仓库外的独立 Skill。模块可读不代表程序已安装或工程已验收。

电子电路、原理图/PCB、器件模型与接线指导进入第七种专业能力 [electronics-design](modules/electronics-design/SKILL.md)。该入口统一选择五个内部模块，各自 `GUIDE.md` 按需展开；保持同一 Project、对象修订、条件与来源，经共同结果合同回交，再保存实际使用反馈。安装、仿真、EDA 原生检查和实物验收分别取证。

需要执行 v2 程序、维护软件或声明当前能力时，按[使用说明](docs/USAGE.md)核发行 manifest、实际程序、根目录及维护回执；统一源码安装从 `var/install/INSTALLATION.json` 核实际根与八程序身份，初次配置读 [AGENTS.md](AGENTS.md)。旧维护状态只接续对应的历史发行，不覆盖新安装。源码候选版本、已安装程序、历史回执和当前任务各保留身份，能力范围见[发布范围](docs/RELEASE-SCOPE.md)。

CAD 使用 v2 自有 Rust 模块及显式原生 SDK；调用前读[独立执行合同](docs/CAD-INDEPENDENT-EXECUTION.md)。不调用原 cad-agent 的脚本、环境、MCP、Journal runner 或旧默认配置，缺项在 v2 内补齐并验证。Fusion 已退出执行范围。原生工具结果按对应合同回读、保存及独立重开；浏览器遵守当前用户的不抢焦点规则。

工程采用前，由当前 Rust 内核审查真实 Project、对象、条件与来源，并核必要的独立批准。生成、编译、语义答复、工具完成、审查覆盖和工程采用分别取证，未实现、未执行及部分失败保留具体状态。局部验收说明对父目标的贡献和剩余义务，不扩大为整体完成。

七个专业模块统一由本仓库维护；模块上游原文若提到 setup，按[本布局安装约定](docs/TOOL-INTEGRATIONS.md)只配置 MCP，不能重新向宿主顶层安装同名 Skill。

## 维护与反馈

修改 v2 软件时，`specs/` 为正本，共享类型、接口、错误、状态及可表达约束由锁定的真实 ggen 生成，复杂算法手写 Rust，生成区不可手改。方法文字调整应核对来源、约束迁移和实际使用；涉及规范或程序行为的改动，另执行相应生成、构建、回归与独立接收。[使用说明](docs/USAGE.md)规定当前执行身份与恢复边界，v1 和全局 `oe` 保留原身份。

保存目标、条件、行动、观察、成功/失败、用户纠正与反例，区分项目事实、工作方法、领域候选和软件缺陷。已有表达足够时修正实例或记 `no_delta`。项目内 RRSI 先经 workspace 的 learning-goal / learning-freeze 接续当前完整 Project、根锚定与全部验收；接续试验、评估及记录采用决定前运行 learning-check，再沿[有证据的改进](references/regularized-evolution.md)比较固定候选和判据。目标或来源变化重建基线；恢复会话不自动触发方法晋升。来源匹配、规则复述或文字精简本身不证明方法收益。

项目进度和专属验收从相应项目原件接续；历史案例保留适用范围。`var/` 保存私有任务、调查、构建与维护，`sources/` 保存工程资料原件、处理成果及索引。这些私人内容不进入分发，`sources/` 仅发布接入说明 `README.md`。历史命令按原身份阅读，当前执行使用已核入口。每轮报告先写根验收进度，再写实际新增、完成范围、未决问题及下一依赖。
