# 空 Skill 目录安装与 rc.7 新会话验收

日期：2026-10-22。平台：macOS / arm64。待测发行源码为
[`675b72a`](https://github.com/jiaqiwang969/OntologyEngineering/commit/675b72aff1af45ab3fd4a2dcbcb81a99acaa3023)，
对应 `v2.0.0-rc.7`。本页补充实际重装结果，不覆盖此前开发验证、失败或历史发行身份。

## 安装起点与版本

整棵 `~/.codex/skills` 实际改名为 `skills-bak`，建立新的真实 `skills` 目录；仅复制用户指定的 fleet，逐项核对 52 个文件。系统文档由宿主自行提供。随后从公开 GitHub 克隆统一仓库，按公开锁定版本、完整 Skill、diff 和说明安装六个外部工具及电子模块，没有从旧工具目录或备份复制实现、程序、项目、记忆或验收结果。

首次从 `373093c` 安装的 Codex 会话因额度中断。主控与后续新会话继续使用同一新目录中的公开源码和本轮新下载缓存；中断、接续及失败记录分别保留，不能称为一次连续成功安装。`bef0808` 与 `b831c55` 补充 Z3 打包修正；远端随后发布 rc.7，实际 fetch 到 `675b72a` 并重新构建八程序。

系统编译器、解释器与现有 Codex 登录身份属于明确复用。新建专业工具环境、Ghidra、JDK、依赖缓存及测试数据，不表示整台主机被清空。验证会话使用独立配置，关闭旧 MCP、hooks、插件和历史记忆入口；没有修改其他会话的运行配置。

## 实际结果

| 项目 | 结果与范围 |
|---|---|
| 整树备份及新安装根 | 实际执行，备份保留原 inode，新目录身份不同；fleet 文件逐项匹配 |
| 八个核心程序 | rc.7 从公开源码重新构建并核安装 manifest 与实际摘要 |
| Skill 自动发现 | 新会话实际发现本体、六工具、electronics-design、fleet；另有四份宿主系统文档；六工具共 16 个声明文件匹配公开锁 |
| workspace | rc.7 的 35 项 Rust 案例通过；外置 ProjectStore 公开恢复示例通过 |
| GitContribute | 合成 Git 源索引、搜索、独立数据库备份与恢复，提交身份和保存的全文匹配 |
| LeanToken | 原文回读、旧 generation 的 live bytes/stale 报告及显式重建索引核验 |
| REA | JavaScript 样例核三个导出、输入摘要与独立执行结果；rc.7 再对自有 Mach-O arm64 制品实际反编译 `_affine`，与符号及运行输出核对 |
| Jacobian | 精确有理解、独立代回、行列式与无效输入拒绝；Z3 原生 SAT/UNSAT 检查；真实 MCP 调用再核精确结果 `-1/12` |
| Flameox | 真实 MCP 采集自有 Node 工作负载，重放同一证据；1 秒超时反例返回 `EXECUTION_TIMEOUT`，保留 stdout/stderr；该超时没有 CPU profile，两个工作负载均已退出 |
| SmokingGun | 静态候选及本轮真实计时导入；输入不符阻止 promotion；有效输入仍为 inconclusive，未授予优化采用 |
| electronics-design | 固定上游与公开 runtime diff 重建 CLI/连接器，219 项连接器测试及离线正反例通过；rc.7 仅更新交接说明 |
| 四个 MCP | 新 Codex 宿主实际加载并调用 REA、GitContribute、Jacobian、Flameox；不是仅检查配置文件或启动命令 |
| rc.7 接续与记忆 | 42 项公开绑定测试、14 次 memory CLI 示例调用及 1 项原生 receiver 专项测试通过；新 Codex 会话实际恢复外置 ProjectStore，保留两条请求全文、已确认纠正和 open 验收，未自动重派发 |
| 同一 Project 回流 | 先前五项结果重新核验，新增原生分析、采集与数学结果经 verify/register、决定、反馈进入同一合成项目至 r19；随后新建 memory session，完整 handoff 回读最新反馈 |

数学与限定原生案例的决定为 `supported`，Flameox 决定保留 `unknown`，全部 `adopted=false`。最终 route 为 `anchored`，合成项目的 `A-installation` 仍 open；这不妨碍记录已经执行的安装检查，也不能据此虚构总体验收关闭。这里的新 memory session 是显式 CLI 会话，另有真实新 Codex 会话执行验收；未把 memory session 创建说成另启一次 Codex 或自动 hook 验收。

六个上游工具的版本、16 个 Skill 文件和电子运行补丁在 rc.6→rc.7 之间未变。表内限定工具结果保留其实际执行版本；核心程序与接续代码变化后另行补验，不能把旧源码的检查直接转授新版本。

## 安装中发现的修正

锁定的 `z3-solver==5.1.0.0` 官方 macOS arm64 wheel 文件名与内部平台标签不一致，使 `pip check` 失败。公开提供保守的 macOS 14+ 元数据重打包：仅 WHEEL 与 RECORD 变化，其他 40 个成员原字节保留，带本地 build 标识。实际依赖检查、安装文件 RECORD 和原生求解通过；原始失败不删除。复现见 [Z3-MACOS.md](../integrations/runtime-locks/Z3-MACOS.md)。

Ghidra 12.1.4 官方 ZIP 缺 macOS arm64 decompiler。实际从包内 348 个未修改源码文件构建 arm64 原生组件，核独立产物摘要后安装；使用 JDK 21.0.9+10 完成限定原生反编译。补建组件标为本机派生产物，不冒充发行包自带文件。公开步骤见 [GHIDRA-MACOS.md](../integrations/runtime-locks/GHIDRA-MACOS.md)。

新 Codex 会话内已启动的 REA MCP 保留了补建前的缺组件诊断，其原生打开失败原件保留；当时原生反编译由新 CLI 进程完成。该会话正常结束后，主控又以相同公开启动器和新根配置启动独立 stdio MCP 进程，实际 `initialize → open_binary → batch_decompile(_affine) → close_binary`，1/1 函数成功且匹配 `param_1 * 3 + 7`，约 29.6 秒结束、退出码 0、无残留进程组。该项是直接 MCP 协议补验，不冒称另一次 Codex 宿主验收。因此文档要求先完成原生组件准备，再启动 MCP；不因补建而重启其他会话。

第一轮验证的四 MCP 名称被测试启动器错误地加上字面引号，因而未加载；后续修正的是测试启动参数。另一个过宽的测试条件误拦了上游工具的内部子进程隔离。后续保持 Codex 宿主与顶层工具环境身份，允许 REA 的私有 Ghidra 工作目录和 Flameox 原有的精简 workload 环境，并核任务归属、预算、证据和清退；未修改上游实现或跳过功能检查。

## 保留的边界

本次验证的是公开安装、有限工具案例与项目接续。所有合成工程案例保留 `adopted=false`；`no_delta` 反馈不证明经验产生收益。未验证原生 EDA 操作、电气与实物制造、NX/SW 工程工作负载、所有专业后端、Windows/Linux 新机、自动压缩 hook 或真实 RRSI 方法收益。

部分早期帮助探查仅保留在会话记录，没有完整本地原始日志；不声称每次诊断输出均完整归档。Skill 扫描曾出现遍历上限警告，但所需入口均由实际会话发现；不把这解释成所有潜在嵌套 Skill 都被扫描。默认日常宿主配置与本次独立验收配置分别管理。

Flameox 的 RSS 限额由上游轮询采样，不证明瞬时峰值硬上限；实际采集在声明预算内完成，未声称存在额外的外层 MCP watchdog。独立 Codex 会话的 MCP 可见文本完整保留，但 Agent 不持有该宿主原始 stdio 帧与全部服务端 stderr；随后直接 MCP 补验单独保留了自身的原始协议流。错误符号名、时间格式、初次 Jacobian 时间取值及 route 声明遗漏均保留原记录，后续实际重做受影响调用与交接。

独立 Codex 会话正常结束后，主控逐个核对该宿主及四个 stdio MCP、代码执行宿主均已退出；未向其他会话进程发送信号。

旧树、原始失败、测试输入与回执继续保留。书籍、米思米原件及资料清单仍通过 README 的 Google Drive 链接交付，不随软件测试进入 GitHub；架构图保持用户提供的原图。
