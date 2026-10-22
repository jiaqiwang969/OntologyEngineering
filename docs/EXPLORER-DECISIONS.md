# 有源决策观察入口

`oev2 services http-decisions` 读取同一工程资料图中的决策记录、邻居链、先例及违规关系。沿现有 HTTP 宿主提供 `--context-graph`、`--context-graph-sha256` 和原私有认证参数；资源使用默认配置或显式 `semantica.context-50k/1`。来源改变、超预算与截止时间仍阻断结果。

五种 GET 操作为 `/api/decisions`、`/{decision_id}`、`/{decision_id}/chain`、`/{decision_id}/precedents` 和 `/{decision_id}/compliance`。借用同一 DatasetSession、来源/修订、预算及模型校验，不另建工程事实或经验账本。规范经真实 ggen 生成合同；Rust 执行原图观察；RRSI 的候选验证和实际收益仍另行取证。

兼容入口保留原路由顺序：`/api/decisions/causal-distance` 在原库被普通 ID 查询遮蔽，本入口也按该 ID 读取，不能将其响应称为因果距离已计算。修正版使用同一宿主的 `/api/v2/decisions/causal-distance?source=...&target=...`，采用显式 `/2` 问题/请求/结果合同及响应 profile。它调用图上因果边计算，而不是读取同名节点。缺参数拒绝；显式空字符串保留原不可达语义。邻居链不是物理因果证明，先例排序不是当前方案适用性；`compliant: true` 仅表示没有匹配的违规边，不说明必要验收已完成。

原字段 ValueError 对应 422，TypeError/OverflowError 对应 500；来源、预算、未知模型故障和超时不冒字段错误。输入是有限 JSON/HTTP 配置。原 GraphSession 的索引构造拒绝按[统一初始化流程](EXPLORER-INITIALIZATION.md)在 ready 前执行；完整宿主的全部数据形态等价仍需验证。本入口不宣称完整原 Server/UI/平台已迁移。

修正版保留原七种精确因果边类型、有向首次到达 BFS、边顺序和小数舍入；这不是最大置信路径。权重乘积保留原负值及大于 1 的结果，不当作概率。算法、原 HTTP 路由遮蔽及修正版实际请求各自取证。非有限乘积、其他图存储后端和全部宿主初始化行为仍需继续验证。
