# 双时间事实与时点快照

`kernel temporal-snapshot --input REQUEST --output RESULT` 将锁定 Semantica 的 `BiTemporalFact`、`reconstruct_at_time` 和 `query_at_time` 接到生成合同与同一 Rust runtime。它用于核对某个时点的来源图；工程适用性与采用仍由同一 Project 的条件、来源和必要检查决定。实际程序、平台与选用状态按最新维护回执确认。

`semantica.temporal-snapshot-request/1` 必须提供 `clock_at`。原实现缺少记录时间和有效起点时使用系统当前时间；本合同改为调用者显式给定时间，结果保留 `clock_at`、`recorded_at_basis` 与实际 `clock_fallback_count`。这项适配使回放可重复；默认记录时间不能被解释为历史事件确实发生于该时刻。

| 操作 | 输入与结果 | 保留的原行为 |
|---|---|---|
| `normalize-fact` | `fact` 字典、`graph:null`、`at_time:null`；返回原字典及四个规范时间字段 | 记录时间依次采用显式值、有效起点、请求时钟；结束空值表示开放。此操作保留倒置区间，并不执行区间或工程验收。 |
| `reconstruct` | `graph` 字典、`fact:null`、查询时点；返回完整图快照与原列表位置 | `time_axis` 为 `valid`、`transaction` 或两者 `both`；半开区间、按粒度取整及短路顺序由原时间 owner 执行。 |
| `query-at` | 与重建相同，增加 `query`、`include_history`、`temporal_precision` | 先重建；历史模式随后用全部原始关系替换筛选关系。`query` 文本不执行查询语言；`temporal_precision` 是原未使用选项，实际粒度取 `granularity`。 |

所有请求字段显式声明，默认场景的 `time_axis:"valid"`、`granularity:"day"`、`query:""`、`include_history:false`、`temporal_precision:null`。非查询操作拒绝混入查询选项；规范化操作只接受上述默认轴与粒度。外层未知字段拒绝，原事实和图的未知元数据保留。请求摘要覆盖全部输入，包括时钟、原顺序和未消费字段。

时点图保留原来的特殊情况：重复活动实体编号采用最后一条值，位置仍按第一次出现；关系顺序及重复项保留。输入实体列表为空时，活动关系可以保留缺失端点；输入实体列表非空时，端点必须在活动实体索引内。关系时间先检查，之后才过滤缺失端点。历史模式仍先执行重建，不能跳过坏时间。结果的原列表位置是额外见证，不冒称原 API 返回字段。

本次范围为字符串时间和实际消费的标量 JSON 标识；复用已有 Python 标量字符串、UTC 微秒解析与区间算法。数字 epoch、Python 对象及完整构造服务未迁移至此入口。原空图对未知轴/粒度或空时点的惰性行为保持；非空图实际需要时点时拒绝空值。缺少有效期在源算法中可视为开放区间，这不证明资料对当前工程持续有效。

完整请求最多 1 MiB、实体与关系合计最多 4096 条、单时间值最多 256 UTF-8 字节、完整格式化结果连换行最多 16 MiB。失败不写部分成功结果。无网络、隐式系统时钟、持久图写入或工程采用副作用。

规范正本为 `specs/modules/semantic-rule-metadata.toml`，真实锁定 ggen 生成类型及端口，Rust 手写算法复用既有 owner。62 个独立字面预期先冻结，再观察锁定原始模块；原时钟默认点显式绑定并计数，原文件字节不改。另核输入身份、位置、资源拒绝与安装平台。区间、事实时间线、Datalog、来源会话及原工程回放继续回归。

范围查询、一致性报告、演化、模式、时间路径及完整 Semantica 继续保留。原63项、11工作包、CAD16、六书与供应商、制造检验、实际项目的专属精度要求均不变。软件来源等价、工程合格和 RRSI 方法收益分别判断。
