# Fusion MCP 歧义结果：同 PID 终止策略

本文定义 Fusion MCP 在超时、取消、断连或响应交付不确定时的统一安全边界。该边界属于
运行时协议，不依赖某一次装配任务、某一个调用端或某一份操作说明。

## 一句话规则

只要请求可能已经到达 Fusion，而调用端不能证明结果是否完成，当前 Fusion PID 就立即
进入 terminal retirement。后续工作必须使用不同的 Fusion PID／进程启动令牌；原 PID
不得再收到任何 MCP 请求。

“窗口看起来没变化”“没有新增星号”“应用仍能响应”都不能证明失败操作没有提交，因而
不能解除终止状态。

唯一能解除的，是 Fusion 自己的 AppLog 对**那一个请求**的完成证据（见「恢复路径 A」）。
它是 MCP 之外的独立证人，取证过程不向该 PID 发送任何请求，因此不违反上面这条边界；
证据不足时边界原样生效，只能换新 PID（路径 B）。

## 歧义边界

以下情况一律按结果未知处理：

- 上游调用超时；
- 客户端取消或 stdio／SSH 断开；
- HTTP、SSE 或 MCP 传输异常，无法证明请求未送达；
- 操作已经完成，但响应尚未可靠刷新和确认；
- 任何其他无法同时证明“未执行”和“未提交”的异常。

上游调用超时这一条要区分成因。以下两种是**预算配置缺陷**，不是 Fusion 故障，应在分诊时先排除：

- **启动预算截断了首个请求**。一次性 cell 的第一个请求也是唯一一个请求；旧实现把
  `startup_timeout_seconds` min() 进请求 deadline，于是 `startup=240 / timeout=280` 的配置实际只给
  Fusion 234.66 s（另外 5.36 s 被激活吃掉）。判据：`retired_at_unix - started_at_unix` 明显接近
  `startup_timeout_seconds` 而不是 `timeout_seconds`。**该缺陷已在 2026-09-06 的补丁中修复**：
  激活预算不再截断首个请求。
- **嵌套预算相等导致外层先到期**。cell 预算与 acceptance client 预算若相等，外层总是先赢几毫秒，
  把本应由 cell 产生的、可分诊的 `TimeoutError` 变成外部投递的 `CancelledError`。判据：marker
  `reason` 以 `CancelledError: Cancelled via cancel scope` 开头，且客户端 `duration_ms` 恰好等于
  acceptance 预算。修复是让 acceptance 预算至少比 cell 预算大 90 s；`fusion_call.py` 现在自动派生。

另新增一类必须单独识别的成因：

- **主机休眠横跨在飞请求**（`HOST_SLEEP_DURING_REQUEST`）。macOS 上 `time.monotonic()` 等价于
  `CLOCK_UPTIME_RAW`，休眠期间冻结，因此客户端**不会**在休眠中到期，而是在唤醒后不久到期，
  此时 Fusion 往往刚开始处理请求。判据：墙钟耗时远大于客户端 `duration_ms`（差值等于休眠时长）、
  AppLog 出现同长度的时间空洞、`E <tid> <M>m <S>s since last heartbeat`、libcurl
  `POST 28 ... dt=<休眠秒数>`，以及 `pmset -g log` 中对应的 Sleep/Wake 行。
  这类退休**优先走恢复路径 A**，因为 Fusion 侧通常已完整成功并存盘。

以下情况**不属于**歧义边界，不得写 retirement：

- 守卫层在写入 `fusion-mcp.in-flight.v2` **之前**拒绝的请求 —— 包括窗口普查
  失败、辅助窗口分类失败、窗口集合未 settle、Peekaboo 权限/桥接/传输故障、
  owner lease 争用。这些请求从未到达 Fusion。
- 判据是可验证的：Fusion AppLog 中没有对应的 MCP 请求，且不存在匹配当前
  PID/启动令牌的 in-flight 标记。调用方侧对应 `classification == "guard_transient"`。
- 处置：退避后用**同一个** Fusion PID 重试（建议 45 s，上限 6 次）。把这类拒绝升级为
  retirement 会白白丢掉一个健康的 Fusion 进程（2026-09-05 23:00 的 PID 12487 即为此例）。

运行时在发出请求前写入 `fusion-mcp.in-flight.v2` 标记。出现上述歧义后：

1. 保留 in-flight 标记；
2. 写入 `fusion-mcp.retirement.v2` 标记；
3. 将当前 guard 标记为 retired；
4. 禁止同 PID 的重试、新会话、读、写、保存、关闭和 DELETE；
5. 不执行任何失败后的 Fusion MCP 探针。

终止动作在取消屏蔽区内完成，避免客户端断连同时打断安全标记写入。

## 旧 quarantine 标记

早期实现可能留下 `fusion-mcp.quarantine.v1`。它现在仅是历史兼容输入，不再表示可恢复
状态：

- runtime guard 将匹配当前 PID／启动令牌的 quarantine 标记视同 retirement；
- health challenge 不能绕过或删除该标记；
- `quarantined_document_title` 不再属于 health challenge 协议；
- recovery collector 可通过 `--quarantine` 收集旧标记，但生成的 incident 固定包含
  `recoverable_same_pid: false` 和完整的 same-PID no-retry 边界；
- 历史 incident 若宣称 `recoverable_same_pid: true`，health gate 必须 fail closed。

旧参数和 schema 的保留只是为了归档既有证据，不是兼容旧恢复行为。

## 恢复路径

歧义发生后有两条路径。**先做 A，只有 A 拒绝时才做 B。**

### A. 基于 AppLog 完成证据的解除（无需重启 Fusion）

Fusion 自己的 AppLog 是先于事故存在、只追加、代理与恢复工具都无法影响的独立证人。当它能证明
被退休的那一个请求确实完成时，重启 Fusion 没有任何安全收益，只有成本。

    scripts/fusion_recovery.py evidence --latest
    scripts/fusion_recovery.py recover  --marker <retirement.json> --apply

工具逐条判定并打印 C1-C12，全部通过才写
`fusion-mcp.completion-evidence.v1` receipt 并把 retirement 与 in-flight marker 改名保留为
`*.superseded-<UTC>.json`。判定条件：

1. marker schema 为 `fusion-mcp.retirement.v2` 或 `fusion-mcp.quarantine.v1`，且带 PID、启动令牌、
   `retired_at_unix`；
2. `reason` 属于纯客户端预算/传输失效（`TimeoutError`、`CancelledError`、`BrokenResourceError`、
   `Timed out while waiting for response`、`stdio input closed before client consumption` 等）。
   **release 变更、PID/启动令牌变更、窗口证据失败、untitled 保全、dirty-recovery 与 health
   identity 相关的退休一律不可用本路径恢复**——它们说明请求的前提本身就是错的；
3. 在 `[in-flight 起始 -2s, 退休 +1800s]` 时间窗内**恰好一条** `...Operation called with payload:`
   起始行，且其 `_ctx.extra.sessionId` 在全日志唯一；
4. 该请求与下一条请求之间**恰好一条** `MCPAnalytics tracked` 行，且 `success=true`
   （失败行形如 `(success=false, duration=Nms, reason=UnknownError)`，多一个 `reason=` 字段，必须能被看见）；
5. `完成时刻 - duration` 与请求行时刻相差不超过 3 s（analytics 行不带 sessionId，只能这样配对）；
6. 日志载荷剥掉 `_ctx` 后按 `json.dumps(..., ensure_ascii=False, sort_keys=True, separators=(",",":"))`
   的 sha256 **等于** marker 的 `arguments_sha256`。这是「被退休的正是这一个请求」的密码学证明；
   降级为 `--binding session-window` 必须由操作者显式指定并写入 receipt；
7. 写类工具（`fusion_mcp_execute` / `fusion_mcp_update`）另需在完成行之后、下一条请求之前出现
   `[pim.core] DocumentSaved`；读类工具（`fusion_mcp_read` / `fusion_mcp_electronics_read` /
   `resources/*`）只需完成行；
8. marker 的 PID 仍存活且 libproc 启动令牌与 marker 一致。

任一条不成立即以退出码 4 拒绝，**不写任何文件**，转 B。fail-closed 仍是默认。

receipt 只解除它指名的那一个 retirement 及其配对的 in-flight marker，**不解除** legacy
quarantine marker。marker 本身被改名保留，永不删除。

该路径**不向 Fusion 发送任何 MCP 请求**，只读 AppLog 与 marker，因此不违反 same-PID
no-retry 边界；它是重启之前必须先走的一步。

当前 Fusion-only `oe.3` cell 固定保留 fail-closed 行为，不提供
`--marker-enforcement` 或 `warn` 覆盖。任何仍在原位置的退休、在飞或 quarantine
标记都会阻断绑定。只有上述独立 `recover --apply` 命令按原生 AppLog 条件验证，
并保留原证据、完成 supersede 后，才可能解除符合条件的纯预算/传输失效。
健康检查、窗口、身份或文档保全失败不适用该恢复路径。

### B. 换新 PID（A 拒绝时的唯一路径）

1. 保全 retirement、in-flight、旧 quarantine、AppLog 和外部窗口证据；
2. 结束旧 Fusion 进程并启动不同 PID／不同进程启动令牌的 Fusion；
3. 对新 PID 做不经过 MCP 的身份与浅层窗口检查；
4. 仅对新 PID 运行一次受 challenge 约束的 initialize、tools/list、document-read health probe；
5. receipt 完整且验证通过后，才允许新 PID 接收正常建模请求。

health probe 的 document-read 只用于证明新进程、监听器、工具目录和读取通路一致，不用于
判断旧请求是否提交，也不能恢复旧 PID。

## 未保存和未命名文档

歧义发生时，带星号、未保存或未命名文档的真实状态保持 `UNKNOWN`。不得为了“确认一下”
再向旧 PID 发 read、save、save-as 或 close。保存与关闭需要走应用外的保全／人工恢复流程，
或者在新 PID 上从已确认的持久化版本重新开始。

## 外部证据的作用

Peekaboo 窗口普查、截图和 AppLog 可以在 MCP 之外用于事件归档、原因分析和决定如何人工
保全数据，但它们不能把 terminal retirement 降级为可重试状态。特别是，失败后再次观察到
与请求前相同的窗口集合，只说明“没有观察到界面变化”，不等于“操作没有提交”。

## 回归门禁

实现至少必须证明：

- 即使前后窗口证据看似干净，歧义也写 retirement、保留 in-flight 且不写 quarantine；
- 终止路径不运行失败后的窗口探针；
- 旧 quarantine 标记在普通启动和 health challenge 启动下都拒绝绑定；
- health receipt 不会清除旧 quarantine；
- recovery collector 将旧 quarantine 固化为 `recoverable_same_pid: false`；
- recovery gate 拒绝所有声称可在失败 PID 上恢复的 incident；
- 同一个 guard 的第二次请求在进入上游前即被拒绝；
- 启动预算只约束激活，不截断首个请求；
- 检测到主机休眠时请求获得有界的清醒预算延长，而不是被取消，且延长总额有上限；
- 完成证据 receipt 缺失时，`evidence-gated` 与 `fail-closed` 行为逐字相同；
- 完成证据条件任一不成立时不写任何文件并以退出码 4 拒绝；
- 同名 retirement/in-flight marker 重写时旧文件被改名保留，而单次授权 marker 绝不被轮转覆盖；
- Peekaboo 传输故障（CLI 超时、桥接握手不全）在有可用的 local 模式时降级采集，
  并把 `peekaboo_mode_requested` / `peekaboo_mode_used` / `peekaboo_mode_fallback`
  写进窗口证据；实质性拒绝与 `FusionRuntimeRetiredError` 绝不降级；
- 窗口普查的抖动（同一窗口 id、同一尺寸、原点被动画平移）不产生拒绝，
  而阻塞标题或 modified 文档只要在任意一次 settle 普查中出现即拒绝；
- 辅助窗口的空白面证明按 (PID, 启动令牌, 窗口签名, 证明时窗口 id) 缓存，
  跨 PID、跨启动令牌、窗口重建或 TTL 过期后必须重新截图证明。

聚焦测试位于：

- `tests/test_fusion_mcp_ambiguity_quarantine.py`
- `tests/test_fusion_mcp_recovery_controller.py`
- `tests/test_fusion_mcp_sleep_gap.py`
- `tests/test_fusion_mcp_long_request_budget.py`
- `tests/test_fusion_mcp_applog_evidence.py`
- `tests/test_fusion_mcp_completion_receipt.py`
- `tests/test_fusion_window_census_stability.py`

> 2026-09-07 的安装事实：后五个文件尚未合入开发仓库
> `~/120-agent-cad/01-fusion-tutorial/cad-agent/tests/`，目前只存在于评审目录
> `reports/fusion_reliability_20260906/{A_window_census,B_budgets_sleep_recovery}/proposed/tests/`。
> 运行时补丁本身已经安装在本 skill 的 `.venv` 里。合入仓库前，这份门禁清单是**要求**，
> 不是「已经在 CI 上跑着」的陈述。
