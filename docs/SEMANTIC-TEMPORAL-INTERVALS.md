# 有效期与时间覆盖

`kernel temporal --input REQUEST.json [--output RESULT.json]` 在同一 Rust Semantica 内执行时间区间计算，可用于查证“证据声明的有效期有没有缺口、某时刻是否在有效期内”。调用者仍须把时间与实际对象版本、来源、适用条件绑定；覆盖时间不代表证据可信、检查充分或工程采用。

请求使用 `semantica.temporal-request/1`，包含 `operation`、有序 `intervals`，以及按操作选用的 `point`、`domain`、`granularity`。每个区间有字符串 `start`、可选字符串 `end` 和可选 `label`；`end` 为 null 或精确 `OPEN` 表示开放终点。时间字符串沿既有 CPython ISO 解析 owner，支持 ASCII 月日补零；无时区按 UTC，有固定偏移时转 UTC，结果保留微秒，年份为 1–9999。数字 epoch、Python datetime/tzinfo 对象和非 ASCII 数字日期尚未纳入此运输合同。

| 操作 | 输入 | 返回与边界 |
|---|---|---|
| compare | 两个区间 | 原 13 种 Allen 关系，同时返回 overlaps 与包含布尔值。包含的等号边界与关系名 contains 含义分别保存。 |
| active | 一个区间与 point，可选 granularity | 起点包含、有限终点排除；若指定粒度，点、起点和有限终点都向下取整。 |
| merge | 区间列表 | 合并重叠或相接区间；按起点稳定排序，保留第一条 label。 |
| gaps | 区间列表与有限 domain | 先裁切、合并，返回标记 gap 的完整缺口。 |
| coverage | 区间列表与有限 domain | 并集时长除以 domain 时长，数值为 0–1；零时长返回 0。不是合格率或 0–100 百分数。 |
| normalize-point | point 与 granularity | 按 second/minute/hour/day/week/month/year 向下取整，周一为一周起点。 |
| normalize-interval | 一个区间与 granularity | 起点向下取整，有限终点扩到该粒度最后一微秒；开放终点保持，label 清空。 |

原始依据为锁定 Semantica `0.6.5+oe.7` 的 `kg/temporal_reasoning.py`、`kg/temporal_model.py` 和原测试。原算法的特殊边界保留：相同的零长度区间先命中 meets；开放终点在关系比较中按 `datetime.max` 处理；normalize-interval 不额外检查结果起终顺序，不能用它代替有效期合法性审查。原先加一个时间单位、再减一微秒的溢出仍失败。其他要求 start ≤ end 的操作照原位置检查。零长度 coverage 先返回 0，不把随后未执行的区间校验冒称已通过。

入口完整请求最多 1 MiB、4096 个区间，单时间字符串 256 UTF-8 字节，label 4096 字节；完整结果含换行最多 16 MiB。拒绝多余字段、操作形状不符与超限，不截断成部分成功。摘要绑定归一化前完整类型化请求。成功退出 0，拒绝或执行失败退出 2，不留下成功结果文件；无模型服务、当前时间采样、Python 运行或 Project 写入。

规范在既有 `semantic-rule-metadata.toml` owner，由锁定真实 ggen 生成 `SemanticTemporalPort` 及共享类型。Rust runtime 负责区间算法，kernel 和 CLI 仅接入。66 个预先冻结的原源码/测试用例保留 57 个结果与 9 个失败；独立边界、身份、预算、平台、安装及实际选用分别按维护回执接收。实际工程输入未给有效期时保持未知，不能从文件生成时间推造有效期。

本项未接完 BiTemporalFact、timeline、TemporalGraphQuery 或全 Semantica。原 63 项、Project/work/Harness、原生 CAD、制造检验、最终计量及 RRSI 独立收益的要求保持。
