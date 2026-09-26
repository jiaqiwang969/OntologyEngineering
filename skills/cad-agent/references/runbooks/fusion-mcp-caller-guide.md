# Fusion-MCP 调用方指南（写给后续接入的用户与 agent）

本文面向**任何后续要调用 fusion-mcp 的用户或 agent**（codex、claude、脚本等）。
读完应当知道：各组件是什么关系、正确的调用姿势、以及出错时什么能做什么绝对不能做。
独立 skill 的执行架构与入口见 `../fusion-execution.md`；事故史与禁令见同目录
`fusion-runtime-safety-incidents.md`。

## cad-agent 与 fusion-mcp 的关系

```
你的 agent (codex / claude / ...)
        ├──────────────► ontology-engineering / Semantica
        │                本体、CQ、SHACL、规则与项目审查
        │
        └──────────────► fusion-mcp（fusion_mcp_proxy.cell_server，“cell”）
                         特权执行代理：守卫 + 会话生命周期 + 自愈
                                 │  HTTP (仅 127.0.0.1)
                                 ▼
                         Autodesk Fusion 内置 MCP 端点
                         http://127.0.0.1:27182/mcp（真正的执行者）
```

- **cad-agent** 是工程本体论中的 CAD 模块，负责原生几何与执行证据。
- **Semantica** 是唯一正式语义执行面。新模块不安装旧的 `cad-agent-mcp`；
  其静态工具目录由 `scripts/semantic_query.py capabilities` 读取。
- **fusion-mcp** 是它的**执行平面**适配器：每个客户端会话一个 cell 进程，
  把 stdio MCP 转发到 Fusion 内置 HTTP MCP，并在中间实施全部安全规则。
  工具：`fusion_mcp_read` / `fusion_mcp_execute` / `fusion_mcp_update` /
  `fusion_mcp_electronics_read`。
- CAD 决定需要语义审查时，按父技能的 binding 与 task 进入 Semantica；
  需要实际几何时，经 fusion-mcp 读回或执行，再把原生证据交回项目。

## 调用方必须遵守的规则

1. **永远不要直连 `127.0.0.1:27182`**。cell 代理不是可选包装：进程身份验真、
   窗口证据（Peekaboo）、跨进程 owner lease、会话生命周期、隔离规则全在代理里。
   绕过代理 = 重新制造 2026-08 的悬挂会话事故。
2. **一次调用 = 一个 Fusion 会话**。cell 为每次工具调用新建 MCP session，
   成功后精确释放（恰好一次 DELETE）；超时/断流/取消则**绝不** DELETE、
   绝不重连、立即隔离。这是防写入歧义的安全线，不要"优化"它。
3. **同 PID 的调用保持互斥**。不同 PID、完整端点/启动令牌绑定且独立文档的分工见
   [多实例路线](../fusion-multi-instance.md)；F1/`_common` 工具仍按全局 busy 检查串行。
   不要通过更换 safety state root 绕过 owner lease。**耗时取决于 CAD 工作量**：只读与小改 8–15 秒，大批量几何导入可达数分钟
   （实测最长 404.6 秒），不要按无响应处理。超过 300 秒的操作必须用 `--long-request`。
4. **超时后绝不盲目重发同一请求**。上游结果可能已提交而回执丢失；
   守卫会把该 Fusion PID 置为隔离。按同目录 `fusion-mcp-ambiguity-triage.md`
   分诊，读取原生状态确认后再继续。
5. **进程会自我退休，这是特性**。cell 在客户端死亡、被同客户端的新实例替换、
   或 stdio 空闲超 1 小时（`--idle-exit-seconds`，0 关闭）时自动退出。
   客户端下次调用重新 spawn 即可，无状态丢失。
6. **健康检查用验收客户端**，不要手写探针：
   `python -m fusion_mcp_proxy.acceptance_client --plan plan.json -- <cell argv>`，
   plan 首键必须是 `"schema": "fusion-mcp.stdio-acceptance-plan.v1"`。
7. **一律走 `scripts/fusion_ops.py`，不要再手写驱动逻辑。**
   2026-09-05/06 的四个驱动各自重写了：guard-transient 重试、`message` → 末行 JSON 解析、
   文档名/lineage 身份校验、"脚本无输出"检测、ASCII 转义、精确文档搜索 ——
   四份实现四种缺陷。现在只有一份、带测试：

   ```python
   from fusion_ops import FusionOps, RetryPolicy
   ops = FusionOps(receipts_dir=OUT, retry=RetryPolicy(attempts=6, delay_seconds=45.0))
   ops.assert_active_document(NAME, LINEAGE)          # 身份先于一切写操作
   outcome = ops.call_execute_script(script_path, stem="A8_parameters")
   if outcome.classification != "delivered_ok":
       raise SystemExit(outcome.message)              # 绝不把非 delivered_ok 当成功
   ```

   也可当 CLI 用：`fusion_ops.py script A8.py --receipt-dir . --stem A8_parameters`、
   `fusion_ops.py read '<args>'`、`fusion_ops.py search-exact <NAME>`、
   `fusion_ops.py escape <script.py>`、`fusion_ops.py render <tmpl>`、
   `fusion_ops.py classify <report.json>`（离线复判历史报告）。
   完整的**结果分类法**（`fusion_ops.TAXONOMY`，9 个 classification + 退出码 + 可否重试）
   只有一张表，在 `../fusion-execution.md` 的「结果分类法」一节；本文不重复。
   退出码 0 **不等于成功**，只等于"已送达"；成功的唯一依据是
   `classification == "delivered_ok"`。
8. **重试只允许发生在 upstream 之前的守卫拒绝上，且必须是全新的一次性 cell。**
   唯一可重试的分类是 `guard_transient`（例如
   `Fusion module-loading auxiliary is not uniquely stable`、窗口 census 类拒绝），
   默认上限 6 次、间隔 45 s，可配置。
   `upstream_timeout`、`transport_failure`、`inner_script_error`、`no_output`
   **一律不得重试**：上游可能已经提交，重发就是二次写入。
   `RetryPolicy` 在构造期就拒绝把这些分类放进可重试集合，写不出来。
   超时之后的纪律不变：同一 Fusion PID 不得再收到任何 MCP 请求，
   包括只读确认、健康探针、save/close/quit 与新建/删除 session。
9. **`document/search` 是模糊匹配，必须做精确过滤。**
   2026-09-06 19:31 实测：搜 `FA04_V019_TROLLEY_6P84P_W850_REBUILD_V0_1`
   返回 `Found 1 document(s)`，而那一条是 `FA04_V018_TROLLEY_6P84P_REBUILD_V0_1`。
   信 `count` 就会把工作做进错的文档。用 `ops.exact_document_search(name)`
   （按 `name` 逐字节相等过滤），命中数不是 1 就 HOLD。
   写操作之前再用 `ops.assert_active_document(name, lineage)` 证明活动文档身份；
   需要切换时用 `ops.activate_document(name, lineage)`：
   激活与回读是两个独立的一次性 cell，中间在 MCP 之外 settle。
10. **失败报告必须留证。**
    `ops.call(..., stem="A8_parameters")` 配合 `receipts_dir` 会写全
    `.args.json` / `.py` / `.result.json` / `.stderr` / `.exit` 五件套，
    其中 `.result.json` 含 `classification`、`raw_text`、`cell_stderr_tail`。
    以前 `A6_import_step.stderr` 只有 129 字节的 `"call": {}`，真因全丢；
    现在 cell 的启动错误会原样落盘。
11. **预算内紧外松，不要手工把它们设成一样**。cell 操作预算 T、acceptance read timeout T+90、
    外层硬杀 T+210；激活预算独立且不超过 300 秒。两个预算相等会让外层先赢，把可分诊的
    `TimeoutError` 变成歧义的 `CancelledError`（2026-09-06 22:12 事故即此）。
    `fusion_call.py` 已自动派生外层，只需设 `--timeout`。预算表见
    `../fusion-execution.md` 的「调用预算与主机休眠」。
12. **超时后先取证再重启**。跑 `scripts/fusion_recovery.py evidence --latest`；Fusion 的 AppLog
    能证明请求已完成并存盘时，用 `recover --marker <retirement.json> --apply` 解除退休即可，
    无需重启 Fusion。证据不足时工具以退出码 4 拒绝并不写任何文件，此时才按
    `fusion-mcp-ambiguity-triage.md` 换新 PID（该 runbook 的恢复路径 A 就是这一步，
    路径 B 才是换 PID）。长任务期间不要合盖笔记本：`caffeinate -i` 不阻止合盖休眠。

## 已知易错点

- `fusion_mcp_read` 的 `queryType: "document"` 必须带 `operation`
  （`search` / `open` / `recent`），否则返回参数错误。
- `activeCommand` 查询返回 `{"activeCommand": null}` 是**健康结果**
  （当前无活动命令），不是失败。
- 截图/文档类查询在 Fusion 停在主页、无打开文档时会返回业务错误，属预期。

## 旧部署记录（2026-08-27，仅作事故溯源）

- 当时本机 cell 由 `~/.codex/config.toml` 以 node `fusion-local` 启动，
  venv 为 editable 安装。现行模块的独立 wheel 与实际进程路径须以
  `doctor.sh` 的实时回读为准。
- 会话生命周期 + 自愈修复：commits `89a35ae`、`f7ba408`、`5af9cba`
  （分支 `feature/fusion-cell-control-plane`）。
