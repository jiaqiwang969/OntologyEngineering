# 专业模块与运行工具接入

工程本体、memory 和专业工具通过同一 Project 协作。工程本体管理当前对象、条件、
证据和决定；memory 提供带来源的历史候选；Agent 依据任务调用专业工具，把实际结果
交回 Project 审查。安装成功、工具执行成功和工程采用分别记录。

本仓库接入七种专业能力，七份 Skill 入口及其引用资源统一在 `modules/` 维护，
保留各自的名称、自动发现元数据和完整规则。原六个工具的运行程序由用户从上游安装；
电子设计的五个内部模块随仓库维护，EDA、连接器和求解器按需另装。全部按任务选用，
不要求每个项目安装或调用全部七项。核心 Project 和 memory 流程不依赖它们全部可用。
这些接入材料不要求额外的托管判断服务。

## 固定上游与用途

以下是本次集成所核对的版本，不代表各上游的最新版本。提交、发行包来源、许可证、
补丁前后摘要见 [tools.lock.json](../integrations/tools.lock.json)。

| 工具 | 固定版本与上游 | 提供的依据 | 我们维护的差异 |
|---|---|---|---|
| [GitContribute](../modules/gitcontribute/SKILL.md) | [3.0.2](https://github.com/morluto/gitcontribute/tree/v3.0.2) | 仓库规范、相关工作、issue/PR、覆盖和新鲜度 | Skill、LOCAL.md、UPSTREAM.md 的完整本地 diff |
| LeanToken | [0.1.28](https://github.com/morluto/leantoken/tree/v0.1.28) | 已选仓库中的源码范围、符号和有界上下文 | [本项目维护的完整 Skill](../modules/leantoken/SKILL.md) |
| [REA](../modules/reverse-engineer-anything/SKILL.md) | [6.5.0](https://github.com/morluto/rea/tree/rea-agents-6.5.0) | 发布制品、静态结构及按需授权的运行观察 | npm 发行 Skill diff、保留六份上游参考文件及可移植启动器 |
| [Jacobian](../modules/jacobian-math/SKILL.md) | [0.23.0](https://github.com/morluto/jacobian/tree/jacobian-v0.23.0) | 按精确合同执行的计算和数学关系 | 上游 Skill 的完整本地 diff |
| [SmokingGun](../modules/smokinggun/SKILL.md) | [5.0.0](https://github.com/morluto/smokinggun/tree/v5.0.0) | 静态复杂度候选、导入的性能证据 | Skill、LOCAL.md、UPSTREAM.md 的完整本地 diff |
| Flameox | [0.2.9](https://github.com/morluto/flameox/tree/v0.2.9) | 显式运行原件的分析及有界工作负载采集 | [本项目维护的完整 Skill](../modules/flameox/SKILL.md) |
| electronics-design | [2.1.4 模块及固定来源](../modules/electronics-design/SOURCES.json) | 电子设计、原理图/PCB、模型和接线证据 | [统一 Skill 与五个内部模块](../modules/electronics-design/README.md)，保留上游来源和必要 diff |

原六项的第三方算法与固定官方发行一致。本仓库直接提供实际使用的完整 Skill 文件、引用资源、
REA 启动环境选择和三个共享接续脚本；完整对应关系见[源码覆盖表](../integrations/COVERAGE.md)。
六项的 `skill_directory` 和 16 份文件摘要均在 tools.lock.json；LeanToken 和 Flameox
采用本项目维护的完整 Skill，原四份 diff 仍保留用于重建核验。新用户无需维护者的旧工作目录；
上游模板、实际发布 Skill、本地修改和本次安装的验证各自保留身份。

## 安装运行程序

先按根目录 [AGENTS.md](../AGENTS.md) 配置 `ONTOLOGY_ENGINEERING_ROOT`，指向本仓库
的绝对路径。CLI 入口为 `$ONTOLOGY_ENGINEERING_ROOT/var/install/bin/oev2`。
以下安装只选择需要的工具；已有合适版本时先核身份，不重复安装。本文命令使用 POSIX
shell；本次小案例运行于 macOS arm64，不据此声称 Windows 或其他平台已经验收。

### Node 工具

使用满足各包要求的 Node.js/npm。四个 npm 包共同可用的 Node 24.x 范围为
24.11.0 及以上；REA 的部分能力另有更高要求，例如 SQLite 检查要求 24.15 及以上。
具体引擎范围以锁定版本的上游说明和包元数据为准。

```sh
npm install --global gitcontribute@3.0.2
npm install --global leantoken@0.1.28
npm install --global rea-agents@6.5.0
npm install --global smokinggun@5.0.0
```

以上只固定顶层版本。要复现本次现用依赖，REA 与 SmokingGun 应分别将
[`runtime-locks/`](../integrations/runtime-locks/README.md) 中对应的 `package.json` 和
`package-lock.json` 复制到新建私有工具目录，再执行 `npm ci`，使用该目录
`node_modules/.bin` 的程序；不要同时依赖另一份全局安装。快照摘要在 tools.lock.json。
REA 的 Node、包目录及可选 Ghidra/JDK 配置见[启动环境](../integrations/bindings/WORKFLOW.md#rea-启动环境)。

macOS arm64 的 REA 原生分析还需准备 Ghidra 对应平台的 decompiler；本次固定官方包、原版源码构建参数与核验方法见 [Ghidra macOS 安装补充](../integrations/runtime-locks/GHIDRA-MACOS.md)。

以上是供用户选择执行的安装命令；本仓库不会因工具缺失而自动执行它们。安装后核对：

```sh
gitcontribute metadata --json
leantoken --version
rea --version
smokinggun --version
```

`gitcontribute` 的版本来自 `metadata`；3.0.2 不支持 `--version`。
CLI 存在不代表当前 Agent 已连接 MCP。实际执行前核可执行文件路径与安装记录；已有全局 wrapper 或软链接可能仍指向旧环境。发生这种情况时，用本次已核程序的绝对路径调用，并显式选择 Node 等解释器；版本字符串相同不能代替来源身份。

### Python 工具

Jacobian 0.23.0 要求 Python 3.12 或 3.13；Flameox 0.2.9 要求 Python 3.12 及以上。
使用两个独立环境，避免它们各自固定的 MCP SDK 依赖冲突。下面以已有 Python 3.12
为例，工具环境路径由用户选择，勿指向已有其他应用环境。

```sh
export ONTOLOGY_TOOL_ENVS="$ONTOLOGY_ENGINEERING_ROOT/var/tool-envs"
python3.12 -m venv "$ONTOLOGY_TOOL_ENVS/jacobian-0.23.0"
"$ONTOLOGY_TOOL_ENVS/jacobian-0.23.0/bin/python" -m pip install -r "$ONTOLOGY_ENGINEERING_ROOT/integrations/runtime-locks/jacobian-requirements.txt"
python3.12 -m venv "$ONTOLOGY_TOOL_ENVS/flameox-0.2.9"
"$ONTOLOGY_TOOL_ENVS/flameox-0.2.9/bin/python" -m pip install -r "$ONTOLOGY_ENGINEERING_ROOT/integrations/runtime-locks/flameox-requirements.txt"
```

macOS arm64 用户先按 [Z3 平台元数据修正](../integrations/runtime-locks/Z3-MACOS.md)核验并处理锁定 wheel 的问题；不得跳过失败的依赖检查。两个环境均执行 `python -m pip check`，保留实际结果。

快照来自 macOS arm64 / Python 3.12.13，含现用可选数学后端。其他平台需核上游支持并
记录重新解析的差异，不能将不同依赖写成同一环境。核版本并保留实际安装清单：

```sh
"$ONTOLOGY_TOOL_ENVS/jacobian-0.23.0/bin/python" -c 'import importlib.metadata; print(importlib.metadata.version("jacobian"))'
"$ONTOLOGY_TOOL_ENVS/flameox-0.2.9/bin/flameox" --version
"$ONTOLOGY_TOOL_ENVS/jacobian-0.23.0/bin/python" -m pip freeze > "$ONTOLOGY_TOOL_ENVS/jacobian-0.23.0/requirements-installed.txt"
"$ONTOLOGY_TOOL_ENVS/flameox-0.2.9/bin/python" -m pip freeze > "$ONTOLOGY_TOOL_ENVS/flameox-0.2.9/requirements-installed.txt"
```

不需要所有可选数学后端、原生反编译器或性能采集器才能运行下述小案例。实际任务需要
额外后端时，再按固定上游说明安装并验证相应能力。

## Agent 接入与 Skill

### 第七项：电子设计

本仓库的 [electronics-design/SKILL.md](../modules/electronics-design/SKILL.md) 是电子任务的
唯一入口，内部按需读取以下五个 `GUIDE.md`，无需把它们分别装成同级 Skill：

```text
modules/electronics-design/
├── SKILL.md
└── modules/
    ├── easyeda-agent/
    ├── easyeda-api/
    ├── easyeda-schematic-net-fanout/
    ├── lceda-jlc-3d-model-extraction/
    └── make-worker-wiring-card/
```

按该模块的 [README](../modules/electronics-design/README.md) 安装所需运行程序，核固定来源、
许可及接入差异。使用 EasyEDA CLI 辅助资源时设置：

```sh
export EASYEDA_SKILLS_DIR="$ONTOLOGY_ENGINEERING_ROOT/modules/electronics-design/modules"
```

项目与原生结果写入私有目录，仍经下述 `project tool-result` 和 memory 接续。模块文件可读、
离线脚本通过、连接器连通、EDA 操作与实物电气检查各自记录；当前范围见模块验收说明。

### 原六项工具：模块与 MCP 分开配置

六个 Skill 已在仓库内，运行程序安装后无需再执行会写入宿主顶层 Skill 的上游 setup。
保留 `modules/` 下的真实目录及其发现元数据；由根入口导航并按宿主能力正常发现。
不要向 `~/.codex/skills/`、`~/.agents/skills/` 或其他宿主目录另复制同名 Skill，
也不要把缓存或临时上游重建目录注册为额外搜索根。

在仓库根只读核对六项完整的 16 份文件；需要检查宿主重复入口时，逐个给出实际搜索根：

```sh
python3 scripts/check_tool_skills.py
python3 scripts/check_tool_skills.py --skill-root "$HOME/.codex/skills" --skill-root "$HOME/.agents/skills"
```

检查宿主时，从 Agent 实际加载的安装根执行；开发 checkout 的路径不能代替实际安装根。`--skill-root` 可重复。检查报告不自动搬动或删除文件；重复入口需先核身份与使用者，
保全旧内容后再从发现范围移出本次替换的目录。fleet 继续独立维护。

CLI 与 MCP 按任务选择。MCP 配置只指向自己机器已核的运行程序，和 Skill 位置分开：

| 名称 | command | args |
|---|---|---|
| GitContribute | `/absolute/path/to/gitcontribute` | `["mcp", "serve", "--transport=stdio"]` |
| REA | `/absolute/node-24/bin/node` | `["/absolute/ontology-engineering-unified/integrations/bindings/rea-launcher.cjs", "mcp"]` |
| Jacobian | `<tool-envs>/jacobian-0.23.0/bin/jacobian-mcp` | `[]` |
| Flameox | `<tool-envs>/flameox-0.2.9/bin/flameox` | `["mcp", "serve"]` |

本接入中的 LeanToken 和 SmokingGun 使用 CLI。表中所有占位路径必须换成真实绝对路径，
不能假设客户端会展开 shell 变量。REA 的包根、Node、Ghidra/JDK 和 env 选择见
[启动环境](../integrations/bindings/WORKFLOW.md#rea-启动环境)；其他工具按其已核程序说明
设置必要的环境变量与自己的私有数据目录。GitContribute 的 token-source `none` 适合本地
语料或匿名公开访问，需要认证时由用户选择上游支持的凭据来源，不写入仓库。

配置前检查实际客户端、已有条目和会话使用范围；只改当前授权的配置，不重启或改写
其他会话的连接。若使用上游配置助手，先核锁定版本的 `--help` 和预览，只有明确支持
MCP-only、不会写 Skill 的方式才适用于本布局，不能沿用会安装 Skill 的 setup/upgrade。
在新会话实际发现并调用一次工具后才记录 MCP 连接可用；配置文件存在不代表连接成功。

### 在干净上游副本重建 diff

日常安装直接使用 `modules/` 的完整文件，**不要对已整合模块重复打补丁**。
四份 `integrations/patches/` 用于说明并验证相对固定上游的差异，完整文件摘要由
`tools.lock.json` 的 `skill_files` 约束，模块根由 `skill_directory` 指定。

重建时先取得锁中固定上游发行的完整 Skill 和引用资源，放到仓库外新建的私有目录；
该目录不加入宿主 Skill 搜索范围。GitContribute/SmokingGun/Jacobian 按锁中源位置取文件，
REA 必须取 **npm 6.5.0 发行包内的 Skill**：仓库模板在打包时会填入版本号与工具数量，
不能代替该发行原件。LeanToken/Flameox 没有这类上游补丁，其完整维护源直接在
`modules/leantoken/` 和 `modules/flameox/`。

`tools.lock.json` 给出原始文件、补丁和应用后 SHA256。先核补丁自身与 `patch_targets`
的每项原始摘要（新增目标应不存在），再在干净 Skill 根应用。以下以 GitContribute 为例，
`ONTOLOGY_SKILL_REBUILD_DIR` 必须指向上述干净上游副本，不能指向 `modules/` 或实际宿主入口：

```sh
export ONTOLOGY_SKILL_REBUILD_DIR='/absolute/private-clean-upstream/gitcontribute'
shasum -a 256 "$ONTOLOGY_SKILL_REBUILD_DIR/SKILL.md"
shasum -a 256 "$ONTOLOGY_ENGINEERING_ROOT/integrations/patches/gitcontribute-3.0.2-ontology.patch"
git -C "$ONTOLOGY_SKILL_REBUILD_DIR" apply --check "$ONTOLOGY_ENGINEERING_ROOT/integrations/patches/gitcontribute-3.0.2-ontology.patch"
git -C "$ONTOLOGY_SKILL_REBUILD_DIR" apply "$ONTOLOGY_ENGINEERING_ROOT/integrations/patches/gitcontribute-3.0.2-ontology.patch"
shasum -a 256 "$ONTOLOGY_SKILL_REBUILD_DIR/SKILL.md"
```

Linux 可用 `sha256sum`。应用根是干净 Skill 目录，不是第三方源码根。上面的命令只演示
主文件；仍须按 `patch_targets` 核全部目标，再按 `skill_files` 核全部结果字节，并与
`skill_directory` 内的完整模块比较。GitContribute/SmokingGun 包括 `LOCAL.md`、
`UPSTREAM.md`，REA 包括六份上游 references；仅比 SKILL.md 不代表完整 Skill 已复现。

原始摘要不匹配时先查确切版本和已有改动，不强行套用。已匹配应用后摘要时不重复打补丁。
回退只针对这个重建副本，先 `git apply --reverse --check`，再 `git apply --reverse`，
最后核回原始摘要；不要用回退命令覆盖仓库模块。上游升级时重新重建、核依赖与案例，
再更新本仓库模块和锁；同时检查宿主是否出现 setup/upgrade 生成的同名入口。

### 共享接续代码

六项共用本仓库 [bindings/WORKFLOW.md](../integrations/bindings/WORKFLOW.md)。
`verify_candidate.py` 检查声明性 Project 修订，`reconcile_reviewed.py` 绑定明确的当前
Project 并监督原生复核，`resume_memory.py` 保留准确上下文的任务、查询和预算继续准备。
它们只从 `ONTOLOGY_ENGINEERING_ROOT` 或 `--root` 选定的新安装读取程序，核安装清单及
实际字节，原生 Rust 继续执行工程语义。按该流程运行包装器和 REA 启动器测试，再核工具案例。

## 同一 Project 的结果交接

具体原生命令及数据合同见
[专业能力与结果交接](../references/professional-capabilities.md)。统一入口是：

1. 调用前取得 `oev2 project context`，明确业务对象、修订、条件、待决定事项和验收项。
2. 固定被分析对象的原件与摘要，记录工具实际版本及程序/发行包身份。包装脚本的摘要
   不能替代它调用的真实程序、Node 包或 Python 环境身份。
3. 调用后保存原生结果、完整/部分/失败状态、输入、覆盖和未知项。写入已有
   `oev2.tool-result/1` 合同，经 `oev2 project tool-result verify/register` 接收；
   登记证据与更新决定通过 CLI，不另造一个脱离当前任务的“工具 Project”。
4. 决定与工程采用仍走各自原生审查。静态线索、模型推断、导入测量和独立观测分开。
   同一原件经多个工具复述，不自动增加独立证据数量。
5. 通过 `oev2 memory` 保存过程与实际使用反馈；经验召回只形成待核候选，不能替代
   当前对象上的测试、工具原件或工程验收。

本机能力登记表位于私有 `var/capabilities/registry.json`，使用
`oev2.capability-registry/1`。在自己的机器完成身份与功能核验后再登记；不能复制
维护者旧机器的通过记录。配置、原始结果、索引与缓存都写入私有 `var/` 或用户指定目录。

## 每个工具的最小功能验收

使用本仓库的 [自有样例](../integrations/examples/)，把结果写到新建私有测试目录。
下面验收的是所列操作，不代表工具的所有平台、后端或完整工程能力均通过。

| 工具 | 实际操作 | 独立检查与边界 |
|---|---|---|
| GitContribute | 复制 `source-repo` 到私有目录，建立干净 Git 提交；用独立 corpus 执行 `index`，再 `search code joinLabels` | 结果包含 `src/lookup.js`；匹配实际提交。合成 `example/ontology-engineering-tool-fixture` 只是本地测试标签，不是已存在的远端仓库 |
| LeanToken | 显式 `--root` 与独立 `--database`，执行 `index`、`context`，再 `read src/lookup.js --policy full` | 返回源码逐字匹配，generation 和 live bytes 一并保留；没有执行业务测试 |
| REA | 将样例 `package.json` 与 `src/lookup.js` 复制为独立发布目录；运行 `analyze-javascript-application ... --format json` | 输出包含原件摘要和导出 `joinLabels`；这是静态关系，不证明函数已运行 |
| Jacobian | 先 `inspect matrix.rational_linear_system.solve`，再用 `linear-system.json` 执行 `run` | 应返回唯一精确解 `x=2, y=1`；独立代回 `2x+y=5` 与 `x-y=1` |
| SmokingGun | 对样例执行 `smokinggun scan . --format json` | 显式双循环产生候选，同时检查 coverage；部分语义覆盖必须保留，候选不代表性能改进 |
| Flameox | 编译运行 `runtime-samples.rs` 取得 8 个真实计时样本，执行 `analyze summarize_benchmarks ... --format samples` | 重读原件，逐个比较返回的 8 个整数计时值、单位和摘要。仅验证分析，未验证 Flameox capture 或优化效果 |

可复用的具体命令如下。`ONTOLOGY_TEST_DIR` 应由用户指向新的私有绝对目录；
`ONTOLOGY_TEST_REPO` 指向从样例复制并提交的独立 Git 仓库；两个 Python 工具使用
上文自己的环境绝对路径。目录应先创建好，所有输出都保留原始字节。

```sh
GITCONTRIBUTE_DATABASE="$ONTOLOGY_TEST_DIR/gitcontribute.db" GITCONTRIBUTE_TOKEN_SOURCE_METHOD=none gitcontribute init --json
GITCONTRIBUTE_DATABASE="$ONTOLOGY_TEST_DIR/gitcontribute.db" GITCONTRIBUTE_TOKEN_SOURCE_METHOD=none gitcontribute index example/ontology-engineering-tool-fixture "$ONTOLOGY_TEST_REPO" --json
GITCONTRIBUTE_DATABASE="$ONTOLOGY_TEST_DIR/gitcontribute.db" GITCONTRIBUTE_TOKEN_SOURCE_METHOD=none gitcontribute search code joinLabels --repo=example/ontology-engineering-tool-fixture --json
leantoken --root "$ONTOLOGY_TEST_REPO" --database "$ONTOLOGY_TEST_DIR/leantoken.db" --json index
leantoken --root "$ONTOLOGY_TEST_REPO" --database "$ONTOLOGY_TEST_DIR/leantoken.db" --json context --task 'Find joinLabels implementation' --budget 2200
leantoken --root "$ONTOLOGY_TEST_REPO" --database "$ONTOLOGY_TEST_DIR/leantoken.db" --json read src/lookup.js --policy full
rea analyze-javascript-application "$ONTOLOGY_TEST_DIR/package" --analysis-timeout-ms 20000 --max-heap-mb 256 --format json > "$ONTOLOGY_TEST_DIR/rea.json"
"$ONTOLOGY_TOOL_ENVS/jacobian-0.23.0/bin/jacobian" inspect matrix.rational_linear_system.solve
"$ONTOLOGY_TOOL_ENVS/jacobian-0.23.0/bin/jacobian" run matrix.rational_linear_system.solve --file "$ONTOLOGY_ENGINEERING_ROOT/integrations/examples/linear-system.json"
smokinggun scan "$ONTOLOGY_TEST_REPO" --format json > "$ONTOLOGY_TEST_DIR/smokinggun.json"
rustc --edition=2021 -O "$ONTOLOGY_ENGINEERING_ROOT/integrations/examples/runtime-samples.rs" -o "$ONTOLOGY_TEST_DIR/runtime-samples"
"$ONTOLOGY_TEST_DIR/runtime-samples" > "$ONTOLOGY_TEST_DIR/samples.json"
"$ONTOLOGY_TOOL_ENVS/flameox-0.2.9/bin/flameox" mcp inspect --tool summarize_benchmarks
"$ONTOLOGY_TOOL_ENVS/flameox-0.2.9/bin/flameox" analyze summarize_benchmarks "$ONTOLOGY_TEST_DIR/samples.json" --format samples > "$ONTOLOGY_TEST_DIR/flameox.json"
```

在仓库根调用 `rustc` 使用本仓库声明的工具链。`runtime-samples.rs` 内的断言检查
每次整数求和的结果；它只用作小型安装检查，没有基线/候选性能比较。

## 缺项、升级和重新安装

- 工具不存在：标记该能力 unavailable，给出本页对应安装入口；不把缺失当作零发现。
- MCP 不可见：区别宿主连接、运行程序和提供方。可用 CLI 时仍按真实入口执行，
  不声明 MCP 已连接；需要重连时由用户控制受影响的客户端。
- 补丁或版本不匹配：保留原文件，比较确切上游版本后重建差异，不默认使用 latest。
- 部分结果：回交 `partial` 与原因；原生结果和未知项都保留。
- 重新安装：保全旧 Skill 并从发现范围移出本次替换的入口后，从公开仓库取得完整模块，
  安装固定上游运行程序；diff 在独立干净副本重建核验。新会话重新核七模块发现、实际调用
  以及 Project/memory 完整案例；旧布局或旧机器的日志不能作为新布局通过。

2026-10-22 的接入准备已经对上述六项执行真实小案例，并对四份固定 Skill 补丁执行
`git apply --check`、实际应用和结果字节核对。该记录属于原安装上的接入准备，
不表示已完成 GitHub 发布后的干净安装验收。SmokingGun 的 JavaScript 结构分析可用，
TypeScript 项目语义范围仍是 partial；REA 只核了静态 JavaScript 发布样例；
Flameox 只核了已有计时原件分析。完整验收以新安装的当次回执为准。

此前的空 Skill 重装范围见[2026-10-22 验收记录](FRESH-INSTALL-20261022.md)。该记录保留原提交与布局身份；本次统一到 `modules/` 后的摘要、发现与调用须有对应的新记录，不继承此前通过。
