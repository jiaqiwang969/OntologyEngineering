# OntologyEngineering：新用户配置与开发合同

本仓库同时提供工程本体 v2 与 engineering-memory，统一技能入口为 `SKILL.md`，CLI 为 `oev2`。专业工具安装与接入差异见 `docs/TOOL-INTEGRATIONS.md`。先确认用户目标、对象、条件和授权范围；不能从 cwd 或 profile 猜测 Project 绑定。

## 新用户配置

1. 按 README 在真实目录运行 `sh scripts/install.sh`，核八程序与 `var/install/INSTALLATION.json`。安装身份是本机私有记录。已有 `var/maintenance/status.json` 可能描述历史发行，不能据此覆盖新安装身份。
2. 将本仓库的 `SKILL.md` 接入当前 Agent 宿主。可克隆到宿主支持的 Skill 目录，或按宿主方式注册真实目录；不再维护脱离源码版本的另一份 Skill。新会话确认加载新入口。CLI 根使用真实路径，不能通过旧安装或软链接绕过来源检查。
3. 按实际安装位置设置：

   ```sh
   export ONTOLOGY_ENGINEERING_ROOT="$HOME/OntologyEngineering"
   export PATH="$ONTOLOGY_ENGINEERING_ROOT/var/install/bin:$PATH"
   oev2 --root "$ONTOLOGY_ENGINEERING_ROOT" --version
   oev2 --root "$ONTOLOGY_ENGINEERING_ROOT" memory capabilities
   oev2 --root "$ONTOLOGY_ENGINEERING_ROOT" memory status
   ```

4. memory 默认在本根 `var/memory/`。多个客户端共享时，显式指定同一真实目录：`oev2 memory --home /absolute/shared-memory status`，后续命令保持同一 home。空库是正常状态；已有数据先核格式与来源，不自动改写旧记录。
5. 按任务安装专业工具并应用匹配上游版本的 diff，执行对应实际案例。四份 diff 按 `integrations/tools.lock.json` 的 `patch_targets` 应用，全部 `skill_files` 核摘要；LeanToken/flameox 安装 `integrations/companions/` 中的完整 Skill，避免同名副本。REA/SmokingGun 按 runtime-locks 执行 npm ci，两个 Python 工具分别建环境、安装各自 requirements。MCP/CLI 配置按工具与当前宿主的说明完成，REA 启动环境和三个共享接续脚本见 `integrations/bindings/WORKFLOW.md`。不复制开发者个人配置。把实际程序、版本、摘要和接入记录写入本根 `var/capabilities/registry.json`，通过 `project tool-result registry` 核验。
6. 核 `oev2 project --help` 和 `oev2 memory --help`。缺工具、原件、模型资产或 SDK 时记录具体缺项。核心 Project 与经验读写无需专用判断服务。hook 为可选便利入口，显式命令必须能独立完成闭环。

电子任务由第七个专业模块 `modules/electronics-design/SKILL.md` 统一进入；内部五个模块按该入口读取 `GUIDE.md`，不另装为五个独立 Skill。先读其 README 和 SOURCES.json，按实际需要安装 EasyEDA、CLI/连接器或求解器。调用辅助脚本时设置 `EASYEDA_SKILLS_DIR="$ONTOLOGY_ENGINEERING_ROOT/modules/electronics-design/modules"`，核实际安装和文档身份，不沿用旧机器路径或接收记录。原生 EDA、仿真、制造交付和实物检查分别验收。

用户用自然语言提出要求；Project、registry 和反馈等内部合同由 Agent 根据真实输入准备，不要求用户代填。

## 同一 Project 的工作方式

- 读 `SKILL.md`、`docs/CAPABILITIES.md`、`docs/RELEASE-SCOPE.md`，按问题展开专门合同。未闭合的范围保持“重构中”。
- 用同一 Project 的对象、修订、条件与验收组织工作。经验经 `memory session prepare/handoff` 与 `project memory-candidate verify/register` 接续，仅为待核候选。
- 专业结果保留输入、版本、来源、原始输出、覆盖和未知，经 `project tool-result` 登记。执行状态、检查覆盖、决定、工程采用分别保存。
- 经 `project memory-feedback` 与 `memory session feedback` 保存实际使用反馈。曝光、未用、无帮助和返工如实记录；模型答复或工具成功不能代替工程通过。
- 历史 session 与回执是资料，不赋予当前执行授权。来源、条件或版本改变后重审受影响结论，不继承旧通过。

## 开发合同

- 当前 checkout 是开发正本。旧工作区、历史发行和全局旧 `oe` 不作为 v2 的执行或开发入口。
- `specs/` 为编辑正本，派生 RDF 与 `generated/` 只能经锁定真实 ggen 产生；不手改派生物，不自造渲染器替代 ggen。memory 的合同生成入口见模块文档。
- 自有业务、规范化、检查和正式语义执行均用 Rust。工程与 memory 共用根 Cargo.lock 和语义实现；专业工具沿自己的原生入口执行。
- 复杂算法在手写区；共享类型、接口、错误、状态与可表达约束从规范生成。修改模块时补齐设计功能、检查、案例、来源与平台映射。
- 先复现、分析影响，再修正规范/模板/实现，回归与独立验证。保留失败、未知、未执行和反例，不放宽判据取得通过。
- 构建用锁定工具链与 Cargo.lock；生成失败不切换工作版本，漂移检查比较重新实际生成的字节。源码、程序、安装、工具执行与工程采用分别取证。
- CAD 使用 v2 自有 Rust、NX controller/worker 或 SW worker 与显式原生 SDK。不得调用旧 cad-agent 脚本、环境、MCP、Journal runner 或默认配置作为入口/备用；Fusion 不执行。详见 `docs/CAD-INDEPENDENT-EXECUTION.md`。
- `var/` 保存私有任务、项目、记忆、运行及维护记录，`sources/` 保存合法原件与私有索引，两者不进入分发。凭据、账号、设备配置、私人 session 不发布。公开案例必须可独立取得。
- 外部工具升级时核上游身份，重查 diff，重跑受影响案例；安装成功不等于结果合同或工程能力通过。

## 重新安装验收

固定远端待测提交，保全旧安装与私人数据后，从 Agent 可发现目录移除本次涉及的旧 Skill。使用新会话、独立配置和空测试记忆库，不能调用备份中的入口。

仅依公开仓库、上游依赖、必要 diff 和资料链接完成安装；验证空库启动、Project—工具—memory 完整回流、各工具案例，以及缺工具、版本不匹配、来源篡改、条件变化时的拒绝。真实工程能力与未验证平台按其范围继续保留。
