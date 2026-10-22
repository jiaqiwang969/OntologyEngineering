# 原值 Explorer 的同会话接续

工程本体规定含义 → ggen 生成合同 → Rust Semantica 执行 → RRSI 根据反馈检验改进。此增量属于 Rust 通用能力的原行为接续：库查询得到的节点仍是线索，须回到同一 Project、原页、条件及必要工程验证后才能支撑决定。

`DatasetSession::prepare_native_explorer_index`、`search_native_explorer`、`observe_native_explorer_index` 和 `checkpoint_native_explorer` 使用同一 context namedgraph 的显式对象状态 /4、既有 GraphSearchIndex、mutation namedgraph、修订和共享缓存额度。没有第二图或新的治理身份。共享 `SemanticExplorerNativeState` 与 `SemanticNativeExplorerSearchResult` 由 specs 经真实 ggen 产生；实现不启动 Python。

对象状态 /4 分开保存节点 id、type、content、metadata、properties、valid_from 和 valid_until；边保留自身字段。初始化临时派生原 ContextNode.to_dict（properties 后覆盖 metadata，再写 content 和非空 validity），随后标准化并建立索引。查询临时派生原 ContextGraph.find_node（metadata 后覆盖 properties，另给原 content，不给独立 validity），再标准化。两种视图不能互换，派生视图不成为第二份保存状态。

metadata 与 properties 的覆盖、相等键首位置、显式 None、falsy 内容、容器、surrogate 和非有限值保持有类型表达。旧 /3 单对象记录投影仍原样保留，但无法恢复被覆盖字段，不能静默升级为 /4 或用于本轮原查询。完整 ContextGraph 配置、回调、导出、边身份、typed add 与 HTTP 尚须各自接续。不能表达的 Python 对象身份（例如 NaN 键）明确拒绝。

查询先使用索引，再按命中 ID 查当前图记录并标准化结果。原版标准化可能失败，但此前索引已更新 LRU；这类原错误保留其已发生效果。资源额度、截止时间和不可表达对象域的拒绝保持旧缓存。原始图 ID 与索引归一化 ID 不混淆；原版空格 ID 无对应命中的行为继续保留。

live 缓存绑定完整 dataset 摘要及发布修订。检查点沿原 mutation namedgraph 保存 context 载荷原字节 SHA、bridge、原 graph_revision、类型历史与当前索引。检查点只增加 dataset 发布修订，保留原 GraphSession 变更计数；发布后重新绑定当前 LRU。载荷来源、封闭字段、原值载体、类型索引及索引内部不变量在恢复时检查。旧 profile 或来源错配拒绝，不自动重建来掩盖漂移。

验证入口是 `crates/semantica-runtime/tests/native_session.rs`：真实原版 43 次标准化、11 次构造、96 次查询逐步保存重载，以及失败结果转换之前已完成的缓存变更。另核对“两个不同内部节点得到同一 to_dict、却有不同 find_node”的信息丢失反例。原版构造输入保留其实际有类型值与容器顺序，不由查询预期反造内部字段。宿主预算、期限、来源和模式反例另验；fixture 是已暴露的开发材料。实际运行及接收以本轮私有回执为准，本文件不授予完整库、HTTP、独立平台或工程采用通过。

下一项接续是原五类 typed 变更及其部分失败、边默认身份的原序列化规则、JSON/CSV 导入和 HTTP 各路由序列化。保留当前工作版本、既有失败和原范围；方法收益须在冻结目标、条件和验收下，经未见任务验证。
