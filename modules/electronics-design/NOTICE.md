# 来源与许可证

本模块把 electronics-design 2.1.4 的自有工程方法与五个内部模块接入统一仓库。程序安装独立；不附 EasyEDA 应用、CLI 二进制、连接器、node_modules、私人板项目或原验收回执。

| 内容 | 固定来源 | 许可依据 |
|---|---|---|
| `modules/easyeda-agent/` 的上游 Skill、脚本、参考和合成测试 | [v0.25.1 / c57f58fd42a5732d91bb42a3ec522e2a199cc18a](https://github.com/zhoushoujianwork/easyeda-agent/tree/c57f58fd42a5732d91bb42a3ec522e2a199cc18a/skills/easyeda-agent) | 此 tag 的 [NOTICE](licenses/easyeda-agent-NOTICE) 明示本项目 Apache-2.0；采用 [Apache 全文](licenses/Apache-2.0.txt)，保留 `zhoushoujianwork and contributors` 署名。不能用后来主线 MIT 覆盖这个固定版本。 |
| `modules/easyeda-api/` | [11da20daeb4e5d7f7bdedc30920f5cac836d8cb3](https://github.com/easyeda/easyeda-api-skill/tree/11da20daeb4e5d7f7bdedc30920f5cac836d8cb3) | 上游 SKILL frontmatter 声明 `license: MIT`、`author: JLCEDA`，在本地 GUIDE 中保留。该提交没有单独 LICENSE 文件；[MIT 全文](licenses/MIT.txt)是许可条款文本，不伪称下载到的上游许可证。 |
| `modules/easyeda-schematic-net-fanout/` | [0c4b9a0ad94d532923dee5c828a6efa7444f4506](https://github.com/easyeda/easyeda-enhanced-schematic-skill/tree/0c4b9a0ad94d532923dee5c828a6efa7444f4506) | 上游 SKILL frontmatter 声明 MIT，在 GUIDE 中保留；该提交没有单独 LICENSE 或 copyright 行，不增造其作者/年份。 |
| 总入口、接续方法、3D 提取/文件审计辅助、工人接线模板与本次接入变更 | electronics-design 2.1.4；本仓库维护 | 原入口声明 MIT；自有部分沿仓库 [LICENSE](../../LICENSE)。历史私人案例已移出公开范围。 |

保留的 easyeda-agent NOTICE 还说明其完整运行程序中的第三方 Apache 算法；本包没有复制 `extension/src/beautify/`。实际 CLI/连接器仍需按发行包所带声明使用。模型、器件库、数据手册和下载原件各有自身权利与用途限制，不因本仓库许可而改变。

## 相对固定上游的修改

[patches](patches/) 中的三个指南补丁 是实际可应用的上游资源差异，基线文件与结果 SHA 见 [SOURCES.json](SOURCES.json)。主要内容为 `SKILL.md → GUIDE.md` 内部化、统一 Project 回交、当前授权/后台操作/任务服务边界、安装位置变量和必要相对链接。easyeda-agent 的 `lint.sh` 增加显式 `EASYEDA_BIN`/PATH 查找与模块资源目录；其他两个指南模块的上游核心未修改。另有独立的 easyeda-agent runtime 补丁，基于 Apache-2.0 上游 v0.23.0 / `f1aab8d2b03ab6a0bd78ce83e7961d838f4d850b`，保留本地 Go/TypeScript 修复和测试；功能与边界见 [运行修复](references/runtime-differences.md)。

本仓库已带应用差异后的指南与必要资源；普通用户无需再对本目录打补丁。维护者在更新固定上游时使用补丁核对。自有代码的通用化不是第三方 diff：移除私有案例/默认项目、改用显式根变量、去除下载请求中无效的 cookie 占位头。迁移不继承任何旧工程通过状态。
