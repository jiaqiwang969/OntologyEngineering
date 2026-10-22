# 工程本体 v2：按工程问题选择能力

项目长期接续见[项目文件体系与会话恢复](PROJECT-WORKSPACE.md)：源码候选新增显式 `--project-home` 与 `project workspace init/commit/resume/begin`，保留当前提交、完整根请求和验收、纠正义务、因果来源、待复核影响及未知动作。原工具/Harness owner 回读结果；RRSI goal/freeze 绑定同一完整根目标。安装根、项目数据根和 memory home 分开；没有自动压缩 hook 或全局派发拦截，实际构建、安装与恢复验证范围按本轮回执报告。

工具结果回交见[专业能力](../references/professional-capabilities.md)：`project tool-result verify/register/compare/registry` 核 Project 绑定、本机登记的工具身份与程序字节、原始记录、结论依据与覆盖；登记只写新的 Project 副本，不改结论或采用；性能比较只接受先基线后候选的两份已登记结果。

路线锚定接续[根目标锚定](../references/goal-anchoring.md)：`project route-audit` 读取当前 Project 与 agent 维护的 `oev2.route-anchor/1`，检查阶段对象能否沿声明关系追溯到根对象、离开根对象或未推进验收的连续段、路线回看与用户纠正。只核声明关系与计数；关系是否真实、交付物与工程采用另验。

决定更新与交付范围：`project decide` 用已登记且重新核验的工具结果更新一个决定的结论和依据，写入新 Project 副本，不改采用。`project route-audit` 读 `oev2.route-anchor/2` 时给出 delivery_verified / delivery_open / delivery_inconsistent；绑定旧对象修订的决定不支持交付。见[根目标锚定](../references/goal-anchoring.md)。

记忆接续：`project memory-candidate verify/register` 原生重核 memory 交接（采用声明、经验摘要、原消息字节、Project 绑定），登记为不支持决定的候选证据；`project memory-feedback` 按 memory 合同生成绑定当前 Project 的使用反馈；`project memory-export` 列出 Project 记录供 memory 引用。见[专业能力](../references/professional-capabilities.md)。

快照内容一致性接续[原版校验和](SEMANTIC-SNAPSHOT-CHECKSUM.md)：显式区分普通 JSON 与原生值，保留原排序、ASCII 文本、短路和失败。源码、当前程序、平台接收及工程采用分别核验，完整存储与修订义务继续。

有源版本比较能力见[版本差异](SEMANTIC-TEMPORAL-VERSION-DIFF.md)：按显式种子核新增、删除、修改与查找失败。来源差异、Project重审和工程采用分别判断；实际可用范围核当前程序和接收回执。

版本记录接续[生成、类型检查与旧格式迁移](SEMANTIC-TEMPORAL-VERSION-RECORDS.md)：保留原即时内容、真值回退、异常及迁移后仍不合格的情况。Project 与受控版本各沿原 owner，原存储/差异/修订义务继续，软件与工程验收分别接收。

时间模式接续[序列、循环与连续配置](SEMANTIC-TEMPORAL-PATTERNS.md)：保留有源贪心选择、默认支持数、失败后状态和输入位置；明确原非终止与资源拒绝。实际软件、平台、安装和工程采用分别接收，完整目标保持。

时间路径接续[有源遍历与限制](SEMANTIC-TEMPORAL-PATHS.md)：保留原节点长度边界、全局visited、时间排序和完整失败；附原始位置与显式时钟。源码、平台接收、选用及工程结论分别核验，完整目标与原验收继续保留。

时间一致性接续[原报告与失败边界](SEMANTIC-TEMPORAL-CONSISTENCY.md)：保留两遍检查、输入位置、原缺失ID异常和时间粒度限制；报告问题、整次失败及工程采用分别留证。实际能力与选用按当前源、平台及安装回执接收。

时间范围接续[区间查询与日历分组](SEMANTIC-TEMPORAL-RANGE.md)：保留原边界、双时间轴、分组行为和显式时钟，采用独立冻结案例与当前平台/安装接收。完整工程目标、必要验收、原失败和未知继续保留。

当前源码接续[双时间事实与时点快照](SEMANTIC-TEMPORAL-SNAPSHOT.md)：显式时钟、有效轴与记录轴沿既有 Rust 时间 owner 执行。原始行为、当前安装与平台接收分别验收，缺少日期不推定工程有效，完整目标与原验收保留。


数值字典术语按原 Python 文本规则共用 Rust 格式 owner，覆盖两种 Datalog 入口的推导与去重；旧数值反例和当前平台接收见[规则推导](SEMANTIC-DATALOG.md#数值作为推理符号)。工程目标、采用与 RRSI 收益的判据保持。
先按[主方法](../SKILL.md)辨明当前对象、活动与依据关系，再从下表选择需要的能力。表中入口是合同导航，不表示本机已安装或验收通过；[执行条件](../ENTRY-CONTRACT-DETAILS.md)解释版本化操作约束，[使用说明](USAGE.md)给出调用方式，[发布范围](RELEASE-SCOPE.md)限定具体版本，实际资格仍回到维护回执。

阅读时分清三种身份：方法原则指导判断，合同说明可以表达和检查什么，已验证执行能力须有相应版本、平台与输入的实际证据。能力名称、字段存在和源码实现不自动成为当前已验证能力。

内部 Turtle 文本的当前接续见[读取与全文见证](SOURCE-TURTLE-TRANSCRIPT.md)：同一写出器核实际扫描、值缓存、比较与命名空间关系；原版全文和篡改反例分别验证。内部见证、公共批次/进程来源、安装与正式选用分别接收。

当前源码新增[多变量 Datalog 入口](SEMANTIC-DATALOG.md)：`kernel datalog --input` 委托已有半朴素推导器，返回完整闭包及可选查询。 `kernel datalog-graph --input` 另接收字典节点/边快照，由原图导入 owner 接续同一推导器；原始记录数与实际新增事实数分别返回。实际构建、安装与平台接收分别以维护记录为准；推导本身不批准工程关系。

## 工程能力与责任

[事实时间线与修订区间关系](SEMANTIC-TEMPORAL-FACTS.md)按原算法返回完整事实和输入位置，保留重复、缺失与解析失败；适用性、因果和采用仍由当前 Project 审查。

[时间区间计算](SEMANTIC-TEMPORAL-INTERVALS.md)提供关系、指定时刻有效性、缺口、时长覆盖与粒度归一化，供来源版本调查使用。它不采样当前时间、不写 Project，也不把时间覆盖升级为工程验收；实际选用资格核维护回执。

连续推理通过[有界 Datalog 动作](SEMANTIC-DATALOG-SESSION.md)保留添加、查询、清空及失败后的真实状态，复用同一 Rust 推导器。它是一次请求内的过程记录，实际平台与安装资格仍按维护回执核对。

| 工程问题 | 责任与合同入口 | 分别核验的范围 |
|---|---|---|
| 要做什么、为什么、满足哪些条件 | `oe-engineering` 组织 work/Harness；`oe-kernel::project` 维护同一 Project 的对象、版本、条件、决定、依赖与重审；[根目标锚定](../references/goal-anchoring.md)维护根请求、根交付物、阶段关系与偏移检查 | 完整总目标与局部贡献、必要验收、当前条件及来源；关系声明通过不能替代整机合格 |
| 怎样实现功能、比较方案 | `project work`、`project design-attempt`、`project design-comparison`；Rust 计算与模型适用检查 | 单位、假设、载荷和边界；独立设计意图与原全部验收；局部指标改善、设计通过和制造采用分别判断 |
| 书籍和供应商资料如何支持判断 | `oe-knowledge` 的登记、检索、原页、模型线索与反馈；[工程工作](../references/engineering-work.md) | 六书职责、来源页、资料版本、适用/不适用条件、反例；检索命中或 OCR 成功不认证工程参数 |
| 现在有哪些变化、失效决定和缺项 | `project task-view` / `verify-task-view`；[当前任务](CONVERSATION-METHOD-USE.md) | 完整原 Project/work 与同版本变化、来源原件、当前工程检查；派生视图只读，下一动作仍待编排 |
| 聊天中的经验依据何在 | task-view 的显式 `/2`、原 `oe-knowledge::conversation`；[原消息与方法使用](CONVERSATION-METHOD-USE.md) | 选定原消息的角色、时间、文本、位置与当前方法事实绑定；缺失/伪造/漂移及恢复；来源一致、原因解释、实际使用与收益分开 |
| 长资料、图片和上下文判断 | 当前 Agent 按[协作编排](AGENT-ORCHESTRATION.md)处理；已有登记页、OCR/typed 和明确专用工具按各自合同使用 | 读到和未读到的范围、必要条件、原问题、反例、双预算、模型身份、实际 usage/费用、失败恢复；仍交由同 Project 正式审查 |
| 米思米案例及零件如何进入设计 | supplier 与 `project incad`；[来源接入](CAD-INCAD-INTEGRATION.md)、[CAD 暂存](INCAD-CAD-STAGING.md) | 官网原规格/参考答案、实际下载与文件闭包、真实 CAD 原生核验、选型适用性；已曝光案例作为开发回放 |
| NX/SolidWorks 如何执行设计 | `oe-cad`、NX 协议/controller/worker、有限 SW Rust worker；[独立执行](CAD-INDEPENDENT-EXECUTION.md)、[工程交付](ENGINEERING-DELIVERY.md)、[SW 范围](SOLIDWORKS-NATIVE-WORKER.md) | 原生动作、完整对象与装配、单位与变换、设计意图、保存及独立进程重开、模型/图纸/BOM/工艺/检验依赖；各后端按其当前源独立取证 |
| 如何形成施工图及检验依据 | 数值、孔列、原生图纸意图与 CAD handoff；[数值交付](../references/numeric-cad-delivery.md)、[图纸关联](../references/native-drawing-associations.md) | 尺寸、公差、基准、材料、工艺、装配与测量方法、覆盖及不确定度；几何参考页和视觉演示不替代制造图或实测 |
| 哪个结论现在可以采用 | `kernel review --project`、工程前提/数值/制造检查及现有治理 owner | 当前对象/条件/来源、必要检查、失败/未知/覆盖与批准；语义支持、工具状态和工程采用分别保存 |
| 如何检验并使用改进 | knowledge experience、既有 Rust learning/evolution/version owner；[有证据的改进](../references/regularized-evolution.md) | 冻结原目标、来源、判据、预算与完整拒绝历史；独立任务的质量/成本、实际下一使用反馈、精确恢复；无新增表达时实例或 no_delta |
| 改代码如何保持与含义一致 | `specs/` → Rust 规范化 RDF → 锁定 ggen → 生成合同与手写算法；[设计方法](../references/design-method.md) | 功能、输入输出、副作用、错误、状态、来源、平台及检查对应；真实生成、漂移、当前构建和独立接收 |
| 确定下一验证及探索路线 | [探索接续](ENGINEERING-EXPLORATION.md)、[设计尝试](DESIGN-ATTEMPT.md)、[方案比较](DESIGN-COMPARISON.md) | 参数优化与机制探索的前提分开；freeze/verify/run 与 advance 按实际合同接续，requires_host_agent 是等待，results_recorded 只说明结果记录；共同父目标与必要验收保持 |
| Agent 组织任务、并行与回读 | [会话编排](AGENT-ORCHESTRATION.md)、[工具接入](TOOL-INTEGRATIONS.md) | 独立任务可并行，有依赖的步骤先回读；来源、失败与未知保持，实际方法收益另验 |
| 保存过程与读取历史方法 | [Harness](../references/harness-context.md)、[来源接续](USAGE.md#旧来源和真实过程接续) | 完整条件、纠正、未知与必要验收保持；归档、not_run、来源匹配和实际使用效果分开；memory 可选 |
| 展示工程过程或检索关系线索 | [工程图示](ENGINEERING-PRESENTATION.md)、[Rust 搜索](EXPLORER-SEARCH.md)、[typed 抽取](TYPED-RELATION-EXTRACTION8.md)、[登记页 OCR](REGISTERED-OCR-MODEL7.md) | 图示、命中、抽取与 OCR 分别提供线索；原页适用、真实 CAD、连续行为、制造和工程采用仍按对应证据判断 |
| 七种外接专业能力：电子设计、GitContribute、LeanToken、REA、Jacobian、SmokingGun、flameox | [专业能力的作用与交接](../references/professional-capabilities.md)；按本轮问题进入相应 skill | 共用 Project、原生审查及变更复核；专业工具返回原件并补独立验证。电子工具内部按 EDA/仿真/模型/接线分工；安装、执行、工程适用各自核验，不要求全部调用 |

按需继续读对应部分：[来源与方法](#来源与方法)、[语义与数据](#语义与数据)、[CAD与工程交付](#cad与工程交付)、[判断与路由](#判断与路由)、[构建与平台](#构建与平台)。

## 当前版本与记录的适用范围

实际程序、安装版本和通过范围从 `var/maintenance/status.json` 的 `current_cli`、`latest_round` 及对应完成回执核实；源码、生成合同、程序、输入、行为、安装、正式选择与工程采用分别取证。整体仍为“重构中”。[发布范围](RELEASE-SCOPE.md)限定候选，不能用历史测试数量或接口存在替代当前资格。

本源码候选 `2.0.0-rc.7` 经 kernel 显式编入 `source-session-local-v1` 及 `rdflib760-nullable-internal`，支持合同声明的 source-session `/1–4`；其中 `/4` 显式增加分阶段状态与 Turtle 见证，`/1–3` 含义保持。独立库无默认 feature。公共查询保持 `/1–4`，`/5` 拒绝。`dist release check-profile --profile oev2.rc5-local-source/1` 绑定历史 rc.5 的精确版本与 feature 集合，在 rc.7 上应拒绝；RC.4/RC.5 门槛保持原身份。当前统一安装使用 `oev2.installation/1` 记录八程序与源码身份，行为及工程资格另验。

专用判断服务目标按本次用户要求移出当前范围；其余原工程目标、CAD16 及 Semantica 的 19 模块、19 协议、22 owner、13494 范围条目保持，有限支持不缩减这些验收。方法改进沿原 RRSI 范围，独立迁移和实际收益另验。

本轮从索引移出的过程、历史测试/用量与未闭合记录，原文连同较早记录另存于私有维护记录，带冻结来源身份，不随包分发。历史“当前”“默认关闭”和数量属于原记录，不能覆盖现行发布合同或取得新版本资格。

## 来源与方法

《工程本体论》指导对象、关系、条件和来源的表达；《产品可信工程》指导主张、证据、验证与变化追踪。Roloff/Matek 支撑元件承载与寿命，Norton 支撑运动与动力学，互换性与计量教材支撑公差及检验，DFMA 支撑制造装配。米思米连接实际型号、安装限制、图纸/CAD 和实现条件。

它们围绕具体问题跨来源使用，并保留页、版本、单位、假设、失效边界及反例。两卷方法书的原29章资产和预期继续逐项接续；登记可读、查询命中、模型阅读和工程适用分别证明，不能据单页或检索覆盖宣布整本知识已完成。

## 语义与数据

公共 Turtle 的当前前置依赖见[有源连续动作与观察时机](SOURCE-SESSION-LIFECYCLE.md)：补快照/差异/辅助查询与早期构造失败，区分状态观察副作用，按真实回执接收。正式版本与公共接口保持各自验收。

内部候选增加[命名空间状态见证及连续动作](SOURCE-NAMESPACE-WITNESS.md)，连接前缀生成、查询、更新与导出。只读捕获、结构检查、原库对照和实际平台分别验收；生成类型不表示公共 Turtle 或正式安装已接收。

图与上下文复用同一数据 owner，Project 是工程决定的唯一来源，知识包治理复用原 registry。完整接入和未验范围见[Semantica 边界](SEMANTICA-RUST-EMBEDDING.md)，来源映射与实际执行的区别见[范围关联](SEMANTIC-SCOPE-ASSOCIATION.md)。有限兼容不等于全库等价或工程适用。

- 查询目标、初始上下文与 profile：读[查询目标与上下文](SEMANTIC-DESCRIBE-CONTEXT.md)。并列排序、child-IRI 差异及旧失败保留；[分组空值 `/4`](SEMANTIC-GROUP-NULL-PROJECTION.md)区分 RDFLib bindings 与省略 None 的旧 wrapper，不能混作同层 API 验收。
- 约束与规则：读[SHACL 停用](SEMANTIC-SHACL-ACTIVATION.md)、[规则元数据](SEMANTIC-RULE-METADATA.md)及[显式选图](SEMANTIC-NAMED-GRAPH-VALIDATION.md)。停用单条检查不取消其他活动检查或全图预检；原默认入口仍拒绝混合命名图，规则推导不授予采用。
- 内部查询与数值：按需读[字段存在性](SEMANTIC-NULLABLE-TUPLES.md)、[表达式/聚合](SEMANTIC-NULLABLE-EXPRESSIONS.md)、[原始算式链](SEMANTIC-NULLABLE-CHAINS.md)和[共享数值](SEMANTIC-NUMERIC.md)。缺失、None、RDF 与错误对象分开；原有界诊断、IntegerDecimalV1、显式 MixedBinary64V1 和 SourceAggregateV1 各按原合同选择，分组选择不授权浮点模式。私有增量不开放公共 `/5`；全部函数、上下文、计划、wrapper 和平台兼容按各自当前证据核验。
- 有源更新及连续动作：读[Source UPDATE](SEMANTIC-SOURCE-UPDATE.md)、[source-session `/1–3`](SEMANTIC-SOURCE-SESSION.md)和[显式 `/4`](SEMANTIC-SOURCE-SESSION4.md)。来源仅限显式绑定资源，不取得任意文件、网络或 Project 写入权限；动作顺序、失败后观察/跳过、资源预算和总期限保持。旧两项 update 失配保留历史身份，完整格式、跨请求持久会话和声明平台继续分别验收。历史“默认关闭”不覆盖上述 RC.5 编译范围。

## CAD与工程交付

当前删孔版原型的补孔候选使用显式 [有源新增通孔](NX-FEATURE-ADD.md)：在完整来源副本上真实创建，复用完整几何/装配 owner，保留原删除、全部必要验收与失败。当前工作零件上下文修复及方法合并见私有维护记录，原新增通孔实现的保存前失败继续保留。源码、软件、Windows/native、独立保存重开与正式选用分别验收；不从候选代码推定当前平台或工程通过。

执行按[独立执行合同](CAD-INDEPENDENT-EXECUTION.md)使用 v2 自有 Rust 与显式原生 SDK；旧 cad-agent 执行入口/备用和 Fusion 不在当前范围。每项须分别核当前源、实际平台/native、保存、独立重开及同 Project 采用，历史原件回放不能替代本次原生执行。

- 运动及主张：读[原生主张](NATIVE-MOTION-ASSERTIONS.md)、[逐调用捕获与接收](CAD-MOTION-RECEPTION.md)、[装配运动与持久位置](NX-ASSEMBLY-MOTION.md)。保留实际 job/fetch、实体身份、保存失败/未知与进程独立性；原 capture/1 判据不改。
- 构造与复制：读[构造来源 `/1–2`](CAD-ASSEMBLY-CONSTRUCTION.md)和[组件复制/接收](NX-COMPONENT-COPY.md)。分别观察 definition instance 与 part occurrence 参考集；复制的直接叶 profile 核新身份、原父/原型、原对象几何、完整保存及独立重开。构造来源不能代替复制/删除谱系。
- 删除与诊断：读[目标孔删除候选](NX-FEATURE-DELETE.md)和[质量属性诊断](NX-MASS-PROPERTIES.md)。删除限受控两孔板，核真实特征、非目标保持、保存/独立重开；原记录尚未完成接收，使用前核新源回执。旧 declared_only 不改签；mass-probe/1 保留 raw47/statistics13 与 copy/1 原严格失败，测量返回不直接解释原因。
- 圆柱及执行输入：读[NX 圆柱](NX-CYLINDER.md)、[v5 限定恢复](NX-CYLINDER-RECOVERY.md)、[完整载荷发布/单次派发/分段取回](NX-PUBLISHED-EXECUTION.md)及[私有供应商输入](PRIVATE-SUPPLIER-INPUT.md)。圆柱限原 mm/+Z、新体与独立意图；恢复不改签旧失败或 unknown，私有来源权限和重读条件保持。
- 变更及重审：读[特征对应](CAD-CORRESPONDENCE.md)、[当前实例绑定](CAD-OCCURRENCE-BINDING.md)、[实例对应 `/2`](CAD-INSTANCE-CORRESPONDENCE.md)和[当前复核重放](REVIEWED-RECONCILIATION.md)。根坐标几何是有源推导；两端版本/条件、旧新依赖及完整来源须对应。实例正向、创建谱系、split/merge 与完整拓扑各需其自身证据；复核状态不替代原采用入口和独立批准。

图纸、数值、SW 与工程图示入口见上表。有限离散 HTML/SVG、浏览器观察、连续运动、制造检验与现场计量分别判断；本索引不以局部 CAD 通过关闭完整 CAD16 或工程验收。

## Agent、专业工具与记忆

当前 Agent 负责理解任务、审阅资料、调用工具与组织验证；Rust 执行已表达的检查，当前 Project 保存事实和决定。工作方式见[编排方法](AGENT-ORCHESTRATION.md)。

engineering-memory 是同一安装中的正式模块，通过 `oev2 memory` 读取、审阅和保存有源经验；是否采用历史线索按当前任务判断。经验不直接支持决定或关闭验收，具体双侧交接见[专业能力](../references/professional-capabilities.md)。

六工具由用户安装，版本、最小 diff、companion 与验证范围见[接入说明](TOOL-INTEGRATIONS.md)。独立工作可并行，有依赖的活动先回读前置结果。模型意见和多个共享来源的报告不能代替独立证据。

核心流程无需专用判断服务。可选普通模型、OCR、typed 抽取仍各有明确源、预算和失败边界，不成为核心启动依赖。

## 构建与平台

规范经真实 ggen 生成，生成区不可手改；维护后按当前源生成、构建和接收。当前程序使用已编译的库。程序身份、来源捕获、两次完整原生产物、调试/签名、平台执行、安装/恢复和工程采用分别留证；操作入口见[生成与维护](../ENTRY-CONTRACT-DETAILS.md#语义生成编译与维护)和[使用说明](USAGE.md)。

- 公开样例与私有验收：读[独立任务视图样例](CURRENT-TASK-VIEW-FIXTURES.md)和[运行时私有输入](PRIVATE-ACCEPTANCE-INPUTS.md)。私有原件/SDK/凭据不分发，固定身份、缺件拒绝、Windows reparse 拒绝及原字节核验保持；公开编译不替代私有验收、新机离线或平台接收，默认忽略的验收未执行不计通过。历史输入数量不能作为当前完整覆盖。
- 进程与 Windows 输入：按[source-session 合同](SEMANTIC-SOURCE-SESSION.md)核实际子进程、来源、预算与失败恢复；公共进程仍受取消、逐流上限和调用者期限限制。原 Unix 超时和 Windows stdin 失败保留在历史页，局部修复不授予所有负载吞吐或完整平台资格。
- 私有权限与发行：读[Windows 工程权限](ENGINEERING-AUTHORITY-WINDOWS.md)、[私有文件/有限 HTTP 认证](PRIVATE-SUPPLIER-INPUT.md)及[发布门槛](RELEASE-SCOPE.md)。原 owner、受保护 DACL、实际文件身份、权限变化拒绝、锁与原子选择/回退分别核验；Windows manifest/2 的七个程序和 Mac /1 各按原平台合同接收。Windows、Unix、其他文件系统和当前正式选择不能互授资格。

## 全计划与项目条件

本页整理不改变原工作包、完整语义接续或工程验收。总目标与延期项见[发布范围](RELEASE-SCOPE.md)；项目专属授权、进度、闭环状态、专属尺寸及计量要求核项目原件。案例参数只用于其原对象和条件，不成为其他工程任务的默认值。

Historical Turtle preparation stage: source namespace allocation/cache, pinned Unicode splitting and graph/base-aware preprocessing have a Rust owner inside DatasetSession. The later explicit source-session `/4` composes these owners with text and state witnesses; source-session1–3 public format support remains NT/NQ. Current execution, platform receiving and selection come from var/maintenance/status.json. See the original [preparation scope](SOURCE-TURTLE-PREPARATION.md) and current [version4 contract](SEMANTIC-SOURCE-SESSION4.md).

Subsequent [Turtle text candidate](SOURCE-TURTLE-WRITER.md) adds complete document
traversal, retained source value ordering, literals and recursive blank/list
presentation. Independent original text and local resource checks are separate
from public result/witness, installed consumers and formal selection. Existing
source-session1–3 NT/NQ meaning and original SPARQL ORDER restrictions remain.

Explicit source-session `/4` is a source candidate with phased state and Turtle witness receiving through kernel/CLI; [contract](SEMANTIC-SOURCE-SESSION4.md). Its current execution, platform and selection status must be read from maintenance receipts. Source-session `/1–3` remain supported under their own meanings.

有源失效尺寸退役沿[图纸退役](DRAWING-RETIREMENT.md)接续：新参数合同、Rust完整清单/唯一目标/保留关联检查、原生副本删除和独立重开接收分别验收。源码存在不授予当前worker、图面或制造资格。

当前[原生图页PDF输出](DRAWING-PDF.md)接续已绑定图页/标注检查；实际Windows SDK、源副本导出、独立PDF解析和图面可读性各自验收，原制造/计量和全部63目标保持。候选源码不代替当前选用回执。

当前[尺寸引用辨识PDF](DRAWING-IDENTIFICATION.md)在明确新动作内核原尺寸父引用，派生模型Z上下圆心说明，控制WCS、导出后恢复内存状态。原PDF/1与制造计量义务保持；实际平台/图面/采用分别验收。

当前[图页显示调查](DRAWING-DISPLAY.md)复用原尺寸/恢复owner，在新合同内记录可见对象/CSYS/WCS，提供两行源派生附注与显式唯一CSYS临时隐藏；原模型不保存，实际平台、恢复及图面分别接收。

显式[图页显示/2](DRAWING-DISPLAY-RESTORATION.md)接续真实WCS恢复失败，先记录各阶段，再按原当前进程身份受控恢复；原/1文本和完整恢复判据保留，CTRS短注记另经真实图面接收。

显式[已准备图页的只读观察](DRAWING-PREPARED-OBSERVATION.md)接续详细回读改变WCS的原生反例：display/3将一次准备与观察/7分开，未调用的设置操作明确记录，不伪造成功码；原完整恢复、来源/几何/尺寸及工程验收保持，平台/native按维护原件接收。

显式[注记位置恢复](DRAWING-ANNOTATION-RESTORATION.md)接续此前观察到的文字框移位：display/4查询实际原点/居中/原点关联，仅恢复有证据的非关联原点变化，保持/3只读观察及原完整签名；同源平台、真实NX与图面按维护回执分别验收，原全部工程范围保持。

- 局部更新候选：逐标注明确编辑/局部更新、错误与自有撤销点证据；沿 /4 原完整验收及只读 /7。实现与原生结果分别接收，详见 [局部更新](DRAWING-LOCAL-UPDATE.md)；已正式选用的版本不变。

当前单圆名义直径增量沿[有源孔径合同](NX-SHEET-DIAMETER.md)接续，绑定源圆和图页并检查真实保存/独立重开。原制造公差未知及完整验收保留；实际选用核维护回执。

单圆径向标注的实际关联语义增量见[整圆关联/2](NX-SHEET-DIAMETER-ASSOCIATION.md)：0表示整个圆对象，追加独立绝对圆心检查；原/1及失败保留，当前资格核新的保存与独立重开回执。

混合线性/整圆孔径图页沿[显式混合清单](DRAWING-MIXED-INVENTORY.md)接续：原合同保持，新源绑定输出、恢复、独立PDF与图面分别接收，完整制造/计量及原全范围不变。
