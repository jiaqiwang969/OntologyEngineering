# 专业工具的上游与接续差异

运行程序从各上游安装。本目录发布 OntologyEngineering 的接续差异、固定来源清单和
自有验收样例。七个专业 Skill 的完整文件统一在根目录 `modules/` 维护，
运行环境继续放在 `var/` 或用户选定位置，不随仓库复制。

第七种专业能力 [electronics-design](../modules/electronics-design/README.md) 的统一 Skill 和五个内部模块随仓库维护，运行工具按其说明安装；固定来源与接入差异见模块 `SOURCES.json`。内部模块不作为五个同级 Skill 安装。

- [工具接入、安装和使用](../docs/TOOL-INTEGRATIONS.md)
- [固定版本、提交与 Skill 摘要](tools.lock.json)：`skill_directory` 指明六工具各自在仓库根相对的 `modules/` 路径，`skill_files` 约束全部 16 份文件。
- `patches/`：四个固定上游 Skill 的完整本地差异，含 GitContribute/SmokingGun 的 LOCAL.md 与 UPSTREAM.md；保留原补丁用于干净上游副本重建，不重复应用到已整合的 `modules/`，不修改上游算法。
- [LeanToken](../modules/leantoken/SKILL.md) 与 [Flameox](../modules/flameox/SKILL.md)：完整本项目 Skill，统一由 `modules/` 维护，不伪称上游差异，不保留旧 `companions/` 同名入口。
- [当前六 Skill 源码覆盖](COVERAGE.md)与[可移植接续入口](bindings/WORKFLOW.md)。
- `examples/`：自有小样例。运行结果写入 `var/`，不把本机测量作为新安装的通过记录。
- `licenses/`：覆盖补丁上下文、来源及随 `modules/` 分发的对应上游 Skill；第三方及派生文件仍按各自许可证分发。

上游升级必须重新核对差异及功能；不要把旧补丁强行应用到新版本，也不要以 Skill
安装成功替代运行程序、宿主连接和业务能力的验证。

在仓库根运行 `python3 scripts/check_tool_skills.py` 核模块完整摘要；需要检查宿主重复入口时追加可重复的 `--skill-root`。各模块保留正常发现元数据，fleet 外置；不要通过上游 setup/upgrade 再向宿主顶层安装同名 Skill。MCP 独立配置并在新会话实际调用，具体流程见工具接入文档。
