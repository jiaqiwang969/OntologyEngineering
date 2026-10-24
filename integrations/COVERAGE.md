# 当前专业模块的源代码覆盖

本表从实际安装的六个运行工具、electronics-design 与 Anatomy 模块及其可达资源出发，对照固定上游和本项目实现。此前仅有“四个简短追加 diff + 两份说明”不足以复现原接续行为：通用接收器、明确当前 Project 的复核包装、显式记忆续接和部分本地规则遗漏。本次补齐它们，不把历史私人案例作为运行依赖。

当前专用桥状态：Anatomy 已有 CAD→受限渲染代理；electronics-design 已有
`project electronics project` 的严格外层投影（请求、canonical source、辅助原件、哈希、
Project context、projection 和 receipt）。REA/LeanToken/GitContribute 已有源/制品图谱投影，
Jacobian 已有带单位、前提和证书的数学投影，SmokingGun/Flameox 已有性能测量与清理边界投影；
这些入口仍只做原件接入和严格回执，通用 ToolResult 登记、决定更新与工程采用另行审查。

## 随仓库维护的完整 Skill 文件

六个模块直接位于仓库根的 `modules/`，每项路径在 [tools.lock.json](tools.lock.json) 的 `skill_directory` 中；每个文件的实际 SHA256 在 `skill_files` 中。四份补丁均对固定真实上游执行了 `git apply --check`、实际应用和输出字节核对。REA 的上游基线是 npm 发行包内的 Skill，不是仓库模板。

| 当前 Skill 文件 | 新用户取得方式 | 处理 |
|---|---|---|
| GitContribute：SKILL.md、LOCAL.md、UPSTREAM.md | [`modules/gitcontribute/`](../modules/gitcontribute/SKILL.md)，由固定上游 + `patches/gitcontribute-3.0.2-ontology.patch` 重建 | 保留完整上游规则、本地匿名联网/离线语料边界、精确 MCP 参数和完整接续 |
| LeanToken：SKILL.md | [`modules/leantoken/`](../modules/leantoken/SKILL.md) | 保留本地维护的完整入口、显式 root、先 index、有界 context、generation/live bytes 及变更后重索引；不另装同名上游 Skill |
| REA：SKILL.md 与 references 下六文件 | [`modules/reverse-engineer-anything/`](../modules/reverse-engineer-anything/SKILL.md)，由完整 npm 6.5.0 Skill + `patches/rea-6.5.0-ontology.patch` 重建 | 六份 reference 与固定 npm 原件逐字节一致；保留 Peekaboo 后台规则、分页/原生 Evidence、安装/新 stdio/当前宿主身份区分及任务会话清退 |
| Jacobian：SKILL.md | [`modules/jacobian-math/`](../modules/jacobian-math/SKILL.md)，由固定上游 + `patches/jacobian-0.23.0-ontology.patch` 重建 | 保留完整数学规则、独立验算、标量类型、可选后端边界与本地接续 |
| SmokingGun：SKILL.md、LOCAL.md、UPSTREAM.md | [`modules/smokinggun/`](../modules/smokinggun/SKILL.md)，由固定上游 + `patches/smokinggun-5.0.0-ontology.patch` 重建 | 保留完整上游规则、Rust 结构覆盖边界、外部测量授权、可比性和接续 |
| Flameox：SKILL.md | [`modules/flameox/`](../modules/flameox/SKILL.md) | 保留完整本地入口、三层预算、采样内存边界、preserve→reopen→inspect_evidence、原样分页/重放和清退；不伪称上游 Skill |

共 16 个文件：3 + 1 + 7 + 1 + 3 + 1，完整文件随本仓库维护，并保留原锁定摘要。个人安装路径已转换为用户配置的程序入口与 `ONTOLOGY_ENGINEERING_ROOT`。四份原始 diff 继续保留，仅用于干净上游副本的重建核验，不能重复应用到已整合模块。上游及派生文件的许可见 [licenses/](licenses/)。LeanToken/Flameox 维护源统一移到 `modules/`，不再保留 `companions/` 的同名发现入口。

`[electronics-design](../modules/electronics-design/SKILL.md)` 和 `[Anatomy](../modules/anatomy/SKILL.md)` 同在 `modules/`。电子模块的五个内部模块、来源和验收范围见自己的 [README](../modules/electronics-design/README.md) 与 [SOURCES.json](../modules/electronics-design/SOURCES.json)；Anatomy 的固定上游树、Project/source/evidence 边界见自己的 [LOCAL.md](../modules/anatomy/LOCAL.md) 与 [SOURCES.json](../modules/anatomy/SOURCES.json)。engineering-memory 路径保持 `modules/engineering-memory/skill/`，fleet 继续在本仓库外维护。

在仓库根用 `python3 scripts/check_tool_skills.py` 核全部八个模块的入口和关键文件摘要；用可重复的 `--skill-root` 检查宿主同名入口。文件核验、宿主实际发现、MCP 调用和工程能力分别取证。运行程序继续位于原已核 `var/` 或用户指定路径；Skill 目录统一不授予运行或工程资格。

## 可执行依赖闭包

| 原有行为 | 本项目代码/合同 | 验证与边界 |
|---|---|---|
| 原件固定缓冲哈希、前后 Project 验证与变化影响 | `bindings/verify_candidate.py`、`bindings/common.py` → Rust project validate/reconcile | 5 工具名 × 5 合成未变/对象/条件/来源/篡改情形；原件哈希不解释证据语义 |
| 显式当前或历史模式、当前快照逐字节匹配、可选 memory 同一绑定 | `bindings/reconcile_reviewed.py` → Rust reconcile-reviewed 与 memory get/verify | 保留原包装层的保护；仅有 Rust 请求内的 current 字段不能代替外层显式当前锚点 |
| 输出预算、超时、部分原件、原退出码、任务进程组清退 | `bindings/common.py` | POSIX 受控进程反例，包括忽略 TERM 的子进程；这些假进程不当作原生语义验收 |
| 显式 context SHA/provider/session/current Project 锁定，原 task/query/条件/预算不变 | `bindings/resume_memory.py` → 原生 get→verify→prepare→verify | 真实 memory 与隔离存储反例；不 activate、不改 Project，不由 hook 代替此入口 |
| 当前原件重读、真正重放内核、依赖阻塞、独立采用 | `crates/oe-engineering/src/reviewed_reconciliation.rs`、`specs/modules/rdf-object-focus.toml` | 原有本地内核扩展已进入共享核心；包装器不复制审查判据 |
| 六种“继续优化”短句与否定/新对象/预算变化边界 | `modules/engineering-memory/runtime/src/hooks.rs`、其 session 测试与合同 | 原生补丁已保留；hook policy 与显式旧请求预算各有身份 |
| REA 选定 Node、Ghidra/JDK 配置和固定 npm dispatcher | `bindings/rea-launcher.cjs` | 个人路径改显式配置；保留已合格 JAVA_HOME，否则核选定 JDK≥21，PATH 使用当前 Node；配置测试不代表真实 Ghidra 能力通过 |

详细调用与资源边界见 [bindings/WORKFLOW.md](bindings/WORKFLOW.md)。三个包装器只依赖 Python 标准库与本次安装的 Rust 程序；不需要任何维护者的旧 Skill、外部工作目录、私有配置或历史 Project。

## 运行程序来源

六个源码克隆仍保留各自原始 GitHub 身份，未发现对其工作树或提交的本地修改；这项检查本身不足以证明实际安装程序没有差异。因此另核了实际运行包：REA 1650 个 npm 发布文件、SmokingGun 129 个 npm 发布文件、Jacobian 2155 个 wheel 包文件和 Flameox 95 个 wheel 包文件均与固定官方发行逐字节一致。Python 的安装 RECORD 也分别核对了 2164 与 102 个有摘要条目。其算法不需要本地 source patch；启动器、Skill 和本项目接续代码有各自的本地来源。

REA 使用发布标签/发行包身份，不能用源码 main 的不同提交冒充发布程序。GitContribute 与 LeanToken 的当前 macOS arm64 二进制也已与固定官方发行归档解出的原件逐字节核对一致；归档摘要与发行资产记录相符。身份与平台仍应在新用户安装中重新核对。现用 npm/Python 依赖快照见 [runtime-locks](runtime-locks/README.md)，锁内记录摘要。它们固定选择，不表示已逐字节审计所有传递依赖源码。依赖版本及后端能力仍按用户机器重新安装和验证，不复用旧机通过回执。

## 验收范围和保留项

本次测试覆盖传输入口、实际 Rust 核验与固定 Skill 字节重建。还必须在新的客户端会话中分别核安装、MCP 发现和实际工具调用。REA 的静态 JavaScript 小样例不代替 Ghidra，Flameox 的已有样本分析不代替 capture/超时清退，工具结果接收也不等于业务工程验收。

GitContribute 3.0.2 在 macOS 的源码将配置定位到系统用户的 Application Support 目录；`GITCONTRIBUTE_DATABASE` 只隔离数据库，XDG 配置变量不会覆盖其 macOS 配置路径。保持 HOME 不变时不得声称仅凭独立数据库实现了全部配置隔离；新安装验收应明确验证实际读取范围或使用独立系统用户。这是上游路径行为，不由 Skill 补丁虚构覆盖。

未发布原始会话、私人 Project/证据、数据库、旧安装恢复备份及只服务一次历史案例的接收脚本。旧的 `six-tools.handoff/1` 不是当前 Project 合同。它们可用于解释历史，不属于新用户运行闭包，也不把旧性能收益作为这次新安装的通过。

本表保留原接入准备的历史验证范围。此前的[公开重装验收](../docs/FRESH-INSTALL-20261022.md)另有固定提交；本次模块布局仍需对应的新会话发现和调用记录，不能以文件迁入替代验证。
