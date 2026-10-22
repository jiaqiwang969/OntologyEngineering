# 有源时间序列、循环与连续配置

`kernel temporal-patterns --input REQUEST --output RESULT` 复用现有时间解析、双时间事实和 Python 值比较 owner，执行原 `TemporalPatternDetector` 与 `TemporalGraphQuery.query_temporal_pattern` 的有界 JSON 行为。规范正本生成会话、动作、状态与结果合同；复杂遍历留在手写 Rust。实际平台、安装及正式选用核维护回执，源码存在不表示已经接收。

请求 `semantica.temporal-patterns-request/1` 包含显式 `clock_at`、对象 `initial_config` 和有序 `actions`。每个动作的 `entry` 为 `detector` 或 `query`，`arguments` 对应原关键字，必须带 `graph`；query 还必须有 `pattern`。detector 默认 `pattern_type=sequence,min_frequency=2`；query 默认 `min_support=1`。双方共享配置，`gap_tolerance` 的修改先于图检查，后续失败也不撤销。结果逐步保存完整返回、失败、配置、输入边位置及实际时钟与工作量；`final_config` 可显式传入下一请求，没有全局隐含状态。

序列按输入身份排除当前边，稳定选择时间最早的可接续边，不做回溯。循环保留最多六条边及不同起点的旋转签名，按原 Python 布尔/数值相等规则计数；同组首签名与各次原始节点值分别保留。只读 `source/target`，不借用 canonical 端点。缺失时间仍未知。`time_window` 没有过滤作用，trend/anomaly 和未知标签返回空列表，不能宣称实现趋势或异常检测。

时间字段在被消费时限字符串或 null，每个最多256字节；数值时间和 Python 对象不属此运输范围。gap 支持整数与有限浮点天数，非数值配置按原行为视为零；微秒舍入接续 [CPython3.13.11 的 timedelta 构造](https://github.com/python/cpython/blob/v3.13.11/Modules/_datetimemodule.c)。query 的重复绑定参数、缺少必需参数、非对象参数等运输错误整次拒绝；原异常与部署资源拒绝分别标记，不伪装成原库异常。

完整输入限1MiB、最多128动作、每动作4096边、65536边访问、16777216次身份比较、1048576累计路径单元，完整缩进输出连换行限16MiB。原序列在零时长循环上可能不终止；部署达到资源限额时返回该动作失败，不返回截断模式，后续动作仍可继续。外层合同或整体输出超限时不输出部分会话。

独立冻结原79组历史98动作核默认、原错误、连续状态、输入身份及显式时钟；再检资源拒绝、Rust/CLI、实际平台与安装。来源回放属于 development_replay。模式重复、时间先后、软件通过、工程采用及 RRSI 收益分别判断；Project/work/Harness、原63项、CAD16、制造检验和最终计量全部保留。
