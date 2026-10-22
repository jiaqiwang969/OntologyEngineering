# CAD 独立执行

ontology-engineering-v2 承担 CAD 执行，不依赖原 ontology-engineering 的 cad-agent 安装。工程本体规定对象、版本、条件与证据含义，ggen 生成共享合同；v2 Rust CAD 模块执行，原生结果回到同一 Project 审查，实际反馈进入既有改进流程。

## 当前执行链

- NX：本根 `oev2 cad nx` → `oe-cad::nx` → 显式 Fleet/SSH 传输 → 按摘要核验的 `oe-windows-controller` → 任务绑定的 `oe-nx-worker` 与 NX 原生 SDK。控制器、worker、原生运行库和输入各按实际身份取证；旧 schema 名称不指定可执行程序。
- SolidWorks：本根 `oe-sw-worker` 与 `oe-sw-session-host` 实现受控原生 COM 操作；`oev2 cad solidworks` 核验原始记录和独立重开证据。可执行范围及实际资格见 [SolidWorks 原生 worker](SOLIDWORKS-NATIVE-WORKER.md)，不能把记录核验当成已执行 CAD。
- CAD 证据、工艺衔接与设计依赖分别由 `oe-cad`、`oe-engineering` 和同一 Project 的既有语义审查承担。缺少图纸、装配、运动或其他原生动作时，在 v2 的实现与合同中补齐并验证，不退回旧工具执行。

## 历史兼容边界

本根 `skills/cad-agent/contracts/` 是随 v2 分发的静态合同；Rust 在构建时嵌入相关 schema。`cad-agent.*` 协议标识、来源记录中的历史路径和历史资料接收器保留原身份，不表示调用旧 skill。不得为消除名称而改写历史回执、schema 或原验收预期。

原 cad-agent 的脚本、Python 环境、MCP、Journal runner 和默认配置均不能作为当前任务入口或失败备用。不得将旧执行器运行出的结果包装成 v2 当前执行成功。旧模型、教材、图纸、方法及回执可作为只读资料；新执行需自己的程序、输入和原生观察身份。

`cad solidworks verify` 接收第三方历史证据包，只校验来源完整性；它不会启动其中的 Python/MCP 适配器。历史资料接收命令只复制并核验来源，不执行其中的命令。

## 验收范围

独立运行需要验证任务可以只用 v2 分发内容、登记的原始资料、明确安装的原生 SDK 和工具完成。实际程序路径及摘要、输入、任务、原生回读、保存与独立重开分别记录。源码中未发现旧入口调用只支持静态依赖检查；完整功能迁移和各平台的真实执行仍需各自证据。

六书知识、供应商案例和 CAD 结果仍围绕同一工程问题使用。程序独立、模型可打开、施工图完备和工程适用是不同的验收项，缺项保留明确未知。
