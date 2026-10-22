# 当前工程前提与模型边界

用户说目的、工况、取舍与纠正；agent 从原件和真实过程整理以下内部合同。它不是要求用户填表，也不是自动理解所有自然语言的规则。

同一 Project 的 `conditions[condition_scope]` 可以保存 `oev2.engineering-basis/1` JSON 文本。对象、版本、目的、验收、事实与主张继续属于现有 Project；不新增项目账本。共享类型由 `specs/engineering.toml` 经 Rust RDF 和锁定的真实 ggen 生成。

| 字段 | agent 填写要求 | 检查与限制 |
|---|---|---|
| `object_id/object_revision` | @required 同一工程对象与修订 | 必须与审查对象一致 |
| `purpose/acceptance` | @write 保留原目的及必要验收，不自行降级 | 目的偏离检出；work `/3` 与此验收逐字绑定。自由文字数值验收尚不自动执行 |
| `facts[].property/value` | @field 工况、结构或状态的明确属性；数量用 `{value,unit,precision}` | 同名属性唯一。数值有限、单位明确；未做自动单位换算、误差传播或自然语言等价判断 |
| `facts[].status` | `declared/assumed/unknown`；缺信息用 null/unknown | assumed/unknown 不转成已知。declared 是来源绑定的整理，不证明原文理解或物理真实性 |
| `source_refs` | @required 现有 `Project.evidence` 的 ID；保留原件、页、提取与版本的外部记录 | 未登记来源拒绝；摘要一致不证明来源内容支持主张 |
| `assertions[].subject` | 具体计算、功能、资料采用或受支持主张 | 计算 ID 与模型 subject 一致；不可随意映射输入属性绕过必要前提 |
| `affected_decisions` | 已登记决定 ID，补齐 Project 依赖 | 计算影响闭包，报告受影响决定；未声明依赖不等于没有影响 |

agent 按性质选择主张：

- `model_boundary`：固定算法及必要前提。当前覆盖悬臂端载梁、简支中载梁、实心圆轴惯性矩、线性最坏尺寸链。载荷、跨度、材料参数、截面、支承、载荷位置、线性范围或尺寸链完整性分别核对。空心管不能套实心轴模型。
- `source_applicability`：原页相关之外，核对产品、介质、工况等已声明限制。限制整理仍须读原页，检索相关性或 Agent 的解释不替代适用性证据。
- `scope_transfer`：显式数量范围及单位；短样件不能因记录完整就覆盖另一尺寸或状态。
- `function_continuity`：工艺替换后，必要功能的实现、机制、验证计划是否承接。清单明确完整而无实现则违反；清单不完整、缺信息则未知。字段写齐不证明工艺有效。
- `counterexample`：新反例重开主张及依赖的工艺、检验、成本、交期决定。开放反例不自动等同物理失效；关闭须新证据，不能只改状态文字。

`assertions[].requirements` 是按种类选择的内部对象，不能只填自由文字：

| 种类 | 内层字段与格式 | 必要解释 |
|---|---|---|
| `model_boundary` | `{method, inputs}`，输入 Quantity 或 Quantity 数组，与计算的同名输入一致 | 模型的必要属性由固定算法合同决定，不能自选删减 |
| `source_applicability` | `{limits: {property: [allowed_value, ...]}}` | 枚举值为类别、布尔值或 Quantity；数量比较需要相同单位 |
| `scope_transfer` | `{limits: {property: {minimum, maximum, unit}}}`，也可使用枚举范围 | 上下界有限、包含边界；没有范围的来源不能编成无限适用 |
| `function_continuity` | `{function_id, inventory_scope, current_realization, mechanism, verification_plan}` | `declared_complete` 明确声明盘点完整；空实现不能靠完整清单取得通过 |
| `counterexample` | `{status: "open", evidence_scope}`；关闭时还需 `resolution_source` | resolution 必须在同一 Project 登记并引用；其摘要须区别于原反例依据。新文件身份不能自动证明反例已消解 |

固定模型的类别属性使用下列值；所有事实都必须有 `property/value/status/source_refs`。Quantity 使用 `{value, unit, precision}`，`precision` 是精度说明，不是允许差：

| `method` | 数量属性（单位） | 类别属性 |
|---|---|---|
| `beam-cantilever-end-load/1` | force（N）、span（mm）、elastic_modulus（MPa）、second_moment（mm⁴）；单位字符串按当前计算合同分别写 `N`、`mm`、`MPa`、`mm4`。MPa 与 N/mm² 物理等价，但入口不自动替换字面单位 | support=`cantilever_fixed_free`；load_position=`end`；regime=`linear_elastic_small_deflection` |
| `beam-simple-center-load/1` | 同上 | support=`simple_two_end`；load_position=`center`；同一 regime |
| `round-shaft-second-moment/1` | diameter（mm） | section=`solid_round`；不用于空心截面 |
| `linear-error-worst-case/1` | components（mm Quantity 数组） | error_model=`linear_worst_case`；coverage=`declared_complete` |

例如 agent 从已核来源整理悬臂模型时，主张可以写为以下结构。这里是内部格式片段，`beam-check` 必须对应当前计算 ID，来源和受影响决定须是实际 Project 的现有 ID；示例数值不属于真实工程事实：

```json
{
  "assertion_id": "cantilever-premises",
  "kind": "model_boundary",
  "subject": "beam-check",
  "requirements": {
    "method": "beam-cantilever-end-load/1",
    "inputs": {
      "force": {"value": 20, "unit": "N", "precision": "declared input"},
      "span": {"value": 100, "unit": "mm", "precision": "declared input"},
      "elastic_modulus": {"value": 200000, "unit": "MPa", "precision": "model assumption"},
      "second_moment": {"value": 1000, "unit": "mm4", "precision": "model assumption"}
    }
  },
  "source_refs": ["load-declaration", "model-basis"],
  "affected_decisions": ["support-selection"]
}
```

agent 还须在 `facts` 中整理这些数量及三项类别前提，注明 declared/assumed/unknown；只有模型输入而没有当前事实，会给出具体缺失属性并保持 unknown。不得将模型假设反填为实物已知条件。

`project work /3` 保留原计算，同时给出 `model_applicability` 与 `current_support`。10 N 计算即使重新绑定了 20 N 的 Project/context/RDF，仍报告载荷冲突；结果只能作诊断。计算和模型假设一致后，仍须按验收、误差、代表性验证和责任范围作决定。`analysis_completed` 不是工程采用。

`kernel review --project` 先执行原查询/形状及身份检查，再加这些工程前提检查；错误前提不掩盖原诊断。历史快照保持历史意义。Harness 记录同一 Project 的完整检查，Agent 据原件审阅；违反或未知前提不能通过记录完成解除。experience `/2` 的必要条件不能由收益补偿。

正文未整理成该合同的旧条件继续作历史诊断。不能声称 Rust 已证明其自然语言与物理模型等价。遇到来源冲突、未支持算法、缺未知边界或数值验收未执行，记录具体缺项并安排下一验证。

本地完整检查点、归档、可选普通模型辅助各自声明资源预算。字节、结构节点、模型 token、调用次数与实际费用分别报告，不互相换算或代替。旧回执保持当时预算和执行身份。超限先记录实际大小、字段构成与拒绝层，再明确组织有界任务；保留同一 Project/context、完整目的、验收、功能、机制、工程检查和交接。失败和未处理项保留，不能截掉禁忌、必要条件或检查结果来塞进预算。检查点与归档的当前范围见[完整检查点](harness-context.md)，模型辅助见[独立输入与预算合同](direct-model-assistance.md)。


## 局部工作如何支持总目的

旧 `engineering-basis/1` 保持目的精确相等。新 `/2` 的 `goal_support` 使用生成的 EngineeringGoalSupport：同一Project ID/revision/purpose、父子ObjectIdentity、父条件完整UTF8摘要、局部原七字段canonical JSON摘要（保留schema/2、排除goal_support）、来源ID/SHA、局部验收到父验收的原文映射、贡献解释及现有fact/assertion引用。当前仅支持一跳父basis/1；父/2、缺关系或缺义务保持具体unknown。

每项局部验收都有明确贡献依据；父验收全量保留为remaining_parent_obligations，贡献不等于完成。`claim_scope=local_obligation_only` 不授予整体工程达成。目的、验收、条件、对象版本或来源声明变化会使旧关系失效并沿同一Project依赖重审；破损的goal_support不掩盖原模型、数值、CAD和反例检查。来源SHA声明绑定不等于已读取原件；原件内容校验由各实际来源适配器继续负责。

真实项目案例中的局部孔边/32mm参考是整机设计的一个有界输入，不能替代承载、材料、公差、制造图、计量及最终项目专属精度证据。已闭环项目的经验沿原关联与变化方法复用为实例/no_delta，其80mm孔距和42mm端距不成为通用参数。

缺信息与当前冲突分别诊断：空必填值、无效摘要或不能比较的身份保持unknown；格式有效且与当前目的、验收、对象、条件或来源不一致才是violated。两者都阻断相应采用；其他可执行主张仍照常检查，不能用一个坏支持合同掩盖原失败。

当前 `engineering::basis` 返回[只读完整来源视图](../docs/ENGINEERING-BASIS-VIEW.md)。原条件UTF8、SHA和完整goal_support保留；protocol()取原/1或/2，借用的common DTO明确为/1字段视图。view及clone不可直接Serialize/Deserialize，也没有DerefMut；不能冒完整/2正本。目标支持仍从原完整载荷检查，坏/缺支持不掩盖独立诊断，原规则及工程采用条件保持。此内存API修复的实际生成、回归、接收和安装按当前回执取证。
