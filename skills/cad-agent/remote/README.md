# 已退役的 CAD MCP 传输入口

`mcp_bridge.py` 仅保留兼容报错入口：无条件返回退出码 64，不读取连接配置，
不连接主机、不启动进程或服务。旧的 `CAD_AGENT_LEGACY_CAD=explicit` 不能恢复它。
本目录保留的历史服务端资料不属于当前执行路径，也不作为回退方案。

当前 CAD 通过 [NXOpen／Journal 直连](../references/nx-execution.md)执行，
入口为 [nx_direct.py](../scripts/nx_direct.py)。原生模型、文件版次和回读证据仍按
[CAD 证据合同](../contracts/cad-evidence.v1.schema.json)记录。
