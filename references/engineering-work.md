# 从工程缺口到当前项目证据

用户讨论目的、条件和取舍，agent 整理本合同。入口为 `oev2 project work`，实现为 `oe-engineering::workflow`；消费现有 `oe-kernel::project::Project`，不建立另一项目账本。共享 `EngineeringGap`、`EngineeringSourceRequest`、`EngineeringCalculation` 沿带注释规范 → Rust 规范化 RDF → 锁定真实 ggen 产生。

先写目的与验收、必要功能及输入输出，再提出可被计算或测量区分的物理机制。机制是假设时明确标注。对象、修订、条件范围、来源摘要必须与 Project 一致；缺条件不能靠“资料相关”抵消。所有计算注明量纲、假设、边界、失效条件和当前 Project 证据 ID。

当前输入使用 `engineering-work/3`，包含从同一 Project/ObjectIdentity 调用 Rust `project context` 得到的 `project_context`，并在同一 Project 条件中整理工程前提。检查覆盖 Project 审查输入、对象修订、所选条件的完整 UTF-8 内容与声明来源摘要。只更新 Project 文件引用摘要、保留旧 context 时拒绝；旧 `/1`、`/2` 保留历史诊断意义，不能自动升级为已核工程前提。条件、来源真实性与模型假设还须工程证据，内容摘要不能代替它们。

按缺口安排查证，不要求每个任务强行引用六本书。`source_role` 与来源绑定：ontology_method/vol1 组织对象、关系和条件；trust_method/vol2 组织主张、证据、依赖与变化；strength/roloff-matek 查载荷、变形与寿命边界；kinematics/norton 查自由度、轨迹及动力学；tolerances/interchangeability 查公差、配合和测量判断；dfma/dfma 查制造、装配及费用依据。local-pdf 请求只从指定书取得页候选；该书没有证据时保持空结果，不由别的书替代。

supplier/misumi-offline 使用历史米思米原页，用于选型与安装条件线索。具体产品须另外核对完整型号、配置、现行规格与 CAD；采购价格、交期没有当前证据时未知。旧版次、物理页、印刷页、提取身份各自保留。阅读整页及关联图表后才能判断候选适用；关键词、标题或模型相关性判断不足以采用物理结论。

当前计算模型为 `beam-cantilever-end-load/1`、`beam-simple-center-load/1`、`round-shaft-second-moment/1`、`linear-error-worst-case/1`。单位严格为 N/mm/MPa/mm4；不暗中把 m 当 mm。前两者分别假定端部悬臂载荷、简支跨中载荷，不能互换边界条件取得较好结果。误差上界求和不假定统计独立。缺输入输出 not_run/unknown；负的误差界、非有限值、非法单位或模型边界拒绝。模型结果不是材料认证、实物测量或工程采用。

CAD/工艺交接复用 `oe-cad::handoff` 的文件与证据审计、双向问题。交接包须绑定同一 Project 和对象修订，缺原生几何/装配/制造/检验证据时不补造。NX 建模、读回、保存、独立重开、交换文件和物理结论分别验收。此次查证入口不会自行执行 NX；已有 NX worker/控制器入口沿原合同运行，Fusion 保持退役。

当前 Agent 组织工程工作并审阅缺口、来源适用性和下一检查。`project work` 默认执行本地 Rust 计算与来源查证，不调用专用判断 API；检索结果保持 `applicability=not_established`，内容支持与工程适用性仍须回读原件。内置 engineering-memory 通过 `oev2 memory` 使用，是否召回经验按当前需要决定。Agent 的解释和经验线索都不直接产生工程采用决定，仍以当前对象、条件、来源和实际验证核查。见[协作编排](../docs/AGENT-ORCHESTRATION.md)。

agent 阅读原页、记录采用/拒绝理由后，把证据 ID、摘要、适用条件及必要检查续接 Project/Harness 和 `kernel review`。ReviewClaim 映射之外的领域对象仍需正式映射；未执行检查保持 unknown。变化用 `project impact/reconcile` 重审，原工作输入和旧输出保留。学到的工作方法、项目事实、软件缺陷与领域机理候选分开记录，已有表达足够时 no_delta。

本轮导向例是明确假设参数的开发回放，不是实物项目或独立迁移。受控 STEP 四进程往返已取得 v2 原生证据；全链工程采用、实际装配导入/保存、复杂机构与碰撞、成本和未见任务收益仍待验证。

历史开发版本接续真实项目的制造/检验目的、原始纠正和阶段条件，查源、模型辅助、Rust 计算及交接保留各自原执行记录，不能改签为当前版本通过；不适用的教材工艺页与供应商空气配件没有成为该项目证据。原生 UF2-5 导入/保存/独立重开属于另一个工具验收任务，不能用它证明该项目产品已建模、量测或采用。

当前模型前提合同见 [工程前提与模型边界](engineering-premises.md)。`/3` 在现有 Project 条件中绑定源事实及必要模型前提；支持旧 `/1`、`/2` 的历史意义，不自动将其升级成已核物理条件。实际计算、资料适用、函数承接与反例分别报告原因和决定依赖，字段齐全、摘要更新和检索 ready 不能代替相应验收。

有源薄膜牵引问题可使用[三个明确的牵引模型](../docs/TRACTION-MODELS.md)：单主动接触μN、所需摩擦比F/N及稳态接触功率Fv。每个模型保留原必要前提、单位、未知和违反；正确算式不证明原载荷含义、材料摩擦或电机选型。
