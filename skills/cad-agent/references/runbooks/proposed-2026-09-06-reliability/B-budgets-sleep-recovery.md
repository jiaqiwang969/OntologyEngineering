# RUNBOOK_DELTA：待并入协议文档的确切文本

四份文档均在 `/Users/<user>/.codex/skills/cad-agent/references/`。行号基于 2026-09-06 的当前版本。
注意 `docs/fusion-mcp-ambiguity-triage.md` 与技能副本**逐字节相同**（已确认），修改需同步两处。

本文件只给出待插入/替换的文本，不代为落盘。

---

## 1. `runbooks/fusion-mcp-ambiguity-triage.md`（现 90 行）

### 1.1 替换 `## 恢复路径`（第 49 行，正文 51-61 行）

**现状**：第 51-61 行把「结束旧 Fusion 进程并启动不同 PID」写成唯一允许的恢复路径。
**替换为**：

```markdown
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

任一条不成立即以 exit 4 拒绝，**不写任何文件**，转 B。fail-closed 仍是默认。

receipt 只解除它指名的那一个 retirement 及其配对的 in-flight marker，**不解除** legacy
quarantine marker。marker 本身被改名保留，永不删除。

### B. 换新 PID（A 拒绝时的唯一路径）

1. 保全 retirement、in-flight、旧 quarantine、AppLog 和外部窗口证据；
2. 结束旧 Fusion 进程并启动不同 PID／不同进程启动令牌的 Fusion；
3. 对新 PID 做不经过 MCP 的身份与浅层窗口检查；
4. 仅对新 PID 运行一次受 challenge 约束的 initialize、tools/list、document-read health probe；
5. receipt 完整且验证通过后，才允许新 PID 接收正常建模请求。

health probe 的 document-read 只用于证明新进程、监听器、工具目录和读取通路一致，不用于
判断旧请求是否提交，也不能恢复旧 PID。
```

### 1.2 在 `## 歧义边界`（第 15 行）的清单（19-23 行）后追加

```markdown
上游调用超时这一条要区分成因。以下两种是**预算配置缺陷**，不是 Fusion 故障，应在分诊时先排除：

- **启动预算截断了首个请求**。一次性 cell 的第一个请求也是唯一一个请求；旧实现把
  `startup_timeout_seconds` min() 进请求 deadline，于是 `startup=240 / timeout=280` 的配置实际只给
  Fusion 234.66 s（另外 5.36 s 被激活吃掉）。判据：`retired_at_unix - started_at_unix` 明显接近
  `startup_timeout_seconds` 而不是 `timeout_seconds`。
- **嵌套预算相等导致外层先到期**。cell 预算与 acceptance client 预算若相等，外层总是先赢几毫秒，
  把本应由 cell 产生的、可分诊的 `TimeoutError` 变成外部投递的 `CancelledError`。判据：marker
  `reason` 以 `CancelledError: Cancelled via cancel scope` 开头，且客户端 `duration_ms` 恰好等于
  acceptance 预算。修复是让 acceptance 预算至少比 cell 预算大 90 s。

另新增一类必须单独识别的成因：

- **主机休眠横跨在飞请求**（`HOST_SLEEP_DURING_REQUEST`）。macOS 上 `time.monotonic()` 等价于
  `CLOCK_UPTIME_RAW`，休眠期间冻结，因此客户端**不会**在休眠中到期，而是在唤醒后不久到期，
  此时 Fusion 往往刚开始处理请求。判据：墙钟耗时远大于客户端 `duration_ms`（差值等于休眠时长）、
  AppLog 出现同长度的时间空洞、`E <tid> <M>m <S>s since last heartbeat`、libcurl
  `POST 28 ... dt=<休眠秒数>`，以及 `pmset -g log` 中对应的 Sleep/Wake 行。
  这类退休**优先走 A 路径**，因为 Fusion 侧通常已完整成功并存盘。
```

### 1.3 在 `## 回归门禁`（第 75 行）的清单（79-85 行）与测试列表（89-90 行）追加

```markdown
- 启动预算只约束激活，不截断首个请求；
- 检测到主机休眠时请求获得有界的清醒预算延长，而不是被取消，且延长总额有上限；
- 完成证据 receipt 缺失时，`evidence-gated` 与 `fail-closed` 行为逐字相同；
- 完成证据条件任一不成立时不写任何文件并以 exit 4 拒绝；
- 同名 retirement/in-flight marker 重写时旧文件被改名保留，而单次授权 marker 绝不被轮转覆盖。

聚焦测试位于：

- `tests/test_fusion_mcp_ambiguity_quarantine.py`
- `tests/test_fusion_mcp_recovery_controller.py`
- `tests/test_fusion_mcp_sleep_gap.py`
- `tests/test_fusion_mcp_long_request_budget.py`
- `tests/test_fusion_mcp_applog_evidence.py`
- `tests/test_fusion_mcp_completion_receipt.py`
```

---

## 2. `fusion-execution.md`（现 172 行）

### 2.1 替换 `## 其他铁律` 第 127 行

**现状**：`1. **单次调用 8–20 秒是正常的**(含窗口验真与 cell 启动),不是卡死。`
**替换为**：

```markdown
1. **单次调用的正常耗时取决于 CAD 工作量，不是固定值**。只读与小改约 8-20 秒；一次 40 体 STEP
   chunk 导入 2026-09-06 实测 p50 64 秒、p90 141 秒；一次 168 体导入实测 404.6 秒。**不要按
   「超过 20 秒＝卡死」判断。** 预算表见下节。
```

### 2.2 在 `## 其他铁律` 正文之后、`## 已知易错点`（第 139 行）之前插入新节

```markdown
## 调用预算与主机休眠

### 预算表

一次 `fusion_call.py` 调用有四层嵌套计时器，全部基于 `time.monotonic()`，必须**内紧外松**：

| 层 | 参数 | 普通 | `--long-request` |
|---|---|---|---|
| 激活（进程发现 / owner 租约 / 窗口普查） | cell `--startup-timeout-seconds` | 240 s（硬上限 300） | 240 s（硬上限 300，**不随操作预算变大**） |
| 操作（upstream initialize + 选定调用） | cell `--timeout-seconds` = T | 240 s（硬上限 300） | 最高 1800 s |
| acceptance client read timeout | 由 T 派生 | T + 90 s | T + 90 s |
| `subprocess.run` 硬杀 | 由 T 派生 | T + 210 s | T + 210 s |

- **激活预算只管激活**。它不再截断首个请求（旧实现会，这正是 2026-09-06 17:18 事故的成因）。
- **外层必须比内层宽至少 90 秒**。两者相等时外层先赢，把可分诊的 `TimeoutError` 变成歧义的
  `CancelledError`。`fusion_call.py` 现在自动派生外层预算，不要手工把它们设成一样。
- 需要超过 300 秒时**必须**加 `--long-request`，不要试图把普通预算调到 300 以上（会在构造期
  `ValueError`）。判据：预计导入体数大于约 80，或同型历史 `duration` 曾超过 200 秒。

        scripts/fusion_call.py --long-request --timeout 900 fusion_mcp_execute '<args>'

  `--long-request` 只放大预算，不改变任何安全性质：仍然只发一个请求、仍然写 in-flight marker、
  仍然绝不重试。

### 主机休眠

macOS 上 `time.monotonic()` 等价于 `CLOCK_UPTIME_RAW`，**休眠期间冻结**；`CLOCK_MONOTONIC` 才是
连续的。因此休眠不会缩短预算，但会让请求在唤醒后才被 Fusion 处理，唤醒后剩余的清醒预算往往
不够，于是客户端在 Fusion 即将成功前到期。

1. **长任务期间不要合盖，不要手动休眠。** 2026-09-06 的四次休眠全部是 `Clamshell Sleep`（合盖）。
2. **`caffeinate -i` 不阻止合盖休眠。** `caffeinate -i` / `PreventUserIdleSystemSleep` 只阻止 idle
   休眠；`caffeinate -s` / `PreventSystemSleep` 也不阻止合盖，且仅在交流供电下生效。不要把
   caffeinate 当作充分缓解。
3. **cell 会在请求在飞期间自动持有电源断言**（IOKit `IOPMAssertionCreateWithName`，失败则退回
   `caffeinate -i -m -w <pid>`），并在请求结束时释放。`--no-power-assertion` 可关闭。
   验证：请求在飞时 `pmset -g assertions` 应能看到 `fusion-mcp guarded request (...)`。
4. **休眠仍然发生时，cell 会检测到并延长预算而不是取消请求**，stderr 打印
   `HOST_SLEEP_DURING_REQUEST`，退休 marker 的 `host_sleep` 字段会记录累计休眠秒数与次数。
   延长有上限（默认最多 1800 秒），耗尽后仍按普通超时退休。
5. 真要在系统层禁止合盖休眠，只有 `sudo pmset -a disablesleep 1`（系统级、需要 sudo、影响所有
   应用），用完记得改回 `0`。这不是默认做法。
```

### 2.3 替换 `## 保存纪律` 第 113-115 行（规则 4）

**现状**：`4. **怀疑有模态对话框挡着、调用超时或结果歧义时**:立即停止该 PID 的全部 MCP 活动…`
**替换为**：

```markdown
4. **怀疑有模态对话框挡着、调用超时或结果歧义时**：立即停止该 PID 的全部 MCP
   活动，包括 read、save、close、quit、session 新建/删除、截图查询和健康探针；
   只能用 Peekaboo/OS/AppLog 等 MCP 外部证据分诊。不得为「核对一下」再碰同 PID。
   **但在重启 Fusion 之前，先跑 `scripts/fusion_recovery.py evidence --latest`。**
   若 Fusion 的 AppLog 能证明这一个请求已经完成并存盘，重启没有任何安全收益。
   2026-09-06 的两次退休都属于这种情况，其中一次白白重启了一遍 Fusion。
```

### 2.4 替换 `## 其他铁律` 第 128-131 行（规则 2）

**替换为**：

```markdown
2. **超时/退出码 2/结果歧义之后，同一 Fusion PID 不得再收到任何 MCP 请求**。
   上游结果可能已提交；禁止只读确认、重发、save/close/quit、健康探针、建立新
   session 或 DELETE。冻结现有日志，仅用 MCP 外部证据按
   `references/runbooks/fusion-mcp-ambiguity-triage.md`（本 skill 内）分诊。
   该 runbook 的恢复路径 A（AppLog 完成证据）**不向 Fusion 发送任何请求**，只读日志与
   marker，因此不违反本条；它是重启之前必须先走的一步。状态不能证明时写 `UNKNOWN` 并 HOLD。
```

---

## 3. `runbooks/fusion-mcp-caller-guide.md`（现 68 行）

### 3.1 替换第 43-44 行（规则 3）

```markdown
3. **并发天然安全但串行**。多个 agent 同时调用时由 owner lease 串行化；
   排队是正常现象。**耗时取决于 CAD 工作量**：只读与小改 8-15 秒，大批量几何导入可达数分钟
   （实测最长 404.6 秒），不要按无响应处理。超过 300 秒的操作必须用 `--long-request`。
```

### 3.2 在第 53 行之后追加两条

```markdown
7. **预算内紧外松，不要手工把它们设成一样**。cell 操作预算 T、acceptance read timeout T+90、
   外层硬杀 T+210；激活预算独立且不超过 300 秒。两个预算相等会让外层先赢，把可分诊的
   `TimeoutError` 变成歧义的 `CancelledError`（2026-09-06 22:12 事故即此）。
   `fusion_call.py` 已自动派生外层，只需设 `--timeout`。
8. **超时后先取证再重启**。跑 `scripts/fusion_recovery.py evidence --latest`；Fusion 的 AppLog
   能证明请求已完成并存盘时，用 `recover --apply` 解除退休即可，无需重启 Fusion。证据不足时
   工具以 exit 4 拒绝并不写任何文件，此时才按 `fusion-mcp-ambiguity-triage.md` 换新 PID。
   长任务期间不要合盖笔记本：`caffeinate -i` 不阻止合盖休眠。
```

---

## 4. `runbooks/fusion-runtime-safety-incidents.md`（现 105 行）

### 4.1 在 `## 非原生崩溃安全事件` 表尾（第 65 行）之后追加两行

```markdown
| R3-V018 chunk3，PID 12487 | MCP timeout；已退休的 in-flight 脚本随后完成并存盘 | `StartupBudgetTruncatesFirstGuardedRequest`；运行时边界 `InFlightFusionScriptMayCommitAfterClientRetirement` | 168 体 STEP `importToTarget` 在 Fusion 内运行 404.579 s。lazy 激活把 `startup_timeout_seconds` min() 进请求 deadline，激活耗 5.36 s 后只剩 234.66 s，客户端在 17:18:50 退休（`duration_ms: 240020.279`，`reason: "TimeoutError: "`）。Fusion 在 17:21:43 成功、17:21:47 `DocumentSaved`、17:22:01 云端 v4。用户在 17:27:35 退出并重启 Fusion，代价约 11 分 49 秒，而这次重启在事后被证明毫无必要。窗口内 `pmset -g log` 无任何 sleep/wake。见 `cad/r3_full_vehicle_plan/v018/fusion-rebuild-v1/incident-20260906-1718-chunk3-upstream-timeout.json`。 |
| R3-V020 chunk36，PID 56355 | 主机休眠横跨在飞请求 + 嵌套预算相等 | `HostSleepDuringInFlightFusionMcpRequest`；`NestedClientBudgetsEqualCauseAmbiguousCancellation` | 21:43:08 写 in-flight marker；21:46:21 合盖休眠 1454 s；22:10:35 唤醒；22:11:32 Fusion 才收到请求；22:12:14 客户端到期（墙钟 1752 s，`duration_ms: 300000.966`，差值 1452 s 等于休眠时长——`time.monotonic()` 在 macOS 休眠期间冻结）。cell 预算与 acceptance 预算都是 300 s，外层先赢，`reason` 因此是 `CancelledError: Cancelled via cancel scope ...` 而非 `TimeoutError`。Fusion 在 22:13:19 成功（`duration=107070ms`）、22:13:23 `DocumentSaved`、22:14:22 云端 v37。未重启 Fusion，但流水线 STOP。同日 20:48:55-20:52:50 另有一次 235 s 合盖休眠横跨在飞脚本而侥幸未超时（Fusion 报 `duration=350993ms`，含休眠）。见 `cad/r3_full_vehicle_plan/v020/fusion-rebuild-v1/incident-20260906-2212-mac-sleep-during-chunk36.json`。 |
```

### 4.2 在 `## 强制检索与拒绝映射` 表尾（第 84 行）之后追加三行

```markdown
| 单次 CAD 操作预计超过 200 s（大批量几何导入、复杂布尔、大装配 readback） | `StartupBudgetTruncatesFirstGuardedRequest` | 必须显式 `--long-request`，且确认 acceptance 预算比 cell 预算大至少 90 s。不得把普通预算调到 300 以上（构造期即 `ValueError`）。激活预算独立，不随之放大。 |
| 长任务运行期间主机可能休眠（笔记本、电池供电、可能合盖） | `HostSleepDuringInFlightFusionMcpRequest` | cell 在请求在飞期间自动持有电源断言并检测休眠间隙，超时前给出有界延长；但 `caffeinate -i` 与 `PreventSystemSleep` **都不阻止合盖休眠**，因此运行纪律是长任务期间不合盖。退休后先按 AppLog 完成证据分诊，不要默认重启。 |
| 客户端已 timeout/取消并退休，且 `reason` 属于纯预算/传输失效 | `ClientBudgetLossIsNotFusionFailure` | 先跑 `scripts/fusion_recovery.py evidence`；AppLog 证明该请求完成（且写类操作有 `DocumentSaved`）时用 `recover --apply` 解除退休，marker 改名保留。证据不足或成因属于身份/窗口/保全类时，仍按原 no-retry 边界换新 PID。 |
```

### 4.3 在 `## 计数与证据边界` 第 21 行之后追加

```markdown
- 客户端 timeout 与 Fusion 侧失败必须分开计数。2026-09-06 的两次 timeout 在 Fusion 侧都是
  `success=true` 并已 `DocumentSaved`，把它们计入「Fusion 故障」会污染可靠性统计。
- Fusion 的 `MCPAnalytics tracked ... duration=` 是**墙钟**（含休眠），客户端预算是
  **monotonic**（不含休眠）。跨休眠事件比较两者时必须先减去 `pmset` 记录的休眠时长。
```

---

## 5. 落地顺序建议

1. 先并入 `fusion-execution.md` 与 `fusion-mcp-caller-guide.md` 的预算表与休眠纪律——它们不依赖代码，
   立刻能减少误判与无谓重启。
2. 再并入 `fusion-mcp-ambiguity-triage.md` 的恢复路径 A，同时同步 `docs/fusion-mcp-ambiguity-triage.md`。
   注意：该文本假设 `scripts/fusion_recovery.py` 已安装；未安装前应标注为「工具就绪后启用」。
3. `fusion-runtime-safety-incidents.md` 的两条事故行与三条拒绝映射行随时可并入。
4. 代码补丁落地时，`--marker-enforcement` 默认从当前的等效 `warn`（SIMPLIFIED-MODE）切到
   `evidence-gated`。切换前确认当前活着的 Fusion PID 没有未解除的 marker；有的话先跑
   `fusion_recovery.py recover --apply`，或临时用 `--marker-enforcement warn` 过渡。
