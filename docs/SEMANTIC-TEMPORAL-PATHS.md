# 有源时间路径

`kernel temporal-paths --input REQUEST.json --output RESULT.json` 从对象关系图查找给定端点之间的时间路径。agent 整理输入，用户继续用自然语言表达问题。结果绑定完整请求摘要、原始关系位置及显式时钟；可交回同一 Project/work/Harness 核对依赖链和后续义务。

来源是固定 Semantica 0.6.5+oe.7 的 `TemporalGraphQuery.find_temporal_paths`，共享合同经 specs → 锁定 ggen 生成，算法在 `semantica-runtime::temporal_paths` 执行。它复用已有 Python 数值相等、日期、双时间字段及粒度 owner。旧入口语义保持，平台执行与正式选用分别核当前维护回执。

## 原行为与含义

- 关系按输入顺序建立邻接表，再作广度优先遍历。`source`/`target` 缺失或 null 时才回退 canonical `_id` 字段，空字符串保留。
- 端点保留原 JSON 值。布尔与数值按 Python 相等规则比较；字符串不自动数字化。容器在需要哈希时按原库失败；到达目标的判断在 visited 哈希之前。
- `max_path_length` 原实现比较节点数，结果 `length` 才是边数。缺失或零表示无此语义限制，负数剪去全部路径。资源上限始终另外执行。
- 全局 visited 可能跳过经过同一中间节点的后续路径。平行直达边可以返回重复节点序列，独立输入位置区分原边。因此结果不是所有简单路径的枚举。
- 起止窗口保留相接边界，缺少日期保持未知。空字符串不启用窗口，`OPEN` 启用字段解析但对应边界未知。未访问的日期按原算法保持未解析；完整原关系照样返回。
- `strict` 只比较连续两条边的开始时间；`overlap` 只检查当前开始是否晚于前边结束，不要求它晚于前边开始。`loose` 或关闭排序跳过归一化。未知策略仍解析两条边，然后接受。时间粒度只在实际比较时检查。
- `temporal_constraints` 由原库保留但不执行，不能靠此字段传入工程必要验收并声称已检查。时间排序不证明物理因果、工程适用性或采用。

显式 `clock_at` 只代替原库缺少 recorded 时间时的默认时钟。重复归一化按实际次数计数，不能解释为关系已取得真实记录时间。

## 范围与资源

请求使用 `semantica.temporal-paths-request/1`，结果使用 `semantica.temporal-paths-result/1`、profile `semantica.source-temporal-paths/1`。请求必须明确 graph、source、target、clock_at、排序开关/策略和 granularity；窗口、长度及额外 constraints 可为 null。日期只支持被消费的字符串/null，长度为有符号64位整数/null；Python datetime 对象、数值时间和其他动态参数类型不在当前 JSON profile 内。

完整请求最多1 MiB、4096条关系。entities 和其他图元数据不参与原路径算法，只随完整请求摘要保留。执行最多65536次邻边展开、16777216次原值比较、累计1048576个入队节点/边索引单元；关系原文在遍历中借用，构造副本前先核完整借用报告的字节。被消费的日期字符串最多256字节；完整 pretty JSON 结果连同换行最多16 MiB。

任何资源、profile 或原库异常使整次失败，不返回截断路径。公开 CLI 沿既有失败合同记录 unknown 和无结果文件，原库具体异常在 Rust 接口及私有回执中核对。

## 验证与接续

70个原始结果/异常预期含上一轮24例，原预期不改。新增案例在执行原库和 Rust 前固定；独立核路径位置、时钟调用、输入不变、摘要身份、缺少工程条件及资源边界。Mac/Windows 同源编译、实际安装接收、旧消费者及精确恢复各自留证。

当前 Project 的依赖投影可作为 development_replay，时间未知仍保持未知。采用还须当前对象版本、工程条件、独立检查和批准；源码对应、通过测试、安装选用、工程完成与 RRSI 收益分别判断。完整19模块/19协议/22 owner 与原工程目标继续接续，后续仍需时间模式等原算法及其他未闭合能力。
