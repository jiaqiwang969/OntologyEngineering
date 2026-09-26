# Fusion runtime safety incidents

> 本文件的规则与事故摘要随 canonical skill 分发；表格中的原始证据链接保留
> 旧开发仓库相对路径，仅作 provenance 提示，不是独立运行所需入口，也不保证
> 在本目录可点击。需要复核原始证据时，必须显式绑定相应历史仓库与精确版本。

本文件是 Fusion 运行时事故的跨课程检索索引。它把分散在 `runs/`、课程 attempt、CER、
AppLog 和实践日志中的证据按运行时边界归类，使后续任务能够先命中同型 warning；它不是新的
workflow record，不覆盖原始证据，也不把历史推测升级为已确认根因。

## 计数与证据边界

截至 2026-08-18，仓库可辨认出 13 起异常终止：12 起出现原生 crash/CER，1 起只确认
Fusion 主进程退出。另有 9 起 timeout、queue-hang 或 transport 安全事件，不属于原生 crash；
窗口可见性/响应序列化与 T-Spline token 不稳定另行列示，不计入这 9 起。

- dump、AppLog 崩溃边界或 CER service/database 能确认事故并支持调用栈分析；
- 结构化事件记录或 CER 截图能确认事故，但没有堆栈时不能确认技术根因；
- practice log 只能保留其明确声称的观察，不能补写缺失的 PID、请求或堆栈；
- Recovery Documents/autosave node 是恢复候选，不是 crash 计数；
- 同一 CER 的多张截图只算一次；人工 SIGKILL 和客户端 timeout 必须单列。

`causal_confidence` 只评价“该触发机制是否为原因”，不评价 CER 图片是否真实：

- `CONFIRMED`：调用栈或等价原生证据闭合；
- `PROBABLE`：动作与 crash 边界具有强时序关系，但缺少完整原生因果链；
- `CONTEXT_ONLY`：只知道 crash 前所在状态，不得写成根因；
- `UNKNOWN`：触发上下文也不足。

## 异常终止索引

| Retrospective ID | 事件 | `incident_kind` | 原因签名 | `causal_confidence` | 原始证据 |
|---|---|---|---|---|---|
| FRS-20260716-01 | New Design 类型选择器销毁时 crash | native crash/CER | `QtTransientDialogAccessibilityLifetimeRace` | `CONFIRMED` | `../runs/fusion-practice/00_official_2026_intent_driven/session-recovery/2026-07-16T150938_fusion-crash/crashLog.txt.dmp.zip` |
| FRS-20260716-02 | 打开 Recovery UI 时再次 crash | native crash/CER | `QtTransientDialogAccessibilityLifetimeRace` | `CONFIRMED` | `../runs/fusion-practice/00_official_2026_intent_driven/session-recovery/2026-07-16T150938_fusion-crash/recovery-open-crashLog.txt.dmp.zip` |
| FRS-20260716-03 | M02 chooser AX press 后 crash | native crash/CER | `QtTransientDialogAccessibilityLifetimeRace` | `CONFIRMED` | `../runs/fusion-practice/00_official_2026_intent_driven/session-recovery/2026-07-16T150938_fusion-crash/m02-chooser-axpress-crash-20260716T1919/crashLog.txt.dmp.zip` |
| FRS-20260717-M03 | 旧 Assembly target 的 Insert 分支出现 CER | native crash/CER | `UNKNOWN` | `CONTEXT_ONLY` | `../runs/fusion-practice/00_official_2026_intent_driven/lesson-01/attempt-0002/full_follow_in_progress/screenshots/M03_retry04_old_target_insert_crash_report.png` |
| FRS-20260720-M06 | 第一次 GUI save 后出现 CER | native crash/CER | `UNKNOWN` | `CONTEXT_ONLY` | `../runs/fusion-practice/00_official_2026_intent_driven/lesson-01/attempt-0002/full_follow_in_progress/execution/M06-hybrid-mode-course-path.json` |
| FRS-20260720-M09 | 按下 Insert 后立即出现 CER | native crash/CER | `UNKNOWN` | `CONTEXT_ONLY` | `../runs/fusion-practice/00_official_2026_intent_driven/lesson-01/attempt-0002/full_follow_in_progress/execution/M09-insert-component-course-path.json` |
| FRS-20260721-I03 | placement 未解决，`activeCommand` probe timeout 后越过 crash boundary | native crash/CER | `McpProbeDuringUnsettledInteractivePlacement` | `CONTEXT_ONLY` | `../curriculum-learning/I03/attempt-0001/session-events.jsonl` |
| FRS-20260731-S18-01 | MCP 脚本中执行 `RenderingEnvCmd` command definition 后 crash | native crash/CER | `InteractiveCommandInsideMcpTransaction` | `PROBABLE` | `../curriculum-learning/S18/attempt-0001/artifacts/crash-recovery-safety-copy-20260731-1311/AppLogFile20260731T113322.log` |
| FRS-20260731-S18-02 | 反复检查 Fusion 浮动对话框 AX tree 时 crash | native crash/CER | `QtTransientDialogAccessibilityLifetimeRace` | `PROBABLE` | `../curriculum-learning/S18/attempt-0001/practice-log.md` |
| FRS-20260802-S24-01 | MCP 脚本内 `Commands.Start thomasa88_ParametricText_Map` 后 crash | native crash/CER | `InteractiveCommandInsideMcpTransaction` | `PROBABLE` | `../curriculum-learning/S24/attempt-0001/artifacts/crash-recovery-safety-copy-20260802-2226/logs/AppLogFile20260731T185400.log`, `../curriculum-learning/S24/attempt-0001/artifacts/crash-recovery-safety-copy-20260802-2226/cer/service.log` |
| FRS-20260802-S24-02 | 重启并进入恢复/模块加载流程后再次出现 CER | native crash/CER | `UNKNOWN` | `UNKNOWN` | `../curriculum-learning/S24/attempt-0001/checks/s24-a0001-second-fusion-cer.png` |
| FRS-20260803-S26-A0001 | hidden `SetTranslation` canary 被拒绝时主进程退出 | process exit without CER | `UNKNOWN` | `UNKNOWN` | `../curriculum-learning/S26/attempt-0001/checks/s26-a0001-crash-recovery.json` |
| FRS-20260803-S26-A0003 | MCP 事务内启动 `TSplineCreasedEdgeCommand` 后 crash | native crash/CER | `InteractiveCommandInsideMcpTransaction` | `CONFIRMED` | `../curriculum-learning/S26/attempt-0003/checks/03-native-crease-canary-crash.json` |

未知根因的 M03、M06、M09、S24 第二次和 S26 attempt-0001 不得各自制造“根因签名”；只有新的
同栈证据或可区分实验闭合机制后，才可追加归并解释。I03 的执行禁令是风险控制，不是因果升级。

## 非原生崩溃安全事件

| 事件 | 类型 | 签名 | 结论 |
|---|---|---|---|
| S17 attempt-0013 | MCP timeout | `CloudEnumerationInsideMutationCriticalCall` | 60 次 folder-content 与 15 次 lineage 请求使脚本运行 133.050 s；客户端 120 s 先超时，唯一写调用未到达。见 `../curriculum-learning/S17/attempt-0013/experiences/evidence/INSERT-01-fusion-applog-timeout-root-cause-v1.json`。 |
| I01 attempt-0003，PID 27117 | MCP timeout；已退休的 in-flight 脚本随后完成并提交 Save As | `CloudEnumerationInsideMutationCriticalCall`；运行时边界 `InFlightFusionScriptMayCommitAfterClientRetirement` | writer 在 `document.saveAs` 前逐项调用 `folder.dataFiles.item(index)`，触发 76 次同目录云读取；客户端约 75 s 后 timeout 并退休 PID，但 Fusion 内脚本继续到 413.992 s，随后成功创建并上传 exact v1。期间没有同 PID 重试，保存状态在外部 `CONTENT_AVAILABLE` 到达前保持 `UNKNOWN`。见 `../curriculum-learning/I01/attempt-0003/checks/runtime-incident-20260818-2116/incident.json`。 |
| S26 attempt-0003，PID 28487 | queue hang + forced termination | 首因 `McpDispatchBlockedByHiddenOrDelayedModal`；后续违规 `McpRetryAfterTimeoutOrHiddenModal` | 隐藏 Recovery Documents 阻塞请求；timeout 后重复 session/call 污染队列，最终人工 SIGKILL，不是第二次原生 crash。见 `../curriculum-learning/S26/attempt-0003/checks/04-mcp-hidden-modal-timeout-forced-restart.json`。 |
| S26 attempt-0003，PID 33346 | MCP timeout | `McpDispatchBlockedByHiddenOrDelayedModal` | 启动清点通过、文档打开后 Recovery Documents 延迟出现；随后一次回读 timeout，未再重试。见 `../curriculum-learning/S26/attempt-0003/checks/05-delayed-recovery-modal-after-open-timeout.json`。 |
| S27 attempt-0001，PID 90870 | MCP timeout + 同执行单元后续请求 near miss | 首因按 `McpDispatchBlockedByHiddenOrDelayedModal` 管控，因果仅 `CONTEXT_ONLY`；执行器缺陷 `SequentialMcpBatchMissingFailFastAfterToolError` | exact S26 v2 打开成功且 clean；文档转换后 `activeCommand` 只读探针 120 s timeout，Fusion AppLog 未观察到该请求 dispatch。外层脚本因未对嵌套 tool error fail-fast 而开始第二项 read，随即被终止；同 PID 未做建模写入并受控退出。见 `../curriculum-learning/S27/attempt-0001/checks/runtime-incident-20260804-1924/incident.json`。 |
| S27 attempt-0001，PID 92863 | MCP timeout | `McpDispatchBlockedByHiddenOrDelayedModal`（`CONTEXT_ONLY`） | 新 PID、单请求执行单元、DashboardReady、两次稳定 post-open 清点和 disposable execute-canary 仍未改变结果：canary 120 s timeout，且没有进入 Fusion AppLog、没有创建 scratch 或 receipt。该同型 MCP 路径不得再重放；转向独立 CustomEvent add-in 机制。见 `../curriculum-learning/S27/attempt-0001/checks/runtime-incident-20260804-1937/incident.json`。 |
| S25 attempt-0005，PID 96749 | 窗口可见性误判 + MCP response serialization error | `FusionWindowOnDifferentDisplayOrSpace`；伴随 `activeCommand` 的 `invalid UTF-8 byte` | Fusion 主窗位于另一显示区域/Space，进程、AppLog 和 clean A0005 v1 均持续存在；不是退出或 crash。画面中央的操纵器是代理图内嵌污染，不是活跃命令。先用 Peekaboo 定位窗口，再用不同 query surface 的单一 document-list canary 确认保存状态，最后只恢复窗口边界。见 `../curriculum-learning/S25/attempt-0005/checks/runtime-visibility-event-20260804-2126/incident.json`。 |
| S27 attempt-0003，02a | MCP transport incident | 传输层异常，按 `McpDispatchBlockedByHiddenOrDelayedModal` 管控（`CONTEXT_ONLY`） | 尺寸复验会话中 MCP 传输异常，核验脚本未进入 Fusion；模型无新增变化，第二屏 0.50 候选保持未保存。见 `../curriculum-learning/S27/attempt-0003/checks/02a-fusion-mcp-transport-incident.json`。 |
| S27 attempt-0003，02b | 原生核验器读数不稳定 | `TSplineSerializationTokenInstability` | 原生 verifier 的 T-Spline 序列化 token 跨读取不稳定：原始文本可变而拓扑持久；身份判定必须走拓扑/结构/解析控制点，不得依赖序列化字节比较。见 `../curriculum-learning/S27/attempt-0003/checks/02b-native-verifier-tspline-token-instability.json`。 |
| S27 attempt-0003，PID 96749 | MCP initialize timeout | `McpRetryAfterTimeoutOrHiddenModal` 预防性禁令 | MCP 在 `initialize` 阶段超时；按同型禁令该 PID 不得再接收任何 MCP 请求，恢复必须换新进程并做启动与文档转换两阶段模态清点。见 `../curriculum-learning/S27/attempt-0003/checks/02c-mcp-initialize-transport-timeout.json`。 |
| S28 attempt-0001，PID 97550 | MCP timeout + 2 个已排队 follow-up | 首因 `ConcurrentFusionMcpSessionQueueContention`（`PROBABLE`）；并发条件 `ActiveDocumentDriftAcrossConcurrentMcpSessions`；执行器缺陷 `SequentialMcpBatchMissingFailFastAfterToolError` | 另一 MCP session 的 Piper 脚本占用执行路径 260.074 s；S28 的 request 13/14/15 只在它结束后才于同一秒出现在 AppLog，并都被 exact-v4 active-document guard 在 8–9 ms 内拒绝，未写模型。该顺序强烈支持队列串行化，但 AppLog 不含客户端最初提交时间，故不升级为 `CONFIRMED`。之后同 PID 退出 MCP 使用；因两个无关未保存文档仍在，重启延后。稍后出现的“服务器验证警告”只作上下文证据。见 `../curriculum-learning/S28/attempt-0001/checks/runtime-incident-20260811-0054/incident.json`。 |

## 强制检索与拒绝映射

后续任务不能只按课号、命令 ID 或对象名查记忆。出现下列拟议动作或状态时，必须先按“操作语义
+ 执行表面 + 生命周期边界 + failure condition”命中对应 warning：

| 当前拟议动作/状态 | 必须检索的签名 | 无新机制时的处置 |
|---|---|---|
| 在 `fusion_mcp_execute` 中出现 `Commands.Start`、交互式文本命令、`CommandDefinition.execute()`、选择或 Commit/Cancel | `InteractiveCommandInsideMcpTransaction` | 直接拒绝；更换命令 ID 或增加 `doEvents()` 不算新机制。 |
| 对 chooser、Recovery Documents 或浮动 Qt 对话框做 AX tree 递归、轮询或 press | `QtTransientDialogAccessibilityLifetimeRace` | 直接拒绝；只做浅层顶层窗口清点。 |
| 应用启动或文档 open/activate/close/版本切换之后 | `McpDispatchBlockedByHiddenOrDelayedModal` | 下一次 MCP 前等待 settle 并再次清点模态。 |
| 任一 MCP 调用已 timeout | `McpRetryAfterTimeoutOrHiddenModal` | 同 PID 不得再建 session 或发送请求；冻结外部证据并受控重启。 |
| 一个外层执行单元计划串行或并行发送多个 Fusion MCP 请求 | `SequentialMcpBatchMissingFailFastAfterToolError` | 默认拆成“一单元一请求”；否则首项以 error object 返回时，后续请求可能在 timeout 边界后自动发出。 |
| 同一 Fusion PID 已有其他 thread/session 执行 MCP，或 active document 被外部 writer 改变 | `ConcurrentFusionMcpSessionQueueContention` / `ActiveDocumentDriftAcrossConcurrentMcpSessions` | 不得并发排队；一个 PID 同时只允许一个已确认 writer。发现外部 session 后停止发送新请求，等待其完全结束；已经 timeout 的 PID 仍按 no-retry 边界退休。 |
| placement/manipulator 尚未完成或取消 | `McpProbeDuringUnsettledInteractivePlacement` | 不得用 MCP 查询其状态；先在事务外完成/取消并 settle。 |
| Fusion 似乎消失但 PID 仍在，主窗位于另一 display/Space | `FusionWindowOnDifferentDisplayOrSpace` | 先用 Peekaboo 按 PID/窗口 ID 做浅层清点；用单一 document-list canary 区分窗口不可见与进程退出。不得因此重启 Fusion 或重做已保存事务。 |
| `activeCommand` 立即返回 `invalid UTF-8 byte` | `McpActiveCommandResponseSerializationError` | 视为 MCP 响应序列化错误，不是 crash 证据；不要同型重试，用外部窗口证据和不同 query surface 的单一只读 canary。 |
| 写调用之前计划枚举 folder、lineage 或网络资源 | `CloudEnumerationInsideMutationCriticalCall` | 将发现与写入拆成独立调用；timeout 不得自动重试。 |
| 客户端/代理已 timeout 并退休，但 AppLog 尚未记录原 in-flight 脚本结束 | `InFlightFusionScriptMayCommitAfterClientRetirement` | 退休只阻止新请求，不能证明 Fusion 已取消原脚本。保持保存状态 `UNKNOWN`，只从外部监测 AppLog/窗口；不得重放、关闭或终止可能仍会提交的工件。自然结束并保护工件后，仍须换新 PID。 |

现有 `S18CommandDefinitionCrashWarning` 已能作为 advisory failure warning 返回，但它没有形成脚本
发送前的 adapter 级 hard guard。当前 `src/fusion_mcp_proxy/mcmaster.py` 仍包含在 MCP 脚本中调用
`definition.execute()` 的历史构造；在它被拆成事务外 UI 启动与独立 CDP 操作之前，不得把该构造
视为已通过本安全规则。这个 open risk 不是新的事故记录。

## 每次事故的最小闭环

```text
首次异常
  -> 停止同 PID 的 Fusion/MCP 写入与探针
  -> 冻结关键原始证据
  -> 区分 incident_kind、primary_signature、unsafe follow-up 和因果置信度
  -> 记录精确文档/lineage/version/保存证据
  -> 检索同型历史与 no-retry boundary
  -> 新进程 + 启动模态清点 + 单一最小健康探针
  -> 文档生命周期转换后的第二次模态清点
  -> 只有新隔离机制和对应回归通过后，才可尝试不同路径
```

恢复成功只证明工件恢复；它不证明原触发动作变安全，也不授权再次执行该动作。
