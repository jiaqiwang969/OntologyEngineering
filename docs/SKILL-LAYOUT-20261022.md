# 七个专业 Skill 统一目录验收（2026-10-22）

本次将七个专业 Skill 统一放入 `ontology-engineering-unified/modules/`，保留各模块的正常发现元数据及完整规则；engineering-memory 保持原路径，fleet 独立在仓库外。已发布并实际核验的布局提交为 [651dad2363d27d641c7d3167caab7ea2953cdd10](https://github.com/jiaqiwang969/OntologyEngineering/commit/651dad2363d27d641c7d3167caab7ea2953cdd10)。本记录针对目录分发、宿主发现和限定接入，不重新授予完整工程资格。

## 文件与安装身份

- GitContribute、LeanToken、REA、Jacobian、SmokingGun、Flameox 的 16 份 Skill 文件与迁移前锁定文件逐字节一致；electronics-design 的入口另行核对。REA 的六份引用资源和 GitContribute/SmokingGun 的本地补充文件完整保留。
- 四份固定上游 diff 保持原字节，供干净上游副本重建核验。已整合模块不重复打补丁；LeanToken/Flameox 的维护源移入 `modules/`，不再保留 `companions/` 同名入口。
- 两个宿主搜索根的 12 个旧同名目录，以及未使用的上游 SmokingGun Skill 快照，已原子移入私有维护备份并核对逐文件内容与元数据。旧 `skills-bak` 完整备份未动；运行程序、项目数据和记忆库未随 Skill 目录搬迁。
- 七个模块通过 Skill 快速校验。首次执行因校验环境缺 PyYAML 失败，失败记录保留；补齐校验依赖后重新执行通过，未把缺依赖的失败计作通过。
- `scripts/check_tool_skills.py` 的 12 个针对性案例经独立复核符合预期，包括内容修改、文件缺失、同名入口、名称表示差异和符号链接情形。另从公开 GitHub 克隆固定提交，重新核验全部模块文件、补丁与入口。
- 既有八个核心程序及安装清单摘要未变。本轮没有重新构建 Rust 程序；运行安装身份继续对应此前的 rc.7 安装，布局源码与运行程序保留各自身份。

## 实际宿主发现

真实 Codex `skills/list` 在用户目录和统一安装根两个 cwd 下分别返回 10 个自有且唯一的入口：工程本体、engineering-memory、七个专业模块与 fleet。独立新模型会话也实际收到这 10 个入口，没有通过 skills 配置覆盖伪造发现结果。七个专业入口均来自 `modules/`，fleet 在仓库外；系统和插件 Skill 不计入这 10 项。

宿主仍报告扫描数量上限告警，原始告警保留。已核的 10 个目标均可见且各有唯一入口；这不表示扫描已穷尽所有嵌套目录。文件检查器也只核其声明范围，不替代宿主的实际发现行为。

## 限定运行接入

独立新会话实际发起以下四个 MCP 调用，保留返回状态与边界：

| 工具 | 实际调用 | 本次支持的结论与限制 |
|---|---|---|
| REA | `binary_session` | 6.5.0 服务响应，`open=false`；未进行新的反编译或运行观察 |
| GitContribute | `corpus_get_coverage` | 服务响应为 `partial` / `repository_not_indexed`；未建立该仓库索引，不能计作覆盖完成 |
| Jacobian | `math_find` | 检索命中行列式相关操作；本轮未执行数学计算 |
| Flameox | `query_evidence` | 查询已有证据元数据；本轮未采集工作负载或产生新的性能结论 |

LeanToken 与 SmokingGun 的首次新会话版本检查由 PATH 选中了旧全局入口，这两项输出已排除出新安装验收，原记录保留。随后主验收进程使用当前安装的绝对路径，核对 LeanToken 原生程序、Node 和 SmokingGun CLI 文件的 SHA256，实际获得 LeanToken 0.1.28 与 SmokingGun 5.0.0 / Node 24.18.0。全局包装器未改写。

随后独立新 Codex 会话按明确 argv 补验，三份文件的真实路径与 SHA256 全部匹配，两条版本命令均退出 0，结果与主验收进程一致。首次补验受测试编排人为设置的三次调用上限影响，提前结束且没有完整结果，不计通过；去掉该编排限制后重新执行并完整回读，原失败保留。此次补验仅确认安装入口、文件身份与版本，不代表源码索引或复杂度分析已重新执行。四 MCP 会话、CLI 补验会话及其已观察记录的任务子进程均已退出，没有借此启动常驻服务或重启其他会话。

## 复核方式与范围

按 [AGENTS.md](../AGENTS.md) 和[工具接入说明](TOOL-INTEGRATIONS.md)配置实际路径。在仓库根检查模块文件，再按宿主实际搜索根检查同名入口：

```sh
python3 scripts/check_tool_skills.py
python3 scripts/check_tool_skills.py --skill-root "$HOME/.codex/skills" --skill-root "$HOME/.agents/skills"
```

该命令只读检查，不代为移动文件。新会话应核实际发现路径；MCP 与 CLI 另核真实运行入口，不能仅凭 PATH 版本字符串确认安装来源。上游 setup/upgrade 可能再生成顶层 Skill，升级后应重新检查。

本轮没有重跑 Rust 全套回归、Project—工具—memory 全闭环、原生 EDA、真实工程交付或其他平台验收。此前限定案例的原提交、失败和通过继续由[空 Skill 重装记录](FRESH-INSTALL-20261022.md)保存，不改写成新布局的完整复测。`v2.0.0-rc.7` 标签仍指向 `675b72aff1af45ab3fd4a2dcbcb81a99acaa3023`，未移动标签。原架构图和外部资料链接保持原样。
