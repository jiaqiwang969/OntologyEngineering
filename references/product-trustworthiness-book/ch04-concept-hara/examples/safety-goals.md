# 安全目标清单：EPS 教学案例

依据 ISO 26262-3:2018 6.4.4：每个 ASIL A-D 危害事件需要对应安全目标；相似目标可以合并，合并目标采用关联事件中的最高 ASIL。QM 或 S0/E0/C0 不应被机械套入同一规则。

## SG1——防止非预期助力导致方向失控

| 属性 | 教学值 | 状态/依据 |
|---|---|---|
| 来源危害事件 | `HE_UnintendedAssist_Highway` | 3-6.4.4.1 |
| 功能目标 | 防止或限制非预期转向助力，使车辆不会因此失去方向控制 | 3-6.4.4 NOTE |
| ASIL | D | 关联事件最高 ASIL，3-6.4.4.2 |
| FTTI | 待定（TBD） | 与定级相关时方可写入（3-6.4.4.3 NOTE）；数值由第 5 章时序论证后填入，候选交接包不携带未论证数值 |
| 候选安全状态 | 助力受限或受控关断，同时保持机械转向能力 | 作者案例假设，待系统设计验证 |

## 假设与未决事项

- `Assumption_UnintendedTorqueProfile`：非预期转矩幅值、方向和上升时间足以支持当前 S/C 判断；
- `Assumption_FTTI_100ms`：仅供隔离演示件 `FTTI_Field_Illustration` 示范字段形态（100 ms），ValidationUnplanned；SG1 不引用它，门禁 `CandidateFreezeUnplannedAssumptionShape` 禁止候选包消费对象依赖此类假设；
- 安全目标中的物理阈值、FTTI 和安全状态必须由车辆动态、故障传播、驾驶员反应和安全机制时序共同论证；
- 若未来多个危害事件合并到 SG1，门禁要求 SG1 等于这些事件的最高 ASIL。

## 追溯与工作产物

事件通过 `leadsToSafetyGoal` 连接 SG1；后续 FSR/TSR 使用项目需求派生关系继续向下追溯。`eps:HARA_Report_Draft` 是教学草稿，`eps:HARA_Verification_Planned` 和验证报告模板保持待执行状态。

SPARQL/SHACL 只能检查知识模型的完整性与一致性，不能替代 3-6.4.6.1 要求的 HARA 验证。
