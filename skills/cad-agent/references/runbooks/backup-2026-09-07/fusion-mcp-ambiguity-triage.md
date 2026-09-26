# Fusion MCP 歧义结果：同 PID 终止策略

本文定义 Fusion MCP 在超时、取消、断连或响应交付不确定时的统一安全边界。该边界属于
运行时协议，不依赖某一次装配任务、某一个调用端或某一份操作说明。

## 一句话规则

只要请求可能已经到达 Fusion，而调用端不能证明结果是否完成，当前 Fusion PID 就立即
进入 terminal retirement。后续工作必须使用不同的 Fusion PID／进程启动令牌；原 PID
不得再收到任何 MCP 请求。

“窗口看起来没变化”“没有新增星号”“应用仍能响应”都不能证明失败操作没有提交，因而
不能解除终止状态。

## 歧义边界

以下情况一律按结果未知处理：

- 上游调用超时；
- 客户端取消或 stdio／SSH 断开；
- HTTP、SSE 或 MCP 传输异常，无法证明请求未送达；
- 操作已经完成，但响应尚未可靠刷新和确认；
- 任何其他无法同时证明“未执行”和“未提交”的异常。

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

唯一允许的 MCP 恢复路径是：

1. 保全 retirement、in-flight、旧 quarantine、AppLog 和外部窗口证据；
2. 结束旧 Fusion 进程并启动不同 PID／不同进程启动令牌的 Fusion；
3. 对新 PID 做不经过 MCP 的身份与浅层窗口检查；
4. 仅对新 PID 运行一次受 challenge 约束的 initialize、tools/list、document-read
   health probe；
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
- 同一个 guard 的第二次请求在进入上游前即被拒绝。

聚焦测试位于：

- `tests/test_fusion_mcp_ambiguity_quarantine.py`
- `tests/test_fusion_mcp_recovery_controller.py`
