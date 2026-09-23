---
name: cad-agent
description: Reconstruct, design and validate parts, drawings, assemblies and mechanisms from CAD, images, video or patents using source-bound evidence and first-principles reasoning; hand relevant features to manufacturing.
---

# CAD Agent：通用 CAD 工程与证据交接

本模块是工程本体论的 CAD 执行模块，适用于零件设计/修复、图纸和 BOM、装配、机构运动以及 CAD 到工艺的交接。按[通用 CAD 工程工作流](references/cad-engineering-workflow.md)识别对象、身份、关系、坐标系、配置、状态、来源与待判断的主张，再选择原生回读和专项检查。CAD 工具报告几何与文档状态；正式 TBox、CQ、SHACL、规则和项目语义审查由父级[工程本体论](../../SKILL.md)绑定的 Semantica 控制。项目 ABox 可由本模块投影，不是行业本体正本。

原生 CAD 事实、图纸/BOM 审阅、仿真、测量和工程推断分别记录。按[通用证据合同](contracts/cad-evidence.v1.schema.json)与 `scripts/cad_evidence.py` 核对来源哈希、对象引用、原生回读指针和预期值，生成项目 ABox 与变更影响提示。装配中的定义与实例、配合与真实接触、机构轨迹与物理力、模型几何与实物性能不得混同。完整性通过不等于工程主张成立。

从图片、视频、专利或残缺 CAD 反推外形和机构时，先读[证据驱动的重建方法](references/evidence-driven-reconstruction.md)：把可见内容、隐藏机理候选、物理/机构/数学推导、本方 CAD 方案和原物结论分开。用第一性原理收窄缺口，并保留现有证据无法区分的候选；不能由功能自由度直接认定原设备的电机或传动布局。

装配或力学问题先读[装配与力学通用判断内核](references/assembly-and-physics-kernel.md)：辨认定义/实例、完整界面图、逐动作状态、外部支撑、装入路径、载荷路径和局部容量；把名义几何、静力、接触仿真与实物证据分层。可用[装配有界筛查器](assembly/README.md)检查给定 AABB 的候选直线装入方向，用[机构四连杆筛查器](mechanism/README.md)检查平面闭环位置；这些工具不替代方法内核、原生回读、物理求解或 Semantica。

当制造路线依赖 CAD 的特征、界面、配合、尺寸链、运动包络或工具空间时，使用[CAD／工艺双向交接](references/cad-process-integration.md)和 `scripts/cad_process_handoff.py`。把通用 CAD 包中已核对的对象 ID 链到本轮工艺特征、功能、操作、决定主张或问题；分别写明候选工序的作用机制、适用状态、待验证功能和反证。焊接密封只是一个示例，钻孔、装配、紧固、加工、成形、检验和机构制造均走同一因果链。

## Fusion 本机执行

先运行 `setup.sh`，再阅读 [Fusion 安全调用](references/fusion-execution.md)。所有 Fusion MCP 调用经 `scripts/fusion_call.py` 一次性守卫，不直连端口，不对歧义超时盲目重试。`scripts/fusion_ops.py` 是结果分类的共同入口。调用前运行 `doctor.sh --fusion`；它只预检，不发业务请求。

```bash
python3 scripts/fusion_call.py fusion_mcp_read '{"queryType":"projects"}'
```

一次调用返回 `classification`；进程退出 0 只表示已送达，`delivered_ok` 才是可继续的执行结果。原生保存、重新打开及来源摘要要记录在项目证据里。没有 Fusion 或本机端点时，仍可用来自 STEP/图纸/其他 CAD 的已核验回读 JSON 完成工艺交接；不能声称本模块亲自完成原生回读。

## NX / AutoCAD 远程执行

按[可配置远程桥](remote/README.md)部署。`remote/mcp_bridge.py` 可将一次 MCP 会话传输到接收方的 Windows CAD 主机；`remote/autocad_server.py` 与受 token 保护的本机回环 relay 提供 AutoCAD COM 的受控检查/工作副本绘图。NX 端 MCP sidecar、NX GUI bridge、商业软件和许可由接收方自行提供；本包不包含旧工作站的固定地址或历史客户案例。远程工具在新主机上的实际调用和原生回读须单独记录，不能把离线协议测试称为设备验收。
