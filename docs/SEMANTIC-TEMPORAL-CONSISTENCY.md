# 时间一致性报告

`oev2 kernel temporal-consistency --input REQUEST [--output RESULT]` 按来源关系记录检查时间顺序、端点存在、实体存续范围，以及同边同类型的相邻区间。它接在既有时间解析、四字段规范化、区间关系、Python JSON 比较与 kernel owner 上。

请求 `semantica.temporal-consistency-request/1` 包含完整 `graph`、`granularity` 和必填的 `clock_at`。图中的 `entities`、`relationships` 缺省为空数组；原始记录和未使用的元数据都参与请求身份。实体 ID 采用既有标量文本规则；日期按既有字符串/null profile，数值 epoch、Python 对象和复杂实体 ID 尚未接收。`clock_at` 只替代缺失 recorded_at 时原库读取系统时钟的行为，不能补充真实工程有效期。

结果 `semantica.temporal-consistency-result/1` 使用 `semantica.source-temporal-consistency/1`：保留原 `report.errors`、`report.warnings` 的内容及顺序，并分别附对应输入关系位置、实际时钟替代次数和分组比较次数。ID 重复不丢失位置。输入不会修改，也不写 Project、变更采用或调用远程服务。

## 原行为与检查边界

- 第一遍按输入顺序检查；第二遍按分组首次出现顺序重新解析。原库重复报告的日期错误保留两次。无效关系日期会跳过该遍的端点检查。
- 实体 ID 字符串化，重复 ID 后项覆盖前项；关系端点按原值查找，数字/null 端点不会静默转换。缺少或假值关系 ID 触发原类不存在 helper 的整次失败；它不是空报告。
- 关系组保持原 source/type/target 含义，缺 type 才使用 relationship。数值/bool 键比较复用现有 Python 等值 owner。
- 同组按原始开始时间稳定排序，仅比较相邻记录；刚好相接也报告重叠。不会据此宣称检查了所有区间对。
- 倒置及实体存续比较沿原粒度截断；较粗粒度可能隐藏日内先后。未知开始时间可能不足以发现超出实体范围。空错误列表不证明时间信息齐全或工程有效。
- 报告中的错误、警告与整次调用失败分别保存。原粒度仅在实际比较时检查，原源异常继续失败，不将失败或资源拒绝伪装成通过。

后续若修正原库的 ID、时间比较或检查覆盖，需另立显式 profile 与独立预期，保留本版本和原反例。采用判断仍由 Project 的当前对象、条件、来源、必要检查及批准承担。

## 来源、资源与接收

来源为固定 Semantica 0.6.5+oe.7 的 `kg/temporal_query.py::TemporalGraphQuery.validate_temporal_consistency` 及其时间/存续辅助方法，许可证 MIT。合同正本在 `specs/modules/semantic-rule-metadata.toml`，派生类型与接口由锁定 ggen 真实生成；算法在 `crates/semantica-runtime/src/temporal_consistency.rs`，工程入口沿 `oe-kernel::rules` 和 `oe-cli`。

完整请求最多 1 MiB，实体与关系共最多 4096 条，最多 65536 次分组比较；实际读取的日期字符串最多 256 UTF-8 字节；完整 pretty 结果含换行最多 16 MiB。任一拒绝都不输出部分成功报告。原容量、权限与来源边界保持。

63 个案例在本轮原库执行与 Rust 实现之前冻结，其中 25 个接续既有原件；独立比较完整问题文本、顺序、时钟次数、位置和整次异常。另核身份、未知条件、重复 ID、资源限制和 CLI 无残留结果。Mac、Windows、安装及正式选用按实际维护回执分别接收。案例是 development_replay，不是工程实测或 RRSI 净收益证据。完整 Semantica、原63项目标、原生 CAD、制造检验和最终计量继续保留。
