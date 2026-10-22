# 从工程缺口到可判别验证

本候选接续完整原63项目标、完整Semantica及原工程验收。新增 `project explore freeze / verify / run` 使用既有 Project、work `/3`、engineering basis、当前任务视图和 Harness；原 Project/work/basis/experience、参数比较及供应商 design-attempt 协议不改。正式入口是否具备此能力，以其安装原件、当前程序和生成合同为准。

## 工程动作

Agent 围绕影响当前决定的缺口维护 `oev2.engineering-exploration/1`，绑定要求、对象版本、当前条件和来源。已有表达足够时直接记录实例或 no_delta。计划区分已经观察的来源、仍未知的条件、有来源的解释和待查假设；假设指向既有未采用决定，不写入 Project 事实。

选择动作要说明验证目的、可区分的解释、预期决定影响、必要要求、实际执行条件及成本依据。没有来源的成本写 unknown；不生成虚构的信息收益或置信度。每个候选用原 work 声明模型前提、计算/调查方法及来源，保留完整父级验收和剩余义务。不同对象须通过原 goal_support 检查，并保留父级外部条件。

先 freeze：保存原计划、独立预期、来源闭包和结果判据，再 verify 重新检查当前原件。run 按冻结顺序与已观察分支决定是否执行下一动作，并遵守冻结动作上限。当前 owner 实际支持既有 Rust 计算、来源字节完整性与严格 JSON 回读；外部查证、原生仿真或测量保留明确 not_run，须由相应原生/测量 owner 接入真实回执，不能代填完成。

Rust 回读保存实际数值/单位、原模型适用性、失败、部分结果、缺值、重叠分支和无法区分。单位不隐式转换；严格 `<` 上限不改成 `<=`。未知执行条件可以被调查，计算及工程采用仍接受原检查。

匹配唯一有效分支后，结果只更新限定来源或模型范围的未采用决定，记录候选保留、限定否定或重审。原因假设仍为 unknown；数值匹配可以说明模型预测符合该判据，不能独自证明物理原因。没有匹配、比较失败或多个匹配，相关决定保持 unknown。原事实、条件、对象与必要验收不改；原 Project 字节保留，新副本添加结果证据。现有 owner 再派生任务视图和真实 Harness 检查点；Harness 失败保留原输出与 partial_or_failed，不冒称工程通过。

## 当前会话接替判断后的实际接续

候选新增 `project explore advance --input <请求>`，使用显式 `oev2.exploration-host-input/1`；合同由真实 ggen 生成，原 freeze/verify/run 的含义保留。当前会话整理计划和工具输入，Rust 检查并维护可追溯状态。当前正式安装是否具备该入口，仍以安装回执为准。

1. 当前会话先写完整计划并 freeze。第一次 advance 绑定 frozen，previous 与 receipt 为空，output 为新的私有目录。Rust执行已有本地检查，遇到需要外部结果的动作停止，列出该动作、工具输入、对象/条件、完整验收、判据及不同结果的影响。返回 `requires_host_agent`，CLI退出1，表示等待；不表示工具已经执行。
2. 当前会话按原授权调用对应工具，保留实际输出原件。下一次 advance 引用上一份 completed.json 的摘要，并提交一个 `action_id / owner / artifact` 回读。只能提交当前待办；直接交结果、跳步、重复交旧动作、篡改或丢弃前态均拒绝。
3. Rust重新核全部冻结来源和步骤历史，按冻结判据回读结果，保存新的Project/work、task-view与真实Harness。原Project原件不变，新副本仍接受原内核采用审查。没有匹配、失败、部分完成与无法区分保留 unknown/needs_review；已记录结果的状态为 results_recorded，不表示工程完成。
4. 下一动作只在实际前置结果支持时出现。失败后的重试或改变路线须另立有依据的计划并关联旧记录；该入口不重复浏览器、供应商或原生CAD副作用。

当前有两个明确回读范围：`project_work` 验证既有无检索/CAD handoff的计算work实际输出，重跑原Rust计算与工程前提检查，核Project内容/对象/完整验收及结果；`source_statement` 接收带原件的会话观察，只能支持对应来源陈述，不能证明模型预测或物理原因。后者使用生成的 `oev2.host-source-observation/1`，明确声明来源、局限、冻结任务和当前对象。源摘要一致仅证明内容身份，不证明会话解释正确。原生CAD、测量、仿真及供应商执行仍须对应owner自己的验证适配，不能填一个completed标志获得认证。

新状态为 `oev2.exploration-host-state/1`，引用旧owner的共享执行产物；内部结果与评估显式使用 `/2`。前态由请求、累积回读和原计算重建，原工程事实、验收与失败不改。等待结果时评估为 pending_verification；开发回放不证明方法收益。记录的零模型调用仅属于该Rust owner，当前会话、engineering-memory、外部工具及全任务费用仍分别核算。

## 参数优化和机制探索

parameter 复用 `oev2.design-comparison/1` 的原模型身份、假设、边界、载荷、来源和有限设计变量政策。换计算模型、改变共同载荷、删除要求或放松验收不能制造改善。

mechanism 允许另声明实现原理、构造、表示或模型，保持共同父目标及完整验收。检查旧比较合同是否适用；需要扩展时使用显式新探索合同、扩展理由、冻结分支和独立预期，原 prior_contract 仅按原身份保留。新合同不自动继承旧参数比较的通过或收益。未执行的替代路线仍有自己的验证方法和剩余义务。

原 design-attempt::prepare 实际需要 IncadWorkPreparationRequest 和准备好的 inCAD 来源/CAD角色，继续用于该适用范围。通用原因调查、书籍/对话方法和源码证据从 Project/work/Harness 进入，不能虚构 inCAD 来源。

## 规范、执行和方法评估

`specs/modules/knowledge-query-planning.toml` 为共享合同、接口、约束与验证正本，经锁定真实 ggen 生成。完整私有活动体字段在 `specs/owner-schemas/engineering-exploration.json`，与既有 WorkInput 模式一样由 workflow owner 严格解析；它的 SHA 同时进入 specs 来源、编译器输入、Rust include_bytes 和冻结闭包。两种共享执行 DTO 由 ggen 生成；五种私有活动体保留全字段及严格未知字段拒绝，没有第二个 Project 或晋升权威。该方法补充最初使用 normalized `/2`；当前候选组合规范接续到显式分组的 `oev2.design/4`、`oev2.normalized-design/4` 和 `oev2.design-composition-source/3`，逐项保留完整检查与默认值/覆盖值的原始来源，仍保留 16MiB 输出预算。旧版本的含义、判据和冻结回执按原身份保留，不把新版生成原件回写为旧版本。具体接收范围见[组合规范](DESIGN-COMPOSITION.md)及本轮原件。

既有 experience `/1–3` 实际评估语义审查覆盖及可选检索排序回放，不足以评价一般探索收益。新增 `oev2.exploration-assessment/1` 比较预先独立记录的动作顺序、所有假设决定、完整验收，以及实际动作数/模型调用数与冻结预算。仅说明 development_replay 的行为和实测局部成本；总成本另记来源创建、CAD、工厂与完整任务，未知部分不写零。

局部模型指标、候选可行性、整体工程达成和方法收益分别记录。当前评估保持 method_benefit/whole_engineering_achievement unknown，不能由它直接晋升方法。后续需冻结 baseline/candidate、完整质量与成本、资料曝光和后续新任务，再沿原RRSI审查/采用/撤回流程评估。不要把同一已曝光案例重复成功当成独立泛化。

## 本轮证据范围

核聚变对话属于已曝光 development_replay。解读中的关键判断追溯原 prompt1/prompt2、SymPy notebook 与局部验证：一个拉伸构造的非零curl/力平衡障碍只否定该构造和条件；换压力、换二次构造或真空边界属于不同模型义务，局部恒等式不证明全局磁面、稳定性、粒子约束或线圈可制造性。这些是方法启发，不是本skill工程有效性证明。

实际主线选择曾经发生的查询预期文件漏输入登记问题：冻结完整原63项，先查原查询/扫描字节是否变动，再回读真实旧 Rust owner 的输入清单，按实际结果继续查当前修复。原失败和成功回执保留；最后得到有来源、有限范围的决定变化、任务视图与 Harness。报告、源定位、冻结原件和运行回执在该轮私有维护记录中，发行不包含私人原对话/项目材料。

完整Semantica、公共新profile、独立接收、平台发布、NX/SW完整装配与施工图、实际项目精度验收及方法收益继续原主线，不因本增量减少范围。
