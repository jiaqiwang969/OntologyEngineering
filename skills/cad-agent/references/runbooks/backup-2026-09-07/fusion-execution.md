# Fusion 执行面:一次性 fusion-mcp 调用(cad-agent 执行模块)

以一次性子进程调用 Fusion,不依赖 MCP server 注册:每次调用 spawn 一个全新
fusion-mcp cell(守卫代理)→ 执行一次受保护的工具调用 → 打印结果 → 进程退出。
没有常驻进程,没有会话残留,claude / codex / 任意能跑 bash 的 agent 通用。

## 架构(调用前必须理解)

```
本 skill(一次性调用) ─► fusion-mcp cell(守卫代理,进程即用即抛)
                              │ HTTP,仅 127.0.0.1
                              ▼
                     Fusion 内置 MCP 端点 :27182(真正执行者)
```

- cell 不是可选包装:进程身份验真、Peekaboo 窗口证据、跨进程 owner lease、
  一调用一会话(成功恰好一次 DELETE,异常绝不 DELETE/重试)全在里面。
- **永远不要直连 127.0.0.1:27182**,不要绕过 cell 自己说 MCP。
- 语义问题(本体/约束/该怎么做)用 `cad-agent` skill 的语义侧车,不要问 Fusion。

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

输出为 JSON:`status: "delivered"` + `payload`(Fusion 的内层结果,可能是业务
错误,自行判读);退出码 2 = 传输/守卫层失败,详情在 stderr。

## 供应商标准件获取(McMaster-Carr)

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
(行虚化/URL 分面语法/最接近真件+回执 spec_delta/同族缩放等)沉淀在
`mcmaster_discover.py` 模块 docstring,改前先读。已用它为紫铜水冷板项目
取得四把真刀(Ø8.5 钻/Ø10/Ø20 立铣/Ø6 倒角)与力链五金正版 STEP。

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
5. **绝不在存在未闭环的保护义务时杀死/重启 Fusion**。正常重启前必须通过原生
   MCP census 确认：0 个 modified 文档，且 0 个无标题/从未保存文档；后者即使
   clean 也必须先完成规则 2。已经跨越 timeout/歧义边界的同 PID 禁止再保存，
   只能保留外部证据并 HOLD，不能借重启绕过。
6. 对已知脏文档做恢复性调用,走 cell 的 `--dirty-recovery-receipt` 一次性授权
   机制(见仓库文档),不要临时放宽守卫。一次性调用器可传
   `--dirty-recovery-receipt <owner-only-receipt.json>`，并仍使用同一 canonical
   safety state root。

## 其他铁律

1. **单次调用 8–20 秒是正常的**(含窗口验真与 cell 启动),不是卡死。
2. **超时/退出码 2/结果歧义之后,同一 Fusion PID 不得再收到任何 MCP 请求**。
   上游结果可能已提交；禁止只读确认、重发、save/close/quit、健康探针、建立新
   session 或 DELETE。冻结现有日志，仅用 MCP 外部证据按
   `references/runbooks/fusion-mcp-ambiguity-triage.md`(本 skill 内) 分诊；状态不能证明时写 `UNKNOWN` 并 HOLD。
3. **写操作前先读**:确认目标文档正确、无无关的未保存修改。
4. 并发调用由 owner lease 串行化,排队正常;不要并行轰炸。
5. Fusion 没开时调用会在守卫层失败(退出码 2),这是预期,不是 bug。
6. 同一执行节点的所有生产 cell 必须共用一个稳定、owner-only 的
   canonical safety state root。不得按项目、attempt 或单次请求更换
   `--state-directory`,否则新 cell 会看不到旧 PID 的 in-flight/retirement marker。

## 已知易错点

- `queryType: "document"` 必须带 `operation`(search/open/recent),否则参数错误。
- `{"activeCommand": null}` 是健康结果(无活动命令),不是失败。
- Fusion 停在主页、无打开文档时,截图/文档类查询返回业务错误,属预期。
- `fusion_mcp_execute` 脚本中用于精确身份守卫的非 ASCII 字面量应写成
  ASCII-only Python Unicode 转义(例如 `"\u65e0\u6807\u9898"`),再回读 Fusion
  返回的原生 Unicode。否则某些 MCP 传输/执行链会把脚本字面量误解码为
  mojibake,造成“界面名称正确、脚本精确比对却失败”。

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

## 部署事实

- **本目录是独立正本**:`~/.codex/skills/cad-agent/` 是真实目录,自带
  `.venv`(代码 wheel 快照)+ `dist/` + 脚本;调用时**不读开发仓库**。
  `~/.claude/skills/` 与各 `~/.claude-profiles/<profile>/skills/` 下的
  cad-agent 是指向它的 symlink(由 `~/.agents/bin/sync-codex-skills-to-claude.sh` 维护)。
- **贡献直接进入本正本**:不得用 `~/120-agent-cad` 的旧副本或
  `rsync --delete` 覆盖本目录。仓库里产生的正式运行时代码或本体增量只能在
  差异审查后定向合并进来，并保留本目录独有内容。
- **分发给他人**:把 `~/.codex/skills/cad-agent/` 里除 `.venv` 外的内容
  (SKILL.md、scripts/、dist/、setup.sh)拷给对方,放进对方的 skills 目录
  后跑一次 `setup.sh`;机器差异用 `FUSION_CAD_*` 环境变量覆盖。
