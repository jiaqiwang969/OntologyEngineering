# 时间范围查询与日历分组

`oev2 kernel temporal-range --input REQUEST [--output RESULT]` 提供固定原版 `TemporalGraphQuery.query_time_range` 的有界 Rust 接续。它回答某段时间内有哪些关系、哪些关系覆盖全段，以及选中关系在各日历段如何出现。它与[时点快照](SEMANTIC-TEMPORAL-SNAPSHOT.md)、[时间区间](SEMANTIC-TEMPORAL-INTERVALS.md)、[事实时间线](SEMANTIC-TEMPORAL-FACTS.md)共用解析、双时间字段和区间关系实现。

## 合同与原行为

请求 schema 为 `semantica.temporal-range-request/1`；结果为 `semantica.temporal-range-result/1`，profile 为 `semantica.source-temporal-range/1`。请求需要显式 `clock_at`、图字典、起止时间、`time_axis`、`granularity`、`query`、`aggregation` 和 `include_intervals`。起点必须是有效时间字符串；终点 null 或 `OPEN` 表示开放。支持原字符串日期、偏移和微秒；数值 epoch、Python 对象及构造器外部服务未包括在本合同中。

- 起点向粒度边界取整，终点扩展到所在时段最后一微秒。`union` 包括重叠和刚好相接的边界；这与时点快照的右端不包含不同。倒置的候选或查询区间返回不匹配，解析错误仍失败。
- `intersection` 先做上述筛选，再核关系是否覆盖整个归一化范围。`valid`、`transaction`、`both` 分别使用有效轴、记录轴、两轴同时满足；保留原逐轴解析与短路顺序。
- `evolution` 先按请求轴选择关系，再按原算法以 **valid 轴** 分组，只返回非空组。日、周、月、年按各自日历步长；秒、分、小时沿原算法的日步长。开放终点无法分组。原行为先推进下一边界，年份溢出仍失败。
- 未识别的 aggregation 沿原库返回并集，同时保留原字符串。`query` 仅原样返回；`include_intervals` 是未使用的原参数，不表示裁剪。图中实体、端点和其他元数据不被本入口审查。
- 保留完整原关系、重复值及顺序。`relationship_indices`、`bucket_indices` 额外指向输入位置。请求摘要包含完整类型化输入；未使用字段变化也改变身份。

`clock_at` 替代原版缺失 recorded_at 时的当前时钟；结果返回规范化时钟及实际回退次数。此项为显式适配。回退时钟、软件登记时间和文件时间均不能推定工程证据的有效期。

## 资源、来源与检查

单次请求 1 MiB、关系 4096 条、时间字符串 256 UTF-8 字节、日历段 4096 个、分组关系检查 65536 次。先检查重复原关系的累计字节，才展开值；完整美化结果连末尾换行不超过 16 MiB。超过限制返回完整失败，不截断为成功。入口不写图、不调用模型或原生程序、不修改 Project、不作工程采用。

来源是锁定 Semantica 0.6.5+oe.7 wheel（SHA256 `142ddd6fe788b3a716646babc410628aa7606e44ed63b98f5227863f9ea7ba15`）中的 temporal_query.py、temporal_model.py 和 temporal_reasoning.py。67 项独立文字预期先冻结，再执行原源码；成功、失败、UTC 边界、日历分组和时钟次数分别比较。Rust tests 包含边界区别、原输入位置、身份变化、资源拒绝和 CLI 文件结果。源码存在、Mac/Windows 验证、实际安装和正式选择分别查看维护回执。

新增合同触发了原 32 MiB 派生模型上限，失败保留。`oe-design::inspection` 为固定源码属性使用 Turtle 前缀；完整 URI、原文字面量与来源均保持，由独立 Oxigraph 解析等价测试和真实 ggen 验证。原模型与其他资源上限未扩大；生成物经实际 ggen 重新生成，新的字节身份单独登记。

## 工程接续与未决项

登记历史可用于定位旧新软件输入及来源，再将实际有效期、对象条件和必要检查交给同一 Project。当前 Project 缺失的材料、载荷、制造、装配、检验与最终实测义务不由时间筛选关闭。保留完整 63 项目标、CAD 和工程验收；方法收益需新任务、冻结判据和成本对照。

原 consistency 的缺失关系 id 异常与数字端点查找差异已有反例，继续保留，未在本入口偷偷修正。时间模式、路径、全构造器、完整一致性接口和剩余 Semantica 范围仍待各自接续。此处 `evolution` 是原库日历分组，不表示 RRSI 或其他工程方法的学习收益。
