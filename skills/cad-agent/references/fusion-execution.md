# Fusion 执行面:一次性 fusion-mcp 调用(cad-agent 执行模块)

> Historical/explicit legacy route only. Default CAD work uses nx-execution.md, and current CLI guards refuse this route unless CAD_AGENT_LEGACY_CAD=explicit is intentionally set for a user-requested legacy task.

以一次性子进程调用 Fusion,不依赖 MCP server 注册:每次调用 spawn 一个全新
fusion-mcp cell(守卫代理)→ 执行一次受保护的工具调用 → 打印结果 → 进程退出。
没有常驻进程,没有会话残留,claude / codex / 任意能跑 bash 的 agent 通用。

## 架构(调用前必须理解)

```
本 skill(一次性调用) ─► fusion-mcp cell(守卫代理,进程即用即抛)
                              │ HTTP,仅 127.0.0.1
                              ▼
                     选定 Fusion 实例的内置 MCP 端点(默认 :27182)
```

- cell 不是可选包装:进程身份验真、Peekaboo 窗口证据、跨进程 owner lease、
  一调用一会话(成功恰好一次 DELETE,异常绝不 DELETE/重试)全在里面。
- **永远不要直连任何 Fusion MCP 端点**,不要绕过 cell 自己说 MCP。
- 语义问题通过父级 ontology-engineering 的 source-locked Semantica 入口执行。

多个 Fusion 窗口或多个 Agent 分工时，先读
[多实例与多 Agent 建模](fusion-multi-instance.md)：完整端点/PID/启动令牌选择才能
走多实例路线；每个 worker 还须绑定独立文档。窗口多开不自动等于并行隔离。

## 调用方式

```bash
SKILL_DIR=<cad-agent skill 目录>
python3 "$SKILL_DIR/scripts/fusion_call.py" <tool> '<json-arguments>'
```

工具只有四个:`fusion_mcp_read`、`fusion_mcp_execute`、`fusion_mcp_update`、
`fusion_mcp_electronics_read`。常用读取:

```bash
# 项目列表(最好的连通性烟测)
python3 "$SKILL_DIR/scripts/fusion_call.py" fusion_mcp_read '{"queryType": "projects"}'
# 最近文档
python3 "$SKILL_DIR/scripts/fusion_call.py" fusion_mcp_read '{"queryType": "document", "operation": "recent"}'
# 当前活动命令(null = 空闲,是健康结果)
python3 "$SKILL_DIR/scripts/fusion_call.py" fusion_mcp_read '{"queryType": "activeCommand"}'
# 视口截图
python3 "$SKILL_DIR/scripts/fusion_call.py" fusion_mcp_read '{"queryType": "screenshot", "width": 1024, "direction": "iso-top-right"}'
```

输出为 JSON:`classification`(见下面的「结果分类法」)+ `payload`(Fusion 的内层结果原样)
+ `raw_text`(`content[0].text` 原文,永远保留)+ `inner`(脚本 stdout 末行解析出的对象)。
兼容旧驱动的 `status` / `duration_ms` / `delivery_ack_confirmed` / `payload` 顶层键仍在。
退出码:**0 = 已送达**(还要读 `classification` 才知道成没成);**2 = 传输/会话失败或上游超时**;
**3 = 用法错误**;**4 = 传输说成功但没有可用结果**(`no_output` / `response_truncated`);
**5 = 命中 `--fail-on` 指定的分类**。
失败时 stderr 额外带 `--- cell stderr (tail) ---`,cell 自己的启动错误就在那里
(例如 `timeout_seconds must be finite and between 0 and 300`)。

**不要再自己解析 `payload`。** 用 `scripts/fusion_ops.py` 里已测的
`call_read` / `call_execute_script`,它们返回同一套分类。

### 结果分类法(`fusion_ops.TAXONOMY`)

每次调用都会得到 **恰好一个** `classification`,并且始终保留原文 `raw_text`。
这是本 skill 唯一的结果分类法,调用方指南引用同一张表:

| classification | 含义 | 退出码 | 允许重试 | 驱动应做什么 |
|---|---|---|---|---|
| `delivered_ok` | 送达且内层 `success=true` | 0 | — | 继续 |
| `inner_business_error` | 脚本/工具跑完,报告业务性失败 | 0 | 否 | 读 `inner`,按业务分支 |
| `inner_script_error` | 脚本抛异常(带 `trace`)或 Fusion 报 `success=false` | 0 | 否 | 修脚本;文档状态可能已部分改变,先回读 |
| `no_output` | **`success=true` 但脚本无输出 / 末行不是 JSON** | **4** | 否 | 当硬失败;先独立回读,再改脚本形状(长行!) |
| `response_truncated` | 响应超过 `--max-response-bytes`(默认 1 000 000) | 4 | 否 | 提高该上限或缩小查询 |
| `guard_transient` | upstream 之前的守卫拒绝(窗口普查等) | 0 | **是** | 等 45 s,起一个全新 cell 重试,上限 6 次 |
| `guard_rejected` | 其它 `isError=true` 的拒绝(含静态脚本拒绝) | 0 | 否 | 读 `raw_text`;改脚本或改环境,不要重试 |
| `upstream_timeout` | 已发出的请求超时 → **歧义** | 2 | 否 | 冻结日志,同 PID 不再发任何 MCP 请求,走歧义分诊 |
| `transport_failure` | cell 启动/传输失败 | 2 | 否 | 读 `cell_stderr_tail`(真因在那里) |

退出码 0 **不等于成功**,只等于"已送达"。判断成功的唯一依据是
`classification == "delivered_ok"`。
想让某些已送达的分类也硬停,用 `--fail-on guard_rejected,inner_script_error`(退出码 5)。

> 安装事实:作业工具族(`scripts/fusion_step_import.py` 等,共享 `scripts/_common.py`)
> 目前仍带自己的一套 kind 名与退出码,见下面的「作业工具族」。两套分类法并存,
> 映射关系写在那一节里;新代码一律用上表这一套。

## 供应商标准件获取(McMaster-Carr)

中文界面的入口与用户提供的原图见
[McMaster-Carr 入口图解](mcmaster-entry-guide.md)：
**实体 → 插入 → 插入 McMaster-Carr 零部件**。

外部标准件按零件号精确获取正版 STEP,走 Fusion 内嵌目录(CDP 驱动,
不碰文档、不绕过官方渠道):

```bash
python3 "$SKILL_DIR/scripts/mcmaster_fetch.py" probe           # 看 CDP 端点与对话框状态
python3 "$SKILL_DIR/scripts/mcmaster_fetch.py" open-dialog     # 守卫式打开插入对话框(需 Fusion 干净)
python3 "$SKILL_DIR/scripts/mcmaster_fetch.py" fetch --part 91290A115 --out ./mcmaster-downloads
```

现实提醒(2026-08-27):当前守卫策略把 `open-dialog` 的交互命令路径按
`InteractiveCommandInsideMcpTransaction` 直接拒绝——**对话框由用户在 Fusion
手动打开一次**(插入 → Insert McMaster-Carr Component),之后发现与下载全程自动。

**只知道品类/规格、不知道货号时**,先用发现面(只读 DOM,不碰文档):

```bash
# 站内搜索+分面点击(短分面适用, 如 screw-size)
"$SKILL_DIR/.venv/bin/python3" "$SKILL_DIR/scripts/mcmaster_discover.py" \
    search "belleville spring washers" system-of-measurement~metric screw-size~m12/
# 冷导航直达"类目+筛选"URL —— 绕过分面虚拟滚动的关键手段
"$SKILL_DIR/.venv/bin/python3" "$SKILL_DIR/scripts/mcmaster_discover.py" \
    cold "https://www.mcmaster.com/products/end-mills/mill-diameter~20-mm/"
```

输出候选货号+规格行,挑定后交 `fetch --part` 严格校验下载。实战规律
(行虚化/URL 分面语法/候选身份与 spec_delta/实际件与示意模型隔离等)沉淀在
`mcmaster_discover.py` 模块 docstring,改前先读。已用它为紫铜水冷板项目
取得四把真刀(Ø8.5 钻/Ø10/Ø20 立铣/Ø6 倒角)与力链五金正版 STEP。

取到真实供应商 CAD 不等于选型、装夹或替换通过；不得按同直径缩放或别名映射
到当前刀号。刀具身份或刃形改变时，材料、HUD、原生镜头及现用剪辑引用须一起
重验，不能新增演示片段却保留现用旧消费者。历史经验中的同族缩放建议已撤回。

输出 STEP 文件 + `*.provenance.json`(零件号/格式/官方 URL/SHA-256)。
要点:格式用严格全等匹配("3-D STEP" 永不误选 "3-D STEP no threads");
CAD 面板只在站内搜索(SPA 路由)后渲染,脚本已按此实现;下载后的导入、
约束与保存是独立的守卫步骤,按保存纪律执行。

## 保存纪律(智能体卡死的头号原因,重点)

**卡死链条**:文档有未保存修改 → 守卫按 fail-closed 拒绝调用,或 Fusion 弹出
模态保存对话框 → 之后所有 MCP 调用挂起/超时 → agent 盲目重试 → PID 被隔离,
任务彻底卡死。打断这条链的方法是纪律,不是重试。

1. **每次写操作(execute/update)之后立即显式保存并回读验证**,不要攒:
   ```bash
   # 保存当前文档
   python3 "$SKILL_DIR/scripts/fusion_call.py" fusion_mcp_execute \
     '{"featureType": "document", "object": {"operation": "save"}}'
   ```
   随后用 document 读取确认 `isSaved=true`、`isModified=false` 再继续下一步。
2. **任何未命名或从未保存的文档都是受保护工件**:`Untitled`、`无标题`、其他
   locale 等价名称,以及原生 `isSaved=false` 或 `dataFile=None`,任一成立即触发；
   这条规则与 `isModified` 无关。在做任何无关操作、清理工作台、close、退出/
   重启 Fusion、退役 PID 之前,先用独立 MCP 请求绑定 creation ID 与原生内容，
   从任务/模型语义生成可追溯名称；再用独立只读请求解析 exact project/folder 并
   证明不覆盖，最后只经 MCP/API `Document.saveAs` 首次保存。保存返回后先在 MCP
   外等待窗口 settle/census，再用全新 MCP request/session 回读 exact name、
   lineage、version、`isSaved=true`、`isModified=false` 和关键 native state。
   身份、名称或目标不唯一时 HOLD。禁止 GUI、快捷键或 UI 自动化首次保存；
   **绝不推断"可以丢弃"**，只有用户明确点名这份精确文档才能豁免。
   受保护的 `fusion_mcp_execute` 脚本必须在顶层且只声明一次
   `FUSION_MCP_UNTITLED_PRESERVATION_ROLE`，取值为 `inspect`、
   `destination-preflight`、`save-as` 或 `readback`；普通
   `fusion_mcp_read` 不需要该常量。该角色不代替脚本内 exact identity、
   project/folder、no-overwrite 和 native-state 守卫。
3. **close 必须带真实的用户确认标志**:`operation: "close"` 配
   `userConfirmedSaveAndClose` 或 `userConfirmedCloseWithoutSave` —— 这两个
   标志代表"用户确认过",agent 不许自己猜着填 true。没有确认就先保存(规则 1)
   或去问用户。
4. **怀疑有模态对话框挡着、调用超时或结果歧义时**:立即停止该 PID 的全部 MCP
   活动，包括 read、save、close、quit、session 新建/删除、截图查询和健康探针；
   只能用 Peekaboo/OS/AppLog 等 MCP 外部证据分诊。不得为“核对一下”再碰同 PID。
   **但在重启 Fusion 之前，先跑 `scripts/fusion_recovery.py evidence --latest`。**
   若 Fusion 的 AppLog 能证明这一个请求已经完成并存盘，重启没有任何安全收益。
   2026-09-06 的两次退休都属于这种情况，其中一次白白重启了一遍 Fusion。
   该工具只读 AppLog 与 marker，不向 Fusion 发任何请求，因此不违反本条。
5. **绝不在存在未闭环的保护义务时杀死/重启 Fusion**。正常重启前必须通过原生
   MCP census 确认：0 个 modified 文档，且 0 个无标题/从未保存文档；后者即使
   clean 也必须先完成规则 2。已经跨越 timeout/歧义边界的同 PID 禁止再保存，
   只能保留外部证据并 HOLD，不能借重启绕过。
6. 对已知脏文档做恢复性调用,走 cell 的 `--dirty-recovery-receipt` 一次性授权
   机制(见仓库文档),不要临时放宽守卫。一次性调用器可传
   `--dirty-recovery-receipt <owner-only-receipt.json>`，并仍使用同一 canonical
   safety state root。

## 作业工具族(大 STEP → 装配,无人值守)

一次性 `fusion_call.py` 是**单次调用**的正确形状,但一次几百个 occurrence 的
装配作业需要几十次调用串起来。不要再手搓驱动脚本:用 `scripts/` 里的工具族,
它们把 2026-09-05/06 手工做过的每一步都固化了。完整说明见
`references/runbooks/fusion-import-tools.md`。

```
fusion_preflight.py            作业前只读体检(不发任何 MCP 请求)
fusion_step_split.py           大 STEP 按族切块 + 绝对路径 manifest(不碰 Fusion)
fusion_document_f1.py          F1:重名检查 → 建空文档 → save-as → 新会话回读
fusion_step_import.py          按 manifest 逐块导入并在同一脚本内保存,可续跑
fusion_post_import.py          回读 / 用户参数 / as-built 关节 / 全组件普查 / close-reopen
fusion_supplier_occurrences.py 供应商 STEP 原件按 4×4 位姿放置
```

共同约定:

- **每块导入 + 保存写在同一个脚本里**,文档在两次调用之间永远不处于 modified 状态。
- **一次一个 cell**。工具在每次调用前 `pgrep` 检查,发现另一个 cell 就退出码 9。
- **只重试 `guard-transient`**(上游请求发出之前的守卫拒绝:窗口普查、辅助窗
  不唯一稳定、socket 巡检失败……)。`inner-error` / `timeout` / `guard-terminal`
  一律 fail-fast——超时之后同一 PID 不得再收任何 MCP 请求,这条铁律工具会替你守住。
- **回执**:每次调用留 `<stage>.py / .args.json / .result.json / .stderr / .exit /
  .receipt.json`;作业级留 `preflight.json`、状态文件和 `*.receipt.json`。
- **`--dry-run`** 把全部脚本渲染出来并逐个过 `reject_unsafe_execute_script`,
  一次 Fusion 都不碰。真开跑前先 dry-run 一遍。

### 工具族自带的 kind 名(与 `fusion_ops.TAXONOMY` 的映射)

安装事实:`scripts/_common.py` 的 `classify_call()` 仍是 D 落地时的本地实现,
尚未替换为 `fusion_ops` 的分类法(其 docstring 也仍自称 LOCAL PLACEHOLDER)。
读工具族的 `.receipt.json` 时按下表换算:

| `_common` kind | 退出码 | 对应 `fusion_ops.TAXONOMY` |
|---|---|---|
| `ok` | 0 | `delivered_ok` |
| `inner-error` | 3 | `inner_business_error` / `inner_script_error` |
| `script-no-output` | 4 | `no_output` |
| `guard-transient` | 6 | `guard_transient` |
| `guard-terminal` | 8 | `guard_rejected`(终局子集) |
| `transport-failure` | 2 | `transport_failure` |
| `timeout` | 7 | `upstream_timeout` |
| `refused-busy` | 9 | (工具族独有:另一个 cell 在跑) |

两侧对「什么能重试」的判断一致:只有 `guard-transient` / `guard_transient` 可重试。

### 三条按构造保证的传输规则(对应三次「看着成功、其实没跑」)

1. **字面量必须 ASCII-only**。非 ASCII 路径写成 `\uXXXX` 转义;工具用
   `py_str()` 渲染并用 `assert_transport_safe()` 强制。原始 UTF-8 路径会让
   `createSTEPImportOptions` 报 `3 : The selected file does not exist.`。
   注意别二次转义:`py_str()` 内部已经转过,再包一层 `ascii_escape()` 得到的是
   字面反斜杠,同样打不开文件。
2. **生成的字面量必须多行、每行短**(`json.dumps(..., indent=1)`)。
   单行上限有两档,都是安装事实:`fusion_ops.MAX_SCRIPT_LINE = 2000`(通用调用路径),
   `_common.MAX_SCRIPT_LINE = 400`(作业工具族,更严)。
   一个 ~7 KB 的单行 `PARAMS` 让 Fusion 2705 返回 delivered / success=true /
   message "" / 29 ms —— 脚本根本没执行。
3. **「空 message + ~30 ms」= 没跑,不是成功**。工具把它分类为 `script-no-output`
   并 fail-fast;通用调用路径分类为 `no_output` 并退出码 4。人工判读时同理。

### 切块尺寸

作业工具族把 `FUSION_CAD_TIMEOUT_SECONDS` 与 `FUSION_CAD_STARTUP_TIMEOUT_SECONDS`
都设成 `_common.GUARD_CALL_CAP_SECONDS = 300`,并且**不传 `--long-request`**,
所以它们的单次调用上限就是 300 s。(激活预算截断首个请求的旧缺陷已由 B 的补丁修掉,
见下面的「调用预算与主机休眠」;但工具族的 300 s 上限是它自己的选择,不受影响。)
Fusion 2705 实测 `importToTarget`:40 个简单车架体 ≈ 10 s;10 个带盲槽的侧板 ≈ 25 s;
168 个链节类实体 = 404 s(超时)。
`_common.max_bodies_for_budget()` 给出的默认是 **20 体/块**:线性模型本身允许 ~43,
但每体成本随目标文档的 occupancy 上升,而**一次 300 s 超时的代价是重启 Fusion**,
所以取实测跑通的那个切法(`fusion_chunks_small20`)。要更大就显式 `--max-bodies`。
确实需要单次超过 300 s 时,不要改工具族,直接用
`fusion_call.py --long-request` 发那一个调用。

### 续跑

失败或 Fusion 重启后,先做一次只读回读,再把结果喂给导入器:

```bash
python3 scripts/fusion_post_import.py --doc-name D --lineage L --out-dir ./job --stages readback
python3 scripts/fusion_step_import.py ... --present-from-readback ./job/A7_readback_geometry.result.json
```

present-skip 三态:整块都在 → 跳过;一个都不在 → 导入;**只在一部分 → 立即停
(退出码 7)**。部分存在意味着 manifest 与文档对「切法」的理解已经不一致,
再导会产生重复 occurrence,永远不自动修复。

### 关节普查要走 allComponents

导入 chunk 之后,as-built 关节建在 **chunk 包装组件**内部,只数
`rootComponent.asBuiltJoints` 会得到误导性的 0。用
`fusion_post_import.py --stages census`,它遍历 `design.allComponents`。

## 其他铁律

1. **单次调用的正常耗时取决于 CAD 工作量,不是固定值**。只读与小改约 8-20 秒;
   一次 40 体 STEP chunk 导入 2026-09-06 实测 p50 64 秒、p90 141 秒;一次 168 体
   导入实测 404.6 秒。**不要按「超过 20 秒＝卡死」判断。** 预算表见「调用预算与
   主机休眠」。这段时间里守卫层的窗口验真会连续做 2–3 次浅层普查直到「窗口签名
   视图」稳定;同一个 Fusion PID 只在第一次调用时抓一次辅助窗口截图,之后复用
   `~/.local/state/cad-agent-fusion-mcp/local/fusion-mcp-auxiliary-<uid>-<pid>-*.json`
   里的证明。看到 `known_nonblocking_auxiliary.proof_source` 从 `screenshot`
   变成 `cached-per-pid-signature` 是正常的。
2. **超时/退出码 2/结果歧义之后,同一 Fusion PID 不得再收到任何 MCP 请求**。
   上游结果可能已提交；禁止只读确认、重发、save/close/quit、健康探针、建立新
   session 或 DELETE。冻结现有日志，仅用 MCP 外部证据按
   `references/runbooks/fusion-mcp-ambiguity-triage.md`(本 skill 内) 分诊。
   该 runbook 的恢复路径 A(AppLog 完成证据)**不向 Fusion 发送任何请求**,只读日志与
   marker,因此不违反本条;它是重启之前必须先走的一步。状态不能证明时写 `UNKNOWN` 并 HOLD。
3. **写操作前先读**:确认目标文档正确、无无关的未保存修改。
4. 同一 Fusion PID 的调用仍由 owner lease 互斥；不要同时投递多个写请求。
   不同 PID 的普通 `fusion_ops.py` 调用可按显式实例路线分工；F1/`_common` 工具
   仍有全局 busy 检查，先串行完成创建与保存。见[多实例调度边界](fusion-multi-instance.md)。
5. Fusion 没开时调用会在守卫层失败(退出码 2),这是预期,不是 bug。
6. 同一执行节点的所有生产 cell 必须共用一个稳定、owner-only 的
   canonical safety state root。不得按项目、attempt 或单次请求更换
   `--state-directory`,否则新 cell 会看不到旧 PID 的 in-flight/retirement marker。
7. **窗口普查期间不要求你停止使用电脑。** 稳定性签名使用
   「标题 + 属主 PID + 尺寸类 + 是否主窗口」，不把窗口原点和 CGWindowID 当作
   跨普查稳定签名；Space 动画会改变原点。显式多实例路线还逐个核对实际 CGWindowID
   的 OS 属主，这与稳定性检查是两层证据。其他应用遮挡 Fusion 本身不要求抢回前台。
8. **优先 Peekaboo 后台只读观察，不打断用户前台。** 默认不 activate、bring-to-front、
   抢焦点或切 Space；后台失败不能自动改用前台操作。`local` 模式不保证任意 UI 动作
   都支持后台，版本能力与已验证边界见[多实例说明](fusion-multi-instance.md)。

## 调用预算与主机休眠

### 预算表

一次 `fusion_call.py` 调用有四层嵌套计时器,全部基于 `time.monotonic()`,必须**内紧外松**:

| 层 | 参数 | 普通 | `--long-request` |
|---|---|---|---|
| 激活(进程发现 / owner 租约 / 窗口普查) | cell `--startup-timeout-seconds` | 240 s(硬上限 300) | 240 s(硬上限 300,**不随操作预算变大**) |
| 操作(upstream initialize + 选定调用) | cell `--timeout-seconds` = T | 240 s(硬上限 300) | 最高 1800 s |
| acceptance client read timeout | 由 T 派生 | T + 90 s | T + 90 s |
| `subprocess.run` 硬杀 | 由 T 派生 | T + 210 s | T + 210 s |

- **激活预算只管激活**。它不再截断首个请求(旧实现会,这正是 2026-09-06 17:18 事故的成因)。
- **外层必须比内层宽至少 90 秒**。两者相等时外层先赢,把可分诊的 `TimeoutError` 变成歧义的
  `CancelledError`。`fusion_call.py` 现在自动派生外层预算,不要手工把它们设成一样。
- 需要超过 300 秒时**必须**加 `--long-request`,不要试图把普通预算调到 300 以上(会在构造期
  `ValueError`)。判据:预计导入体数大于约 80,或同型历史 `duration` 曾超过 200 秒。

        scripts/fusion_call.py --long-request --timeout 900 fusion_mcp_execute '<args>'

  `--long-request` 只放大预算,不改变任何安全性质:仍然只发一个请求、仍然写 in-flight marker、
  仍然绝不重试。

### 主机休眠

macOS 上 `time.monotonic()` 等价于 `CLOCK_UPTIME_RAW`,**休眠期间冻结**;`CLOCK_MONOTONIC` 才是
连续的。因此休眠不会缩短预算,但会让请求在唤醒后才被 Fusion 处理,唤醒后剩余的清醒预算往往
不够,于是客户端在 Fusion 即将成功前到期。

1. **长任务期间不要合盖,不要手动休眠。** 2026-09-06 的四次休眠全部是 `Clamshell Sleep`(合盖)。
2. **`caffeinate -i` 不阻止合盖休眠。** `caffeinate -i` / `PreventUserIdleSystemSleep` 只阻止 idle
   休眠;`caffeinate -s` / `PreventSystemSleep` 也不阻止合盖,且仅在交流供电下生效。不要把
   caffeinate 当作充分缓解。
3. **cell 会在请求在飞期间自动持有电源断言**(IOKit `IOPMAssertionCreateWithName`,失败则退回
   `caffeinate -i -m -w <pid>`),并在请求结束时释放。`--no-power-assertion` 可关闭。
   验证:请求在飞时 `pmset -g assertions` 应能看到 `fusion-mcp guarded request (...)`。
4. **休眠仍然发生时,cell 会检测到并延长预算而不是取消请求**,stderr 打印
   `HOST_SLEEP_DURING_REQUEST`,退休 marker 的 `host_sleep` 字段会记录累计休眠秒数与次数。
   延长有上限(`sleep_guard.DEFAULT_SLEEP_EXTENSION_ALLOWANCE_SECONDS`,默认 1800 秒),
   耗尽后仍按普通超时退休。`--no-sleep-resilience` 可关闭该检测。
5. 真要在系统层禁止合盖休眠,只有 `sudo pmset -a disablesleep 1`(系统级、需要 sudo、影响所有
   应用),用完记得改回 `0`。这不是默认做法。

## Peekaboo 模式选择规则

守卫支持三种模式(`FUSION_CAD_PEEKABOO_MODE` / `--peekaboo-mode`),验真强度和
故障面完全不同:

| 模式 | peekaboo argv | 依赖 | 典型故障 |
|---|---|---|---|
| `local` | 每条命令加 `--no-remote`,进程内完成 | 只依赖已签名的 CLI 与 TCC 授权 | 最少 |
| `local-ondemand` | `--bridge-socket ~/Library/Application Support/Peekaboo/bridge.sock` | 必须有一个**钉死七参数**的常驻 daemon:`peekaboo daemon run --mode manual --bridge-socket <sock>` | socket 陈旧/无监听者;daemon 参数不符;每条命令前后各一次 lsof+libproc 括号证明,叠加共享截止时间后容易整体超时 |
| `bridge` | 不加 `--no-remote`,socket 取自 `bridge status` | 必须有 Peekaboo **GUI app** 在跑(`hostKind: gui`) | 无人值守时不可靠 |

**各入口的默认值不同,显式多实例路线固定使用 `local`**:

- `scripts/fusion_call.py`(通用一次性调用)默认 `FUSION_CAD_PEEKABOO_MODE=local-ondemand`;
- `fusion_document_f1.py` 的 CLI 默认是 `auto`；有完整实例选择时将它收窄为 `local`，
  各阶段保持同一端点/PID/启动令牌，其它模式不作自动 fallback。
- 其它作业工具的默认值以各自 `--help` 为准；不要把一个入口的默认值套用到另一个入口。

**推荐值是 `local`。** 理由:活动部件最少(没有 daemon 生命周期、没有 socket、
没有陈旧 socket 这一整类故障、没有每条命令的 lsof 括号证明),实测最快,而且
V018/V020 整轮导入实际就是靠它跑完的。无人值守作业请显式设
`FUSION_CAD_PEEKABOO_MODE=local`。`local-ondemand` 只在探测健康**且**确实
需要 daemon 的 window tracker 时才用;`bridge` 不要用于无人值守。

### ≤3 s 自动选模

默认单实例路线的 `fusion_preflight.py`(以及工具族的 `--peekaboo-mode auto`)按下面的顺序在一个墙钟预算里选
(`--probe-budget`,默认 3.0 s):

1. 一次 `codesign --verify --strict --requirements`(identifier `boo.peekaboo.peekaboo`,
   OU `Y5PE65HELJ`)+ `codesign --display --verbose=4` 取 CDHash;不通过就没有任何
   模式可用。
2. `local-ondemand` 先做**零成本前置**:socket 存在且权限恰为 0600/nlink 1、
   `lsof` 能报出监听者、`ps -ww -o args=` 恰好是那七个参数的 manual daemon。
   任一不满足立即跳过并给出确切补救(`peekaboo daemon stop && peekaboo daemon start`)。
3. 每个候选模式跑它自己**最便宜的正向证据**:
   `peekaboo list windows --pid <fusion> --include-details off_screen,bounds,ids --json`
   加上该模式的开关,要求 `success == true`、`data.windows` 非空,
   并且 `debug_logs` 里的 `Runtime host:` 与模式相符
   (`local (in-process)` / `remote onDemand via …`)。
4. 健康者按 `local > local-ondemand > bridge` 取第一个。

显式多实例路线只验证 `local`，并增加精确返回 PID、CG 窗口归属及前后实例身份检查；
检查失败即拒绝就绪，不降级到其它模式，不为此激活应用。

本机实测三个模式探完 **0.6 s**(local 0.10 s / on-demand 0.32 s / codesign 0.16 s)。

> **硬规则:探测不得改变被探测的状态。** 永远不要在既没有 `--no-remote`
> 也没有 `--bridge-socket` 的情况下调用任何 peekaboo 子命令。那种形状会按需拉起
> `daemon run --mode auto --bridge-socket <sock> --idle-timeout-seconds 300`:
> 它的 argv 是 9 项且是 `auto`,过不了守卫钉死的七参数 `manual` 校验
> (报 “not the pinned signed daemon”),而且空闲 5 分钟后自己退出,留下一个
> 没有监听者的陈旧 socket——下一次 `lsof` 就会 exit 1,并把 Time Machine 的
> smbfs 警告当成失败原因显示出来(2026-09-04 事故)。因此 `bridge` 模式默认
> 不探测,要探必须显式 `--probe-bridge`。

## 无人值守导入检查单

开跑前逐条过。`fusion_preflight.py` 已经自动检查其中 8 条,退出码 3 = 有阻断项
(退出码 2 = 探测本身失败)。

1. **端口与属主**:默认路线仍是 `127.0.0.1:27182/mcp`。多实例路线使用刚发现的
   实际 loopback 监听器，并同时绑定精确 PID 与启动令牌；不能只改端口或轮询碰运气。
   第二实例无需改回默认端口，首选项显示值也不能替代 OS listener 属主证明。
2. **Fusion 实例明确**:默认路线仍拒绝多个 PID；完整显式选择存在时，只对所选 PID
   及其文档取证。不同窗口、标签页和相同应用名称不代替实例绑定。
3. **守卫 marker**:当前 pid 不能有 `fusion-mcp-retired-*` / `fusion-mcp-quarantined-*`。
   **已退役但仍活着的 PID 是死锁源**:任何「等一个新 PID」的续跑循环都永远不会退出。
   先跑 `scripts/fusion_recovery.py evidence --latest`——AppLog 能证明那一个请求完成时
   用 `recover --apply` 解除,不必重启;证据不足才退出并重启 Fusion。
   有 `fusion-mcp-inflight-*` 时同理:先用 AppLog 证明保存状态再动。
4. **工具族的全局 busy 检查保留**:F1 和 `_common` 驱动仍要求没有其它 cell 在跑，
   `fusion_preflight.py` 也保留这项检查。先依次完成初始化；不同 PID 的普通
   `fusion_ops.py` 并行建模按[多实例路线](fusion-multi-instance.md)另行调度。
5. **Peekaboo 模式**:按上面的规则探,拿到推荐值再开跑。
6. **窗口普查按选定 PID 分开**:本机常驻的透明辅助窗
   `正在加载其他模块 - Autodesk Fusion` 会触发守卫的辅助窗证明,而该证明要求
   该 PID 下「恰好一个稳定的 primary canvas」；其它 Fusion PID 的窗口不能混入。
   同 PID 的多个文档窗口仍须满足原证明，不能当作独立并行 worker。
   同属 Fusion 进程的 `作业状态` 和 `Control Panel - EAGLE …` 窗口不匹配
   `<名> - Autodesk Fusion`,不会被误判。另外不能有任何加载/保存/恢复/安全类窗口。
7. **电源**:作业必须在 `caffeinate -i` 下跑(工具的 `--caffeinate` 会自己 re-exec),
   并且**不要合盖**——`caffeinate -i` 不阻止合盖休眠,见「主机休眠」。
   调用进行中主机休眠会让客户端在唤醒后到期(2026-09-06 22:12,`CancelledError:
   Cancelled via cancel scope`);cell 侧的休眠检测能给出有界延长,但不是保证。
8. **磁盘**:每块保存都产生一个新的 Fusion 版本,外加逐调用回执(`--min-free-gb`,默认 5)。
9. **STEP 路径**:manifest 里必须是**绝对路径**且文件存在;非 ASCII 路径没问题,
   工具会按构造转义。
10. **目标文档已打开并绑定**(`--expect-doc` 只核对窗口文字，还要原生 lineage 回读)。
    不因后台检查失败自动使用 `--activate` 或切前台；需要前台交互时遵守用户的焦点约束。
11. **切块尺寸** ≤20 体(见上),manifest 通过唯一性与体积闭合校验。
12. **先 `--dry-run`**:全部脚本渲染 + 静态安全检查通过,再真跑。

`doctor.sh --smoke` 仍然只做一次只读 `fusion_mcp_read {"queryType":"projects"}`;
新版在 preflight 有阻断项时会**拒绝**这次烟测——已退役/被隔离的 PID 不得再收任何
MCP 请求,健康探针也不行。`doctor.sh --json` 是纯 preflight,不发任何 MCP 请求。

## 已知易错点

- `queryType: "document"` 必须带 `operation`(search/open/recent),否则参数错误。
- `{"activeCommand": null}` 是健康结果(无活动命令),不是失败。
- Fusion 停在主页、无打开文档时,截图/文档类查询返回业务错误,属预期。
- **非 ASCII 字面量由工具转义,不许人手写。**
  `fusion_mcp_execute` 脚本里任何非 ASCII 字面量(中文路径、中文文档名、注释)
  在到达 Fusion 之前必须已经是 ASCII-only 的 Python Unicode 转义
  (例如 `"\u65e0\u6807\u9898"`),回读时再拿 Fusion 返回的原生 Unicode 比对。
  原因是链路的最后一段(cell → Fusion 的 loopback HTTP)发的是**裸 UTF-8** body 且
  `Content-Type` 不带 `charset`;Fusion 侧一旦不按 UTF-8 解码,脚本字面量就变成 mojibake,
  出现“界面名称明明对、脚本精确比对却失败”“STEP 路径明明在、
  `createSTEPImportOptions` 报 `3 : The selected file does not exist.`”。
  **做法**:所有脚本都经 `scripts/fusion_ops.py` 发出
  (`call_execute_script` 会自动调用 `ascii_safe_script`),
  或先手工跑一次 `python3 scripts/fusion_ops.py escape <script.py>`。
  该转换只改字符串字面量与注释,并用 AST 相等性证明语义未变;
  遇到非 ASCII 标识符或 raw f-string 会**拒绝**而不是硬改。
  `plan.json` 看起来是纯 ASCII(`json.dumps` 默认 `ensure_ascii=True`)**不代表已经安全**,
  MCP SDK 在下一跳就会把码点还原成裸 UTF-8。
- **脚本源代码单行不得过长(多行字面量规则)。**
  2026-09-06 18:21,一个 10 KB 脚本因为 `PARAMS` 被写成一条约 8 KB 的单行,
  Fusion 返回 `success=true`、`message=""`、AppLog 记 `duration=29ms`,
  **一条参数都没建**;同一脚本改成 `json.dumps(indent=1)`(最长行 253 字符)后正常跑完 42 s。
  我们这一侧(caller → cell → HTTP)经实测对该脚本字节无损,
  所以截断发生在 Fusion 内置 `mcp_execute_script` 里,不可控。
  **做法**:大字面量一律用 `fusion_ops.render_script(template, **subs)` 生成
  (占位符 `{{NAME}}`,自动折行 + 自动 ASCII 转义),或自己 `json.dumps(..., indent=1)`。
  本地上限是安装事实:`fusion_ops.MAX_SCRIPT_LINE = 2000`,作业工具族的
  `_common.MAX_SCRIPT_LINE = 400`(更严);超限在发出前本地拒绝。
  **推论**:`success=true` 但脚本 stdout 为空 = **脚本没跑**,不是“跑成功但没打印”。
  `fusion_call.py` 现在对此退出码 4(`no_output`),必须当硬失败处理,
  并且在做任何后续动作之前先独立回读一次文档状态。

## 故障排查

- 烟测:先跑 projects 查询;成功说明 Fusion + cell 全链路健康。
- 残留进程检查:`pgrep -fl 'cell_server|cad-agent-mcp'`。2026-08-27 起 cell 有
  自愈逻辑(父进程死亡/被替换/空闲 1h 自动退出),本 skill 的一次性调用本身
  不产生常驻进程。
- 历史事故与禁令:本 skill `references/runbooks/fusion-runtime-safety-incidents.md`、
  `references/runbooks/fusion-mcp-caller-guide.md`；这些随包副本是独立运行时的
  操作入口，仓库历史只用于明确选择的来源审计。
- 深度健康验收:`python -m fusion_mcp_proxy.acceptance_client --plan <plan> -- <cell argv>`,
  plan 需 `"schema": "fusion-mcp.stdio-acceptance-plan.v1"`。
- 守卫层瞬态拒绝(**上游未收到任何请求**,分类 `guard_transient`)的判别与处置:
  - `Fusion module-loading auxiliary is not uniquely stable`
  - `Fusion module-loading auxiliary has no unique primary canvas`
  - `Fusion top-level window set did not settle: …`
  - `Peekaboo Fusion window census failed: …`
  这四条都发生在 `fusion-mcp.in-flight.v2` 写入之前,不构成 same-PID
  no-retry 边界,可以直接重试(建议退避 45 s)。判据是 Fusion AppLog 里
  没有对应的 MCP 请求。**不要**因为这类拒绝去重启 Fusion 或退役 PID。
- Peekaboo 传输模式:见「Peekaboo 模式选择规则」。
  当 bridge / on-demand 传输本身不可用(超时、握手不全、socket 不在)且
  local 模式权限齐全时,守卫会自动降级到 `local` 采集窗口证据,并在证据里
  写下 `peekaboo_mode_requested` / `peekaboo_mode_used` /
  `peekaboo_mode_fallback`。降级只影响证据采集方式,不放宽任何判定;
  实质性拒绝(阻塞窗口、辅助窗口不唯一、应用身份不符)绝不降级重试。
  如果证据里出现 `peekaboo_mode_fallback`,说明 daemon 需要修:
  `peekaboo daemon stop && peekaboo daemon start`(manual 模式)。
- 超时/退休之后先取证再重启:`scripts/fusion_recovery.py evidence --latest`
  (只读 AppLog 与 marker,不发 MCP 请求);证据齐全时
  `scripts/fusion_recovery.py recover --marker <retirement.json> --apply` 直接解除退休,
  证据不足则退出码 4 且不写任何文件,此时才按
  `references/runbooks/fusion-mcp-ambiguity-triage.md` 换新 PID。

## 部署事实

- **本目录是 CAD 模块正本**:`~/.codex/skills/ontology-engineering/skills/cad-agent/`
  是真实目录,自带 `.venv`(代码 wheel 快照)+ `dist/` + 脚本;调用时**不读开发仓库**。
  旧独立 `~/.codex/skills/cad-agent/` 已按用户要求归档；新调用使用本正本，
  不依赖旧入口。
- **贡献直接进入本正本**:不得用 `~/120-agent-cad` 的旧副本或
  `rsync --delete` 覆盖本目录。仓库里产生的正式运行时代码或本体增量只能在
  差异审查后定向合并进来，并保留本目录独有内容。
- **分发给他人**:随父级 ontology-engineering 的受控分发包携带本模块，
  参见 [父级分发说明](../../../docs/PORTABLE-DISTRIBUTION.md)。接收方按包内说明
  运行 setup 与身份核验；机器差异通过 `FUSION_CAD_*` 配置。
