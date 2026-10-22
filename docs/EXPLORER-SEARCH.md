# 有源 Rust 搜索入口

搜索承担当前工程资料图中节点候选的词项/别名检索、类型/置信度/标签筛选和相对锚点的有向跳数排序。结果是待查证线索，不直接成为当前工程条件或采用结论。与工程本体、ggen、Rust执行及RRSI的关系沿SEMANTICA-RUST-EMBEDDING.md；索引不另建Project、来源授权或改进评分。

`services http-search`沿同一个有限HTTP宿主运行。agent提供`--context-graph`和原字节`--context-graph-sha256`，必要时选择`--resource-profile semantica.context-50k/1`；默认原dataset资源。使用已有私有`--api-key-file`或显式本地`--allow-anonymous`。只监听loopback，连接及宿主有界；不是新建一个常驻全功能Explorer。

POST `/api/graph/search` 的原请求字段为query、filters、limit、anchor_node、max_hops、min_semantic_similarity及rank_by。请求、同一数据会话的revision/摘要及结果合同均来自specs和真实ggen。Rust原模型先验证，索引先截取limit，再按锚点筛选并稳定排序；负/零跳数保持原行为。原min_semantic_similarity未参与搜索，响应semantic_similarity仍为null；非有限参数保留原输入，以原类摘要锁定的执行投影处理，不伪称完整模型已支持所有非有限值。

所有图发布使派生缓存失效；缓存只绑定同一revision/数据摘要。原12000节点次级扫描上限、命中上限及128条LRU保持。索引/posting/LRU按结构JSON逻辑字节和原查询缓存共享所选允许值；不是RSS保证，也不将超预算改成静默少搜。Clone不共享可变缓存，服务回查来源，过期或来源改变不发布成功。

当前UTF8有限HTTP处理顺序为路由/方法 → JSON解码 → host认证 → 原模型 → 有源执行。JSON字段重复按同一JSON owner的有限Python数值/last-key-wins显式profile，源资产strict_json规则不变；语法诊断保留原字符位置。其他编码、原JSON非有限literal、全部原Mutation历史/strip冲突、整个Server/UI及全库迁移仍另验，不能从本路由语料继承通过。开发回归不是未见任务的泛化收益；正式采用继续要求当前构建与新根接收。
