---
name: electronics-design
description: >-
  Coordinate electronic circuit and PCB engineering under ontology-engineering-v2,
  retaining the same Project, object revisions, conditions and evidence through
  component selection, EasyEDA Pro schematic/PCB work, simulation, review, component
  3D model extraction, board handoff and worker wiring cards. Use for 电路设计、原理图、
  PCB、嘉立创EDA、器件选型、仿真、器件三维模型、发板准备、工人接线卡与焊接指导.
  EDA execution uses easyeda-agent; project-specific parameters are not defaults.
license: MIT
metadata:
  author: jqwang + Claude
  compatibility: macOS 本机；实际操作按需使用 EasyEDA Pro、easyeda CLI/连接器、Node.js、Python、ngspice；随统一仓库使用；外部 EDA 程序和求解器按 INSTALL.md 单独安装。
  version: "2.1.4"
  integration_revision: "ontology.1"
---

# 电子设计：v2 主导的专业能力

从所需功能、物理机制和实际对象开始，沿 `ontology-engineering-v2` 的同一 Project
推进电子设计。v2 组织对象、模型、方法、实现、主张与证据关系；本 skill 选择电子专业
工具并把结果交回。用户用自然语言提出需求，agent 维护合同与来源。

这是与 GitContribute、LeanToken、REA、Jacobian、SmokingGun、flameox 并列的第七种
专业能力；本目录 `modules/` 下的五个专业模块承担内部执行分工，与本入口合计六部分。
对外只使用本 `SKILL.md`，各模块的 `GUIDE.md` 按需读取。接入深度以真实调用、独立核验、
Project 决定及变更复核为准。当前结果交接合同和验收范围见
[专业能力接入](references/capability-integration.md)，仿真、EDA 与实物各自保留验收状态。

## 先接续对象和条件

使用当前可用的 `ontology-engineering-v2`，先读其 `SKILL.md`。本轮要运行程序时，按
其 `docs/USAGE.md` 与 `var/install/INSTALLATION.json` 核实际 CLI、根与摘要，不把源码版本或历史命令当当前入口。
对象映射、证据交接和失效关系见 [v2 接续](references/v2-handoff.md)。

- 已有 Project 时保留其 project_id、完整父目标、对象修订、条件、来源和未决验收；
  不从 cwd、会话号、供应商型号或历史案例猜项目。指定会话先核原消息与项目原件。
- 先区分本轮是资料研究、模型试算、原理图候选、PCB 实现还是样机验证。资料中的电路、
  数值和“验证成功”只覆盖原试件与条件，不能直接成为目标产品规格。
- 研究阶段可以提出可证伪候选，不需要填造器件值或接口。进入具体设计时，明确所需
  电源、信号范围/参考面、时序、负载、连接器、机械安装、制造与测量条件；缺项保持未知，
  只在该缺项会改变当前动作时补查或询问。
- 多会话共享项目时核当前指针及其摘要，保留不可变输入快照；其他会话在提交的新修订
  未闭合前，不覆盖其正本，也不把中间文件称为正式当前。快照只用于本轮绑定与交接。

普通文件检查或方法讨论按需要读取资料；执行 v2 工程判断时必须有真实 Project 绑定。
v2 不可用时说明缺项，可做有范围的资料整理，不声称完成 v2 审查或工程采用。

## 按缺口选择工具

| 当前问题 | 专业入口 | 交回的证据 |
|---|---|---|
| 器件选型、符号/封装匹配 | EasyEDA/LCSC 实际库、厂商原始资料 | 精确 MPN、供应商编号、封装/引脚/额定值、资料版次与适用条件 |
| 原理图、PCB 布局布线及原生检查 | [easyeda-agent](modules/easyeda-agent/GUIDE.md) | 当前工程/文档/页身份、操作前后状态、pin→net、规则、保存文件和检查原件 |
| typed action 未覆盖的操作 | [easyeda-api](modules/easyeda-api/GUIDE.md) | API/工具版本、明确输入输出与回读；沿主工具的 debug 授权条件，不借底层绕过阶段约束 |
| 器件放置、短线接网络标记/端口 | [easyeda-schematic-net-fanout](modules/easyeda-schematic-net-fanout/GUIDE.md) | 实际引脚与网络对应、真连线和布局；不以同名文字证明连接 |
| 电路模型和动态响应 | 实际可用的 ngspice 等求解器 | 网表、模型来源、边界、单位、配置/日志/曲线、收敛与适用范围 |
| 器件三维模型 | [lceda-jlc-3d-model-extraction](modules/lceda-jlc-3d-model-extraction/GUIDE.md) | 原模型、派生关系、封装/原点/方向/尺寸与对齐证据 |
| 数据手册、BOM、EMC/制造审查 | 已安装并核可用的 kicad-happy 等工具 | 审查对象、来源、覆盖、未知和冲突；安装目录不等于已审查 |
| 工人接线与焊接指导 | [make-worker-wiring-card](modules/make-worker-wiring-card/GUIDE.md) | 实物身份、接线矩阵、线色/丝印证据、打印文件与未决项 |
| 板与机构的机械接口 | v2 自有 CAD/NX 入口 | 板框/孔/禁布/高度与装配条件、原生保存回读；按 v2 独立执行合同，不走旧 CAD runner |

每次按需读取表中模块指南；其 `scripts/`、`references/` 和 `assets/` 均留在各模块内。
运行 `easyeda` CLI 的辅助脚本前，把 `EASYEDA_SKILLS_DIR` 设为本包的 `modules/` 实际绝对路径；
统一仓库中为 `export EASYEDA_SKILLS_DIR="$ONTOLOGY_ENGINEERING_ROOT/modules/electronics-design/modules"`。
CLI、连接器按 [INSTALL](INSTALL.md) 的固定上游加真实 local.4 补丁成套构建，
指南的 0.25.1 与运行程序的 0.23.0-local.4 分别记录；求解器按实际入口运行。更新上游模块时保持本布局和总入口链接，
保留上游版本与本地改动，不重新安装为同级独立 skill。

EasyEDA 是当前主 EDA 工作流；工具缺失不自动改用其他 EDA、
安装软件或恢复旧服务。求解器、API 桥、连接器、CLI 和应用各自核当前可用性。

## 执行和检查

画图前按当前 `easyeda-agent` 检查 health、动作目录、目标窗口及文档，恢复既有阶段状态。
修改前 inspect，保存后核原生数据，PCB mutation 后按工具要求 reload。
多副本可能共享内部 UUID，删改前核 parentProjectUuid；网表、截图和磁盘文件的身份分别保存。
当前用户要求后台操作时不得抢焦点；若检查必须前台渲染，记录该项未执行并说明具体限制。
连接超时先核实际执行状态，不能盲目重放。历史 API 经验按需见
[操作记录](references/easyeda-operational-notes.md)，当前工具实际接口优先。核 PCB 额外属性、
双侧网表或连接状态时，读 [属性、网表与连接核验](references/easyeda-readback-and-lifecycle.md)。

沿项目本轮已有授权推进。设计、局部试验、制造交付和实际上电的授权及证据分别核对；
skill 内出现历史命令不意味着本轮授权执行。阶段确认针对已经做成、可审阅的结果。

把以下结论分别记录：

1. **模型结论**：元件模型、负载、激励、参考面、结果、简化和遗漏。
2. **电气连接**：独立设计意图/引脚功能与实际 pin→net 一致；原理图和 PCB 网表对应。
3. **EDA 检查**：DRC/ERC/布局或制造检查具体覆盖什么，规则值来自哪里；没跑和不支持不算通过。
4. **实现与实测**：保存/重开、实装版本、接线/仪器/校准/工况、观测和不确定度。
5. **工程决定**：依据支持哪项当前要求，哪些仍 unknown，是否具备相应采用资格。

DRC 无错不能证明电路功能、噪声、动态恢复或目标产品兼容。仿真与截图不能替代实物测量。
制造文件、BOM、贴片坐标和检查绑定同一板修订；修改器件/网络/层叠/规则/模型或工况后，
沿依赖关系重审受影响的结果，不把旧通过沿用到新字节。

## 规则维护与历史边界

本接入复用 v2 既有 Project/context/来源与审查合同，通过专业工具执行电子任务。
外部结果先整理为 `oev2.electronics-native-observation/1`，再由
`oev2 project electronics project` 做 Project、路径、SHA、辅助原件和状态投影；这条外层桥
不启动 EDA、不重算专业规则，也不把投影当作工程采用。五类操作的详细字段仍由各自原生
source 合同逐步收紧，也不表示旧 SHACL 已迁移进当前内核。
新增 v2 自有可执行规则时，在 v2 `specs/` 表达含义、输入、错误/状态和来源，沿
真实 ggen → Rust → 独立正反例与接收流程实施；不能在本 skill 新造平行检查器并自称 v2 权威。

历史项目的 Python/SHACL、项目参数与旧门禁不随本模块分发，也不成为当前 Rust 内核规则。
新增电子规则须有来源、独立预期和反例，并沿本系统规范、生成与接收流程落实。
本包只带专业指南、必要上游 Skill 资源与自有辅助文件；来源、许可和差异见
[README](README.md)、[INSTALL](INSTALL.md) 与 [NOTICE](NOTICE.md)。

项目实例、源会话、工作快照、物理参数与私有配置放在项目或 v2 `var/` 中，不进入通用
skill 分发。完成一轮保存真实变化、失败、纠正、未执行项与下一依赖；可迁移经验交
内置 `oev2 memory` 时记录实际使用，收益须有可比证据。
