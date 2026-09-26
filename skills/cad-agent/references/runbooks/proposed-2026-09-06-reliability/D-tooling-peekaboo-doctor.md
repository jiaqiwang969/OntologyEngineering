# RUNBOOK_DELTA — 追加进 `references/fusion-execution.md` 的正文

下面的三节按原文风格写好,可直接插入 `~/.codex/skills/cad-agent/references/fusion-execution.md`:

- 「作业工具族」插在 **「保存纪律」之后、「其他铁律」之前**;
- 「Peekaboo 模式选择规则」插在 **「其他铁律」之后**;
- 「无人值守导入检查单」插在 **「已知易错点」之前**。

同时建议把 `doctor.sh` 换成 `proposed/doctor.sh`(diff 见 `proposed/doctor.sh.diff`),
并把 `proposed/scripts/` 拷进 `scripts/`。

---

## 作业工具族(大 STEP → 装配,无人值守)

一次性 `fusion_call.py` 是**单次调用**的正确形状,但一次几百个 occurrence 的
装配作业需要几十次调用串起来。不要再手搓驱动脚本:用 `scripts/` 里的工具族,
它们把 2026-09-05/06 手工做过的每一步都固化了。

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

### 三条按构造保证的传输规则(对应三次「看着成功、其实没跑」)

1. **字面量必须 ASCII-only**。非 ASCII 路径写成 `\uXXXX` 转义;工具用
   `py_str()` 渲染并用 `assert_transport_safe()` 强制。原始 UTF-8 路径会让
   `createSTEPImportOptions` 报 `3 : The selected file does not exist.`。
   注意别二次转义:`py_str()` 内部已经转过,再包一层 `ascii_escape()` 得到的是
   字面反斜杠,同样打不开文件。
2. **生成的字面量必须多行、每行短**(`json.dumps(..., indent=1)`,单行上限 400 字符)。
   一个 ~7 KB 的单行 `PARAMS` 让 Fusion 2705 返回 delivered / success=true /
   message "" / 29 ms —— 脚本根本没执行。
3. **「空 message + ~30 ms」= 没跑,不是成功**。工具把它分类为 `script-no-output`
   并 fail-fast。人工判读时同理。

### 切块尺寸

守卫把单次调用封顶 300 s,且惰性激活时**第一个**上游请求跑在 *startup* 截止时间上
——两个预算都要给到 300(工具已经自动设 `FUSION_CAD_TIMEOUT_SECONDS` 与
`FUSION_CAD_STARTUP_TIMEOUT_SECONDS`)。Fusion 2705 实测 `importToTarget`:
40 个简单车架体 ≈ 10 s;10 个带盲槽的侧板 ≈ 25 s;168 个链节类实体 = 404 s(超时)。
`_common.max_bodies_for_budget()` 给出的默认是 **20 体/块**:线性模型本身允许 ~43,
但每体成本随目标文档的 occupancy 上升,而**一次 300 s 超时的代价是重启 Fusion**,
所以取实测跑通的那个切法(`fusion_chunks_small20`)。要更大就显式 `--max-bodies`。

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

---

## Peekaboo 模式选择规则

守卫支持三种模式(`FUSION_CAD_PEEKABOO_MODE` / `--peekaboo-mode`),验真强度和
故障面完全不同:

| 模式 | peekaboo argv | 依赖 | 典型故障 |
|---|---|---|---|
| `local` | 每条命令加 `--no-remote`,进程内完成 | 只依赖已签名的 CLI 与 TCC 授权 | 最少 |
| `local-ondemand` | `--bridge-socket ~/Library/Application Support/Peekaboo/bridge.sock` | 必须有一个**钉死七参数**的常驻 daemon:`peekaboo daemon run --mode manual --bridge-socket <sock>` | socket 陈旧/无监听者;daemon 参数不符;每条命令前后各一次 lsof+libproc 括号证明,叠加共享截止时间后容易整体超时 |
| `bridge` | 不加 `--no-remote`,socket 取自 `bridge status` | 必须有 Peekaboo **GUI app** 在跑(`hostKind: gui`) | 无人值守时不可靠 |

**本机默认取 `local`。** 理由:活动部件最少(没有 daemon 生命周期、没有 socket、
没有陈旧 socket 这一整类故障、没有每条命令的 lsof 括号证明),实测最快,而且
V018/V020 整轮导入实际就是靠它跑完的。`local-ondemand` 只在探测健康**且**确实
需要 daemon 的 window tracker 时才用;`bridge` 不要用于无人值守。

### ≤3 s 自动选模

`fusion_preflight.py`(以及 `--peekaboo-mode auto`)按下面的顺序在一个墙钟预算里选:

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

本机实测三个模式探完 **0.6 s**(local 0.10 s / on-demand 0.32 s / codesign 0.16 s)。

> **硬规则:探测不得改变被探测的状态。** 永远不要在既没有 `--no-remote`
> 也没有 `--bridge-socket` 的情况下调用任何 peekaboo 子命令。那种形状会按需拉起
> `daemon run --mode auto --bridge-socket <sock> --idle-timeout-seconds 300`:
> 它的 argv 是 9 项且是 `auto`,过不了守卫钉死的七参数 `manual` 校验
> (报 “not the pinned signed daemon”),而且空闲 5 分钟后自己退出,留下一个
> 没有监听者的陈旧 socket——下一次 `lsof` 就会 exit 1,并把 Time Machine 的
> smbfs 警告当成失败原因显示出来(2026-09-04 事故)。因此 `bridge` 模式默认
> 不探测,要探必须显式 `--probe-bridge`。

---

## 无人值守导入检查单

开跑前逐条过。`fusion_preflight.py` 已经自动检查其中 8 条,退出码 3 = 有阻断项。

1. **端口**:`lsof -nP -iTCP:27182 -sTCP:LISTEN` 必须存在**且属主是 Fusion 进程**。
   端口必须是 27182(动态端口如 49892 会被守卫拒绝)。这条只能核验,不能自动修——
   要在 Fusion 偏好里手动固定。
2. **Fusion 进程唯一**:多于一个实例守卫无法绑定。
3. **守卫 marker**:当前 pid 不能有 `fusion-mcp-retired-*` / `fusion-mcp-quarantined-*`。
   **已退役但仍活着的 PID 是死锁源**:任何「等一个新 PID」的续跑循环都永远不会退出,
   必须先退出并重启 Fusion。有 `fusion-mcp-inflight-*` 时先用 AppLog 证明保存状态再写。
4. **没有别的 cell 在跑**:`pgrep -f 'fusion_mcp_proxy\.(cell_server|acceptance_client)'`。
5. **Peekaboo 模式**:按上面的规则探,拿到推荐值再开跑。
6. **窗口普查**:只能有**一个** Fusion 文档窗口。本机常驻的透明辅助窗
   `正在加载其他模块 - Autodesk Fusion` 会触发守卫的辅助窗证明,而该证明要求
   「恰好一个稳定的 primary canvas」——多开一个文档就会让**每一次**受保护调用失败。
   同属 Fusion 进程的 `作业状态` 和 `Control Panel - EAGLE …` 窗口不匹配
   `<名> - Autodesk Fusion`,不会被误判。另外不能有任何加载/保存/恢复/安全类窗口。
7. **电源**:作业必须在 `caffeinate -i` 下跑(工具的 `--caffeinate` 会自己 re-exec)。
   调用进行中主机休眠会退役 PID(2026-09-06 22:12,`CancelledError: Cancelled via
   cancel scope`)。
8. **磁盘**:每块保存都产生一个新的 Fusion 版本,外加逐调用回执。
9. **STEP 路径**:manifest 里必须是**绝对路径**且文件存在;非 ASCII 路径没问题,
   工具会按构造转义。
10. **目标文档已打开**(`--expect-doc`),或者用 `--activate` 让工具先激活它并 settle 15 s。
11. **切块尺寸** ≤20 体(见上),manifest 通过唯一性与体积闭合校验。
12. **先 `--dry-run`**:全部脚本渲染 + 静态安全检查通过,再真跑。

`doctor.sh --smoke` 仍然只做一次只读 `fusion_mcp_read {"queryType":"projects"}`;
新版在 preflight 有阻断项时会**拒绝**这次烟测——已退役/被隔离的 PID 不得再收任何
MCP 请求,健康探针也不行。
