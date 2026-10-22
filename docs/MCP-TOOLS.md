# Rust MCP 工具与原行为

工程本体规定问题、条件、对象和证据的含义；`specs/engineering.toml` 记录端口和约束，官方锁定 ggen 生成合同；Rust Semantica 执行工具；实际使用结果再进入既有反馈和方法验证流程。MCP 数据集中的一条“决策记录”是观察记录，尚不是 Project 授权、事实采用或方法升级。

`oev2 services mcp-stdio` 保留原来的有限只读入口。显式 `--tool-profile native` 启用中性 Rust 工具，`--tool-profile original` 保留已冻结的原 canonical 异常。`--book-catalog` 使用已经登记、已批准字节身份的历史资料源。stdio、查询、实体、关系和决策操作使用同一个借用的 DatasetSession；服务输出预算或结果合同失败时不会发布拟写变更。模型抽取端口须由现有 SharedModelCallContext 和 oe-model-credentials 的显式凭据 owner 提供，当前内置 pattern 算法不会发起模型调用。

| 原工具 | Rust 执行路径 | 条件与剩余范围 |
|---|---|---|
| extract_entities | `runtime::extraction`，generated ExtractionPort | 原模型不可用 pattern fallback；spaCy/学习型 NER 未由此证明 |
| extract_relations | 同一 extraction port，原纯 pattern | 原 canonical 缺 entities 的实际 TypeError 保留；native 修正版有单独 profile |
| record_decision | `runtime::context` 原字段校验和节点/关系写入 | UUID4/有效期/元数据意义保留；一次事务发布是显式 Rust 变化，不自动工程采用 |
| query_decisions | 同一 context records 和原 query/category 分支 | 原 category 位于 metadata 导致类别分支为空的行为保留；相似性是原词集合＋结构分数 |
| find_precedents | 同一决策记录和相似性计算 | 原有效期/阈值/limit；同分候选固定插入顺序，原 Python set 顺序不作为跨进程合同 |
| get_causal_chain | 原大写因果边与 BFS/depth/排序 | original 保留非空 Decision 不能直接 JSON 序列化的错误；native 显式 to_dict 修正；数字时间戳保留 Unix 本地时区条件 |
| add_entity | 同一数据集的 context 记录 | 原 label 和 metadata 嵌套；空白 ID 原 no-op；工程事实仍由既有采用门禁审查 |
| add_relationship | 同一数据集，原内容 UUID5 | 缺端点自动补 entity；原重复关系 no-op；原 SKOS broader/narrower 归一化后拒绝受影响分量的循环，失败不发布；原完整 ContextGraph 其它公开管理 API 不由此闭合 |
| run_reasoning | 原高层 Reasoner 字符串规则算法 | 原默认50轮、同轮连锁与原替换次序；有明确资源上限，未涵盖类全部管理/回向链 API |
| get_graph_analytics | 明确修正版 native 有限图计算 | 原实际 PageRank RuntimeError 保留；native 返回实际 PageRank/社区，原第三方全部算法仍单列未覆盖 |
| export_graph | 同一 context named graph 的现有 RDF serializer | 原 ContextGraph/Exporter 形状错误保留；native 是 context 记录导出的新 profile，未冒充原全部知识图谱导出业务 |
| get_graph_summary | 同一数据集的经核对 context records | 原节点/decision 语义，不以任意 RDF subject 数冒认 context 节点 |
| list_chapter_packages | 原登记 Catalog 和批准 archive | 29章＋固定 normative；不是新 registry |
| get_chapter_package | 同一 Catalog 精确 manifest | 只允许原固定公开 package ID，不接受任意源路径 |
| verify_book_sources | 同源 Catalog 原完整 checker | 238 来源判据保留；限制为 host 已注入的登记根，missing/not_run 和执行后 blocked 分开 |
| run_chapter_package | 原场景选择/实际操作/原 oracle 比较 | 省略或空串 scenario_id 使用唯一默认场景，其它字符串 ID 保留精确匹配；本次隔离 task session 用同一 DatasetSession 算法；RDFC 新摘要与历史 RDFLib 摘要分开，历史回执不改写 |
| verify_chapter_package | 同一次 runner 的实际 receipt/snapshot proof | 必须原 CQ/SHACL/oracle 和 declared release 均成立；当前编译 origin 未连唯一版本 owner 时仍 blocked，不能自填 pins 升级 complete |

`oev2 books runtime-identity` 只观察本次源码和可执行文件，供输入 pins 比对；它明确保留 `compilation_origin=unknown`。它不是构建来源证书。原 approved archive SHA 不随这项 Rust 观察更换。

可调用工具名称和实际通过行为是两项证据。原完整 13,494 项 scope 及原文件/测试断言保持，以上新测试不能代替原库范围验收。模型/SDK/其它服务器协议、原类其它方法、真实工程采用和可信构建监督尚须逐项接线与验证；现有 book authoring 原资料、两本方法书、四本机械教材、米思米和工程反馈主线不迁移成第二套权威。

书场景 SELECT 的新兼容投影把 Oxigraph 统一的 xsd:string 输出映射为旧未显式类型 literal 的 datatype=null；原 RDFLib 能区分显式 `^^xsd:string` 与未显式类型字符串，而当前 dataset 不保留这个词法区别。这是一项明确的新 profile 边界，不能据此改写旧迁移复核的原始失败、声称任意 TermDTO 等价或全库通过。

旧发布资料清单也从同一个已批准 archive 读取，继续核原 source-lock 和完整 RECORD，以及 29 包、333 个资料角色的原判据。出现当前登记或历史资料目录时，缺登记或字节不符会拒绝，不会退回旧 `runtime/vendor`。现有来源快照同时绑定 continuity 原字节。未登记的历史静态测试根仍保留原清单和反例条件；它只用于静态资料读取，不恢复旧 Python 执行。

原 RDF 导出接口的完整初始化条件另有独立对照：实际执行原 NamespaceManager、RDFSerializer、RDFValidator、RDFExporter 构造函数，以及 MCP 的 ContextGraph 构造和 handler，五种格式均返回 ContextGraph 的 `get` 缺项；同一原 exporter 的字典输入五种格式可以序列化。早期新增 supplement 用 `__new__` 跳过初始化，turtle/ttl 的 namespace_manager 缺项只属于那个部分构造条件。旧样例与失败回执保留，本轮修正新增 Rust 测试的条件并增加完整初始化案例，未修改原 Semantica 测试或生产实现来模拟坏构造。
