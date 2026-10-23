# OntologyEngineering：新用户配置与开发合同

本仓库同时提供工程本体 v2 与 engineering-memory，统一技能入口为 `SKILL.md`，CLI 为 `oev2`。专业工具安装与接入差异见 `docs/TOOL-INTEGRATIONS.md`。先确认用户目标、对象、条件和授权范围；不能从 cwd 或 profile 猜测 Project 绑定。

## 新用户配置

1. 按 README 在真实目录运行 `sh scripts/install.sh`，核八程序与 `var/install/INSTALLATION.json`。安装身份是本机私有记录。已有 `var/maintenance/status.json` 可能描述历史发行，不能据此覆盖新安装身份。
2. 将整个仓库接入当前 Agent 宿主的真实目录，例如 `~/.codex/skills/ontology-engineering-unified/`，以根 `SKILL.md` 统一导航。七个专业 Skill 固定在本根 `modules/`，engineering-memory 保留 `modules/engineering-memory/skill/`，fleet 在仓库外独立维护。按宿主支持的方式发现模块或从根入口按需读取，勿在宿主顶层另装七份 Skill。新会话核实际加载路径和同名副本；CLI 根使用真实路径，不能通过旧安装或软链接绕过来源检查。
3. 按实际安装位置设置：

   ```sh
   export ONTOLOGY_ENGINEERING_ROOT="$HOME/OntologyEngineering"
   export PATH="$ONTOLOGY_ENGINEERING_ROOT/var/install/bin:$PATH"
   oev2 --root "$ONTOLOGY_ENGINEERING_ROOT" --version
   oev2 --root "$ONTOLOGY_ENGINEERING_ROOT" memory capabilities
   oev2 --root "$ONTOLOGY_ENGINEERING_ROOT" memory status
   ```

4. memory 默认在本根 `var/memory/`。多个客户端共享时，显式指定同一真实目录：`oev2 memory --home /absolute/shared-memory status`，后续命令保持同一 home。空库是正常状态；已有数据先核格式与来源，不自动改写旧记录。
5. 七个专业 Skill 随本仓库维护，按任务另装固定上游运行程序并执行对应实际案例。`integrations/tools.lock.json` 的 `skill_directory` 指向仓库内模块目录；在仓库根运行 `python3 scripts/check_tool_skills.py` 核六项全部 16 个 `skill_files` 的摘要。四份 diff 保留用于从干净上游副本重建和复核，按 `patch_targets` 应用；不要在已整合的 `modules/` 上重复打补丁。LeanToken/Flameox 的完整维护源为 `modules/leantoken/` 与 `modules/flameox/`。REA/SmokingGun 按 runtime-locks 执行 npm ci，两个 Python 工具分别建环境、安装各自 requirements，执行 pip check；macOS arm64 的 Z3 修正见 `integrations/runtime-locks/Z3-MACOS.md`。同平台的 REA/Ghidra 原生组件准备见 `integrations/runtime-locks/GHIDRA-MACOS.md`。执行前核 CLI 实际路径与本次安装记录；PATH 若指向旧工作区，使用本次已核安装的绝对路径，不能仅凭版本号相同认定来源相同。MCP 配置与 Skill 文件布局分开；勿运行会向顶层写 Skill 的上游 setup/upgrade。仅对选定客户端配置已核程序的 MCP 入口，配置前检查影响范围，不重启或改写其他会话的连接。REA 启动环境和三个共享接续脚本见 `integrations/bindings/WORKFLOW.md`。不复制开发者个人配置。把实际程序、版本、摘要和接入记录写入本根 `var/capabilities/registry.json`，通过 `project tool-result registry` 核验。
6. 核 `oev2 project --help` 和 `oev2 memory --help`。缺工具、原件、模型资产或 SDK 时记录具体缺项。核心 Project 与经验读写无需专用判断服务。hook 为可选便利入口，显式命令必须能独立完成闭环。
7. 接入实际工程项目时，先确认 Project ID，再按 [项目文件体系](docs/PROJECT-WORKSPACE.md)显式指定 `--project-home /absolute/project/.engineering`。安装根、项目数据根与 memory home 分开；在该工程项目的宿主 AGENTS 或配置中写入恢复命令与明确身份。源码候选、实际安装与新会话接续分别核验；不以文件存在认定自动 hook 已生效。
8. 工程资料统一投放本安装根 `sources/`，书籍使用 `sources/books/`，米思米使用 `sources/misumi/`，PDF 索引保持 `sources/.indexes/local-pdf/`。按 [sources/README.md](sources/README.md)清点来源、版本、摘要和现有覆盖，选择与格式相符的读取或登记入口；用户不手填清单。已有本地原件核验后移动，无需重复下载；安装根或路径变化时保全原来源身份与回执，再显式重建完整登记，不以软链接或静默回退维持旧身份。

资料处理先核当前程序的 `knowledge --help` 与 `knowledge library source-delivery --help`。有已核本地原件时不运行 fetch；交付清单中的相对路径继续映射到 `sources/` 下。PDF 新增或修订由 Agent 准备清单，先 `knowledge register --merge --dry-run` 核原件、旧索引与差异，再 `--merge` 按 ID 更新并保留未列旧来源；source-delivery register 同样支持这些选项。预览不提取或发布，也不是后续执行凭证。`--replace` 仍按完整清单重建并移除未列来源，路径迁移等完整重建先预览移除项。成功后核全部来源与页数，保留原回执及失败；`unchanged` 只表示索引无需修改。详见[资料接入说明](sources/README.md)。新增 PDF 登记不会扩展 `project work` 既有六书角色；非 PDF 与供应商资料按各自合同接入。入库、索引、本体化和当前项目采用分别记录，不把路径整理宣称为全书处理。

安装或升级后检查宿主的全部实际 Skill 搜索目录。以下命令只读核摘要和同名入口；`--skill-root` 可重复，未使用的宿主目录可省略：

```sh
cd "$ONTOLOGY_ENGINEERING_ROOT"
python3 scripts/check_tool_skills.py
python3 scripts/check_tool_skills.py --skill-root "$HOME/.codex/skills" --skill-root "$HOME/.agents/skills"
```

发现同名入口时先核内容、来源及使用者，保全需留存的旧文件，再从宿主发现目录移出本次替换的副本；不要删除私人数据或修改其他会话。运行环境继续使用其已核绝对路径。新会话确认七模块、memory 与本体入口来自同一 checkout；MCP 还需实际发现和调用，不能由 Skill 文件存在推定连接成功。

电子任务由第七个专业模块 `modules/electronics-design/SKILL.md` 统一进入；内部五个模块按该入口读取 `GUIDE.md`，不另装为五个独立 Skill。先读其 README 和 SOURCES.json，按实际需要安装 EasyEDA、CLI/连接器或求解器。调用辅助脚本时设置 `EASYEDA_SKILLS_DIR="$ONTOLOGY_ENGINEERING_ROOT/modules/electronics-design/modules"`，核实际安装和文档身份，不沿用旧机器路径或接收记录。原生 EDA、仿真、制造交付和实物检查分别验收。

用户用自然语言提出要求；Project、registry 和反馈等内部合同由 Agent 根据真实输入准备，不要求用户代填。

## 同一 Project 的工作方式

- 读 `SKILL.md`、`docs/CAPABILITIES.md`、`docs/RELEASE-SCOPE.md`，按问题展开专门合同。未闭合的范围保持“重构中”。
- 新会话、压缩接续及新阶段前，读取当前项目 HEAD 并执行 `project workspace resume`。先核根请求、全部验收、纠正义务、`pending_decisions` 与动作状态；`incomplete` 补原件，`effect_unknown` 先回读。旧派发记录不等于本次重试授权，`ready` 不表示工程通过。
- 用 workspace commit 保存来源、修订、因果依据与阶段状态；核 expected parent，冲突保留候选并重新对账，不手改 HEAD。工具执行前记录意图和派发，结果经实际 owner 回读；纠正解决需要当前复核，失败、延后和未知不得被阶段摘要抹除。细节、命令与预算以 `docs/PROJECT-WORKSPACE.md` 为准。
- 用同一 Project 的对象、修订、条件与验收组织工作。经验经 `memory session prepare/handoff` 与 `project memory-candidate verify/register` 接续，仅为待核候选。
- 专业结果保留输入、版本、来源、原始输出、覆盖和未知，经 `project tool-result` 登记。执行状态、检查覆盖、决定、工程采用分别保存。
- 经 `project memory-feedback` 与 `memory session feedback` 保存实际使用反馈。曝光、未用、无帮助和返工如实记录；模型答复或工具成功不能代替工程通过。
- 历史 session 与回执是资料，不赋予当前执行授权。来源、条件或版本改变后重审受影响结论，不继承旧通过。
- 项目内方法试验经 workspace learning-goal / learning-freeze 绑定同一完整根目标、当前 Project 和全部验收；接续试验、评估及记录采用决定前运行 learning-check，再用既有 RRSI 合同比较。普通接续不自动晋升方法，memory 召回不替代当前项目事实。

## 开发合同

- 当前 checkout 是开发正本。旧工作区、历史发行和全局旧 `oe` 不作为 v2 的执行或开发入口。
- `specs/` 为编辑正本，派生 RDF 与 `generated/` 只能经锁定真实 ggen 产生；不手改派生物，不自造渲染器替代 ggen。memory 的合同生成入口见模块文档。
- 自有业务、规范化、检查和正式语义执行均用 Rust。工程与 memory 共用根 Cargo.lock 和语义实现；专业工具沿自己的原生入口执行。
- 复杂算法在手写区；共享类型、接口、错误、状态与可表达约束从规范生成。修改模块时补齐设计功能、检查、案例、来源与平台映射。
- 先复现、分析影响，再修正规范/模板/实现，回归与独立验证。保留失败、未知、未执行和反例，不放宽判据取得通过。
- 构建用锁定工具链与 Cargo.lock；生成失败不切换工作版本，漂移检查比较重新实际生成的字节。源码、程序、安装、工具执行与工程采用分别取证。
- CAD 使用 v2 自有 Rust、NX controller/worker 或 SW worker 与显式原生 SDK。不得调用旧 cad-agent 脚本、环境、MCP、Journal runner 或默认配置作为入口/备用；Fusion 不执行。详见 `docs/CAD-INDEPENDENT-EXECUTION.md`。
- `var/` 保存私有任务、项目、记忆、运行及维护记录，`sources/` 保存工程资料原件、处理成果与私有索引。私人内容不进入分发，`sources/` 只允许公开精确路径 `sources/README.md`；不要强制 git add 原件、提取文本、私人清单或导航笔记。凭据、账号、设备配置、私人 session 不发布。公开案例必须可独立取得。
- 外部工具升级时核上游身份，从干净副本重查 diff，再更新模块及锁内摘要，重跑受影响案例；查宿主同名副本，避免上游 setup/upgrade 又生成顶层 Skill。安装成功不等于结果合同或工程能力通过。

## 重新安装验收

固定远端待测提交，保全旧安装与私人数据后，从 Agent 可发现目录移除本次涉及的旧 Skill。使用新会话、独立配置和空测试记忆库，不能调用备份中的入口。

仅依公开仓库内的模块、上游运行依赖和资料链接完成安装；必要 diff 另在干净上游副本验证可重建性；验证空库启动、Project—工具—memory 完整回流、各工具案例，以及缺工具、版本不匹配、来源篡改、条件变化时的拒绝。真实工程能力与未验证平台按其范围继续保留。
