---
name: ontology-engineering
description: Use Semantica as the single executable semantic authority for engineering, and connect CAD evidence with manufacturing process and cost decisions.
---

# 工程本体论（独立核心版）

先确认工程对象、身份、版本、状态和来源，再陈述关系、能力问题（CQ）、约束与因果假设。项目观察进入 ABox；可复用概念与关系进入 TBox；候选规则和经验未经验证不得冒充事实。对未知项维持开放世界假设（OWA）；仅在明示的局部完整范围内使用封闭世界假设（CWA）。

正式本体查询、SHACL、规则和项目语义审查只由本包锁定的 Semantica 执行。语义回执要与工程决定、实物试验、成本批准分别记录；任何单项 PASS 都不自动构成产品放行。

处理零件设计/修复、图纸/BOM、装配、机构和原生 CAD 回读时，加载 [CAD 模块](skills/cad-agent/SKILL.md)及[通用工作流](skills/cad-agent/references/cad-engineering-workflow.md)。先核对配置、来源、定义与实例、关系、状态和主张；项目 ABox 由有来源的证据适配器投影。当 CAD 对象可能改变工艺选择，或新工艺要求 CAD 复核时，按 [CAD／工艺交接](skills/cad-agent/references/cad-process-integration.md) 建立必要功能、候选工序、覆盖主张、双向问题和变更影响。名义几何、运动动画和仿真边界各有证据范围，不能自动成为实物功能结论。

装配和力学判断另读[通用内核](skills/cad-agent/references/assembly-and-physics-kernel.md)：实例身份、完整界面图、逐动作状态、外部支撑、装入路径、载荷需求和接头容量各自取证。公开的装配/机构筛查器只对声明的有限几何域给出计算结果，不替代实物强度、动力学或 Semantica 正式语义审查。

从照片、视频、专利或残缺 CAD 反推形态与机构时，另读[证据驱动的重建方法](skills/cad-agent/references/evidence-driven-reconstruction.md)。先区分看见的、推断的和本方设计的；用物理、机构学与数学补出条件约束，并保留现有证据无法区分的候选。推导结果通过来源锁定的记录交给 CAD 证据适配器，不把候选伪装成原物事实。

处理不完整客户需求、工艺方案、设备复用、成本和交期时，加载 [制造工艺与成本模块](skills/manufacturing-process-cost/SKILL.md)。先复原客户目标、使用情境与功能，再把路线逐项连接到必要功能、验证方法、资源和成本；缺少的前提应标成开放问题。工艺替代须核查被原工艺顺带实现的所有功能，不能只比较主要加工效果。

每次关键判断保留四类东西：输入快照及 SHA-256、对象与版本映射、证据/假设/冲突状态、决定及其反证条件。变更后沿依赖关系重开受影响的工艺、检验、费用、交期和对外答复。

安装与复验见 [独立分发说明](docs/PORTABLE-DISTRIBUTION.md)。本核心包不含两卷书、客户资料、历史 CAD 案例或工作站配置；包内原生案例仅为合成方法验证。接收方的本地 CAD 软件、项目事实、许可和审批需单独提供。
