# Safety Plan：确认措施计划模板

> 这是教学字段模板，不是项目已批准的 Safety Plan，也不替代组织过程、人员任命或确认措施报告。

## 项目上下文

| 字段 | 内容 |
|---|---|
| Item / element | `<受控标识>` |
| Safety Plan 版本 | `<配置项与版本>` |
| 最高适用 ASIL | `<QM / A / B / C / D>` |
| Safety manager | `<受控角色标识>` |
| 计划状态 | `<Draft / Reviewed / Approved>` |

## 确认措施计划

| 字段 | 内容 |
|---|---|
| Measure ID | `<唯一标识>` |
| Measure kind | `<受控 ConfirmationMeasureKind>` |
| Scope / work product | `<受控对象标识及版本>` |
| Applicable ASIL | `<QM / A / B / C / D>` |
| Table 1 entry | `<— / I0 / I1 / I2 / I3>` |
| Planned independence | `<I0 / I1 / I2 / I3>` |
| Independence rationale | `<人员、团队、直属上级、部门、资源与放行权边界>` |
| Responsible reviewer/auditor/assessor | `<受控角色标识>` |
| Planned date | `<日期>` |
| Execution status | `<Planned / InProgress / Completed / Cancelled>` |
| Confirmation report | `<完成后填写受控报告标识与版本>` |
| Open issues | `<问题、责任人、关闭条件>` |

## 状态门槛

- `Planned`：必须确定 kind、ASIL、Table 1 映射和计划独立性。
- `InProgress`：保留批准的计划配置；不得预先声明完成结论。
- `Completed`：必须记录实际独立性和受控确认措施报告。
- `Cancelled`：记录理由和重新规划影响；不得保留 `performedAtIndependence`。

知识模型门禁只检查字段、映射和状态一致性。人员是否真正独立、活动是否充分以及结论是否可接受，仍需项目证据和有权限的专业评审。
