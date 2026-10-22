# 当前任务的原消息方法来源

`project task-view` 显式接收 `oev2.current-task-view-input/2`，在外层保留
完整原 `CurrentTaskViewInput/1`；输出 `/2` 内的 `task_view` 是同一 owner
实际产生的完整原 `/1`。原 `/1` 的字段、序列化与检查意义保持。

每个 `methods` 绑定当前对象、原 scope 的完整 basis UTF-8 SHA、唯一
`fact_property`、fact 原 `source_refs` 中的 `source_id`、Project evidence
登记的原 intake `ArtifactReference`、显式原 archive `ArtifactReference`
及本次选定 `entry_ids`。完整 basis `/2` 的 goal support 也在原字节中，
不能把共用字段投影视为完整新版。

Rust 沿原 workflow、Project、basis、conversation 和 impact 执行：先保留
全部原 kernel/work3 检查，再重读 intake 和唯一显式 archive，对原 snapshot
prefix/window 做字节核验，并复用 `conversation::verify_entry` 核文本、角色、
kind、record/call ID、parent、时间和 raw SHA。另数实际 newline 及记录边界，
防止同时伪造 line 与 entry ID。输出保留原 Entry、独立文本 SHA、读窗、
partial/超限 issue、缺失值与未选条目的范围。没有显式 archive 时不会按
intake 文本发现路径或回退读取。
`intake_statistics_verification=historical_supplied_declaration` 明确标记原
entry_count、status/partial、issues、record_types 等统计是原摄取供应的声明；
selected-entry 核验没有重新扫描或认证整份 intake 统计。所选原消息及快照
字节/path/range 的通过不能冒称整 session 已读或这些统计取得新 producer 认证。

原知识 archive 512 MiB、摄取 record 2 MiB 和小 artifact/intake 16 MiB
保护保持。原档案可以是任务授权历史路径的绝对普通文件，也可为本根相对
普通文件；均只读，禁止父遍历和符号链接。每次核验前后重读完整来源，不
跨调用缓存通过状态，不读取 archive 中提到的其它文件或执行历史指令。
本版最多 32 绑定、合计 64 选定 entry；它是有限声明核验合同。

缺件给出 `unknown`，源漂移或绑定/消息冲突给出 `refuted`。内部完整原
`task_view` 不被这些新状态改写；外层 `affected` 通过原 `project::impact`
记录失效方法的既有下游，所有原工程检查继续保留。每项 `impact` 保存
该源变化会影响的原依赖闭包；matched 情况不将此假定变化加入外层 affected。
未列出的依赖、未供应其它原件仍是 unknown。

`project verify-task-view` 对显式 `/2` 重新计算完整 task 与方法观察。
缺件、archive/intake 漂移、伪造消息、旧 Project/scope/basis 或同 revision
内容变化均阻止旧 view 复用。只有恢复原字节并保持全部绑定一致，才能恢复
同一来源观察。derive 的退出 0 表示只读派生完成；其 freshness 可以为
unknown/refuted，不能当来源通过。verify 缺源或冲突非零。

matched 仅证明本次选定的方法声明具有核验的原消息来源。原 fact 的
declared 状态、历史执行、其它消息、适用性与物理未知不被升级；没有
Project 写入、模型调用、工程采用、任务派发或经验/治理晋升。
已闭环项目历史会话中的 assistant 原因说明以及其中旧 11 个视图刷新的说法，保持
历史消息声明身份；本入口没有重新执行原生工具，不能作为当前 native 结果。

本轮原已闭环项目的两段历史会话、真实项目 work 记录中的 V262 service 与完整 typed Project 修订
均是已读 development replay。该 work 记录的四条验收、九项 unknown 决定与最终
项目专属精度要求未采用须保留；该 Project 修订的五条验收、十项决定与54原来源也完整保留。
它们不证明独立迁移收益或整个当前实际工程项目完成。P7 的未见任务比较、实际
使用反馈、恢复和工程收益，及原19模块/19协议/22 owner/13494行为仍需各自证据。
