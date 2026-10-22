# 安装外部程序与本机配置

本仓库已包含内部 GUIDE 和必要脚本/参考资料，以及已应用的最小接入差异。对外仅加载本目录 [SKILL.md](SKILL.md)，无需重新安装五个独立 Skill。上游 CLI、连接器和 EDA 应用由用户自行安装；原六专业工具不受本模块配置影响。

## 根与资源目录

```sh
export ONTOLOGY_ENGINEERING_ROOT="$HOME/OntologyEngineering"
export ELECTRONICS_MODULE_ROOT="$ONTOLOGY_ENGINEERING_ROOT/modules/electronics-design"
export EASYEDA_SKILLS_DIR="$ELECTRONICS_MODULE_ROOT/modules"
```

第一行改为真实 checkout 根。`EASYEDA_SKILLS_DIR` 是 easyeda 查辅助脚本的目录，不是旧 `~/.codex/skills`。本机 Project、记忆、缓存、下载和输出写入 `var/`；不要将其加入公开模块。

## EasyEDA：指南与运行程序分别锁定

安装当前本地修复版应使用下列 **0.23.0-local.4 源码补丁**。内部指南来自实际 Skill 的
v0.25.1（commit `c57f58fd42a5732d91bb42a3ec522e2a199cc18a`），运行程序来自
v0.23.0（commit `f1aab8d2b03ab6a0bd78ce83e7961d838f4d850b`）加本地修复；两者身份不能合并。
指南所列新命令或参数不保证在该运行版本存在，先核实际 `actions` 与子命令帮助，缺项
保持未知，不自动换成最新版。运行程序与连接器必须由同一补丁源码成套构建。

本地补丁保留真实的属性保留、wire 解码/批量移动预检查、导入确认和失败诊断、原始导出、
PCB 属性读取、手动连接/排空和有限 daemon 生命周期。逐项代码与测试映射见
[运行修复](references/runtime-differences.md)。补丁是独立上游差异，完整第三方程序由用户
从官方固定提交取得；没有把旧本机二进制或私人 Project 放进仓库。

需要 Git、Python 3、Go（能取得 `go1.26.3` 工具链）和 Node.js >=20.17/npm。
在未存在的新目录构建，命令会联网取得固定上游源码与锁定依赖，执行离线测试，产出
CLI、连接器与本次字节摘要，不启动 EDA、服务或自动导入连接器：

```sh
export ELECTRONICS_RUNTIME="$ONTOLOGY_ENGINEERING_ROOT/var/tools/easyeda-agent-0.23.0-local.4"
/bin/sh "$ELECTRONICS_MODULE_ROOT/scripts/build-runtime.sh" "$ELECTRONICS_RUNTIME"
export EASYEDA_BIN="$ELECTRONICS_RUNTIME/bin/easyeda"
export PATH="$ELECTRONICS_RUNTIME/bin:$PATH"
"$EASYEDA_BIN" --version
```

需要检查 `BUILD.json`、`logs/`、`actions.json` 与 `blocks.json`；任一步失败不写成功回执。
`connector.eext` 是同次构建产物，内容来自同一 commit+patch，压缩包时间戳可能随构建改变，
应核本次 entry SHA 与源码身份，不把旧包 SHA 当新包预期。上游声明的宿主兼容范围为
EasyEDA `~3.2.0`；实际宿主、菜单、API、读写结果仍须在目标应用中验证。

只有进入已授权 EDA 任务才按应用安装步骤导入本次连接器。核实际加载版本与包的来源，
不能由磁盘包存在推定已加载；Connect、health、目标窗口/文档只读响应也分别核查。
版本未知或与本次 CLI 不同就停止依赖该修复的写操作。不要调用上游 `install.sh`、
`make dev`、`make eext` 或自动 Skill 同步，它们会改全局安装/版本或启动服务。

本补丁的安装/activate 不连接，Connect 是显式且幂等的；Force Reconnect 单独暴露，
pending SDK 动作会阻止新连接，Stop 不取消或回滚已接受动作。daemon 默认拒绝占用端口，
任务启动仍须核归属，显式使用 `--replace-existing=false --auto-update-skill=false
--autosave-debounce=0 --pid-file <任务目录>/daemon.pid --max-runtime <本次期限>`，不静默替换
其他会话。具体启动/清退由已授权任务管理，不设自启、重试恢复或长期服务。

[runtime-assets.json](runtime-assets.json) 仅保留 **纯上游0.25.1对照资产** 的官方URL和摘要。
这些二进制没有本项目 local.4 修复，不能作为本模块的等价安装或验收。

## easyeda-api 1.1.3

此官方 API 模块是按需文档/调试入口，不能绕过 easyeda-agent typed action 与阶段要求。Node.js 18+ 是该固定版本声明的最低依赖，实际 Node 版本另记。为保持公开模块干净，先创建私有运行副本：

```sh
export ELECTRONICS_API_DIR="$ONTOLOGY_ENGINEERING_ROOT/var/tools/easyeda-api-1.1.3"
mkdir -p "$ONTOLOGY_ENGINEERING_ROOT/var/tools"
mkdir "$ELECTRONICS_API_DIR"
cp "$ELECTRONICS_MODULE_ROOT/modules/easyeda-api/package.json" "$ELECTRONICS_API_DIR/"
cp "$ELECTRONICS_MODULE_ROOT/modules/easyeda-api/package-lock.json" "$ELECTRONICS_API_DIR/"
cp -R "$ELECTRONICS_MODULE_ROOT/modules/easyeda-api/scripts" "$ELECTRONICS_API_DIR/"
(cd "$ELECTRONICS_API_DIR" && npm ci --ignore-scripts)
node --check "$ELECTRONICS_API_DIR/scripts/bridge-server.mjs"
```

文档始终从公开模块读取，运行依赖和日志只在私有副本内。上述命令不启动 API server。进入已授权 raw API 任务时按 [GUIDE](modules/easyeda-api/GUIDE.md) 核当前端口、目标窗口和 [官方 gateway](https://jlc-ext.com/item/oshwhub/run-api-gateway)，保存服务归属与清退记录。该 gateway 与 easyeda-agent connector 是不同入口，不以安装其中一个冒称另一个可用。

## 其余内部模块与验证

fanout 为工作指南，依赖本次可用的 typed 操作或已授权 API 路径。3D 辅助使用 Python 3 标准库；网络提取需当前精确器件身份、公开来源和下载权限，不读 Cookie 或账号存储。公开端点可变，本次迁移未验证网络下载可用性；失败保留实际状态，不能猜测私有接口替代。OBJ/STEP 文件包检查不是模型几何/封装对齐验收。

工人接线卡使用 XeLaTeX/CTeX 与所需字体，由新用户单独配置。模板含明显未填提示；必须先有实物证据、冻结连接矩阵和独立审阅，不能直接打印给工人执行。ngspice、原生 CAD SDK 等只按任务另装，未安装不写成仿真或 CAD 通过。

执行 [README 的最小离线验收](README.md#最小离线验收) 后，将实际程序/版本/摘要和检查范围登记到本机能力 registry。已有模块资源不等于所有外部工具已装；重新安装后必须新运行，不使用开发者的旧回执。
