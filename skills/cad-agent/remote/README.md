# NX / AutoCAD 可配置远程桥

> Historical MCP transport package only. The current supported CAD execution entry is ../scripts/nx_direct.py with ../references/nx-execution.md. No bridge service is required; do not start this transport as a fallback.

`mcp_bridge.py` 是一次性 MCP stdio 客户端。它通过用户明确配置的 SSH 别名把 JSON-RPC 发到 Windows 主机；没有配置就不会连接。每个业务调用只发送一次，超时表示执行状态未知，不自动重试。调用结果仍需按 [CAD 证据合同](../contracts/cad-evidence.v1.schema.json)保存来源、模型版次、原生回读与适用范围。

复制 `profiles.example.json` 到仓库外的私有配置文件，填入接收方自己的 SSH 别名、Python 路径及工作目录。保留 SSH 正常主机密钥验证；该 JSON 不放密码或客户数据。客户端仅需 Python 3.11+；Windows 端应安装其自行许可的 CAD 软件及依赖。

```bash
python3 remote/mcp_bridge.py --config /path/to/cad-profiles.json --profile nx --list-tools
python3 remote/mcp_bridge.py --config /path/to/cad-profiles.json --profile nx nx_status '{}'
python3 remote/mcp_bridge.py --config /path/to/cad-profiles.json --profile autocad acad_status '{}'
```

**NX：** 本包公开的是可配置 SSH/MCP 传输桥，不包含 Siemens NX、外部 `nx_mcp.server` 实现、NX 许可或 GUI journal。示例 profile 假定接收方已在其 Windows 主机安装兼容 MCP sidecar 并启动 NX 内部桥；从 `--list-tools` 核对实际工具名。未在接收方 NX 会话调用、保存并重新打开模型前，不声称完成了原生回读。

**AutoCAD：** 本包同时包含独立的只用 Python 标准库实现 MCP 协议的 `autocad_server.py`，其 COM 操作需要 Windows、AutoCAD 和另行安装的 `pywin32`。把 `autocad_server.py`、`relay_server.py`、`relay_client.py` 放到 Windows 主机同一目录。以登录桌面的用户身份运行 `python relay_server.py`；它只监听 `127.0.0.1`，首次生成 `relay.token`。限制该文件为该用户可读，并保持 SSH 用户与桌面用户相同。SSH 进程通过 `relay_client.py` 连接桌面会话。按需设置 `CAD_BRIDGE_PYTHON`、`CAD_BRIDGE_PORT`、`CAD_BRIDGE_TOKEN_FILE`、`CAD_BRIDGE_SERVER` 环境变量；两端端口与 token 文件要一致。

AutoCAD 工具默认只读。`open_drawing` 默认以只读方式打开。若需要修改**工作副本**，先把图纸复制到专用目录，再在桌面会话显式设置 `CAD_BRIDGE_ALLOW_WRITE=1` 和 `CAD_BRIDGE_WORKSPACE`；`add_line` 与 `save_drawing` 仅能操作该目录内的活动图纸。接口提供状态、文档、图层、实体检查及一条受控绘图操作；不等于旧私有工作站上的全部 CAD 命令已经移植。真实 COM、桌面会话和软件版本需在接收方环境单独验收。本包的回归只覆盖协议模拟与纯几何算例。
