# 专业工具接入与独立安装

工程本体、memory 和专业工具通过同一 Project 协作。工程本体管理当前对象、条件、
证据和决定；memory 提供带来源的历史候选；Agent 依据任务调用专业工具，把实际结果
交回 Project 审查。安装成功、工具执行成功和工程采用分别记录。

本仓库接入七种专业能力。原六个工具的运行程序由用户从上游安装；第七项电子设计的
统一 Skill 与五个内部模块随仓库维护，EDA、连接器和求解器按需另装。全部按任务选用，
不要求每个项目安装或调用全部七项。核心 Project 和 memory 流程不依赖它们全部可用。
这些接入材料不要求额外的托管判断服务。

## 固定上游与用途

以下是本次集成所核对的版本，不代表各上游的最新版本。提交、发行包来源、许可证、
补丁前后摘要见 [tools.lock.json](../integrations/tools.lock.json)。

| 工具 | 固定版本与上游 | 提供的依据 | 我们维护的差异 |
|---|---|---|---|
| GitContribute | [3.0.2](https://github.com/morluto/gitcontribute/tree/v3.0.2) | 仓库规范、相关工作、issue/PR、覆盖和新鲜度 | Skill、LOCAL.md、UPSTREAM.md 的完整本地 diff |
| LeanToken | [0.1.28](https://github.com/morluto/leantoken/tree/v0.1.28) | 已选仓库中的源码范围、符号和有界上下文 | [本项目维护的完整 Skill](../integrations/companions/leantoken/SKILL.md) |
| REA | [6.5.0](https://github.com/morluto/rea/tree/rea-agents-6.5.0) | 发布制品、静态结构及按需授权的运行观察 | npm 发行 Skill diff、保留六份上游参考文件及可移植启动器 |
| Jacobian | [0.23.0](https://github.com/morluto/jacobian/tree/jacobian-v0.23.0) | 按精确合同执行的计算和数学关系 | 上游 Skill 的完整本地 diff |
| SmokingGun | [5.0.0](https://github.com/morluto/smokinggun/tree/v5.0.0) | 静态复杂度候选、导入的性能证据 | Skill、LOCAL.md、UPSTREAM.md 的完整本地 diff |
| Flameox | [0.2.9](https://github.com/morluto/flameox/tree/v0.2.9) | 显式运行原件的分析及有界工作负载采集 | [本项目维护的完整 Skill](../integrations/companions/flameox/SKILL.md) |
| electronics-design | [2.1.4 模块及固定来源](../modules/electronics-design/SOURCES.json) | 电子设计、原理图/PCB、模型和接线证据 | [统一 Skill 与五个内部模块](../modules/electronics-design/README.md)，保留上游来源和必要 diff |

原六项的第三方算法与固定官方发行一致。本仓库保留实际使用的 Skill 差异、引用资源、
REA 启动环境选择和三个共享接续脚本；完整对应关系见[源码覆盖表](../integrations/COVERAGE.md)。
LeanToken 和 Flameox 采用本项目维护的完整 Skill。新用户无需维护者的旧工作目录；
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
CLI 存在不代表当前 Agent 已连接 MCP。

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

### 原六项工具

用户可以选 CLI，也可以按上游方式配置 MCP。新用户先预览目标客户端的变更，再应用
同一范围。下面以 Codex 为例；只配置实际使用的客户端。

```sh
gitcontribute setup --mode both --codex --token-source none --dry-run --json
leantoken setup --codex --dry-run
rea setup --client codex --dry-run --json
```

确认预览后，分别把 `--dry-run` 换成 `--yes`。GitContribute 的 `none` 适合本地语料
或匿名公开访问；需要认证时由用户选择上游支持的凭据来源。不要把凭据写进仓库。
Claude Code 对应 GitContribute/LeanToken 的 `--claude` 和 REA 的
`--client claude_code`。不要配置所有客户端来替代实际选择。

Jacobian/Flameox 可在客户端的本地 MCP 配置中指向自己环境中的绝对路径：

| 名称 | command | args |
|---|---|---|
| Jacobian | `<tool-envs>/jacobian-0.23.0/bin/jacobian-mcp` | `[]` |
| Flameox | `<tool-envs>/flameox-0.2.9/bin/flameox` | `["mcp", "serve"]` |

配置中的 `<tool-envs>` 必须替换成真实绝对路径，不能假设客户端会展开 shell 变量。
必要时在配置的 env 中传入 `ONTOLOGY_ENGINEERING_ROOT` 和用户自己的私有数据目录。
客户端重连后实际发现并调用一次工具，才记录宿主连接可用。SmokingGun 本接入使用 CLI。

GitContribute/REA 的上游 setup 会安装匹配的 Skill。SmokingGun 和 Jacobian 的固定
上游 Skill 可从下列源码位置取得；保留完整 Skill 目录和所引用资源：

- [SmokingGun 5.0.0 的 skills/smokinggun](https://github.com/morluto/smokinggun/tree/b416059734f69d3ead4fe963c009e2e29674ff56/skills/smokinggun)
- [Jacobian 0.23.0 的 npm/skill/jacobian-math](https://github.com/morluto/jacobian/tree/bad9b899715992bd2a72ed79e726a7deb89e9395/npm/skill/jacobian-math)

将所需目录放入实际客户端支持的 Skill 安装位置，避免同时安装同名的多个副本。
LeanToken 和 Flameox 分别安装本仓库 `integrations/companions/leantoken/`、
`integrations/companions/flameox/` 的完整 Skill 目录。若 LeanToken setup 已生成同名上游
Skill，先保全它，再用这里的完整版本替换；不能同时发现两份。这两份明确由本项目维护。

### 应用最小 diff

在纯上游 Skill 上应用 `integrations/patches/` 的对应补丁。
`tools.lock.json` 给出原始 Skill SHA256、补丁 SHA256 和应用后 SHA256，三者都要核对。
示例中的 `ONTOLOGY_SKILL_DIR` 指向一个已安装 Skill 的目录，内含 `SKILL.md`：

```sh
export ONTOLOGY_SKILL_DIR='/absolute/path/to/installed/gitcontribute'
shasum -a 256 "$ONTOLOGY_SKILL_DIR/SKILL.md"
git -C "$ONTOLOGY_SKILL_DIR" apply --check "$ONTOLOGY_ENGINEERING_ROOT/integrations/patches/gitcontribute-3.0.2-ontology.patch"
git -C "$ONTOLOGY_SKILL_DIR" apply "$ONTOLOGY_ENGINEERING_ROOT/integrations/patches/gitcontribute-3.0.2-ontology.patch"
shasum -a 256 "$ONTOLOGY_SKILL_DIR/SKILL.md"
```

Linux 可用 `sha256sum`。应用根是已安装 Skill 目录，不是第三方源码根。
按 tools.lock.json 的 `patch_targets` 核所有目标，再按 `skill_files` 核全部结果文件：
GitContribute/SmokingGun 包括 `LOCAL.md`、`UPSTREAM.md`；REA 还须保留完整六份上游
references。不能只比较 SKILL.md 就声称整个 Skill 已复现。
原始摘要不匹配时，先辨明版本或既有修改；不要跳过摘要检查、强行应用或覆盖自己的改动。
已匹配“应用后摘要”时不重复打补丁。应用前留存原版，回退可对该目录使用
`git apply --reverse --check` 和 `git apply --reverse`，然后重核原始摘要。

REA 使用 **npm 6.5.0 发行包内的 Skill** 作为基线；上游仓库模板会在打包时填入
版本号和工具数量，直接对仓库模板应用不能冒充对已安装 Skill 的验收。
上游 setup/upgrade 可能替换 Skill，升级后必须重新核版本、差异和功能。

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
- 重新安装：移除旧 Skill 的发现入口后，在新会话中重装上游、应用对应 diff，并重新跑
  上述实际操作以及 Project/memory 完整案例；旧机的日志不能作为新安装通过。

2026-10-22 的接入准备已经对上述六项执行真实小案例，并对四份固定 Skill 补丁执行
`git apply --check`、实际应用和结果字节核对。该记录属于原安装上的接入准备，
不表示已完成 GitHub 发布后的干净安装验收。SmokingGun 的 JavaScript 结构分析可用，
TypeScript 项目语义范围仍是 partial；REA 只核了静态 JavaScript 发布样例；
Flameox 只核了已有计时原件分析。完整验收以新安装的当次回执为准。
