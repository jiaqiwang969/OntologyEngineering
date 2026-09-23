---
name: cad-agent
description: Design, repair and validate parts, drawings, assemblies and mechanisms using source-bound native CAD evidence; hand relevant features and functions to manufacturing. Use guarded local Fusion execution when available.
---

# CAD Agent：通用 CAD 工程与证据交接

本模块是工程本体论的 CAD 执行模块，适用于零件设计/修复、图纸和 BOM、装配、机构运动以及 CAD 到工艺的交接。按[通用 CAD 工程工作流](references/cad-engineering-workflow.md)识别对象、身份、关系、坐标系、配置、状态、来源与待判断的主张，再选择原生回读和专项检查。CAD 工具报告几何与文档状态；正式 TBox、CQ、SHACL、规则和项目语义审查由父级[工程本体论](../../SKILL.md)绑定的 Semantica 控制。项目 ABox 可由本模块投影，不是行业本体正本。

原生 CAD 事实、图纸/BOM 审阅、仿真、测量和工程推断分别记录。按[通用证据合同](contracts/cad-evidence.v1.schema.json)与 `scripts/cad_evidence.py` 核对来源哈希、对象引用、原生回读指针和预期值，生成项目 ABox 与变更影响提示。装配中的定义与实例、配合与真实接触、机构轨迹与物理力、模型几何与实物性能不得混同。完整性通过不等于工程主张成立。

当制造路线依赖 CAD 的特征、界面、配合、尺寸链、运动包络或工具空间时，使用[CAD／工艺双向交接](references/cad-process-integration.md)和 `scripts/cad_process_handoff.py`。把通用 CAD 包中已核对的对象 ID 链到本轮工艺特征、功能、操作、决定主张或问题；分别写明候选工序的作用机制、适用状态、待验证功能和反证。焊接密封只是一个示例，钻孔、装配、紧固、加工、成形、检验和机构制造均走同一因果链。

## Fusion 本机执行

先运行 `setup.sh`，再阅读 [Fusion 安全调用](references/fusion-execution.md)。所有 Fusion MCP 调用经 `scripts/fusion_call.py` 一次性守卫，不直连端口，不对歧义超时盲目重试。`scripts/fusion_ops.py` 是结果分类的共同入口。调用前运行 `doctor.sh --fusion`；它只预检，不发业务请求。

```bash
python3 scripts/fusion_call.py fusion_mcp_read '{"queryType":"projects"}'
```

一次调用返回 `classification`；进程退出 0 只表示已送达，`delivered_ok` 才是可继续的执行结果。原生保存、重新打开及来源摘要要记录在项目证据里。没有 Fusion 或本机端点时，仍可用来自 STEP/图纸/其他 CAD 的已核验回读 JSON 完成工艺交接；不能声称本模块亲自完成原生回读。

NX/AutoCAD 远程桥、历史装配影片工具、案例经验库和具体工作站配置不在这个可分发核心版中。接收方如需这些执行能力，应在自己的受控环境中按同一通用 CAD 证据合同接入；未调用的工具不得报告为已执行。
