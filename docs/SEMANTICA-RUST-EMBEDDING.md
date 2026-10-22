# Semantica Rust 全库接入

状态：重构中。主线为 **工程本体论规定含义 → ggen 生成合同 → Rust Semantica 实现执行 → RRSI 根据反馈检验改进**。工程本体论明确目的、承担功能的对象、关系、版本、条件、来源、证据和变更影响；ggen 将这些约定生成共享合同；RRSI 方法检验改法在原目标和判据下是否有效。

工程本体论与 Rust Semantica 属于同一体系，是方法与程序实现的关系。Rust Semantica 是工程本体论的代码化核心，其实现使用 ggen 生成的合同。完整代码化包含通用能力、工程本体合同、领域规则和执行编排。规范到合同的构建顺序不改变工程本体论的主导责任。新增能力先解释承担什么功能和适用条件，再登记规范、接口、实际实现和独立验证；生成完成、计算完成、语义符合与工程采用分别记录。

## 同一执行与清晰依赖

- 编辑正本在 specs；Rust 规范化 RDF，经锁定的真实 ggen 生成 engineering 和 semantic 两组产品。生成区不能手改。
- semantica-contracts 只包含生成的通用类型、错误和接口；semantica-runtime 实现算法，不依赖 oe-*、Project、CAD、模型凭据或私人环境路径。
- oe-kernel 保留当前 Project、条件与专业采用判断；RDF、SHACL、规则委托同一份通用算法。工程查询入口使用实际生成的 SemanticQueryPort。
- oe-semantic 提供历史来源归档、YAML/资产读取与受控持久化适配；通用书籍算法接受注入的 SourceProvider，不硬编码私有资料根。
- 模型、来源、存储及设备通过明确适配供给。复用现有来源/权限/预算/经验事件，不另造项目、晋升记录或账号系统。

两卷方法书、四本机械教材与米思米继续沿同一工程问题参与工作，Agent 核查来源及适用条件。已闭环项目保留其历史闭环；从专利/视频到施工图、制造与实测的实际工程项目是后续实际使用路径。库迁移案例不能代替整机工程验收。

## 反馈如何回到工程本体

实际聊天、用户纠正和工程执行先形成有来源的经验候选，保留对象、版本、工况、目的、行动、结果与未知。软件修复、操作方法、领域知识和项目事实沿既有分类分别审查；已有表达足够时保留实例或 no_delta。RRSI 比较的是改法在冻结目标和判据下的效果，不能自行改变含义或判据来获得收益。

若反馈说明对象关系、适用条件或证据要求需要修订，先形成可审查的本体/规范候选，重新经 ggen 生成合同，再编译执行与验证。验证后的候选仍由同一版本与采用入口管理；实际选中使用后的反馈写回既有经验事件。工程本体指导生成与执行，执行证据供给下一轮改进，构建依赖不倒置这条责任顺序。

## 完整范围和证据

参照锁定 Semantica 0.6.5+oe.7 的全部源码和原始接口。360 个包内 Python 文件、根 CLI/server/worker、外部 integrations、MCP 和 Explorer 的交互逻辑均在范围内。原有占位/抽象方法保留其原身份。前端状态、布局、交互仍须迁移到 Rust/WASM；仅将后端改成 Rust 不构成全代码库完成。

每项能力须连接：原源码/API和测试身份 → 工程功能/条件 → 规范 → 生成合同 → 实际生产实现 → 原场景及独立反例 → 平台/后台/调用入口 → 实际使用反馈。静态盘点、空 crate、名称、通过的局部测试和开发回放不能抵扣未迁移能力。每项明确 implemented、behavior_verified、native_verified 或 receiver_verified 的实际范围；未执行保留未执行。

`design semantica-scope` 核对 `runtime/semantica-scope.json` 和固定原身份，显式输出完整范围及当前未知状态；`--require-complete` 对未完成范围失败。该检查属于迁移任务身份，既不建立新的工程本体或Project，也不授予知识包晋升。声明完整Semantica来源的规范在Rust规范化与真实ggen之前执行同一范围检查；编译来源与派生source-map绑定这两份范围资产。原范围缩减、自报verified和重签替代固定身份均拒绝。

新规范化快照显式绑定 RDFC1.0 SHA256 profile。它不继承旧 RDFLib 全图规范化摘要或旧通过回执；精确旧摘要兼容仍须单独迁移验证。术语词法值、named graph、blank node 上下文、原 UTF8 和来源摘要分别保护；无法无损处理或超预算时拒绝，不能静默转换。

## ggen 与 RRSI 的接续

候选先冻结原目标、规则、预期、数据、前驱和拒绝历史，再修改规范/模板/算法，在隔离根实际生成、锁定构建和执行验证。失败保持原工作版本。测试预期与候选不得同时被调整来获得通过。

现有 knowledge experience 支持有限操作方法的冻结、对照、试用、反馈和恢复。旧流程固定同一评估执行器；它不能直接把新库二进制作为同一评估身份，也不能作为全库版本切换。完整库改进仍需固定的独立评估驱动，分别绑定前驱/候选的 source、spec、generated、build、资产和可执行物，对同一判据进行比较，再选择实际版本、记录使用反馈并精确恢复。保留旧执行器保护，不能用重新 freeze 不同执行器冒称同基线比较。

当前隔离实现和正式根分别取证，主根尚未合并的能力保持候选身份。完整库、平台、外部后台、Explorer 与完整版本闭环未逐项闭合时，整体状态保持重构中。


## 初始绑定与 MINUS 的当前作用域

`graph::query_with_options` 使用同一锁定 `spareval0.2.7` 求值器；`vendor/spareval` 保存完整 Rust 来源、原文件 SHA 与上游 commit、MIT/Apache2.0 许可证。受控补丁将可读取的 application initial context 与 `Project` 的显式结果域分开；它不创建第二 Dataset、不替换 SPARQL 字符串、不改变 RDF 词法、语言、类型或 blank node 身份。无初始绑定继续走原求值路径。

原一行 `MINUS` 反例保持原预期，旧错误空集和后续拒绝测试原字节归档。当前新增共有域、互不相交域、左右投影、嵌套 subselect、OPTIONAL/UNION、空右侧及原上下文对照；typed/blank/lang 和无绑定回归分别验证。原 15 项保留，14 项按原结果验证；隐藏初始参数的 `DESCRIBE`、初始目标重赋值/aggregate target 等未实现边界继续明确拒绝。当前支持这些有限场景，不能据此声称完整 SPARQL 或全部 Semantica 等价。

同一规范声明 `semantic_evaluator` 的责任、来源、功能、约束与独立测试。真实 ggen 的源映射和 normalizer/CLI 编译来源核验均覆盖本地求值器；新增、缺失或改字节拒绝。原完整 Semantica scope/lock/mappings 保持其冻结身份；新增实现闭包不重签原库范围。上述查询一致性支持语义执行能力，不授予工程采用或全版本 RRSI 资格。


## 当前聊天凝练交接

当前 CLI 的凝练回执使用 `oev2.consolidation/2` 与真实 ggen 生成的 `ConsolidationHandoff`。当前引用指向本根 `oev2 semantics propose` 和 `oev2 kernel review`；仅生成交接时保持 `current_registry_not_verified`、`not_created` 与 `not_performed`。原 `consolidate` 数据接口及测试作为历史协议保留，旧入口内容在新回执单列 `historical_handoff`。有正面模型答复仍须 agent 整理有来源与条件的 PackageDelta，由当前 Rust 执行器验证；无新增经验保留 no_delta。轨迹、原件、共享上下文和资源保护使用同一实现，交接本身不授予晋升或工程采用。


执行 provenance 的 storage-free Rust 接续新增完整39字段 Entry、原 checksum、Bundle链/完整内容摘要和 Receipt绑定，见 [执行 provenance](EXECUTION-PROVENANCE.md)。同一 BookRunnerState 的实际 retained run 可产生新 profile完整bundle；旧观察与原断言保留。单项行为证据、编译来源、发布资格、工程采用及完整13494原范围分别验收，整体继续重构中。


完整原导入涉及NaN、非字符串字典键及孤立surrogate，不能用普通JSON转换抹去。当前[有类型原值](NATIVE-VALUES.md)由specs定义共享SemanticNativeValueEnvelope，真实ggen生成，Runtime委托同一semantica-json中性值层；严格wire规则和唯一Project/来源/预算保持。原值保存、原JSON/CSV解析、实际图写入/HTTP及工程采用分别验收；六份原typed状态只支持当前数据保真，不是全库或RRSI迁移完成。
