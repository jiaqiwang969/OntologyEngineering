# OntologyEngineering

**工程本体 v2 × engineering-memory × 专业工具：让工程任务、证据、决定与经验形成闭环。**

用户用自然语言提出目标、条件和取舍。Codex、Claude 等 Agent 组织任务；工程本体在同一 Project 中维护对象、版本、来源、检查与决定；工程记忆保存过程、失败、纠正和可核查的经验。专业工具按需提供调查、计算或运行证据，结果回到同一项目审查。

当前为 **2.0.0-rc.7 整合候选，重构中**。本版增加[项目文件体系](docs/PROJECT-WORKSPACE.md)，将根请求、纠正、因果依据、阶段状态及 RRSI 根目标绑定保存到显式项目目录，以供新会话恢复。实际验证见[验证记录](docs/UNIFIED-VERIFICATION.md)，具体工程能力与未完成项见[发布范围](docs/RELEASE-SCOPE.md)。

![工程本体 v2、engineering-memory 与六种专业能力的闭环架构](docs/assets/engineering-system-architecture.png)

原图保留 2026-10-14 的状态标注；当前安装方式与验证进展以下文及[验证记录](docs/UNIFIED-VERIFICATION.md)为准。

## 系统组成

| 部分 | 职责 | 安装方式 |
|---|---|---|
| 工程本体 v2 | Project、工程检查、语义执行、生成合同、版本与采用 | 本仓库统一构建 |
| engineering-memory | 原文来源、时间线、经验修订、召回、项目交接与反馈 | 随本仓库构建，入口 `oev2 memory` |
| [GitContribute](modules/gitcontribute/SKILL.md) | 仓库、相关工作与贡献约束调查 | Skill 随仓库维护，运行程序从固定上游安装 |
| [LeanToken](modules/leantoken/SKILL.md) | 本地源码定位与精确上下文回读 | 同上 |
| [REA](modules/reverse-engineer-anything/SKILL.md) | 发布制品、二进制与限定运行行为调查 | 同上 |
| [Jacobian](modules/jacobian-math/SKILL.md) | 精确计算、有限搜索与数学结构分析 | 同上 |
| [SmokingGun](modules/smokinggun/SKILL.md) | 静态复杂度候选与优化证据组织 | 同上 |
| [Flameox](modules/flameox/SKILL.md) | 已有性能原件分析及授权工作负载测量 | 同上 |
| [electronics-design](modules/electronics-design/SKILL.md) | 电子设计、原理图/PCB、器件模型与工人接线指导 | Skill 与五个内部模块随仓库维护，运行工具按模块说明安装 |

七个专业 Skill 统一位于本仓库的 `modules/`，连同工程本体入口与 engineering-memory 一起维护：

```text
ontology-engineering-unified/
├── SKILL.md
├── modules/
│   ├── engineering-memory/skill/SKILL.md
│   ├── gitcontribute/SKILL.md
│   ├── leantoken/SKILL.md
│   ├── reverse-engineer-anything/SKILL.md
│   ├── jacobian-math/SKILL.md
│   ├── smokinggun/SKILL.md
│   ├── flameox/SKILL.md
│   └── electronics-design/SKILL.md
├── integrations/   # 上游身份、diff、依赖锁与共享接续代码
└── var/            # 本机运行程序、数据与验收记录
```

例如 Codex 可将整个仓库放在 `~/.codex/skills/ontology-engineering-unified/`；fleet 独立保留在其原有 Skill 目录。不要将七项再复制到宿主 Skill 顶层，也不要把旧安装留作同名入口。宿主发现和 MCP 连接在新会话中分别核验。

任务按缺口选择工具，不要求每次全部调用。完整版本、上游运行程序安装、四份原始 diff 及重建核验方法见[专业工具安装与接入](docs/TOOL-INTEGRATIONS.md)。六模块的 16 份 Skill 文件及其共享接续代码对应关系见[覆盖表](integrations/COVERAGE.md)；这些文件随仓库提供，新用户无需维护者的旧工作目录。上游实现由各项目提供，运行程序继续放在 `var/` 等用户选定位置；移动 Skill 不搬迁运行环境。macOS arm64 安装 Jacobian 时还需核 [Z3 wheel 平台元数据修正](integrations/runtime-locks/Z3-MACOS.md)。

[electronics-design](modules/electronics-design/SKILL.md) 是第七种专业能力，电子任务只从它的 `SKILL.md` 进入，按需读取五个内部模块：`easyeda-agent`、`easyeda-api`、`easyeda-schematic-net-fanout`、`lceda-jlc-3d-model-extraction`、`make-worker-wiring-card`。安装说明见[电子设计模块](modules/electronics-design/README.md)；原图保持原样。

核心工程与记忆流程不需要 Jev 或相关专用判断服务。任务解释、资料审阅和开放判断由当前 Agent 完成；Rust 检查明确的合同、来源与关系。模型答复、程序退出成功、经验召回和工程采用分别记录。

## 安装核心系统

准备 Git、[Rust/rustup](https://rustup.rs/) 和平台 C/C++ 编译工具。工具链由 `rust-toolchain.toml` 固定，依赖使用 `Cargo.lock`。首次构建需要下载依赖；已备齐时可加 `--offline`。

把仓库克隆到真实目录。也可选择当前 Agent 宿主支持的 Skill 搜索目录。

```sh
git clone https://github.com/jiaqiwang969/OntologyEngineering.git "$HOME/OntologyEngineering"
cd "$HOME/OntologyEngineering"
sh scripts/install.sh

export ONTOLOGY_ENGINEERING_ROOT="$HOME/OntologyEngineering"
export PATH="$ONTOLOGY_ENGINEERING_ROOT/var/install/bin:$PATH"
oev2 --root "$ONTOLOGY_ENGINEERING_ROOT" --version
oev2 --root "$ONTOLOGY_ENGINEERING_ROOT" memory capabilities
oev2 --root "$ONTOLOGY_ENGINEERING_ROOT" memory status
```

安装器构建本体与记忆的八个原生程序，在 `var/install/` 保存二进制摘要、构建配置和安装身份。默认 development profile、两个编译任务；优化构建使用 `sh scripts/install.sh --release`。安装器不安装外部专业工具、不启动常驻服务，也不替换旧全局 `oe`。

将本仓库 [SKILL.md](SKILL.md) 作为统一技能入口交给 Agent，按 [AGENTS.md](AGENTS.md) 配置项目、记忆及工具；由该入口按需读取 `modules/` 下的专业 Skill。新会话确认加载路径及同名副本情况。先在仓库根运行 `python3 scripts/check_tool_skills.py` 核六工具完整 Skill 摘要，再按 AGENTS 的方法检查宿主发现目录；文件核验不代替新会话的实际发现与调用。memory 默认使用本安装的 `var/memory/`；共享库通过 `oev2 memory --home /absolute/shared-memory …` 显式选择，不自动搜索其他 profile 的数据。

平台资格以实际验证为准。原生 CAD 另需对应平台、SDK 与真实验收；一次源码编译不能授予跨平台或工程资格。

## 从一个任务开始

例如向 Agent 提出：

> 检查这个仓库的重复查找是否值得优化。保持行为和输入条件，查相关实现、记录基线，把结论与依据放进同一项目，最后保存方法及失败经验。

Agent 按以下流程工作：

1. 建立或明确选择同一 Project，保存原要求、对象版本、条件和验收项。
2. 召回经验，核对原文、纠正与适用条件，交接为待核候选。
3. 选择工具，保存实际输入、工具身份和原始输出；性能任务先取得可比基线。
4. 经 `project tool-result verify/register` 回交，再经 `project decide` 更新决定；工程采用另走原采用合同。
5. 将过程与真实使用反馈交给 memory，在后续任务中重新核对适用性。

按[第一个完整 Project](docs/FIRST-PROJECT.md)可实际跑通记忆、数学工具、决定、反馈和下一会话召回。详见[专业能力交接](references/professional-capabilities.md)、[工程方法](SKILL.md)、[记忆模块](modules/engineering-memory/README.md)和[命令说明](docs/USAGE.md)。Agent 维护结构化记录，用户无需手填 TOML、YAML 或 RDF。

## 原始资料与私人数据

书籍和米思米目录单独放在 [Google Drive 原始资料包](https://drive.google.com/drive/folders/1QoNTu0gy9aPwQ-Sk6y5IWSBcVUCDVdXK)。`books/`、`misumi/` 和 `upload-manifest.json` 用于资料交付；来源登记、内容适用性与工程采用分别核查。资料按其权限和许可使用。

仓库不包含私人书籍、聊天 session、记忆库、客户项目、设备配置、凭据、历史工作区和编译缓存。`sources/` 保存用户合法取得的原件，`var/` 保存项目、记忆与运行记录。软件升级不自动更新 Drive；原件实际变化时才更新对应文件与清单。

## 开发与范围

`specs/` 是规范正本，锁定真实 ggen 生成 `generated/`，复杂算法由 Rust 实现。工程与 memory 共享 workspace、Cargo.lock 和语义实现。外部工具版本与 diff 位于 `integrations/`。

- [能力与工程边界](docs/CAPABILITIES.md)
- [当前版本及延期范围](docs/RELEASE-SCOPE.md)
- [新用户和开发 Agent 配置](AGENTS.md)
- [运行与生成方式](docs/USAGE.md)
- [验证记录](docs/UNIFIED-VERIFICATION.md)与[空 Skill 重装、新会话验收](docs/FRESH-INSTALL-20261022.md)
- [七模块目录统一、宿主发现与限定接入验收](docs/SKILL-LAYOUT-20261022.md)

v1 保留在仓库历史中。旧通过不授予本候选或新平台资格。

自有部分适用根 [LICENSE](LICENSE)。第三方与派生部分保留各自许可证及来源；memory 的 [NOTICE](modules/engineering-memory/NOTICE.md) 和所附许可证另行说明来源与条款，整个依赖集合不应被视为同一种许可。
