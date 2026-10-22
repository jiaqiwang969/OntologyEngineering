# 执行条件与合同导航

本页供已确定需要的操作查阅。工程判断从[主 Skill](SKILL.md)和[设计方法](references/design-method.md)接续，能力选择统一从[能力索引](docs/CAPABILITIES.md)进入；不要求每次预读全部合同。以下保留入口选择、协议、预算、拒绝与恢复条件，字段及完整检查以相应专页为准。

当前用户要求和项目规则优先。历史命令、候选源码、已安装程序和本次执行分别保留身份；同名入口不能据名称推断后端、当前能力或通过范围。本页整理不升级旧协议、不改写历史回执，也不宣布当前程序已具备全部列示能力。专业工具按本轮缺口选择，使用各自 skill 的实际合同；结果带来源、条件和未知交回同一 Project，不由工具名称预设固定流水线。

## 程序、根目录与版本

需要执行 v2 程序或声明当前能力时，按[使用说明](docs/USAGE.md)核实际入口。统一源码安装使用 `<root>/var/install/bin/oev2`，核 `var/install/INSTALLATION.json` 的实际根与完整程序身份。旧发行包仍按其预期 manifest 和独立安装回执接续，不覆盖当前统一安装。v1 与全局 `oe` 保留原身份，不作为失败备用；旧书页、对话和来源包中的命令只按其原历史身份阅读。

统一候选 RC.6 保留来源 feature 合同；原 RC.5 的来源会话 `/1–3`、CLI 经 kernel 编入的两个 feature、独立库无默认 feature、公共查询 `/1–4` 与拒绝 `/5`，见[发布范围](docs/RELEASE-SCOPE.md)及[来源会话](docs/SEMANTIC-SOURCE-SESSION.md)。`dist release check-profile --profile oev2.rc5-local-source/1` 核实际依赖编译状态；匹配不代替行为、安装、Windows/native 或工程采用。旧 RC.4 关闭 feature 的门槛保留原身份，不转授新程序。

`var/` 保存私有项目、任务、调查、构建和维护，`sources/` 保存合法原件及索引；分发排除这些私人资料和凭据。完整目标、项目专属验收与延期范围从项目原件和[发布范围](docs/RELEASE-SCOPE.md)接续，文件数、局部测试数与接口存在不表示整体完成比例。

## 当前 Project、局部贡献与变更

| 本次操作 | 入口与必须保持的关系 |
|---|---|
| 整理前提、查证和计算 | [工程工作](references/engineering-work.md)、[工程前提](references/engineering-premises.md)：同一 Project 的 work `/3` 绑定来源事实、模型必要前提、适用性、功能承接、范围及反例。旧 work `/1–2` 保留原诊断意义；更新条件摘要不能使旧载荷或支承模型重新适用。模型/输入一致或算术完成不表示工程合格。 |
| 局部工作支持父目标 | `engineering-basis/2` 的 goal_support 绑定同一 Project 总目的、父/子对象、父条件原文、局部原七字段和来源，逐项映射局部验收到父验收。旧 `/1` 目的精确相等保持；声明贡献有效仍保留父目标及制造、计量、采用义务。[完整来源视图](docs/ENGINEERING-BASIS-VIEW.md)保留 `/1–2` 原件，共用字段视图不能冒充完整新版 wire。 |
| 当前正式审查 | `project context`、`kernel review --project` 绑定输入、对象/修订、条件内容及来源；RDF 焦点声明同一 `conditionSha256`。缺绑定保持 unknown，原检查仍执行；`--historical-snapshot` 只审冻结历史。Harness 与经验流程使用同一检查。见[治理合同](references/governance-policy.md)、[体系验收](docs/ENGINEERING-SYSTEM.md)及[Project 样例](examples/v2-project/README.md)。 |
| 当前任务和聊天方法依据 | task-view `/1` 及显式 `/2` 按[原消息与方法使用](docs/CONVERSATION-METHOD-USE.md)选择。`verify-task-view` 回读全部绑定原字节及完整 Project 值，含同 revision 的 decisions/adopted；旧视图过期与刷新后重审有依赖决定分别处理。动作仍是 candidate/unscheduled，不改 Project、不调用模型、不新增 budget/registry/history，不自动接管会话上下文。原 Project 未表达的排程、审批、无源事实、未提供正文及正式/物理验收保持 unknown；原消息 matched 只支持选定来源一致。 |
| 变化后复核 | 原 `project reconcile` 保留对象、来源、条件、目标及验收的变化影响；无显式目的依赖时保守重开决定。显式 [reconcile-reviewed](docs/REVIEWED-RECONCILIATION.md)先取得准确当前绑定的新审查，再复读原件和重放。旧、新依赖及原影响保留，未复核上游继续影响下游；本入口不授予采用，adopt/verify-adoption 的独立本地批准与实际权限另核。 |

围绕缺口探索时，按[可判别验证](docs/ENGINEERING-EXPLORATION.md)使用当前程序实际支持的 `project explore freeze/verify/run`；需要会话执行外部工具时另用显式 `advance` 合同。冻结原计划、独立预期和结果判据，保留共同父目标、全部必要验收、失败与无法区分。`requires_host_agent` 表示等待，`results_recorded` 只表示限定结果记录；原因假设不因数值匹配晋升为事实，原生验收与工程采用另审。

## 会话、专业工具与记忆

当前 Agent 解释任务、读原件、选择工具并组织并行工作；独立任务可并行，依赖前一步的活动等待真实结果。专用判断服务的自动路由已退出当前候选。通用外部模型只在当前任务明确选择的能力内使用，具体配置与预算见[直接模型辅助](references/direct-model-assistance.md)。

本体与 memory 随同一源码安装，通过 `oev2` / `oev2 memory` 使用。Project 保持当前事实与决定，memory 保留过程与有源经验。召回经双侧绑定核验仅为候选，实际采用仍依当前证据；使用反馈不得把曝光记作收益。入口与范围见[记忆模块](modules/engineering-memory/README.md)。

六工具按[接入说明](docs/TOOL-INTEGRATIONS.md)由用户安装上游版本并应用实际 diff；输入、原始输出、版本、覆盖、失败与未知回到同一 Project。专业调用不跳过原验收、动作权限或工程采用合同，详见[结果交接](references/professional-capabilities.md)。

完整工程过程用显式 Harness checkpoint/archive 保存，资源预算、来源核查、未完成与恢复条件见[完整检查点](references/harness-context.md)。它不补造模型答案或新工程通过。

## 资料、来源与设计尝试

按[工程工作](references/engineering-work.md)和已登记来源跨书查证：两卷方法书分别支持对象/关系/条件及主张/证据/验证；机械设计、计量、制造与供应商原页按实际问题选用。检索/OCR 命中只是来源候选，继续核产品、介质、工况、时间、版本、单位与适用边界。实际原生、制造、检验或成本证据未取得时保持具体未知。

| 操作 | 专门合同与边界 |
|---|---|
| 登记页进入模型读窗 | [OCR 模型通道](docs/REGISTERED-OCR-MODEL7.md)：同 Project、完整原问题与登记 request6，native 提取和 OCR 原始行分开；不能由 OCR 数字推断工程合格。每次按原合同重读来源并执行真实 OCR，prepare audit 不恢复权限；必要背景超预算 blocked。`prepare-ocr` 只准备，`run-ocr --live` 才调用；本地验证不授予外部调用权限。 |
| 登记页的关系线索 | [typed 抽取 `/8`](docs/TYPED-RELATION-EXTRACTION8.md)：当前读窗、完整实体/模型配置、原 native/OCR、来源/scope/未知和安全响应保持；复用共享次数与凭据 guard。关系候选不直接成为 Project 事实，旧后备 `/1`、完整迁移和收益各自验收。 |
| 资料图定位 | [Rust 搜索](docs/EXPLORER-SEARCH.md)：说明图与当前 Project 来源/条件的关系，复用唯一数据会话并核文件身份、版本、预算和期限。命中只定位词项、别名、过滤及相对锚点线索；原 min_semantic_similarity 未参与计算，缓存不成为事实、授权或收益证据。 |
| inCAD 来源、设计和文件暂存 | [接入](docs/CAD-INCAD-INTEGRATION.md)、[设计尝试](docs/DESIGN-ATTEMPT.md)、[暂存](docs/INCAD-CAD-STAGING.md)：同一当前 Project/work3，保留原规格、自然语言变化、缺项、独立意图与参考答案。冻结后实际执行 work 并收集原 Harness/CAD/handoff/审查产物；已曝光案例保持 development_replay。ZIP 与完整成员核原字节、登记及交接，CAD 加载、单位、装配闭包和物理验收另验。Project 变化另建尝试，准备或暂存不表示设计完成或采用。 |
| 受控设计比较 | [设计比较](docs/DESIGN-COMPARISON.md)：冻结父目标及全部验收、两个合法 child work 和原准备；既有 Rust 计算/条件 owner 重算指标，保留全部分母、失败、未知与曝光。来源/曝光/允许变量变化拒绝；成本未知不称更便宜，局部 improved 不表示设计合格或通用方法提升，结果可接原 design-attempt 反馈。 |

## CAD、数值交付与图示

CAD 执行先读[独立执行合同](docs/CAD-INDEPENDENT-EXECUTION.md)：使用 v2 自有 Rust 模块与显式原生 SDK，不调用原 cad-agent 的脚本、环境、MCP、Journal runner 或旧默认配置作入口或备用。历史合同与回执可作有来源数据，不取得当前执行资格；Fusion 不执行。供应商浏览器沿当前用户 Peekaboo 后台、不抢焦点规则操作。

### 数值、图纸和独立原生回读

[数值、孔列与交付](references/numeric-cad-delivery.md)绑定要求、单位、状态/基准、容差、不确定度和来源版本；模型适用、算术、数值符合、工程采用分别报告。参数变化明确设计意图、控制件/配合件及下游，回读原生几何、保存并独立重开。实例尺寸不成为通用默认，缺独立意图保持 unknown，不从回读反造预期。

[原生尺寸关联](references/native-drawing-associations.md)按显式版本使用：`/1` 保留点观测；`/2` 核精确零件中唯一完整圆边、原生 ComputedSize，并可在副本创建有界关联标注。当前 Project/work/Harness/kernel/experience 核观测所绑定的完整原任务，摘要相同不代替内容相符。圆边意图 `/3` 独立给圆心与半径诊断容差，旧 `/2` 共同容差保持，不能放大另一容差补偿超限。

局部新制图按 `NativeDraftingPlan` 和 `cad drawing check-drafting` 核页、视图、比例、方向、模型归属、边界和原码。part 观测 `/5` 的命名视图中心等于 bbox 布局中点只属于该新页构建试验，不推广为所有现有图纸要求。临时工作页只能作为有记录的构造状态，最终满足原计划；成功 flag 或部分内存页不替代最终位置、完整边界、可见性和保存重开，失败副作用不继续保存成工作产物。

另版 `native-drafting-observation/6` 的 NativeLayoutReference 绑定当前完整实体 bbox 的 ABS 中点和实际映射输入输出；smart anchor 仅诊断，原 `/3–5` 必要合同不删。有限构造域从完整实体及原计划派生，最终核幅面、方向、比例、位置、完整边界和显式可见库存；每份相等矩阵先各自核右手正交，数量与原 tag 清单一致。新 saved 产物按同 Project 子实例、版本和父来源链接续；独立意图只改变明确来源绑定，不反造预期。局部关联、保存或文件清单闭合各只支持对应主张；跨版本稳定特征、重关联、完整制造尺寸/公差/材料、视觉、图纸布局及实物验收另取证。

### 平台、构造、复制与运动

| 操作 | 当前专门合同与必须保留的边界 |
|---|---|
| SolidWorks | [有限 worker](docs/SOLIDWORKS-NATIVE-WORKER.md)：历史 `cad solidworks verify` 只核第三方证据与文件绑定；Rust STA COM 和显式 owned 进程的零件检查、受控板件、厚度改型按实际范围执行。编译、COM/类型库、建模、独立重开分别取证；装配、图纸、配置、STEP 等按支持范围补齐，旧 Python MCP 成功不重签。 |
| 圆柱及声明角色恢复 | [NX 圆柱](docs/NX-CYLINDER.md)与[限定恢复](docs/NX-CYLINDER-RECOVERY.md)：使用当前 worker，独立源意图、几何、Windows/native 与新进程分别核验；显式 v5 不改签原错误角色或 unknown。 |
| 装配构造 | [构造来源 `/1–2`](docs/CAD-ASSEMBLY-CONSTRUCTION.md)：原 `/1` 判据保持；`/2` 区分 NXOpen 整件名与 UF None，独立原生任务分别完整读取 definition instance 和实际 part occurrence 参考集。旧资料缺观察时拒绝，不补造回执；新参数/descriptor、平台/native 与采用分别验收。 |
| 原生组件复制 | [组件复制](docs/NX-COMPONENT-COPY.md)：核实际 CopyComponents 返回、完整前后清单、新实例身份、父级/原型、两种参考集、保存及独立新版本重开。`cad evidence verify-copy` 沿同 Project 重审旧、新依赖；复制不继承原工程采用，旧构造/对应合同不静默升级。 |
| 质量属性差异调查 | [质量属性诊断](docs/NX-MASS-PROPERTIES.md)：冻结不变状态、复制前后及实际改型对照，保留原 raw47/statistics13 位模式、来源和失败。诊断不替代原 copy/1 严格判据、保存重开或工程采用；原因判断及当前 Windows 资格另验。 |
| 装配运动、保存位置和结果接收 | [NX 装配运动](docs/NX-ASSEMBLY-MOTION.md)、[运动原件接收](docs/CAD-MOTION-RECEPTION.md)：组件完整路径、逐帧覆盖、来源角色和事件顺序分别核验；采样、保存和独立重开不代替连续运动。当前支持的显式 capture/receive 协议按原件和程序回执接续。 |
| 原生运动主张 | [cad_native_motion](docs/NATIVE-MOTION-ASSERTIONS.md)：实际重核原件、当前条件和下游影响，区分采样已检查、采样无干涉和有界独立重开；连续、制造和精度义务保持。序列化 pass 不代替原件，命题反证与对象/来源变化的影响分别处理。 |

原生任务 COMPLETED 仍核实际回读、独立意图和当前来源。模型、图纸、BOM、工艺、检验、成本及交期依赖按[工程交付](docs/ENGINEERING-DELIVERY.md)接续；软件、平台/native、版本接收和工程采用各自验收，不用一次局部成功抵扣完整 CAD16、Semantica 或父目标。

[有源工程图示](docs/ENGINEERING-PRESENTATION.md)使用 `presentation bind/generate/verify` 接同一 Project、对象、条件和来源；Rust 计算有限离散状态并生成 HTML/SVG。固定投影与当前性核验、浏览器观察、原生 CAD、连续运动、制造及工程采用分开。该能力与构造/实例合同共存，按实际版本回执使用。

## 语义、生成、编译与维护

按[设计方法](references/design-method.md)维护 `specs/` 正本：功能、对象、条件、行为、约束、检查与独立案例建立明确关系。复杂算法手写 Rust，共享类型、接口、错误、状态及可表达约束由锁定的真实 ggen 生成；生成区不可手改，生成失败保留工作版本。生成回执和静态映射不替代实际行为或当前 Project 审查。

[Semantica 接入](docs/SEMANTICA-RUST-EMBEDDING.md)保留唯一数据/Project/来源治理身份，通用能力、工程合同、领域规则和编排共同承担代码化；完整原库、工程作用和改进收益分别验证。[查询目标与上下文](docs/SEMANTIC-DESCRIBE-CONTEXT.md)的显式 query-options `/2` 绑定真正目标、初始变量、排序去重键与应用上下文；旧 neutral `/1`、并列排序及 child-IRI 差异保持原范围，不因同名 profile 静默升级。

| 维护操作 | 完整合同与保留条件 |
|---|---|
| 生成、构建、接收和安装 | [使用说明](docs/USAGE.md)、[runtime 行为接续](docs/RETIRED-RUNTIME-BEHAVIORS.md)：当前 runtime 沿既有 Rust 版本 owner；历史 wheel、PEP610 和原构建对象只按原用途校验。读取实际维护状态，使用锁定工具链/Cargo.lock；声明、生成、编译、来源、显式双产物一致性、可信编译资格、实际选择/安装、独立接收与工程采用分别核验。私有 checkpoint 核原字节，不以 v1 主源码代替已验证增量；旧通过不转授新版本。 |
| 完整原生编译器调用 | [执行 `/7`](docs/NATIVE-COMPILER7-EXECUTION.md)：同一来源、registry、原预算/期限、进程及五阶段 builder，完整库存保留真实原 `/3` 或完整 `/7` 调用。共同 `/6` 来源/过程只作原样数据；原 metadata-only 检查、两份完整 debug/signature 和逐字节比较仍必需。候选源码与实跑分开，正式接收另验。 |
| 临时输入、源码/输出与链接器文件 | [内容观测 `/7`](docs/NATIVE-COMPILER-INPUT-CONTENT7.md)、[源码与输出 `/7`](docs/NATIVE-COMPILER-LINK-INPUTS7.md)、[链接器输入 `/7`](docs/NATIVE-LINK-FILE-INPUT7.md)：保存原临时输入、完整参数及文件 occurrence，从原命令、包声明及原件推导动态库目的；导出符号清单保留全部原字节和参数位置，复用 archive owner。内容捕获不重造活的编译器父进程，不代替执行准入、`/6` 接通、两份产物/调试/签名、原生资格或采用；原 `/3` 隐藏输入拒绝与旧失败保持。 |

维护按“问题→复现/影响→修正规范、模板或实现→回归与独立验证→版本→实际反馈”接续。区分静态线索、已复现缺陷和待核假设，规则调整与实现修复分别说明；不删反例、不改 oracle 或放宽判据获得通过。接收后追加说明与冻结软件快照分开，每轮报告实际新增、依据、未验项和下一依赖。

## 经验候选、归档与恢复

按[有证据的改进](references/regularized-evolution.md)绑定原目标、术语/单位、语义、来源、评估规则与必要反例；候选隔离验证，开发回归与未见任务分开。收益、成本和新颖性不能抵消来源、未知或必要验收偏离。`knowledge experience` 使用既有 Project/Harness/Rust 内核的有限冻结、评估、私有试用、拒绝和恢复；独立迁移、任意代码候选和完整版本切换各有待验范围，不因流程存在而宣布闭合。

新冻结按 `experience-freeze/2` 保留完整拒绝历史；原来源和派生 RDF 不同字节时单独建立 `experience-evaluation/3` 基线。双方保留必要审查，检索顺序只是可评的有限改动。重试登记稳定实验、假设、范围、相关原拒绝和可核验新依据；改文字、漏历史、伪造引用或重复旧依据仍拒绝。无收益保留基线；若已经试用则恢复精确冻结字节，再为恢复后的源码重新生成、构建与接收。一次检索回放、模型判断或来源匹配不算工程改善。

已有表达足够且作者候选为空时，可按[Harness 归档](references/harness-context.md)使用 `consolidation-input/3`：全部轨迹先核验，独立资源上限，原字节另存，终态回执最后写。作者 no_delta 不触发模型、晋升或采用；非空候选仍按原评估协议，旧超限和失败保留。

只需保存完整工程过程时，用显式 `harness-checkpoint-input/1` 沿原 `harness step` 核当前正式检查、原件及历史。`recorded` 与 `model_execution=not_run` 分开，unknown/violated 仍阻断相应采用；该入口不自动接管超限模型请求，也不使非空经验候选免审。具体节点、字节、轨迹与附件上限及显式扩容版本仍读 Harness 专页，不能以模型 token 容量替代归档资源合同。

从成功、失败和用户纠正恢复目的、行动、原因、反例及验证，绑定原消息和产物；已确认闭合的工程交付保留历史成功，不因开发试验重开。下一任务记录方法的实际使用及反馈，分别形成项目事实、工作方法、领域候选或软件缺陷；已有表达足够时保留实例或 no_delta。历史规则、原服务记录和旧执行身份继续可查，不能用整理后的说明追认未发生的执行或收益。
