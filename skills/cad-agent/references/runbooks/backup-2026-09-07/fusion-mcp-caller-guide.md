# Fusion-MCP 调用方指南（写给后续接入的用户与 agent）

本文面向**任何后续要调用 fusion-mcp 的用户或 agent**（codex、claude、脚本等）。
读完应当知道：各组件是什么关系、正确的调用姿势、以及出错时什么能做什么绝对不能做。
独立 skill 的执行架构与入口见 `../fusion-execution.md`；事故史与禁令见同目录
`fusion-runtime-safety-incidents.md`。

## cad-agent 与 fusion-mcp 的关系

```
你的 agent (codex / claude / ...)
        │  MCP over stdio
        ├──────────────► cad-agent-mcp（cad_geometry_mcp，语义侧车）
        │                只读：本体 / SHACL / 能力问题 / 工具目录
        │                永远不接触 Fusion
        │
        └──────────────► fusion-mcp（fusion_mcp_proxy.cell_server，“cell”）
                         特权执行代理：守卫 + 会话生命周期 + 自愈
                                 │  HTTP (仅 127.0.0.1)
                                 ▼
                         Autodesk Fusion 内置 MCP 端点
                         http://127.0.0.1:27182/mcp（真正的执行者）
```

- **cad-agent** 是整个系统（本仓库）：本体引导的 CAD agent，含三个平面
  （agent 编排 / 语义 / 执行）。
- **cad-agent-mcp** 是它的**语义平面** MCP server：只读，回答"是什么/该怎么做"，
  与 Fusion 进程零交互，随便调用，无副作用。
- **fusion-mcp** 是它的**执行平面**适配器：每个客户端会话一个 cell 进程，
  把 stdio MCP 转发到 Fusion 内置 HTTP MCP，并在中间实施全部安全规则。
  工具：`fusion_mcp_read` / `fusion_mcp_execute` / `fusion_mcp_update` /
  `fusion_mcp_electronics_read`。
- 两个 server 互补不重叠：先问语义侧车"该做什么"，再经 fusion-mcp"去做"。

## 调用方必须遵守的规则

1. **永远不要直连 `127.0.0.1:27182`**。cell 代理不是可选包装：进程身份验真、
   窗口证据（Peekaboo）、跨进程 owner lease、会话生命周期、隔离规则全在代理里。
   绕过代理 = 重新制造 2026-08 的悬挂会话事故。
2. **一次调用 = 一个 Fusion 会话**。cell 为每次工具调用新建 MCP session，
   成功后精确释放（恰好一次 DELETE）；超时/断流/取消则**绝不** DELETE、
   绝不重连、立即隔离。这是防写入歧义的安全线，不要"优化"它。
3. **并发天然安全但串行**。多个 agent 同时调用时由 owner lease 串行化；
   排队是正常现象。单次调用典型耗时 8–15 秒（含窗口验真），不要按无响应处理。
4. **超时后绝不盲目重发同一请求**。上游结果可能已提交而回执丢失；
   守卫会把该 Fusion PID 置为隔离。按同目录 `fusion-mcp-ambiguity-triage.md`
   分诊，读取原生状态确认后再继续。
5. **进程会自我退休，这是特性**。cell 在客户端死亡、被同客户端的新实例替换、
   或 stdio 空闲超 1 小时（`--idle-exit-seconds`，0 关闭）时自动退出；
   cad-agent-mcp 同理。客户端下次调用重新 spawn 即可，无状态丢失。
6. **健康检查用验收客户端**，不要手写探针：
   `python -m fusion_mcp_proxy.acceptance_client --plan plan.json -- <cell argv>`，
   plan 首键必须是 `"schema": "fusion-mcp.stdio-acceptance-plan.v1"`。

## 已知易错点

- `fusion_mcp_read` 的 `queryType: "document"` 必须带 `operation`
  （`search` / `open` / `recent`），否则返回参数错误。
- `activeCommand` 查询返回 `{"activeCommand": null}` 是**健康结果**
  （当前无活动命令），不是失败。
- 截图/文档类查询在 Fusion 停在主页、无打开文档时会返回业务错误，属预期。

## 部署事实（2026-08-27）

- 本机 cell 由 `~/.codex/config.toml` 以 node `fusion-local` 启动；
  venv 为 editable 安装，`src/` 的修复对每个新 spawn 的进程立即生效。
- 会话生命周期 + 自愈修复：commits `89a35ae`、`f7ba408`、`5af9cba`
  （分支 `feature/fusion-cell-control-plane`）。
