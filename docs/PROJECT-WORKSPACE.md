# 项目文件体系与会话恢复

状态：**源码实现候选**。本文说明新增的 `project workspace` 合同；实际生成、编译、回归、安装和独立接收仍须以本轮[验证记录](UNIFIED-VERIFICATION.md)及实际程序为准。新增命令不自动出现在旧安装中。

工程项目的目标、对象、条件、依据和当前进度保存在项目自己的文件体系中。上下文压缩、换会话或换 Agent 后，先从这些记录恢复，再确定下一步。当前 Project 仍是工程事实与决定的正本；workspace 组织原件、修订、执行记录和恢复检查，memory 保存可回查、带适用条件的经验。

## 三个目录的身份

| 目录 | 显式入口 | 职责 |
|---|---|---|
| 软件安装根 | `--root "$ONTOLOGY_ENGINEERING_ROOT"` | 源码、生成合同、规则、实际程序与安装身份 |
| 当前项目数据根 | `--project-home "$OEV2_PROJECT_HOME"` | 当前项目的 Project、目标锚定、来源、工作记录、回执与提交；建议为该项目的 `.engineering/` |
| 记忆库 | `memory --home /absolute/shared-memory` | 经验修订、召回与实际使用反馈；未显式指定时沿原安装根的 memory 默认规则 |

先根据用户任务和已有原件明确 Project ID，再指定数据根；不从 cwd、目录名或 profile 猜测项目身份。数据根使用明确的真实绝对目录，不能把项目目录伪装成 Skill 安装或通过软链接绕过来源检查。未传 `--project-home` 的原命令保持原安装根行为，不自动迁移旧数据。新 workspace 命令要求显式数据根。

使用 `--project-home` 后，受支持的 Project、Harness、kernel、`knowledge experience` 和 memory 接续入口按各自合同读取项目材料；软件来源身份仍在安装根。项目引用中的相对路径以明确数据根解释，不搜索两个根，也不隐式删除路径中的 `var/`。安装、生成和发布命令不能借此改换软件根。工具的外部原件、模型、原生 SDK 等仍遵守原 owner 的读取与执行合同。软件 bundle、规则、shape 和 query 默认仍从安装根读取；其余 knowledge 语料命令不接受 `--project-home`。数据根不能是安装根或其祖先；位于安装根内部时仅允许其 `var/` 下的严格子目录。

## 持久结构与当前提交

初始化创建基础目录；具体 Project、anchor、输入和回执文件由 Agent 按真实任务建立：

```text
工程项目/
└── .engineering/
    ├── manifest.json             明确的 workspace 和 Project 身份
    ├── HEAD.json                 唯一当前提交引用与序号
    ├── commits/<commit-id>/      完整提交；冲突候选保留自己的记录
    ├── artifacts/<sha256>        已核来源的原字节归档
    ├── runs/                    工具、Harness 与执行结果
    ├── views/                   派生的恢复阅读视图
    └── …                        当前任务明确引用的 Project、anchor 和其他原件
```

一次提交引用完整 Project、完整路线锚定、请求来源、纠正义务、事件、因果主张、阶段及必要工作/检查点。它保存当前完整状态，不能用新增一句“已完成”覆盖旧原件。提交时按 `expected_parent` 比对当前 HEAD，锁内再次重核来源，再切换当前提交。并发提交冲突会保留候选，返回 `conflict`；文件存在不表示它已经成为当前状态。Agent 读取最新 HEAD，核对差异后准备新的提交，不手改 HEAD 消除冲突。

原请求列表、事件及纠正义务接续旧记录。更改根对象、交付物或已有验收内容需要新的有源纠正或批准记录；不能覆写历史请求。Project 变化沿既有 `reconcile` 计算影响，未复核决定作为 `pending_decisions` 接续；依赖删除不能擦除旧影响，待复核决定不能关闭相关验收。

## 原请求、纠正与因果

每条 route request 都绑定一个 `WorkspaceRequestSource`：`request_id` 指原 anchor 项，`source` 是完整 UTF-8 原话文件的 `ArtifactReference`。文件字节必须精确等于该项 `text`；整份聊天中包含相似句子不足以满足这一合同。它证明所保存原话的字节一致，不独立认证发言者身份。原消息角色、时间、parent 和归档范围需要时沿[原消息方法来源](CONVERSATION-METHOD-USE.md)核验；历史消息不取得本次执行授权。

用户纠正同时有明确义务，`acknowledged`、`pending_review`、`resolved`、`deferred` 分开保存。受影响验收沿原 anchor 的 `affects`，不在另一张表重新定义。`resolved` 当前限定为 anchor `/2` 已绑定决定的纠正：必须覆盖受影响验收所列决定，并提交既有 `reconcile-reviewed` 请求与结果。原 owner 重新执行，核当前 Project、完整检查和真实回执后，才可记录已解决。后续 stage 声称推进过某项验收，不能替代这个闭环。没有对应决定或复核回执的纠正保持待办；明确延后须记录原因，不能删除义务。

事件保留来源、验收关联和向前已存在事件的 `corrects`。因果主张沿用 memory 的 `sequence`、`attribution`、`mechanism`、`controlled_comparison` 四种依据层级，并保留原件、混杂因素和未知。工具原件使用原 `ArtifactReference`，不伪装成聊天的 `SourceAnchor`。Project 的依赖边表示变更影响；时间先后和依赖闭包都不能直接证明物理因果。

## 接入、恢复与阶段推进

以下是参数形式，Agent 应替换为已确认的实际身份。用户无需填写这些内部合同：

```sh
export ONTOLOGY_ENGINEERING_ROOT="/absolute/OntologyEngineering"
export OEV2_PROJECT_HOME="/absolute/engineering-project/.engineering"
export OEV2_BIN="$ONTOLOGY_ENGINEERING_ROOT/var/install/bin/oev2"

"$OEV2_BIN" --root "$ONTOLOGY_ENGINEERING_ROOT" --project-home "$OEV2_PROJECT_HOME" \
  project workspace init --workspace-id ACTUAL-WORKSPACE-ID --project-id ACTUAL-PROJECT-ID

# inputs/commit-001.json 由 Agent 根据真实 Project 和来源准备。
"$OEV2_BIN" --root "$ONTOLOGY_ENGINEERING_ROOT" --project-home "$OEV2_PROJECT_HOME" \
  project workspace commit --input inputs/commit-001.json

"$OEV2_BIN" --root "$ONTOLOGY_ENGINEERING_ROOT" --project-home "$OEV2_PROJECT_HOME" \
  project workspace resume
```

新会话、压缩后和阶段切换时，必须读取实际 HEAD 并运行 `resume`，核对根请求、完整验收、最近纠正、`pending_decisions`、当前阶段和来源。`resume` 重核当前原件、路线和任务视图；历史归档不会在原件缺失时偷偷顶替当前来源。`ready` 表示本轮恢复检查可继续组织工作，不表示未完成验收或纠正已解决。Agent 根据返回义务选择本阶段要推进的事项。

| 恢复/提交结果 | Agent 后续行为 |
|---|---|
| `ready` | 读取完整目标与待办，核下一动作确实推进根验收 |
| `incomplete` | 补齐当前来源或明确缺项，不声称完整恢复 |
| `needs_review` | 处理路线偏移与相关纠正，再重核 |
| `effect_unknown` | 先回读已派发动作的实际状态，不自动再次执行 |
| `conflict` | 保留候选，读取新的当前提交并明确合并；不覆盖别人提交 |

阶段执行状态为 `planned → dispatched → results_recorded`；已派发但结果不确定时保留 `effect_unknown`，经回读再取得记录。尚未派发的阶段可以 `cancelled`。结束当前阶段后才建立新阶段；未完成的外部动作不能因换会话或换阶段从记录中消失。

`begin` 是只读前置检查。Agent 将本次 `resume` 返回的 `head.commit` 原样存为 ArtifactReference JSON，再检查阶段：

```sh
"$OEV2_BIN" --root "$ONTOLOGY_ENGINEERING_ROOT" --project-home "$OEV2_PROJECT_HOME" \
  project workspace begin --expected-head inputs/expected-head.json --stage ACTUAL-STAGE-ID
```

核验通过不会启动工具。Agent 通过新 commit 记录 `dispatched` 后，在当前授权范围内调用专业工具；回读实际结果并提交下一状态。恢复遇到 `dispatched` 也按效果未知处理，避免执行已经发生却尚未来得及写结果的动作。失败和部分结果可以保存，仍须分别说明验收是否推进。

`results_recorded` 需要 `WorkspaceStageReceipt` 包装的实际原件。本版支持以下 owner：

- `tool_result`：核原 registry、实际程序与原始记录，分别绑定执行前 Project 和当前 Project；原结果的 `route_stage` 与验收须对应本阶段。登记后新增证据或修订可被识别，对象、条件、来源和目的仍需核对。失败/partial 的真实记录保留其执行状态。

米思米选型回执也沿这条 workspace 边界保存。`supplier selection-review` 的输出是阶段回执和候选/决定的来源证据；Agent 将其完整字节作为 ArtifactReference 绑定当前 Project/work、对象和验收，再由 `workspace commit` 保存。不要在安装根或项目旁建立第二张选型状态表。原件、官网配置、工况、接口或型号变化时，经 `reconcile` 将受影响决定置为 `pending_decisions`；旧回执保留其历史身份，不能继续作为当前选型依据。
- `harness`：用既有 `load_step` 核完整附件、真实 task ID 与当前完整 Project 快照。`recorded` 不替代工程验收。

`exploration` 阶段接收 `project explore advance` 保存的 `oev2.exploration-host-state/1`。原探索 owner 只读重建冻结输入、请求与回读历史，并核结果、Project/work、当前任务视图、评估及完整 Harness 归档；接收不调用 `advance`，不写新运行或派发工具。

阶段 `id` 须等于 `exploration_id`；该阶段每个验收 ID 对应的当前 anchor 原文须在冻结 work 的完整验收中。回执 `project` 绑定原始执行 Project，当前 workspace 提交的 Project 则须精确等于 `state.run.project`，work 接续 `state.run.work`。提交的 `sources` 保留原来源并追加 `state.run.result`，以供当前任务视图核对新增证据；只有回执归档而未声明当前证据时，恢复仍可返回 `incomplete`。`registry` 为 `null`，`artifact` 引用 host 输出目录的 `completed.json`，不是 CLI 标准输出、`run/completed.json` 或直接 `explore run` 的结果。

存在待执行动作或 `requires_host_agent` 时不能记为 `results_recorded`。原件齐全的 `needs_review` 可记录，失败、partial、unknown 和全部剩余验收继续保留，不因此授予工程采用。Harness 未形成可核完整归档时保留缺项，不伪造通过回执。时间与外部报告保留其声明身份，回读不独立认证工具或物理结果。

探索阶段的 `resume` 另含 `reasoning`，由原 owner 从同一 Project、冻结关系及完整历史重新派生，与 `project explore reasoning` / `advance` 使用相同实现。它保留动作对应的验收、假设、未决前提、前置结果与潜在决定影响；其 `needs_review` 可以与 workspace 的恢复 `ready` 同时存在，前者说明结果尚需研究，后者只说明恢复检查完整。其他 owner 的原恢复结构保持。保存的视图不是新增项目正本，也不自动触发动作或更新决定。

独立项目目录下，探索的 owner schema 从软件安装根核对，冻结记录以 `owner_schema_sha256` 固定其身份；项目不用复制 `specs/`，同名项目文件不能覆盖软件规范。旧版安装根与项目根相同的冻结记录继续核其原有来源闭包。

## 派生阅读入口与宿主接续

可用 `resume --output views/resume-001.md` 写新的完整阅读视图。输出不能覆盖已有文件；每次视图标记对应 HEAD，阅读后继续操作仍重新运行 `resume`。当前视图包含完整请求、验收和待办，不作任意有损摘要；超预算明确失败。视图不成为第二份 Project 正本。

宿主接入时，在**该工程项目**的 AGENTS.md 或相应宿主配置中保留以下接续规则，并配置真实安装根、数据根和明确 Project ID：

```text
本项目使用工程本体 v2；Project ID 与 .engineering/manifest.json 显式绑定。
开始、压缩接续及新阶段前，使用已配置的安装根和本项目数据根运行：
oev2 --root "$ONTOLOGY_ENGINEERING_ROOT" --project-home "$OEV2_PROJECT_HOME" project workspace resume
先读根请求、完整验收、用户纠正、pending_decisions 与当前阶段，再选择下一步。
effect_unknown 先回读；历史记录和 ready 不扩大当前动作授权。
所用安装必须具备 project workspace；环境变量在该宿主按实际目录配置。
```

软件可检查经过这些入口的版本、来源和状态。该机制没有安装自动压缩 hook，也不保证宿主或 Agent 必然读取文件；不能把方法要求写成已实现的全局自动拦截。宿主入口、新会话实际恢复及用户项目 AGENTS 的接入要分别验收。仅复制目录或加载 Skill 名称不足以证明接续已生效。

## RRSI 与 memory

日常保存事件和实际使用反馈。发现可复现的方法缺陷时，先判断现有实例修正是否足够，再建立有限 RRSI 候选。恢复本身不自动触发方法晋升。

```sh
"$OEV2_BIN" --root "$ONTOLOGY_ENGINEERING_ROOT" --project-home "$OEV2_PROJECT_HOME" \
  project workspace learning-goal --output inputs/learning-goal-001.json

# 原 experience-freeze 请求的 goal 指向上一步真实产物，评估合同绑定其摘要。
"$OEV2_BIN" --root "$ONTOLOGY_ENGINEERING_ROOT" --project-home "$OEV2_PROJECT_HOME" \
  project workspace learning-freeze --input inputs/learning-freeze-001.json --output runs/candidate-001

# 把 freeze 返回的 binding 原样保存为 ArtifactReference JSON，再在接续试验前核验。
"$OEV2_BIN" --root "$ONTOLOGY_ENGINEERING_ROOT" --project-home "$OEV2_PROJECT_HOME" \
  project workspace learning-check --input inputs/learning-binding-ref.json
```

新 goal 同时绑定当前完整 Project、完整 anchor 和全部有序验收项；`learning-freeze` 重读当前状态，检查候选的 Project、对象、条件和来源，再调用既有 experience freeze，并保存 `workspace-binding.json`。压缩接续后可运行 `learning-check` 获取显式诊断；绑定候选的正式 `assess`、`decide`、`apply` 与 registry 使用入口会在既有评估/登记前重核同一完整根目标、HEAD、原件和 workspace 锁，不能靠遗漏 `learning-check` 绕过。`root_consistent` 只说明该绑定未变，试验本身仍由原 experience owner 验证。目标变更、纠正或当前来源变化须重新建立相应基线，不能沿用旧 goal 字节宣称根目标未变。原 experience 的有限评估、试用、拒绝、使用反馈与恢复语义保持，见[有证据的改进](../references/regularized-evolution.md)。本层没有扩展任意方法评估器或自动 promotion，`method_benefit` 保持未测。

共享 memory 继续通过原 `session prepare/handoff` 与 Project 候选、反馈接口接续。记忆提示是待核经验，不能取代本项目当前 HEAD 或关闭验收；项目原件与共享经验各保留精确修订。`--project-home` 不等于选择共享 memory home，多个客户端共享库仍须显式指定同一目录。

## 预算、迁移与验收边界

本层每份来源最多 16 MiB、累计来源最多 64 MiB；恢复 Markdown 最多 16 MiB。`workspace-commit-request/1` 保留一次提交或回读最多 256 个不同引用路径；显式 `/2` 支持最多 1024 个路径，提交封装也使用 `workspace-commit/2`。路径按真实引用计数，不按相同内容摘要合并；字节、哈希、链接、父提交及完整义务检查不变。旧合同不会自动采用新预算，新请求不能用旧封装保存；降回 `/1` 仍需满足旧预算。历史原件不改写。

`/2` 的恢复使用 `current-task-view-input/3` 和对应 `/3` 阅读视图，支持最多 1024 个清单路径及 64 MiB 来源字节。原阅读输入 `/1` 的 256 路径边界与独立的方法来源封装 `/2` 保持原义；检查视图时重新派生并核对精确版本。当前提交向前核验最多 1024 步，历史提交原件累计最多 64 MiB，超限拒绝；尚无自动历史分段或压缩迁移。原 Harness、工具、reviewed-reconciliation 和 learning owner 仍保留自己的更细预算，重核可能多次读取同一原件。这些限制不是完整工程项目大小或全任务内存承诺；超限明确返回缺口，不截掉纠正、失败或必要验收取得通过。

当前 workspace 的原子提交依赖 Unix 文件锁、目录同步与原子重命名，本轮实际验证平台为 macOS；Linux 需在目标机重跑公开案例，Windows 的提交路径明确返回 unsupported，尚不能计作可用。

分发软件不包含用户 workspace 或共享记忆库。迁移用户项目时保留该项目完整数据、相对引用和历史提交，再显式选择新真实数据根；不能只复制 RESUME。工具 registry 中的实际程序、来源、原生环境和权限在接收机器重新核验，旧安装身份与通过回执不授予新机器资格。旧 `var/projects` 项目通过明确导入接入，未实现自动搬迁或隐式修订升级。

本轮应分别验证：完整冷恢复；遗漏纠正/验收拒绝；同修订条件变化及旧来源拒绝；并发 HEAD 冲突；派发后中断不重复动作；原生回执重读；旧 goal 不能绑定新根目标；新环境中独立显式命令可用。软件合成反例、真实新会话恢复、跨平台接收和未见任务 RRSI 收益分别报告。

可复跑的公开完整案例见 [project-workspace 示例](../examples/project-workspace/README.md)。


## Explicit extended lossless source profile

Workspace request/commit `/4` explicitly selects 2048 checked source paths and
128 MiB retained lossless payload. Each original remains limited to 16 MiB and
read with its exact raw digest; at most 32 GiB logical source IO is possible.
Raw archives and original paths stay unchanged. Payload accounting is operation
local and is not a total process memory limit. Resume selects task-view input/view
`/5` and reports `oev2.workspace-source-cache/2` with actual and maximum usage.

Earlier request/commit `/1` (256 paths), `/2` (1024 paths, raw 64 MiB), and `/3`
(1024 paths, cached 64 MiB), and all previous task-view profiles keep their exact
semantics. History remains limited to 1024 commits and its existing 64 MiB budget;
receipt budgets are independent. This is finite headroom, not indefinite growth.

The operator opts in through a new native commit after validating old-state
recovery with the candidate owner. Old executables are retained and must reject
unknown new schemas. Downgrading cannot bypass smaller path or byte limits.
This profile grants no CAD capability, scheduling authority or engineering adoption.


## Explicit large extended lossless source profile

Workspace request/commit `/5` explicitly selects 4096 checked source paths and
256 MiB retained lossless payload. Each original remains limited to 16 MiB and
read with its exact raw digest; at most 64 GiB logical source IO is possible.
Resume selects task-view input/view `/6` and reports
`oev2.workspace-source-cache/3` with actual and maximum usage.

Request/commit `/1` through `/4` and task-view `/1` through `/5` keep their exact
semantics and bounds. History remains limited to 1024 commits and its existing
64 MiB budget; receipt budgets are independent. This is finite headroom.

The operator opts in through a new native commit after the candidate owner
recovers the unchanged live state identically to the previous owner. Old
executables are retained and must reject the unknown `/5` schema. Downgrading
cannot bypass smaller path or byte limits. No CAD capability, scheduling
authority or engineering adoption is granted.

## Explicit history capacity profile

Workspace request/commit `/6` keeps the `/5` source profile and raises the
history budget. Every resume and commit reads the complete history from the
head: at most 1024 commits, 256 MiB of commit bytes and 256 MiB of archived
Project and review bytes. Request/commit `/1` through `/5` keep 1024 commits and
64 MiB. Resume reports `history_budget` for a `/6` head; legacy output is
unchanged.

Resume uses the head's own profile. Commit validates the parent chain under the
new request's profile, so a `/6` request may follow a `/5` head whose history
already exceeds 64 MiB, while a `/5` request cannot carry such a history. Commit
refuses before writing any file when the new head would exceed its own budget,
and history budget failures are reported by name. Each commit keeps the complete
state, so history grows with every commit; this is finite headroom, not
compaction. Old executables are retained and reject the unknown `/6` schema. No
CAD capability, scheduling authority or engineering adoption is granted.
