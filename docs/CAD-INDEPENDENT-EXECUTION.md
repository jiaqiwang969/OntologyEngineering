# CAD 独立执行

ontology-engineering-v2 承担 CAD 执行，不依赖原 ontology-engineering 的 cad-agent 安装。工程本体规定对象、版本、条件与证据含义，ggen 生成共享合同；v2 Rust CAD 模块执行，原生结果回到同一 Project 审查，实际反馈进入既有改进流程。

## 当前执行链

- NX：本根 `oev2 cad nx` → `oe-cad::nx` → 显式 Fleet/SSH 传输 → 按摘要核验的 `oe-windows-controller` → 任务绑定的 `oe-nx-worker` 与 NX 原生 SDK。控制器、worker、原生运行库和输入各按实际身份取证；旧 schema 名称不指定可执行程序。
- SolidWorks：本根 `oe-sw-worker` 与 `oe-sw-session-host` 实现受控原生 COM 操作；`oev2 cad solidworks` 核验原始记录和独立重开证据。可执行范围及实际资格见 [SolidWorks 原生 worker](SOLIDWORKS-NATIVE-WORKER.md)，不能把记录核验当成已执行 CAD。
- SolidCraft：按[源码锁](../integrations/runtime-locks/solidcraft-source.json)固定 fork 提交的 `solidcraft-cli` 原生 CLI/MCP 执行，保留真实上游与 SolidCore 依赖来源。当前 Agent 调用、读取原件和解释结果，v2 既有 Rust `project tool-result verify/register`、`project decide` 与 workspace 接收来源和状态。见 [SolidCraft CAD](SOLIDCRAFT-CAD.md)。源码修订、实际程序与本机验收分别核定；`.sc`、参数时间线、测量、导出、独立重开和截图各自取证；没有新增 v2 CAD worker、`oev2 cad solidcraft` 命令或 NX observation 兼容承诺。
- CAD 证据、工艺衔接与设计依赖分别由 `oe-cad`、`oe-engineering` 和同一 Project 的既有语义审查承担。缺少图纸、装配、运动或其他原生动作时，在 v2 的实现与合同中补齐并验证，不退回旧工具执行。

## 选择后端

用户指定的软件、原生文件、既有模型与下游接口先决定候选；再核实际程序和本轮所需操作。三种入口不是互相等价的资格：

| 当前需求 | 入口 | 必须保留的边界 |
|---|---|---|
| NX 原生 `.prt`、现有 NX 装配/图纸、已绑定的 NX 原生检查 | `oev2 cad nx` 与 NX controller/worker | 沿原 NX 对象、occurrence、变换、保存及独立重开合同 |
| SolidWorks 原生零件/装配与已支持 COM 动作 | SW worker/session host 及 `oev2 cad solidworks` | 沿 SW 原件与实际 worker 资格，不由转换文件推定原生特征完整 |
| 开放源码参数建模、本机 `.sc`、MCP 交互或几何交换 | SolidCraft 原生 MCP/CLI | 当前 `.sc` 时间线与具体命令回读；STEP 是交换表示，目标 CAD 导入另验 |

跨后端时保留源模型、交换文件和目标原生文件各自摘要、单位、坐标、对象映射与损失。SolidCraft 的 body/face 名称不自动成为 NX 或 SW 身份。同一内核的多个入口不是独立算法证据；解析值、其他真正独立的计算或实测另列来源。

## 历史兼容边界

本根 `skills/cad-agent/contracts/` 是随 v2 分发的静态合同；Rust 在构建时嵌入相关 schema。`cad-agent.*` 协议标识、来源记录中的历史路径和历史资料接收器保留原身份，不表示调用旧 skill。不得为消除名称而改写历史回执、schema 或原验收预期。

原 cad-agent 的脚本、Python 环境、MCP、Journal runner 和默认配置均不能作为当前任务入口或失败备用。不得将旧执行器运行出的结果包装成 v2 当前执行成功。旧模型、教材、图纸、方法及回执可作为只读资料；新执行需自己的程序、输入和原生观察身份。

`cad solidworks verify` 接收第三方历史证据包，只校验来源完整性；它不会启动其中的 Python/MCP 适配器。历史资料接收命令只复制并核验来源，不执行其中的命令。

## 验收范围

独立运行需要验证任务可以只用 v2 分发内容、登记的原始资料、明确安装的原生 SDK 和工具完成。实际程序路径及摘要、输入、任务、原生回读、保存与独立重开分别记录。源码中未发现旧入口调用只支持静态依赖检查；完整功能迁移和各平台的真实执行仍需各自证据。

六书知识、供应商案例和 CAD 结果仍围绕同一工程问题使用。程序独立、模型可打开、施工图完备和工程适用是不同的验收项，缺项保留明确未知。
