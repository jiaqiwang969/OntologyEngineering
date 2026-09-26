# 独立分发与新目录验证

当前版本 0.6.0，入口是根 [SKILL.md](../SKILL.md)；日常调用见 [使用说明](USAGE.md)，
变化和未验证范围见 [发布说明](releases/manufacturing-method-0.6.0.md)。普通包不附凭据。

## 分发内容

公开核心包按 `distribution/shareable-core-assets.json` 的逐文件白名单构建，包含工程方法、
源锁定 Semantica、通用 CAD/制造能力、NX 直连、米思米资料取得及可选 Jev 浏览器工具。
机械方法与历史教材保留，Fusion 执行 wheel 和适配器不再安装或分发。CAD 不默认使用 MCP。
真实项目、客户资料、浏览器账号/日志、私有历史记忆候选及书稿不进入核心包。

源仓库保留已有两卷书及其 [权利状态](PUBLIC-RELEASE-STATUS.md)。文件检查和源码标签
不是书籍权利放行，也不是物理工程验收。本次更新的代码和通用方法沿组件许可发布，
见 [组件声明](COMPONENT-NOTICE.md)。完整仓库快照与核心包分别核对，不能扩大公开资产范围。

## 构建与复验

```bash
python3 scripts/package_skill.py
python3 scripts/build_shareable_core.py --output /path/to/new-core.zip
bash runtime/setup_runtime.sh --preflight
bash runtime/setup_runtime.sh
bash runtime/setup_runtime.sh --doctor
bash skills/cad-agent/setup.sh
bash skills/cad-agent/doctor.sh --json
runtime/.venv/bin/python scripts/run_manufacturing_cases.py --run --output /path/to/new-work
```

核心构建仅接受已审文件和固定哈希；入口模板用 `SKILL.md.in` 保存，在 staging 恢复。
每个 ZIP 含 `PORTABLE-MANIFEST.json`。解压到新目录后重新运行 `scripts/package_skill.py`，
会检查文件增删、字节、链接、私密标识和冻结语义包。安装可能访问 Python 包源。
源锁、解释执行、原生工程验证与发布权限分别记录。

可选网页工具另建环境：

```bash
bash runtime/jev-ultrafast/setup.sh
python3 scripts/jev_browser.py doctor
```

它不启动浏览器或启用调试设置；接收方准备自己的 Jev 凭据、已授权浏览器连接和账号。
Apple Events 路径仅用于已有授权的 macOS Chrome 标签；完全后台冷启动、验证挑战和
账号过期恢复没有通用稳定性保证。用当前页面的真实选项与文件证据验收。

NX 软件、许可、NXOpen 环境和远程主机由接收方提供。按 [NX 执行](../skills/cad-agent/references/nx-execution.md)
配置私有 profile，在独立任务中保存并用新进程重开。doctor 就绪不代表原生或实物通过。

经验整理见 [复盘方法](../references/practice-consolidation.md)；它通过任务检查点运行，
没有隐式安装常驻任务。正式查询、约束及行业本体演化继续使用 Semantica。
