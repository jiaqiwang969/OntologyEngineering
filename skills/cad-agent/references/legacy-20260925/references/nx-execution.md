# NX 执行面:一次性 nx_call 调用 dell-nb 上的 Siemens NX 2412(cad-agent 执行模块)

与 Fusion 执行面同一思路:不在 Claude Code / Codex 里注册常驻 MCP,每次调用 spawn 一个
一次性进程 → 完成一次工具调用 → 打印 JSON → 退出。区别在于 NX 装在 Windows 笔记本
dell-nb 上,所以一次性进程要经 SSH 到达 Dell,再由 Dell 上的 sidecar 转给 NX 里的 bridge。
本文保留 2026-09-04 起的 NX 2412(`UGII_VERSION` v2412,内置 Python 3.11.9)实测记录，
并纳入 2026-09-07 制图导出与改型复核的修正；未验证的地方都明确标出。
文中的 `%USERPROFILE%` 和 `%LOCALAPPDATA%` 是 Dell 登录用户的 Windows 环境变量；
它们在说明中代替个人用户名，手动输入 NX 路径时先在 Dell 上解析为实际路径。
改型、孔列、图纸刷新和审核包的完整流程见 [NX 改型与图纸审核](../../nx-variant-drawing-review.md)。

## 架构(调用前必须理解)

```
Mac: scripts/nx_call.py ──stdio──▶ scripts/dell_ssh.sh(fleet 实时寻址与主机密钥校验)
        └─ Dell PowerShell 5.1(会话 0):  venv python -X utf8 -u -m nx_mcp.server   (MCP sidecar,无状态,每次调用新起)
                    └─ 带 token 的 JSON-RPC,仅 127.0.0.1:<随机端口>(描述文件 %LOCALAPPDATA%\nx-mcp\bridge.json)
                                └─ NX bridge = 运行在 NX 进程内的 journal(常驻,保存打开的部件与对象 ID)
                                            ├─ 批处理 bridge:run_journal.exe 起的无界面 NX(会话 0,默认)
                                            └─ GUI bridge:用户在交互式 NX 里播放 start_nx_bridge_gui.py(可选,未验证)
```

- sidecar 是 DreamEnding/NX_MCP 0.2.0.dev0(commit 179086b,MIT)加本 skill 携带的本地补丁
  (`assets/nx-mcp/nx_mcp_local_patches.diff`,8 个文件,见"部署事实")。
- 每次 `nx_call.py` 约 6–8 秒(SSH + Python 启动 + MCP 握手),**不是卡死**;bridge 侧每个请求
  120 秒超时,客户端 `--timeout` 默认 300 秒。
- bridge 一次只处理一个请求(journal 主线程排队执行);bridge 存活期间打开的部件与 `obj_…` 对象 ID
  跨调用有效,`nx_close_part`/`nx_undo` 会使其失效。
- **批处理 bridge 没有图形窗口**：`nx_screenshot` 仍需要图形会话。PDF 导出有独立能力边界：
  2026-09-07 已在 session 0 的独立 `run_journal.exe` 中用原生 `PrintPDFBuilder`、
  `CGMBuilder` 和安装版 `cgm2pdf.exe` 完成导出，不能再由“无 GUI”推断“不能导出图纸”。
  这项实测不等于旧 `nx_export_drawing_pdf` 包装器已经通过验证。
- Windows 会话规则:SSH 进来的进程都在会话 0,用户桌面是会话 1。loopback TCP 全机可达,所以
  会话 0 的 sidecar 能连会话 1 的 GUI bridge;但会话 0 起的任何 GUI 程序用户都看不见。要在用户
  桌面上打开 NX/程序,用"仅用户登录时运行"的计划任务(`New-ScheduledTaskPrincipal -LogonType Interactive`)。

## 调用方式

```bash
SKILL_DIR=<cad-agent skill 目录>
"$SKILL_DIR/scripts/nx_bridge.sh" status                    # bridge 是否在跑(描述文件 + 端口 + 进程)
"$SKILL_DIR/scripts/nx_bridge.sh" start                     # 起批处理 bridge(无界面 NX,约 30 s)
"$SKILL_DIR/scripts/nx_call.py" --status                    # 连通性烟测
"$SKILL_DIR/scripts/nx_call.py" --list-tools                # 50 个工具(16 个认证 + 34 个实验性)
"$SKILL_DIR/scripts/nx_call.py" nx_open_part '{"path": "163-caojun-print/ex/精美达670底座（576）/精美达670底座（576）.prt"}'
"$SKILL_DIR/scripts/nx_call.py" nx_list_components
"$SKILL_DIR/scripts/nx_call.py" nx_run_journal '{"path": "journals/inspect_assembly.py"}'
"$SKILL_DIR/scripts/nx_call.py" nx_close_part '{"save": false}'
"$SKILL_DIR/scripts/nx_bridge.sh" stop                      # 用完可停(NX 会话退出,释放许可证)
```

- 退出码:0 成功;1 传输/协议/用法错误(JSON 含 `stderr_tail`);2 工具返回业务错误(JSON 含 `code`/`message`)。
- **路径一律相对于工作区 `%USERPROFILE%\work`**,正反斜杠皆可,禁止绝对路径和 `..`
  (否则 `NX_PATH_OUTSIDE_WORKSPACE`);`nx_run_journal` 的脚本必须放在 `journals/` 下。
- 环境变量:`NX_CALL_SSH`(SSH 助手路径,默认同目录
  `dell_ssh.sh`)、`NX_CALL_WORKSPACE`、`NX_CALL_VENV_PYTHON`。
- 输出先落文件再解析(同 Fusion 的 SIGPIPE 教训):`nx_call.py … > out.json` 之后独立解析。

## 工具目录(2026-09-04 实测)

图例:✅ 已验证 · ⚠️ 有保留 · ❌ 在 NX 2412 Python 下失败 · ⬜ 未测 · 🖥️ 需要 GUI bridge。

认证工具(16 个,带类型化 schema,写操作带撤销标记):

| 工具 | 参数 | 状态 |
|---|---|---|
| `nx_status` | – | ✅ `{"connected": true, "nx_version": "v2412", "bridge_protocol": 1, "active_part": …}` |
| `nx_open_part` | `path` | ✅ 打开整套装配(组件从同目录加载),成为工作部件 |
| `nx_close_part` | `save`(默认 **true**,客户部件必须传 `false`) | ✅ 关闭整棵树 |
| `nx_export_step` | `path` | ✅ |
| `nx_save_part`、`nx_create_part` | – / `path`,`units` | ⬜ 故意未跑 |
| `nx_list_bodies`、`nx_list_features`、`nx_list_sketches` | – | ✅ 返回不透明对象 ID;装配部件本身 0 个体 |
| `nx_create_sketch`、`nx_sketch_line`、`nx_sketch_rectangle`、`nx_finish_sketch`、`nx_extrude`、`nx_undo` | 见 `--list-tools` | ⬜ 建模未测 |
| `nx_fit_view` | – | ✅ 批处理下也可用 |

实验性工具(34 个,描述带 `EXPERIMENTAL:`,返回 JSON 文本由 `nx_call.py` 解包):

| 领域 | 工具 | 状态 |
|---|---|---|
| 装配 | `nx_list_components` | ✅(已补丁:name/display_name/part/path/suppressed/reference_set/children/origin) |
| 装配 | `nx_add_component`、`nx_mate_component`、`nx_reposition_component` | ⬜ 会改装配;`nx_mate_component` 代码不是合法 NXOpen,勿依赖 |
| 文件 | `nx_list_open_parts` | ✅ |
| 文件 | `nx_save_as`、`nx_import_geometry` | ⬜ |
| 查询 | `nx_get_bounding_box`(可选 `body`) | ✅(重写)棱柱类零件精确;曲面件按边顶点估算会偏小 |
| 查询 | `nx_measure_volume`(可选 `body`) | ✅(重写,`MeasureManager.NewMassProperties`);`mass_kg_nx_density` 用 NX 默认密度,仅供参考 |
| 查询 | `nx_get_feature_info` | ⚠️ 只对命名特征有效;STEP 导入体的特征无名 |
| 查询 | `nx_measure_distance`、`nx_measure_angle` | ⬜ 按对象名解析,脆弱 |
| 制图 | `nx_create_drawing`、`nx_add_base_view`、`nx_add_projection_view`、`nx_add_dimension`、`nx_export_drawing_pdf` | ⬜ 这些包装器仍未验证；独立 journal 的原生 PDF/CGM 导出已在无界面 NX 中验证，见 NX-DRAW |
| 建模 | `nx_revolve`、`nx_sweep`、`nx_blend`、`nx_chamfer`、`nx_hole`、`nx_pattern`、`nx_boolean`、`nx_delete_feature`、`nx_edit_feature`、`nx_mirror_body`、`nx_sketch_arc`、`nx_sketch_constraint` | ⬜ 旧代码未验证 |
| 视图 | `nx_set_view`(`orientation`) | ✅(已补丁) |
| 视图 | `nx_screenshot`(`path`) | ❌ 批处理 `Invalid object state`;🖥️ 已改为 `UI.CreateImageExportBuilder`,仅 GUI bridge 可期待 |
| 脚本 | `nx_run_journal`(`path`,须在 `journals/` 下) | ✅(重写)进程内 `runpy` 执行并返回 stdout(≤60 000 字符)——工具集不够时的万能出口 |
| 脚本 | `nx_record_start`、`nx_record_stop` | ⬜ 大概率仅 GUI |

## 已验证的读数(精美达670底座（576）装配,2026-09-04)

- `nx_list_components`:67 个组件实例、24 个唯一部件;`nx_list_open_parts`:25 个部件。
- `nx_run_journal journals/inspect_assembly.py` 读出每个部件的属性(材料/重量/外形尺寸/加工工艺/表面处理…)。
- 独立(不经 bridge)的只读装配拆解 journal,用 `run_journal.exe` 直接跑,配置经环境变量 `INSPECT_CFG` 指向一个 JSON
  (`{"asm": "<prt>", "out": "<json>", "log": "<log>"}`),Dell 侧启动器 `journals\run_journal_cfg.cmd <cfg名> <journal名>`
  (`assets/nx-mcp/dell/run_journal_cfg.cmd`;cfg 与 journal 都放在 `%USERPROFILE%\work\journals\`):
  - `inspect_asm_standalone.py`:组件树、每个部件的属性/体数/图纸页/材料/包围盒/体积(标准清单)。
  - `inspect_parametrics.py`:每个部件的特征类型统计(`BREP` = 无历史哑实体)、表达式数(系统 `p<n>` 与命名)、装配约束数
    ——回答"这个模型能不能参数驱动"。
  - `inspect_links.py`:WAVE 链接(链接面/镜像体/链接基准)、被特征引用的驱动表达式、公式表达式、草图、基准面位置、
    引用集、布置、组件阵列、部件族——回答"原设计里哪些东西会跟着变"。
  - `inspect_links2.py`:对 `inspect_links.py` 找到的链接逐个解析来源(`CreateWaveLinkBuilder` 只读取选择集后 `Destroy`,
    另试 `UF_WAVE` 的 `AskLinkSource/AskLinkMirrorData/AskLinkedFeatureInfo`),并导出指定布局件的全部表达式。
  三个脚本都以 `CloseModified` 关闭全部部件,从不保存。2026-09-04 在两台印刷机装配上的实测:八色机 17–28 s,4色组 47–71 s。
  经验:`import NXOpen.Utilities` 在 NX 2412 的 Python 里不可导入；本次确认的 tag 解析是
  `NXOpen.TaggedObjectManager.GetTaggedObject(tag)`，纠正旧笔记的 `.Get(tag)`；
  `UF.Wave` 没有 `AskLinkStatus`,`AskLinkSource` 需要两个参数;一次只跑一个 run_journal(共享 cfg 会串)。
  WAVE 链接来源怎么读(NX 2412 Python 实测):`ufs.Wave.AskLinkedFeatureInfo(tag)` 返回
  `[FeatureName=<该链接供给的特征>,OwningPartName=…,SourcePartName=<源部件名,同装配内的组件面时为空>]`,是最可靠的一条;
  `AskLinkSource(tag, bool)` 对镜像/区域链接报错,对链接面返回 0;`AskLinkMirrorData/AskLinkUpdateTime/AskLinkedFeatureMap`
  都要第二个参数。`part.BaseFeatures.CreateWaveLinkBuilder(feat)` 只能在所属部件是工作部件时创建,否则
  "Error during part conversion";只读扫描里不要建 builder——这次建失败的 builder 让 `CloseAll(CloseModified)` 抛
  "memory access violation"(结果 JSON 已写出,磁盘上的 .prt 修改时间核对未变)。镜像面用几何法判:源件与镜像件包围盒
  逐轴比较,翻转轴的对称面偏置量等于组件原点坐标时,镜像面就是装配坐标原点面(印刷机上即整机中心面)。
- 交叉验证:JMD6701004 固定板 体积 2 478 049 mm³ × 7.85(Q235)= 19.45 kg,属性 19.453 kg;
  JMD6701001 大底板 1491.5×675×20 mm,17 683 704 mm³ × 2.70(6061)= 47.75 kg,属性 47.746 kg。
  NX 给的 `mass_kg_nx_density` 对铝件报 138 kg(钢默认密度),证明该字段不可直接采信。

## 批处理建模与交互式 UI 驱动（2026-09-05 实测,NX 2412 Python）

- **改型 journal 只在副本上跑**:`assets/nx-mcp/journals/pilot_width_change.py` 开头断言路径含 `\mod\`,
  副本用 `assets/nx-mcp/dell/make_copy.ps1`(UTF-8 BOM,Chinese 路径经 SSH 命令行会被 GBK 打坏,所以路径写在脚本里)。
  它做四件事:骨架部件 `SKEL_W.prt`(表达式 web_width_ref / web_width / dW / half,`Parts.NewBaseDisplay(path,
  NXOpen.BasePart.Units.Millimeters)`——不是 `Part.Units`)并 `AddComponent` 进总装;幅宽驱动件加跨部件表达式
  `dw_half = SKEL_W::half`,再按整机横向轴把面分成 +侧/−侧各建一个同步建模“移动面”;位置驱动件按几何中心所在侧
  在其所属子装配上下文里 `MoveComponent` ±half;`pilot_state.json` 记录已施加的 ΔW,重跑只补差值。
- **MoveFaceBuilder 在 2412 Python 里的真名**:`MoveFaceCollector`(ScCollector,`ReplaceRules([ScRuleFactory.
  CreateRuleFaceDumb(faces)], False)`)、`Type = MoveFaceBuilder.Types.TranslateDirectionAndDistance`、`Direction`
  (`part.Directions.CreateDirection(Point3d, Vector3d, SmartObject.UpdateOption.WithinModeling)`)、
  `Distance.RightHandSide = "dw_half"`。没有 `FaceToMove` 也没有 `TransformMotion`。面集合不能跨体:多体零件按体
  分别建特征,否则 "Object of wrong type in list";拓扑失败("A face no longer intersects a previous neighbor")
  时退化为只移两端端面。
- **面的范围**：先查对应绑定的真实接口。本次已用 `UFSession.ModlGeneral.AskBoundingBox(tag)`
  读取体/边范围；`UF.Modl` 中没有该成员不代表 NX 没有该能力。旧包装器仍使用边顶点估算，
  曲面范围不能据此声称精确；不适用时再选择并明确说明近似方法。
- **哪根轴是横向**:`component.GetPosition()` 的 Matrix3x3 行向量是部件局部轴在绝对坐标下的方向;取与世界 Y
  点积最大的行。焊接框架的最长边沿纸路,不能用包围盒最长边猜。
- **嵌套组件的移动**:对顶层以下的组件,`disp.ComponentAssembly.MoveComponent(occ, …)` 报内存访问违例;要用
  所属子装配 `ppart.ComponentAssembly.MoveComponent(<该部件自身上下文里的 Component>, 局部向量, 单位阵)`,局部向量
  = 父组件方向矩阵各行与世界向量点积。共享子装配移一次即全部实例生效。
- **交互式 NX 的 UI 驱动**:`assets/nx-mcp/dell/ui-actions.ps1` 读 `tools\ui-actions.json` 执行 activate /
  click / keys / type / snap;必须以 `schtasks /Create … /IT /RL HIGHEST` 建任务(LIMITED 级别的 SendInput 对 NX
  窗口被 UIPI 静默丢弃,现象是“前台成功、什么都没发生”)。NX 2412 中 `Alt+F8` 打开“操作记录管理器”对话框(文件名框
  约 (1235,280),运行按钮 (1235,514),最近列表首项 (1097,341),1920×1080),Play 不在功能区。SendKeys 返回“拒绝访问”
  = 桌面被锁或安全桌面在前台。`assembly_movie.py` 用这条路径在用户 NX 里渲染装配顺序帧
  (`ImageExportBuilder` 需先 `import NXOpen.Gateway`,否则退到 `UF.Disp.CreateImage`)。

- **屏保会让交互式自动化“拒绝访问”**:Dell 上 jiaqi 账户原设 Mystify 屏保 300 s。屏保启动后输入桌面切走,SendInput 报
  “拒绝访问”、CopyFromScreen 失败,`quser` 仍显示“运行中”、LogonUI 不在——不是锁屏。`assets/nx-mcp/dell/nolock_query.ps1`
  查、`nolock_apply.ps1` 关(注册表 + 会话内 SystemParametersInfo,须经 `/IT /RL HIGHEST` 的计划任务在会话 1 里跑一次)。
- **对比渲染与图纸检查**:`compare_views.py`(整机四视图,两台装配同相机:先 Fit 原件读 Matrix/Scale,副本用 `SetOrigin(绝对点)`
  + `SetScale(同值)`;`view.Origin` 读回的是视图坐标,不能回填)、`compare_views2.py`(3× 特写)、`export_sheets.py`
  (部件内图纸:`DraftingViews.UpdateViews` 后 `PrintPDFBuilder` 导出,视图与关联尺寸随模型,文字/属性/面上分界线不随)。
  UI 驱动 `ui-actions.ps1` 新增 `findwin/clickwin/clicknx/requirepixel(nx)`:凡要向 NX 发键,先用像素守卫确认目标对话框在。

## 保存纪律与铁律

1. **客户/生产部件永不保存**:`nx_close_part` 一律 `{"save": false}`;不要调用 `nx_save_part`、
   `nx_save_as`。写操作(建模/装配变更/导出)只在自己创建的工作区部件或明确授权的部件上做。
2. **写操作前先读**:`--status` 看 `active_part`,`nx_list_open_parts` 确认对象,再动手。
3. **一次一个请求,超时不要盲目重发**:bridge 内 120 秒超时。超时后先 `nx_bridge.sh status`,必要时
   `nx_bridge.sh restart`(批处理 bridge 没有用户数据,重启安全;GUI bridge 重启前先确认用户 NX 里
   没有未保存工作)。
4. 批处理 bridge 起一个额外的 NX 会话(占一个许可证座位);用完 `nx_bridge.sh stop`。
5. GUI bridge 与批处理 bridge 共用描述文件和 stop 标志,**不能同时运行**;先 stop 再切换。
6. 不要动用户交互式 NX 里的会话状态(关部件、切工作部件)除非用户要求;用户桌面上打开部件用
   计划任务 + `ugs_router.exe -ug -use_file_dir "<prt>"`(`.prt` 的文件关联命令),直接给 `ugraf.exe`
   传路径不会打开部件(syslog 报 `Error opening  for reading.`)。

独立批量 journal 另记录任务 ID、PID、session、命令行和进度收据。SSH 断开后先读收据及进程，
避免重复启动已在运行的改型/导出。停止任务时只处理身份已核实的本任务进程；不要按程序名结束
全部 `ugraf.exe`/`run_journal.exe`。固定 PID、主机地址和许可配置不属于可复用模板参数。

## GUI bridge(可选,未在 GUI 内验证)

用户想让 agent 在**他正在看的 NX 窗口**里操作/截图时:

1. `nx_bridge.sh stop`(停掉批处理 bridge)。
2. 用户在 NX 里:工具 → 操作记录(Journal) → 播放(Play) → `%USERPROFILE%\work\nxmcp\start_nx_bridge_gui.py`。
   该脚本读同目录 `start_nx_bridge_gui.json`,在 bridge 轮询之间跑一个有界 Win32 消息泵
   (PeekMessage/TranslateMessage/DispatchMessage),让 NX 窗口继续重绘;stop 标志、WM_QUIT 或
   `max_runtime_s`(8 h)结束它。批处理模式下与原版行为一致;NX 的 Qt 界面能否容忍嵌套消息泵**未知**。
3. `nx_call.py --status` 通了之后:`nx_screenshot {"path": "nxmcp/view.png"}`,再 scp 回来看。
4. 结束:`nx_bridge.sh stop`,再 `nx_bridge.sh start` 回到批处理 bridge。
   只在用户 NX 没有未保存生产工作时试点。

看用户桌面(不进 NX 进程):`assets/nx-mcp/dell/capture-desktop.ps1` 由计划任务
`CAD-DesktopCapture`(Interactive)运行,输出 `%USERPROFILE%\work\tools\desktop.png` 与
`windows.txt`(带标题的顶层窗口列表),scp 回本机即可查看。

## 故障排查

| 现象 | 原因 / 处理 |
|---|---|
| `NX_BRIDGE_UNAVAILABLE` "Start the NX MCP bridge inside Siemens NX" | bridge 没跑 → `nx_bridge.sh start`,看 `nx_bridge.sh log` |
| `nx_call.py` 退出码 1,`CONNECTIONERROR`/ssh 错误 | 用 `fleet resolve dell-nb` 与 `fleet st dell-nb` 核对当前身份、路由和 SSH；ICMP 被防火墙挡并不代表 SSH 不通 |
| 中文乱码 | Dell 控制台是 GBK;`nx_bridge.sh` 已转码,`nx_call.py` 两端强制 UTF-8;自己写 ssh 命令时经 `iconv -f GBK -t UTF-8` |
| `NX_PATH_OUTSIDE_WORKSPACE` | 用相对 `%USERPROFILE%\work` 的路径;journal 须在 `journals/` |
| `NX_MAIN_THREAD_UNAVAILABLE` | NX 会话忙或挂起 → `nx_bridge.sh restart` |
| `set_wakeup_fd only works in main thread…` | `experimental.py` 补丁丢失(重新同步 checkout 后重启 bridge) |
| GUI 变体拒绝启动 / 两个 bridge | 先 `nx_bridge.sh stop`;描述文件与 stop 标志共用 |
| bridge 随 NX 一起死 | `nx_bridge.sh log`;`bridge.log` 是 run_journal 的 stdout/stderr |

## 部署事实(2026-09-04)

- Dell(dell-nb,Dell Precision 3591,Windows 11,用户 jiaqi):NX 2412 在 `C:\Program Files\Siemens\NX 2412`
  (`NXBIN\run_journal.exe`,内置 Python 3.11,`UGII_LANG=simpl_chinese`,许可证 27800@localhost);
  用户级 Python 3.11.9 在 `%LOCALAPPDATA%\Programs\Python\Python311`(winget 安装);
  sidecar venv `%USERPROFILE%\work\venvs\nx-mcp`(mcp 1.29.1、pydantic 2.13.5);NX_MCP 可编辑安装于
  `%USERPROFILE%\work\NX_MCP`(已打补丁);bridge 脚本与日志在 `%USERPROFILE%\work\nxmcp\`
  (`bridge.cmd`、`start-bridge.ps1`、`stop-bridge.ps1`、`status.ps1`、`start_nx_bridge_gui.py/.json`、
  `bridge.log`、`stop.flag`);journal 目录 `%USERPROFILE%\work\journals\`;7-Zip 22.01 可解 Mac p7zip 解不开的 RAR5。
- 本 skill 携带可重建 Dell 侧的全部文件:`assets/nx-mcp/dell/*`(含 `run_journal_cfg.cmd`、`run_inspect.ps1`)、`assets/nx-mcp/journals/*`(bridge 内 `inspect_assembly.py` 与四个独立拆解 journal)、
  `assets/nx-mcp/nx_mcp_local_patches.diff`。重建顺序:装 Python 3.11 → venv → `pip install -e NX_MCP`
  → `git apply nx_mcp_local_patches.diff` → 复制 `dell/*` 到 `%USERPROFILE%\work\nxmcp\` → `nx_bridge.sh start`。
- 本地补丁要点:`experimental.py` 延迟导入 `mcp`(NX 内置 Python 没有它)并改用 `SelectorEventLoop`
  (journal 不在主解释器线程,`asyncio.run` 报 `set_wakeup_fd`);NX 的 Python 绑定没有 `.ToArray()`、
  `Body.GetBoundingBox`、`Body.GetMassProperties`、`Session.ExecuteJournal`、`UF.Disp.CreateImageExportBuilder`,
  相应工具已改写。`tools/` 下的改动每次调用即时生效,`experimental.py`/`nx_bridge.py` 的改动需 `nx_bridge.sh restart`。
- SSH 助手 `scripts/dell_ssh.sh`(本 skill 自带):每次经 `fleet ssh dell-nb` 解析当前路由并核对登记的主机密钥，不使用历史 DHCP 地址。
- 上游 NX_MCP 的 bridge 使用 `NX_MCP_ALLOW_UNVERIFIED_PYTHON_BRIDGE=1`(作者称为可行性模式);
  实验性工具通过 `NX_MCP_ENABLE_EXPERIMENTAL=1`、`NX_MCP_ENABLE_JOURNAL=1` 打开,sidecar 与 bridge 两侧都要设。
