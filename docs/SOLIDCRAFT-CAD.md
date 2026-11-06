# SolidCraft CAD：原生执行与工程本体接续

需要用 SolidCraft 建模、编辑 `.sc`、通过 MCP 观察桌面建模或交付几何交换文件时读取。本入口属于工程本体 CAD 流程；NX/SolidWorks 的选择和执行合同见 [独立执行](CAD-INDEPENDENT-EXECUTION.md)。按用户指定后端、现有文件和下游要求选择，不以本机安装方便替换指定的原生交付物。

## 身份和实际入口

[源码锁](../integrations/runtime-locks/solidcraft-source.json) 当前固定 [jiaqiwang969/solidcraft 的 `1453beabd793feb74d2c1b359b0cbbd34024a8bc`](https://github.com/jiaqiwang969/solidcraft/tree/1453beabd793feb74d2c1b359b0cbbd34024a8bc)，版本字段为 `0.40.0-rc.1`。真实上游是 [tangency-labs/solidcraft](https://github.com/tangency-labs/solidcraft)；双方默认分支都是 `main`。构建使用锁中的 `source_repository`、`source_commit` 和 Cargo.lock 摘要，不能只检出会移动的 `main`，也不能把来源字段 `upstream_base_commit` 当作构建目标。

该提交先完整保留 `integrate/engineering-fixes-0.40.0-rc.1` 的工程定制，再依次移植上游 #11974、#11893，已于 2026-11-06 推送到 fork main；它不是新发布版本或上游全量同步。几何实现已使用 [SolidCore `8ddce901`](https://github.com/tangency-labs/solidcore/tree/8ddce901671f1cebd9214a31d21895d725e886c8)，由 [Cargo.toml](https://github.com/jiaqiwang969/solidcraft/blob/1453beabd793feb74d2c1b359b0cbbd34024a8bc/Cargo.toml) 和 [Cargo.lock](https://github.com/jiaqiwang969/solidcraft/blob/1453beabd793feb74d2c1b359b0cbbd34024a8bc/Cargo.lock) 固定。本次未改 SolidCore 修订；两项修复分别位于 SolidCraft 的 SMLib 转换层与 STEP 读取层。旧 0.27.0/Truck 分支的测试或安装回执不自动适用于此版本，也不把几何内核复制到 v2。

| 当前源码中的能力或修复 | 对工程使用的作用及边界 | 提交依据 |
|---|---|---|
| 几何检查、解析支撑和投影来源信息 | 保留边面/圆检查、HLR 来源和版本 2 投影；时间线带错误或警告时拒绝捕获，不把残留实体当成完整模型 | [工程定制](https://github.com/jiaqiwang969/solidcraft/commit/d48fdfbd7d2eccf51a50824482796d5a43db4048) |
| 远离原点的 NURBS 权重比较 | 按控制点坐标量级容纳舍入误差；不代表全部大坐标布尔或平移不变性问题已解决 | [上游 #11974](https://github.com/tangency-labs/solidcraft/pull/11974)、[fork 提交](https://github.com/jiaqiwang969/solidcraft/commit/c73a24d44afcc0db165ccff153adb473b0f8c1b5) |
| STEP 自引用保护 | 直接或间接递归进入曲线/曲面读取时执行深度保护，返回带实体号的错误，避免该用例栈溢出 | [上游 #11893](https://github.com/tangency-labs/solidcraft/pull/11893)、[fork 提交](https://github.com/jiaqiwang969/solidcraft/commit/1453beabd793feb74d2c1b359b0cbbd34024a8bc) |

2026-11-05 的 macOS arm64 / Rust 1.99.1 源码集成检查中，两项修复均验证了修复前失败、修复后通过；HLR 85 项、几何检查接口 5 项、STEP 默认执行 40 项、槽切/STEP 四项定制回归、投影捕获拒绝不完整模型和 17 个仓库 STEP 样本的几何往返检查通过。完整 CI 仍为 **11,699 通过、35 失败、58 跳过**，未达到全绿，后续 Web 离线门禁未执行。31 个失败在已知名单，另 4 个与工程分支历史记录同名；未重跑未修改工程分支的完整基线，不能由名称一致证明原因一致。原始日志与受测提交绑定保存在对应工程项目的集成回执中；这里记录有范围的源码检查结果，不授予运行程序或私人模型资格。

使用前确认实际 `solidcraft-cli --version`、可执行文件的真实路径/摘要，以及 MCP 的初始化、工具发现和一次实际调用。GUI 桥接同时记录桌面应用版本与二进制摘要，优先使用同一次构建的 CLI 与应用。具体路径、控制端口、程序摘要和本机验收保存到私有 `var/capabilities/registry.json` 与 Project 原件，不写入共享 Skill 的默认值。

更新本源码锁和文档不会替换 CLI、桌面应用或现有 MCP 进程，也不会把 registry 中的旧 `qualified` 迁移到新源码。实际切换前核构建提交及程序摘要，补跑下面的小零件、改参/保存/独立重开、STEP 回读和实际需要的接口；按同一 Project 登记新结果并重审受影响决定。旧程序继续使用时明确记录其旧身份，不只改版本号或摘要来沿用旧通过结论。

原生入口如下，变量由 Agent 按当前机器与任务确定：

```sh
"$SOLIDCRAFT_CLI" mcp --allow-path "$CAD_OUTPUT"
"$SOLIDCRAFT_CLI" mcp --headless --allow-path "$CAD_OUTPUT"
"$SOLIDCRAFT_CLI" mcp --connect "127.0.0.1:$CAD_PORT" --allow-path "$CAD_OUTPUT"
```

- 不带 `--headless`、`--connect`、`--in` 的默认模式会自动发现应用：启动时有应用则桥接，否则先无窗口运行；无窗口会话尚无特征时，后续调用发现应用可切换到桥接，已有特征时则保留原会话，不搬移工作。先核 initialize 的 `Backend`，并关注每次调用的后端提示，不能把初始化状态当成整个会话的固定状态。需要固定可见窗口时用 `--connect`，需要独立复验时用 `--headless`。[当前源码](https://github.com/jiaqiwang969/solidcraft/blob/1453beabd793feb74d2c1b359b0cbbd34024a8bc/crates/mcp/src/backend.rs#L758)。
- 输出目录须先存在。工作目录不自动授予文件访问；只加入实际项目所需的 `--allow-path`，路径拒绝按项目授权补配置，不改走控制端口绕过限制。token 由上游管理，不写证据、命令日志或共享文档。
- Codex MCP 名称为 `solidcraft`，底层是实际 CLI 的 `mcp` 子命令。不要创建并不存在的 `oev2 cad solidcraft` 命令。缺程序或连接时报告具体缺项；CLI 与 MCP 可用性分别核验。

## 一次工程任务怎样执行

1. 恢复当前 workspace，明确 Project、对象修订、条件、决定和根验收；取得 `project context`，在派发前保存本阶段意图。需要完成 workspace 当前复核时，条件使用带真实来源、验收及断言的 `oev2.engineering-basis/1`，不能只留一句自由文本。没有 Project 时按真实用户任务建立，不从软件名或 cwd 猜测已有 Project。
2. 读当前服务器的 `skill_reference {"skill":"solidcraft-modeling"}`，编辑已有模型时读 `solidcraft-editing` 的 `SKILL.md`，检查/移植时读 `solidcraft-checking`。用 `inspect_design {"measure":false,"sketches":false}` 核当前模型。`new_design`/`open` 会替换会话，未明确授权覆盖的用户工作先保存或确认；独立无窗口验收可隔离在新进程。
3. 写明毫米/角度单位、基准/坐标、参数与表达式、预期包围盒和可独立计算的量。Project 对象身份和原生 body/feature 名称建立本次映射；不要把 `Body1` 当作跨文件或跨后端的持久工程 ID。
4. `batch` 按可回滚阶段建模，复杂结果用 `verbose:false`。不确定参数先查 `describe_command`，不无差别展开全目录。`set_parameter` 后检查重建错误；`measure` 核体积、包围盒、闭合性和需要的拓扑，`inspect_design` 核时间线错误、warning 和草图状态。
5. 保存 `.sc` 原生设计，再导出所需 STEP/网格，分别保留摘要。用第二个进程重开保存的 `.sc`；STEP 也实际回读测量，记录转换损失。截图使用 `source:model`，需要界面证据再取 `source:window`；不凭图片断言尺寸、公差、装配约束或制造完整。
6. 遇到失败或超时保留命令、原始回复、已知状态和未知效果。`still running` 后使用 `pending`/`running` 回读，确认状态前不重放会修改设计的调用。

GUI、无窗口和同程序 STEP 回读共享几何实现：它们证明状态接续或往返一致性，不是三个独立几何内核。解析体积、其他独立算法与实测各自保留来源和适用条件。复杂布尔、装配运动、图纸、BOM、公差和工艺依实际任务单独验收；小零件通过不扩大到整机或制造采用。

## 同一 Project 的回交

复用 [专业结果合同](../references/professional-capabilities.md#结果回到同一-project)，类型及可表达约束仍以 `specs/modules/rdf-object-focus.toml` 经 ggen 生成的合同为准。本入口没有新增类型、Rust CAD worker 或专用投影，专业操作由上游执行，共同来源/状态检查仍由现有 Rust owner 执行。

| 记录 | 必须承接的含义 |
|---|---|
| capability registry | `name: solidcraft`、`skill: ontology-engineering-v2`、实际 CLI 版本/程序摘要、`invocation: [cli,mcp]`、验收原件位置与摘要；`qualified` 只指明列出的平台和操作范围 |
| `tool-result.project` | 调用前取得的 Project context，保持对象修订、条件内容及源摘要；声明对应决定、根验收及 `route_stage` |
| `tool-result.subject` | 本次对象或技能集成的输入版本；`oev2.subject-manifest/1` 中列绝对原件路径及摘要，以便检测之后改版 |
| `inputs` / `execution` | 实际参数、选择的后端模式、输入文件、单位/坐标和环境、真实 UTC 起止、完成/部分/失败、退出与清理；GUI 模式保留应用进程身份 |
| `records` | 命令及原始 JSON-RPC/CLI 输出、保存的 `.sc`、导出文件、独立重开及测量、截图；原件保存在所选 project-home 下，相对 ArtifactReference 含真实摘要 |
| `findings` / `measurements` | observed 引原始记录；Agent 的解释/解析比较写 inferred；历史导入写 imported；单位、误差和覆盖明确，未测字段不补零或通过 |
| `coverage` / `unknowns` | 原生操作、模型尺寸和平台的实际范围；未覆盖工程要求，以及截图/转换/共享内核的证据限制 |

Agent 整理原件和合同；Rust 执行 `project tool-result registry`、`verify`、`register`，只写新 Project 修订与依赖。用 `project decide` 根据重新核验的已登记结论更新有范围的决定，交付用 `/2` route-audit；`adopted` 保持原值，工程采用仍走独立批准。新增证据或更新决定会触发 workspace 重审，按 [当前复核接续](REVIEWED-RECONCILIATION.md) 提供完整原件清单、实际当前审查回执及 `reconcile-reviewed` 结果，再关闭验收。workspace 的阶段回执由 `owner:tool_result` 重读真实结果，不把 `completed` 文本当作原生验收。

登记表默认在安装根；workspace 需要固定副本时，`--registry` 显式使用项目内绝对路径。缺登记、版本或程序字节变化、原件变动、对象修订/条件改变均按现有合同拒绝或要求补验，不能只更新哈希让旧结论继续生效。失效的编译来源绑定先核当前源码与安装身份，使用匹配构建或正常重装；不绕过 `require_execution`。

SolidCraft 没有 NX occurrence/原生 drawing/assembly observation 的同义合同。当前 `oev2 cad presentation project` 仍使用其声明的 NX 输入；需解释图时单独交 Anatomy 并标记源与代理损失，不把 `.sc` 包装成 NX 观察。

## 可复跑的小零件

[plate.json](../examples/solidcraft/plate.json) 创建 40 × 30 × H mm 板、中心直径 6 mm 通孔，H=12 mm。选用新的空输出目录；它是接口与几何检查例子，草图完全约束、结构强度和制造要求另验。

```sh
"$SOLIDCRAFT_CLI" run "$ONTOLOGY_ENGINEERING_ROOT/examples/solidcraft/plate.json" \
  --save "$CAD_OUTPUT/plate.sc" --out "$CAD_OUTPUT/plate.step"
"$SOLIDCRAFT_CLI" eval "$CAD_OUTPUT/plate.sc"
"$SOLIDCRAFT_CLI" eval "$CAD_OUTPUT/plate.step"
"$SOLIDCRAFT_CLI" snapshot "$CAD_OUTPUT/plate.sc" --out "$CAD_OUTPUT/plate.png"
```

独立解析预期：体积 `(40*30 - π*3²)*H`；包围盒 `[-20,-15,0]` 至 `[20,15,H]`。H=12 时体积约 14060.707993 mm³；将 H 改为 14 后约 16404.159326 mm³。该小例子的相对体积阈值为 `1e-4`，包围盒绝对阈值为 `1e-4 mm`，不是项目级加工公差。用 MCP 改参、保存后独立重开，并为原件篡改、错误工具版本/程序摘要、对象或条件变化准备可保留的拒绝记录。

实际使用的成功、失败、修订与用户纠正通过现有 Project 记录和 `project memory-export` 回流；需要经验复用时进入 engineering-memory，保留来源、条件、同内核依赖及未测范围。安装或小例子成功不自动晋升跨项目方法。
