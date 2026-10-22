# 事实时间线与修订区间关系

`kernel temporal-facts --input REQUEST.json [--output RESULT.json]` 将完整输入事实、时间与输入位置一起返回，供工程来源调查和变更分析使用。`timeline` 整理事件；`revision-impact` 按原区间算法分类。结果不修改 Project，也不直接批准事实、撤销决定或证明因果关系。

请求是 `semantica.temporal-facts-request/1`，包含 `operation`、`graph`、`entity_id`、`revision`、`facts` 六个字段。timeline 的 graph 为带可选 entities/relationships 数组的字典，entity_id 为 JSON 标量（包括 null），revision 为 null、facts 为空。revision-impact 的 revision 是字典、facts 是原事实有序列表，graph/entity_id 为 null。内部字典的其他字段完整保留；外层未知字段拒绝。

时间字段为 valid_from、valid_until、recorded_at、superseded_at。字符串日期复用现有 UTC 微秒解析；缺失/null 表示未提供，两个终点也接受精确 OPEN。数值 epoch、Python 时间对象及复杂标识符不在本运输范围。标识符沿已有 Python 字符串 owner，因此 null→None、true→True，数字 1 和 1.0 分别匹配各自字符串。

timeline 先遍历匹配的实体，再遍历端点匹配的关系。实体按 id 字段存在性选择，缺少 id 时才退回 name；id=null 不退回 name。事件依次是 valid_from 的 added、与起点不同的 recorded_at 的 modified、superseded_at 的 modified、valid_until 的 removed。按实际时间、change_type 稳定排序；相同时间的重复修改仍保留，未匹配记录的时间不解析。原算法不校验起终倒置，时间线不能代替有效期合法性检查。

revision-impact 使用原版名称和方向：原事实区间包含修订区间时为 affected；否则有重叠时为 partial，其余 unaffected。修订区间反过来包含旧事实仍是 partial，包含判断允许端点相等。这些名称不能直接解释成工程影响程度。缺少起点使用原最小时间，开放终点按原最大时间比较。仍解析 recorded_at/superseded_at 并保留错误；原版缺少 recorded_at 时的时钟默认值对这两个投影结果没有可观察影响，Rust 不采样时钟，也不输出归一化 BiTemporalFact。原列表为空时不追加原版未执行的区间顺序检查。

结果为 `semantica.temporal-facts-result/1`，带 profile、完整请求摘要及实际 operation。events 保留完整 fact、origin_kind、origin_index；三类修订列表保留 index 与完整 fact。索引对应原列表位置，不依赖可能重复的业务 id，不合并重复事实。非本操作的结果列表为空。

完整请求最多 1 MiB、图记录或原事实列表最多 4096 条、时间字符串最多 256 UTF-8 字节；完整漂亮打印结果含换行最多 16 MiB。失败不返回部分成功，退出 2；成功退出 0。无 Python、网络、时钟、原生工具或工程状态写入。

原始依据为锁定 Semantica0.6.5+oe.7 的 kg/temporal_reasoning.py、temporal_model.py 与原测试。44 个先冻结的独立用例已在原版观察；源码、真实 ggen、Rust 检查、Mac/Windows、安装和正式选用继续分别核维护回执。原 interval/1、Datalog、查询与 Project 审查合同保持。完整 BiTemporalFact、TemporalGraphQuery、工程实际有效期、原63项目标、制造检验、最终计量和 RRSI 收益仍各需其自身证据。
