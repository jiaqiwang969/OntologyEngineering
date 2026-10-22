# Rust 正式语义合同与支持范围

v2 执行身份为 `oev2.rust-semantic-kernel`。代码、版本、可执行摘要、输入、精确资产及绑定写入新记录。`oe-semantic` 复用包运输、锁、投影与来源合同，历史 Python Semantica 不再是 v2 正式执行权威。旧回执保存原身份，不改签。

当前功能替代与阻断接口见 [职责清单](SEMANTICA-REPLACEMENT.md)。书源、章节、registry/生命周期和工程审查各按实际入口验收。

## 实际支持

| 能力 | 实现 | 证明范围及限制 |
|---|---|---|
| RDF | Oxigraph 0.5.11、oxrdfio 0.2.6，默认持久化功能关闭 | 正式通用查询入口读 Turtle；章级入口另支持 RDF/XML 和逐文档图合并，空白结点分别重标。保留 IRI、词法值、datatype 和语言；16 MiB/100,000 quad 上限。没有远程加载 |
| SPARQL | spargebra / spareval / Oxigraph | 本地只读 ASK、SELECT、CONSTRUCT、DESCRIBE；拒绝 SERVICE 与 FROM 远程 dataset；输入/结果有预算，解析、执行和代数释放统一使用 8 MiB 栈。没有开放更新或联邦查询 |
| SHACL | 基于 Oxigraph 的明确 Rust profile | 直接 IRI path、四种 target、property、基数、hasValue、datatype、长度、pattern、nodeKind、in、or/and/not/node/class、子类关系与 SELECT `$this` 预绑定。profile 不等于全 SHACL；未知 predicate、路径或 datatype 拒绝执行 |
| 规则 | 有界正 Horn 固定点 | 一元谓词、同变量正合取、显式事实及推导来源；无否定、函数符号、多变量连接或概率规则。达到预算且未收敛时失败 |
| 来源与版本 | 核对不可变包、输入及绑定摘要 | 来源/对象/条件/版本变化要求重新审查；历史证据不继承 fresh 采用资格 |
| 审查 | 独立执行状态、结论、覆盖与失败 | `completed/partial/failed/not_run` 与 `pass/violated/unknown` 分开；缺必查项、空目标和不支持不能成为 pass |
| 治理 | 生成状态机、独立本地权限策略、内容身份与事件链；晋升重跑原回归 | 请求不能自授权；禁止跳级、重复后继、换基线和伪造通过。发布明确拒绝；完整 registry/书源与外部权限未闭合 |

支持说明以 `kernel capabilities`、`crates/oe-kernel` 和实际验收记录为准。标准依据为 [W3C SHACL](https://www.w3.org/TR/shacl/) 与 [SPARQL 1.1](https://www.w3.org/TR/sparql11-query/)。库来源为 [Oxigraph 官方源码](https://github.com/oxigraph/oxigraph)。未声称通过全部 W3C 一致性测试。

## 核对与依赖修正

发布的 spargebra 0.4.7 将连续减法及除法按右结合构造表达式。独立测试复现 `120−100−15−5=30`；本地 `vendor/spargebra` 补丁改为各优先级的左折叠，并保留括号与乘除优先级。它是明确标识的本地依赖修正，不冒称未修改的上游版本。详见其 `OE-PATCH.md` 与 `crates/oe-kernel/tests/arithmetic.rs`。ggen 上游源码和二进制未受该补丁修改。

452 个旧包场景使用原资产和原预期重跑，未修改查询、容差或反例。制造包的空结果列与 `sh:message` 检查修复发生在结果处理层，没有改资产判据。日志保留首次失败和修复后结果。

Windows 接收根实际构建后，规范化根读取在包执行前失败；旧 Windows 读取也接受字节相同的硬链接。新后端复用 `oe-windows-controller` 的固定根、父目录与普通单链接文件句柄。路径修复与新增 Windows 单链接拒绝要求分别登记，Unix 原运输判据保持。三包运输身份正例和硬链接反例修复前失败、修复后通过。

继续执行原场景又发现 Windows 小栈下证据包栈溢出。新执行用明确的查询栈，普通 panic 或无法创建线程成为执行错误。原 265 场景在实际 1 MiB 调用方复现失败、修复后通过；Windows CLI 空 PATH 的 119/265/68 原预期分别通过。新记录保留自己的可执行摘要，不继承历史结果。该修复不保证任意查询深度或总内存/CPU 时间安全；致命栈溢出尚无进程隔离回执，这些限制继续开放。

RDF 读取使用原始 `Dataset` 和官方 query evaluator，避免持久化 Store 将 `"003"^^xsd:nonNegativeInteger` 规范成另一词法值或 datatype，导致来源身份与 SHACL 结果改变。独立反例核对原词法值、datatype 与实际查询结果；没有放宽 datatype 规则。

## 正式项目审查

资产包验收、ggen gate、编译、模型评分和工程采用分别报告。直接 review 绑定 project/object/revision、package/version/hash、source、condition、required checks 与 authority。原合同审查通过 `forward::RustExecutor` 读取精确投影，再检查目标存在、目标类型、查询与 shape，保留兼容 DTO 和新 Rust 回执两个身份说明。

SHACL 对空目标可能形式上 conforms，但无法建立工程证据；项目层保留 unknown。局部覆盖只证明声明的检查，不推出原生几何、制造可行、检验放行、采购报价或发布授权。

直接 `kernel review` 通过 ggen 生成的 `SemanticReviewPort` 接口，实际读取 RDF 后核对 focus 和当前绑定。现有直接映射支持保留包的 `ReviewClaim`：对象须为其 RDF IRI，修订、scope、目标列表及当前来源必须匹配。另一对象的有效记录、缺身份或未建立映射的模型保持 unknown。更多领域的 focus 映射需要明确规范与独立反例，不能仅凭非空 RDF 或 shape conforms 通过。

当前采用另需 `kernel review --project <当前文件>`：`project context` 生成当前 Project 输入摘要（不含决定输出）、对象版本、来源与所选条件全文摘要；RDF 的同一 focus 另声明 `urn:oev2:project-context:conditionSha256`。两项生产检查分别核对当前 context 与 RDF 条件声明。载荷、支承或条件文字改变，即使 scope 同名、旧 binding/RDF/回执不变也会阻断；只重签 context 而保留旧 RDF 同样失败。检查保留原查询、形状、焦点与失败诊断，不以统一提前退出替代它们。

缺少当前 Project 时，直接审查完成原检查后增加 not_run/unknown 当前检查；`--historical-snapshot` 明确只审查旧快照。当前 `project work /3`、Harness 和经验 `/2` 接续当前检查；旧 work `/2` 只有原当前内容绑定范围，`project reconcile` 同时处理 scope 的隐含变化依赖。这证明输入身份一致，不证明任意自然语言与物理模型等价、未记录的现场变化或条件清单完备；这些仍需工程验证。

CLI 的执行完成与审查结论分开：`semantics review` 只在全部必要审查 clear 时退出 0；完成但 blocked/unknown/failed 时退出非零。`harness step` 的部分反馈保持独立状态。默认构建的旧 Python native/probe/preflight/install 路径拒绝执行；历史功能仅在明确的 `historical-contract-tests` 配置中保留，不属于 v2 正式能力。

治理策略字段、事件链与证据判据见 [本地治理合同](../references/governance-policy.md)。当前查询有输入及结果预算，但尚无查询 CPU 截止机制；这仍是开放问题，不把结果上限当作总执行时间上限。

规则调整需说明新的适用范围及来源；实现修复保留旧标准、独立预期和失败实例。不得用旧回执、空检查或执行结束来充当新引擎通过。

## 书章与有条件经验

`books migration-review` 从登记过的历史 wheel 读取 29 个原 manifest，核对资产摘要和当前章源，执行支持范围内的原 CQ、正/反约束、复合或 Horn 场景。旧 wheel 不作为执行器。此前开发版本的逐章结果为：75 观察为 62 通过、10 查询 DTO 精确差异失败、3 未知；未就绪的旧报告/端到端 oracle 仍 unknown。RDF/XML 图合并及质量章原预期已由当前 Rust 执行。

旧 plain literal oracle 使用 `datatype:null`，当前 RDF 1.1 返回 `xsd:string`；十项查询保留 exact mismatch。该开发版本另列版本化互操作比较，不改 oracle 字节，只兼容已明确的旧 simple literal；语言、其他 datatype、IRI、词法值与多重结果反例仍拒绝。第二卷规范资产另用原 CQ 与模态正/反例实际执行，其条款转述不替代受控 ISO 原文或工程合规。

SHACL profile `oev2.shacl-v1-corpus/2` 增加四种数值/日期范围与 `lessThan/lessThanOrEquals`、日期词法检查、SHACL 声明前缀。每个不能比较或违反顺序的字段对产生 finding，使用锁定 Oxigraph 的实际值比较，避免把大整数全转 f64。旧质量资产以裸 namespace IRI 引用前缀时，回执单列实际解析到的文档前缀兼容；不冒称标准的 sh:declare。冲突前缀、未支持表达、预算超限继续拒绝。[SHACL 比较合同](https://www.w3.org/TR/shacl/#LessThanOrEqualsConstraintComponent) 与 [前缀合同](https://www.w3.org/TR/shacl/#sparql-prefixes) 是独立预期来源，未声称全 SHACL 一致性。

`knowledge experience` 在同一 Project/Binding/包/来源/条件上调用生成的 `SemanticReviewPort`，只比较冻结判据下的必要条件诊断。这里“条件清单 complete”仅指 agent 明确列出的下一决定条件，不指完整产品要求；清单是否足够仍需独立审查。SHACL 正例与缺条件查询可同时出现，此时结论 unknown，不改签成 pass。

新评估协议 `/2` 的基线与候选均保留必要审查及当前条件检查；检索候选只改冻结来源池的选择顺序。稳定实验身份和全部相关原始拒绝事件在同一 knowledge 账本中校验，新的可读依据允许继续调查，不等于改善或物理证据。旧 `/1` 候选、评估器与回执保持原身份，拒绝后恢复原字节基线是 `/2` 明确新增的状态边界。

当前规则能检查已经整理的条件差集，不能自行从任意对话发现全部物理条件。经验可以告诉 agent 下一次先整理哪些缺口；物理机理、CAD 实现、制造、检验、费用和当前采用继续需要各自证据。私有候选冻结/试用/恢复事件不是语义包正式晋升回执。

## 已声明工程前提与模型边界

当前 `engineering-work/3` 使用同一 Project 的 `engineering-basis/1` 条件：目的/验收、源事实、必要模型假设、资料适用范围、跨尺度支持、功能承接及开放反例由 Rust 分别检查。类型来自注释规范和真实 ggen，算法在 `oe-kernel::engineering`。当前目的影响与 Project reconcile 共用声明边界；明确的受影响决定作为依赖闭包的起点，反例可重开其下游。原查询、shape、当前 context 和 RDF 绑定诊断仍执行。

独立冻结反例实际检出并修复了目的过度影响无关标签、反例只影响起点而漏下游两个缺陷；原预期不变，修复后实际 CLI 重测。源事实由 agent 整理，source hash 一致不证明内容理解正确。理论计算完成、已声明模型前提一致和工程验收是三个状态：自然语言数值验收尚无完整自动执行映射，现场真实性、量测和责任批准仍未知。详见 [工程前提合同](../references/engineering-premises.md)及[体系验收](ENGINEERING-SYSTEM.md)。

数值验收、孔列当前意图与机构运动作为EngineeringBasis中有源assertion由同一Rust正式检查执行，原RDF/SPARQL/SHACL/规则检查保留。NumericAcceptance将当前要求/状态/基准/单位、上下限/容差与模型/观测、不确定度和完整来源/条件快照绑定；数学诊断、模型适用、数值符合、工程采用分开。原查询/形状与当前身份的通过不能抵消数值超限或未知物理前提。

经验评估/3单独绑定原件和派生RDF，复用Rust port的sourceSha256、当前条件检查和全部工程检查；旧评估/2同字节判据与原失败不改。ggen的类型校验拒绝未支持角色用词，但不会补造测量或认证工程事实。最终对象、状态、计量口径/覆盖及校准缺项仍保留unknown；旧书章/生命周期/registry未迁移范围继续列原清单，不恢复Python执行作为备用。

图纸参考意图现作为同一EngineeringBasis中的 `cad_drawing_reference` 执行。正式CAD入口检查原生任务、输入资产与Project声明的完整观测一致；kernel/Harness/work/experience重新计算声明观测的点匹配与源绑定，不以自报pass代替。纯内核层的观测是有源agent声明，并非OS原生执行证明。requirements=null时给出 `drawing_reference_intent_missing` 和必要输入/影响；唯一模型父对象、标注值、公差及工程采用继续单独取证，原RDF/SHACL/数值/运动检查保留。
