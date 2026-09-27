# 可选 Semantica MCP 传输

`scripts/mcp_stdio.py` 仅提供 `semantica` 模式，将 `semantic_doctor`、
`semantic_discover`、`semantic_run` 和 `semantic_review` 转交根 skill 的
source-locked Semantica 与 `scripts/semantic_engagement.py`。
适配器不加载第二套语义引擎，也不提供 CAD 执行；CAD 使用 NXOpen/Journal。

已有注册使用 CAD 模块的 `.venv/bin/python` 启动
`/absolute/cad-agent/scripts/mcp_stdio.py semantica`。
只有用户要求注册工具时才修改客户端配置。保留当前连接和正在运行的会话，
迁移前核对实际注册与运行时，不因清理旧文档而重启或改动它们。

初始化和工具列表证明传输可用；`semantic_doctor`、`semantic_discover` 用于核对
来源身份与能力。正式运行或 review 仍要求对应项目 binding、当前 task、包身份
和原生回执；传输连通不代表工程验收或发布通过。
