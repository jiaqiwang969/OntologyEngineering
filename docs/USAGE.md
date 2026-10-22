# 构建与实际使用

工程方法从[主入口](../SKILL.md)进入，工具选择见[能力索引](CAPABILITIES.md)。本页对应统一候选 `2.0.0-rc.7`；具体通过范围见[验证记录](UNIFIED-VERIFICATION.md)。

## 源码安装与运行身份

在真实 checkout 运行：

```sh
sh scripts/install.sh
export ONTOLOGY_ENGINEERING_ROOT="$PWD"
export PATH="$ONTOLOGY_ENGINEERING_ROOT/var/install/bin:$PATH"
export OEV2_SKILL_ROOT="$ONTOLOGY_ENGINEERING_ROOT"
export OEV2_BIN="$ONTOLOGY_ENGINEERING_ROOT/var/install/bin/oev2"
"$OEV2_BIN" --root "$OEV2_SKILL_ROOT" --version
"$OEV2_BIN" --root "$OEV2_SKILL_ROOT" memory capabilities
"$OEV2_BIN" --root "$OEV2_SKILL_ROOT" memory status
```

安装器从 `rust-toolchain.toml` 读取精确版本，经 `rustup run` 调用 Cargo/rustc，避免 PATH 中其他 Cargo 抢占；缺工具链时给出明确安装命令。安装器固定本仓库的构建目录，使用锁定工具链和 Cargo.lock，编译本体七程序及 `engineering-memory`。默认 development profile；`--release` 使用优化构建，`--offline` 要求依赖已备齐，`--jobs N` 控制编译并发。记录在 `var/install/BUILD.txt`、`SHA256SUMS` 和 `INSTALLATION.json`。升级保留前一安装目录，完整准备后切换；安装身份失败时恢复前一目录。

`oev2 setup` 只能从本根 `var/install/bin/oev2` 登记完整八程序身份。已存在且完全匹配时只核验；程序变化后拒绝覆盖原回执，应重新运行安装器。它不替换全局 `oe`，不迁移私人项目，也不授予工程采用。

memory 子命令仅使用当前 oev2 同目录的程序；缺失时明确报错，不搜索旧 Skill 或 PATH。默认记忆目录为本根 `var/memory/`，共享库显式使用 `oev2 memory --home /absolute/shared-memory …`。模块操作见[记忆文档](../modules/engineering-memory/README.md)。

## Project、工具与记忆

Agent 从真实用户要求维护 Project，保留完整目标、对象、条件与验收。字段见[Project 样例](../examples/v2-project/README.md)及[根目标锚定](../references/goal-anchoring.md)。

源码候选新增 [项目文件体系与恢复](PROJECT-WORKSPACE.md)：显式 `--project-home` 把项目数据放入该项目的 `.engineering/`，安装来源根保持原身份。先按真实任务确认 Project ID，再 `project workspace init`、准备有源 commit；新会话和压缩后从 HEAD 运行 `resume`，读取完整目标、纠正、待复核决定和动作状态。文中的原 `var/projects/...` 命令仍演示未显式选数据根时的旧存储方式；不自动迁移或猜测同名项目。

```sh
export OEV2_PROJECT_HOME="/absolute/engineering-project/.engineering"
"$OEV2_BIN" --root "$OEV2_SKILL_ROOT" --project-home "$OEV2_PROJECT_HOME" project workspace --help
# 已明确初始化并提交的项目，恢复前重核完整来源。
"$OEV2_BIN" --root "$OEV2_SKILL_ROOT" --project-home "$OEV2_PROJECT_HOME" project workspace resume
```

`begin` 只检查当前阶段；执行前保存派发状态，真实工具回读后再提交结果。`dispatched`/`effect_unknown` 恢复时不自动重试。RRSI 经 `learning-goal` / `learning-freeze` 绑定当前完整 Project 与根验收，接续试验和评估前以 `learning-check` 重核；memory home 继续独立显式配置。状态、回执支持范围、宿主 AGENTS 接入及预算统一见上述文档；当前安装是否已有命令以实际程序为准。

```sh
"$OEV2_BIN" --root "$OEV2_SKILL_ROOT" project --help
"$OEV2_BIN" --root "$OEV2_SKILL_ROOT" project validate --input var/projects/example/project.json
"$OEV2_BIN" --root "$OEV2_SKILL_ROOT" project context --input var/projects/example/project.json --object var/projects/example/object.json --output var/projects/example/context.json
"$OEV2_BIN" --root "$OEV2_SKILL_ROOT" project tool-result --help
"$OEV2_BIN" --root "$OEV2_SKILL_ROOT" project memory-candidate --help
"$OEV2_BIN" --root "$OEV2_SKILL_ROOT" memory session --help
```

示例路径是 Agent 应据实际任务创建的输入，不能把不存在的文件当作已运行案例。用户无需手填合同。

工具调用前取得 Project 快照；调用后保存实际输入、程序版本与原始记录，经 `project tool-result verify/register` 接收。登记写新 Project 修订，不自动改决定或采用。`project decide` 根据已登记且重新核验的结果更新有范围的决定；工程采用仍走 `project adopt`。详细字段和性能比较见[专业能力交接](../references/professional-capabilities.md)。

经验经 `memory session prepare/activate/handoff` 与 `project memory-candidate verify/register` 双侧核验，只登记为待核候选。使用后通过 `project memory-feedback` 生成反馈，再交 `memory session feedback`。memory 记录来源、过程与经验；当前事实和工程决定仍由同一 Project 管理。

任务视图及原消息绑定见[当前任务](CONVERSATION-METHOD-USE.md)，变化重审见[有源复核](REVIEWED-RECONCILIATION.md)。对象版本、条件或来源变化后旧结果不能自动支持新决定。

## 来源、计算与完整检查点

```sh
"$OEV2_BIN" --root "$OEV2_SKILL_ROOT" knowledge status
"$OEV2_BIN" --root "$OEV2_SKILL_ROOT" knowledge --help
"$OEV2_BIN" --root "$OEV2_SKILL_ROOT" project work --help
"$OEV2_BIN" --root "$OEV2_SKILL_ROOT" harness --help
```

`knowledge query` 与 `project work` 使用本地来源检索、计算和明确检查，开放式适用性判断由 Agent 对照原文完成。检索候选、OCR、来源摘要匹配和工程适用分别取证。书籍与米思米原件从 README 的独立资料入口取得，按各自合法来源登记；无资料时返回具体缺项。

工程 work 的来源、功能、物理机理、前提和反例见[工程工作](../references/engineering-work.md)、[工程前提](../references/engineering-premises.md)。必要工程条件或原验收不能因局部计算成功而省略。

`harness step` 接受显式 `oev2.harness-checkpoint-input/1`，执行当前工程检查并保存完整原件、失败与未知；`recorded` 仅说明记录完成。`harness consolidate` 支持明确的空候选原件归档。协议、资源上限及恢复条件见[完整检查点](../references/harness-context.md)，不能用归档代替非空候选评估或工程采用。

## Agent、专业工具和可选模型

当前 Agent 组织问题、资料审阅、工具调用与验证，见[编排方法](AGENT-ORCHESTRATION.md)。六个专业工具由使用者按[接入说明](TOOL-INTEGRATIONS.md)安装，并应用匹配版本的 diff 或本项目 companion。不要把接口曝光或 help 成功写成实际能力验收。

普通模型辅助仍属显式可选能力，核心 Project 与 memory 不依赖它。长资料辅助沿[直接模型调用](../references/direct-model-assistance.md)，登记页 OCR 沿[OCR 通道](REGISTERED-OCR-MODEL7.md)，typed 关系线索沿[typed 抽取](TYPED-RELATION-EXTRACTION8.md)。调用者明确来源授权、服务配置和预算；缺失资源不换后端，模型回答不授予工程采用。

旧专用判断服务的自动路由、context/vision 与批次命令已退出本候选，不能用历史服务说明启动它们。通用模型 API 与已移除的服务协议分别辨认。

## 语义、CAD 与工程交付

- `kernel` 查询、审查和显式 source-session 的范围见[语义执行](SEMANTICS.md)与[来源会话](SEMANTIC-SOURCE-SESSION.md)。明确失败、unknown、部分覆盖与资源预算；完整 Semantica 等价尚未宣布。
- `kernel review --project` 绑定当前对象、条件和来源，RDF 焦点声明同一条件摘要。历史快照审查不授予当前采用。
- CAD 沿[独立执行合同](CAD-INDEPENDENT-EXECUTION.md)使用 v2 自有 Rust 与显式 SDK；不执行 Fusion 或旧 cad-agent 入口。
- 数值、图纸、BOM、工艺和检验分别核查[工程交付](ENGINEERING-DELIVERY.md)。保存、独立重开、图面可读及真实计量各需相应证据。
- 浏览器和原生操作遵守当前任务的设备、焦点与动作权限。工具软件可用不表示工程对象合格。

## 规范生成与维护

`specs/` 是正本。先冻结待改来源，按根 AGENTS 的复现、修正与回归合同工作。当前生成条件由 `runtime/ggen-lock.json` 固定；普通用户构建使用随仓库提供的已生成合同。

维护者修改规范时，需要真实锁定的 ggen：官方版本 `26.9.28`，提交 `ff96f04e8c7b851e5cca53f3faf5ce1d5f43ce6e`。原构建工具链为 `nightly-2026-06-22`；新平台的生成器身份需要独立建立，不能仅改摘要绕过核验。

```sh
# 规范新增/移除类型时，先构建不依赖 generated 合同的 bootstrap。
export CARGO_TARGET_DIR="$ONTOLOGY_ENGINEERING_ROOT/var/builds/rust-target"
export ONTOLOGY_GGEN="$ONTOLOGY_ENGINEERING_ROOT/var/tools/ggen-26.9.28/ggen"
rustup run 1.95.0 cargo build --locked -j2 --bin oev2-design-bootstrap
"$CARGO_TARGET_DIR/debug/oev2-design-bootstrap" generate "$ONTOLOGY_ENGINEERING_ROOT" "$ONTOLOGY_GGEN"
# 实际生成成功后，再构建消费者；固定此时的公共源码，不并发修改。
rustup run 1.95.0 cargo build --locked -j2 --bin oev2
export OEV2_BIN="$CARGO_TARGET_DIR/debug/oev2"
"$OEV2_BIN" --root "$ONTOLOGY_ENGINEERING_ROOT" design validate
"$OEV2_BIN" --root "$ONTOLOGY_ENGINEERING_ROOT" design check --ggen "$ONTOLOGY_GGEN" --output var/maintenance/drift-001.json
```

`ONTOLOGY_GGEN` 须指向事先准备并通过锁定摘要核验的真实生成器，源码安装器不会下载它。上述 `OEV2_BIN` 指本次实际重新构建的程序。ggen 在隔离目录实际生成，成功后才切换派生目录；check 重新生成并比较字节，不修改工作版本。结果须为 consistent 且 drift 为空，退出零本身不足以证明一致。生成后重编译消费者；失败不切换版本，不在生成切换时并发构建。

memory 规范生成见[模块说明](../modules/engineering-memory/README.md)。两部分都保持真实生成器、来源与合同身份。编译、生成、有限测试和工程验收分别接收。

公开的安装和 Project/memory 小案例与依赖原件的资格回归分别执行。`oe-engineering` 的 `typed_generation` 使用 `OE_TYPED_ORIGINAL_INPUT_ROOT`（默认本根 `sources/private-regression/typed-generation`）；`model_page_execution`、`model_page_support` 使用 `OE_PAGE_ORIGINAL_INPUT_ROOT`。这些回归核固定原字节摘要；包含个人来源元数据或书籍的输入不进入 GitHub。没有相应合法原件时，不能用合成替代品、跳过或旧回执宣称该资格通过。基础安装不要求这些私有回归输入，当前已验证范围见验证记录。

原 `dist release` 的 Mac /1、Windows /2 是历史本体七程序发行合同，保留其明确身份；当前统一八程序使用上述源码安装器与 `oev2.installation/1`。本候选不把旧包接收记作统一安装完成。Windows/native、原生双构建和其他平台资格仍按[发布范围](RELEASE-SCOPE.md)另验。
