# AutoCAD 执行面:一次性 autocad_call 调用 dell-nb 上的 AutoCAD 2024(cad-agent 执行模块)

与 Fusion / NX 执行面同一思路:不注册常驻 MCP,每次调用 spawn 一次性进程 → 一次工具调用 →
打印 JSON → 退出。AutoCAD 2024(简体中文,R24.3)装在 Windows 笔记本 dell-nb 上,通过 COM 自动化
驱动,而且必须驱动**用户桌面上可见的那个 AutoCAD**,所以中间多了一个跑在交互式会话里的中继。
本文记录 2026-09-04 的实测事实;未验证之处明确标出。
文中的 `%USERPROFILE%` 是 Dell 登录用户的 Windows 环境变量；手动输入 AutoCAD
路径时先在 Dell 上解析为实际路径。

## 架构(调用前必须理解)

```
Mac: scripts/autocad_call.py <tool> '<json>' ──stdio──▶ scripts/dell_ssh.sh(fleet 实时寻址与主机密钥校验)
        └─ Dell PowerShell 5.1(会话 0): venv python -X utf8 -u relay_client.py
                    └─ TCP 127.0.0.1:39901,首行为 token(%USERPROFILE%\work\cadmcp\relay.token)
                                └─ relay_server.py(会话 1,计划任务 CadMcpRelay,登录即起,pythonw.exe)
                                            └─ 每个连接 spawn 一个 acad_mcp_server.py(FastMCP,stdio 逐字节转发)
                                                        └─ win32com GetActiveObject("AutoCAD.Application") → 桌面上可见的 AutoCAD 2024
```

- 为什么要中继:SSH 登录落在会话 0,其 Running Object Table 看不到桌面(会话 1)的 AutoCAD;
  会话 0 直接 COM 会另起一个**看不见的** AutoCAD。中继任务以 jiaqi 的 Interactive 登录类型运行在
  控制台会话,子进程与用户看到的 AutoCAD 共享同一实例——用户能看到每一步改动。
- 服务端 = daobataotie/CAD-MCP(MIT,pywin32 COM,只有绘图工具)作为基础,加本 skill 携带的
  `acad_mcp_server.py`(27 个检查/文档/视图/导出/编辑工具),共 37 个工具;mcp SDK 固定 1.x
  (`mcp<2`,2.x 破坏 CAD-MCP 的导入)。
- 每次调用约 8–10 秒(ssh ≈5 s + CAD-MCP 自带 2 s 连接延时),**不是卡死**。每次调用是新的服务进程,
  但 AutoCAD 是同一个实例:"活动文档"就是 AutoCAD 当前激活的那份,多文档时先 `list_documents`
  再 `activate_document`。
- 端口 39901(47811 在这台机器上被占,疑似 WSL 镜像网络);改端口要同时改两个 relay 文件和
  两个 PS1 脚本(`CADMCP_PORT`)。

## 调用方式

```bash
SKILL_DIR=<cad-agent skill 目录>
"$SKILL_DIR/scripts/autocad_service.sh" status        # 任务状态、监听端口、acad 进程、日志尾
"$SKILL_DIR/scripts/autocad_service.sh" start         # 注册并启动中继任务;AutoCAD 没开则在桌面上拉起
"$SKILL_DIR/scripts/autocad_call.py" --status         # 连通性烟测(acad_status)
"$SKILL_DIR/scripts/autocad_call.py" --list-tools     # 37 个工具
"$SKILL_DIR/scripts/autocad_call.py" open_drawing '{"path": "<Dell 上实际 DWG 绝对路径>", "read_only": true}'
"$SKILL_DIR/scripts/autocad_call.py" list_layers '{"with_counts": true}'
"$SKILL_DIR/scripts/autocad_call.py" read_texts '{"space": "model", "limit": 500}' > texts.json
"$SKILL_DIR/scripts/autocad_call.py" capture_view '{"zoom_extents": true}' --image-out view.png
"$SKILL_DIR/scripts/autocad_call.py" export_pdf '{"path": "<Dell 上实际输出 PDF 绝对路径>", "layout": "Model"}'
"$SKILL_DIR/scripts/autocad_call.py" close_drawing '{"save": false}'
```

- 退出码:0 成功;1 工具或传输错误(stdout 为含 `error` 的 JSON,附 `stderr`/`ssh_exit_code`);2 用法错误。
- 图像结果(`capture_view`)写到 `--image-out`,默认 `./autocad_capture_<时间戳>.png`,JSON 里替换为 `{"image_saved": …}`。
- 路径用 Dell 上的 Windows 绝对路径;图纸与产物放 `%USERPROFILE%\work\` 下。坐标为图形单位,颜色为 ACI
  (256=ByLayer,0=ByBlock),句柄为十六进制字符串,`space` = `model` | `paper` | 布局名。
- 环境变量:`AUTOCAD_CALL_SSH`(默认同目录经 fleet 寻址的 `dell_ssh.sh`)、
  `AUTOCAD_CALL_VENV_PYTHON`、`AUTOCAD_CALL_RELAY_CLIENT`。
- 输出先落文件再解析(SIGPIPE 教训同 Fusion)。

## 工具目录(37,2026-09-04 实测:✅ 已验证 · ⬜ 未测)

| 组 | 工具 | 状态 |
|---|---|---|
| 状态/文档 | `acad_status` ✅、`list_documents` ✅、`open_drawing(path, read_only)` ✅(错误路径返回 `file not found`)、`new_drawing(template=acadiso.dwt)` ✅、`activate_document(name)` ⬜、`save_drawing_as(path, overwrite=false)` ⬜、`close_drawing(save=false)` ✅ | |
| 检查 | `drawing_info` ✅、`list_layers(with_counts)` ✅、`list_entities(layer, types, space, limit, offset, detail, window)` ✅、`get_entity(handle)` ✅、`read_texts(layer, space, include_dimensions, include_attributes, limit)` ✅、`find_text(pattern, regex, space)` ✅、`list_blocks(include_references)` ⬜、`list_dimensions(space)` ⬜ | |
| 视图 | `zoom(mode=extents/window/center/entity/previous, …)` ✅、`capture_view(zoom_extents, max_width=1400, save_path)` ✅(PNG,PrintWindow 抓 AutoCAD 窗口,被遮挡/最小化时是旧图)、`regen` ⬜ | |
| 导出 | `export_pdf(path, layout, paper, plot_area=extents, landscape)` ✅(`DWG To PDF.pc3`,一次一个布局)、`export_dxf(path)` ✅ | |
| 编辑 | `create_layer` ✅(测试图层 MCP-TEST)、`set_layer_state` ⬜、`draw_line` ✅、`draw_circle`/`draw_arc`/`draw_polyline`/`draw_rectangle`/`draw_hatch` ⬜(CAD-MCP 原生)、`add_text` ✅(中文正常)、`add_mtext`/`add_dimension` ⬜、`modify_entities`/`move_entities(copy)`/`delete_entities` ⬜ | |
| 原始 | `send_command(command)` ⬜(异步返回;会弹对话框的命令会阻塞 COM)、`get_variable(name)` ✅、`set_variable(name, value)` ⬜ | |

错误统一为 `{"error": …, "hint": …}`;COM "call rejected"(-2147418111)自动重试 6 次后提示
"AutoCAD busy / dialog open"。

## 已验证的往返(临时图 Drawing2,事后关闭不保存)

```
--status     → {"connected": true, "autocad_version": "24.3s (LMS Tech)", "documents": [{"name": "Drawing1.dwg", "active": true, …}], "active_space": "model", "current_layer": "0", "insunits": 4, …}
drawing_info → 布局 ["Model","布局1","布局2"],标注样式 ["Standard","Annotative","ISO-25"],图限 420×297
draw_line    → {"ok": true, "entity": {"handle": "2B9", "type": "Line", "layer": "MCP-TEST", "start": [0,0,0], "end": [100,50,0], "length": 111.8034}}
add_text     → {"ok": true, "entity": {"handle": "2BB", "type": "Text", "text": "精美达670底座 测试 SW370-576", "height": 5.0}}
find_text    → SW370 命中 1 条;get_entity 2B9 → bbox [[0,0,0],[100,50,0]]
export_pdf   → A4 横向 PDF(85 KB,Creator "AutoCAD 2024 - 简体中文",pdftotext 能取到文字)
export_dxf   → AC1018 DXF,含 MCP-TEST 层上的 AcDbLine/AcDbText
capture_view → 233 KB PNG:桌面 AutoCAD 显示红线、中文文字与"打印和发布作业完成"气泡
```

## 保存纪律与铁律

1. **不覆盖用户图纸**:查看用 `open_drawing … "read_only": true`;要改就 `save_drawing_as` 到新文件
   (`overwrite` 保持 false),不要对用户原文件 `send_command "QSAVE"`。`close_drawing` 默认 `save=false`。
2. **先看再动**:`acad_status`/`list_documents` 确认活动文档;多文档时 `activate_document`。
3. **对话框会锁死 COM**:`send_command` 触发的命令若弹窗,后续调用全部 "busy",需要在 Dell 屏幕上关掉;
   不要在弹窗未处理时反复重试。
4. 一个 AutoCAD 实例大家共用;每次调用是新服务进程,不要假设上一次的选择集还在。
5. 中继只在 jiaqi 登录控制台时工作;重启后中继随登录自动起,AutoCAD 要 `autocad_service.sh start-acad`
   拉起,首次可能要人在屏幕上关一次 Autodesk 登录/试用对话框(这次没有出现)。
6. 会话 0 里绝不直接 COM 连接 AutoCAD(会起隐形实例);要在桌面上开程序,一律走 Interactive 计划任务。

## 故障排查

| 现象 | 处理 |
|---|---|
| `relay token missing` / `cannot reach relay on 127.0.0.1:39901`(退出码 1) | `autocad_service.sh status`;任务不在 Running 就 `start`;用户未登录则先在控制台登录 |
| `AutoCAD is not reachable` | `autocad_service.sh start-acad`,等约 60 s,看桌面有无对话框 |
| `COM error -2147418111` / "AutoCAD is busy" | AutoCAD 里有命令或对话框在进行;Esc / 关掉后重试 |
| `endpoint closed before answering initialize` | ssh 失败(看错误 JSON 里的 `stderr`):用 `fleet resolve dell-nb` 和 `fleet st dell-nb` 核对当前身份、可达性和主机密钥 |
| 端口 39901 被占 | 两个 relay 文件和两个 PS1 里改 `CADMCP_PORT`,再 `restart` |
| 手动跑 PS1 时中文乱码 | 经 `iconv -f GBK -t UTF-8 -c`(`autocad_service.sh` 已设 UTF-8 控制台) |

## 部署事实(2026-09-04)

- Dell(dell-nb,用户 jiaqi):AutoCAD 2024 简体中文在 `C:\Program Files\Autodesk\AutoCAD 2024\acad.exe`
  (ProgID AutoCAD.Application.24.3);用户级 Python 3.11.9;venv `%USERPROFILE%\work\venvs\cad-mcp`
  (pywin32 312、pyautocad 0.2.0、mcp 1.29.1、pillow 12.3);代码在 `%USERPROFILE%\work\cadmcp\`
  (`acad_mcp_server.py`、`relay_server.py`、`relay_client.py`、`CAD-MCP\` 上游克隆,`cad.type=AUTOCAD`,
  `cadmcp-install-task.ps1`、`cadmcp-status.ps1`、`cadmcp-stop.ps1`、`cadmcp-restart.ps1`、`check_import.py`);
  运行文件 `relay.token`、`relay.pid`、`logs\relay.log`、`logs\acad_mcp_server.log`、`logs\server_stderr.log`;
  计划任务 `CadMcpRelay`(Interactive,登录触发,无时限,失败重启 3 次)与 `CadMcp-StartAcad`(`acad.exe /nologo`)。
- 本 skill 携带可重建 Dell 侧的源码:`assets/autocad-mcp/dell/*`(服务端、两个 relay、四个 PS1、导入自检)。
  重建顺序:装 Python 3.11 → venv 装 `pywin32 pyautocad "mcp<2" pillow` → 克隆 CAD-MCP 到 `cadmcp\CAD-MCP`
  并把 `cad.type` 设为 AUTOCAD → 复制 `dell/*` 到 `%USERPROFILE%\work\cadmcp\` → `autocad_service.sh start`。
- 选型备忘:hvkshetry/autocad-mcp(AutoLISP + 键击注入,面向 AutoCAD LT,配套 skill 已归档)读图能力强但脆弱;
  其余 COM 服务器没有检查类工具。现成的 AutoCAD Claude skill 没有可用的,故检查/导出工具自写。
