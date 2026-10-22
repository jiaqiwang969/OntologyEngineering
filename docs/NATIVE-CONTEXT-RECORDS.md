# 同一数据会话的有类型图记录

工程本体规定含义 → ggen生成合同 → Rust Semantica执行 → RRSI检验反馈。原文件解析保真之后，图记录也需要保留对象身份、属性类型和原值，后续工程适用性由既有语义和来源审查承担。

`context::store_native_records/read_native_records` 显式使用既有 SemanticNativeValueEnvelope，其根是只有 nodes/edges 两字段的有类型字典。节点和边保持原 ContextNode.to_dict / ContextEdge.to_dict 的单对象记录投影（边端点字段 source_id/target_id），而非转换成普通JSON DTO。节点投影已按原规则合并 properties、metadata 与 content，不能据此恢复它们原来的分立字段身份。完整 ContextGraph.to_dict 导出另含 statistics，边使用 source/target/metadata；完整导出、单对象记录与 GraphSession 搜索文档需各自按原适配规则接通。身份为Unicode码点文本，可保留surrogate；属性支持None键和非有限float位，额外数据仍只是数据。结构检查确认唯一节点/边及端点归属，不证明原值适用于工程。

记录放入现有 `urn:semantica:context:records/1` 命名图，使用 `semantica.context-record-projection/3` 标记和单个有类型记录载荷；替换此前的context记录，保留同一DatasetSession的其他图。共享合同继续来自ggen，复杂算法手写Rust。没有第二个权威图、Project、来源注册表或服务额度；该图的通用SPARQL只看见标记和编码载荷，尚未实现新版字段投影/查询。

宿主负责当前来源/治理授权并显式传入原限额、截止时间。原session的发布器检查合并后全部RDF字节/quad预算，推进修订并清除查询、快照和派生搜索缓存；暂存克隆用于晚拒绝不发布。原旧变更索引元数据不被重签，旧读取明确拒绝新profile，不从旧索引给出假命中。回读重新核实际RDF和完整有类型值。

12组真实原HTTP导入的节点/边图值已用于保存、逐值回读和N-Quads重载验证。该层不保存完整原GraphSession的查询缓存、索引和graph_revision，不冒全会话恢复。下一步必须接原值索引、变更及HTTP入口，完成后再做独立接收和正式合并；当前仅是存储依赖候选。

原版96次同会话HTTP观测还显示：NaN边导入成功但边列表422；surrogate节点的节点列表/搜索和CSV导出422；JSON导出可转义surrogate及输出NaN。节点属性NaN在部分响应中变null，但原存储和JSON导出仍是NaN。CSV额外列形成None键，与字符串null同时存在时，原JSON导出会出现两个null文本键。必须分别迁移原存储与原接口序列化行为，保留原始响应字节，不能用一种归一化方式抹掉差异。

这些原版观测和当前回归是开发证据，不能当未见泛化、RRSI收益、完整Semantica或实际工程项目交付完成。

后续的[原值操作与搜索文本准备](NATIVE-VALUE-OPERATIONS.md)已复用同一值层、Unicode与索引模块；有类型Document、倒排表和完整索引状态仍须继续实现。
