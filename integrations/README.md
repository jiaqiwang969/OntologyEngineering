# 专业工具的上游与接续差异

运行程序从各上游安装。本目录发布 OntologyEngineering 的接续差异、固定来源清单和
自有验收样例，不随仓库复制第三方运行环境。

第七种专业能力 [electronics-design](../modules/electronics-design/README.md) 的统一 Skill 和五个内部模块随仓库维护，运行工具按其说明安装；固定来源与接入差异见模块 `SOURCES.json`。内部模块不作为五个同级 Skill 安装。

- [工具接入、安装和使用](../docs/TOOL-INTEGRATIONS.md)
- [固定版本、提交与 Skill 摘要](tools.lock.json)
- `patches/`：四个固定上游 Skill 的完整本地差异，含 GitContribute/SmokingGun 的 LOCAL.md 与 UPSTREAM.md；不修改上游算法。
- `companions/`：LeanToken 与 Flameox 的完整本项目 Skill，不伪称上游差异。
- [当前六 Skill 源码覆盖](COVERAGE.md)与[可移植接续入口](bindings/WORKFLOW.md)。
- `examples/`：自有小样例。运行结果写入 `var/`，不把本机测量作为新安装的通过记录。
- `licenses/`：补丁上下文及来源对应的许可证；各工具仍按各自上游许可证分发。

上游升级必须重新核对差异及功能；不要把旧补丁强行应用到新版本，也不要以 Skill
安装成功替代运行程序、宿主连接和业务能力的验证。
