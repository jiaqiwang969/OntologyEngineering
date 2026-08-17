# EPS 软件安全需求规格样例

```text
artifact_id: EPS-SWRS-TEACHING-001
artifact_type: SoftwareSafetyRequirementsSpecification
review_status: Draft
example_status: TeachingExample
expert_review_status: pending
release_evidence: false
```

本样例演示需求对象化与追溯字段，不代表真实 EPS 产品需求已经批准。数值、诊断策略、阈值和时间约束必须由具体项目重新论证。

## 上游分配前提

| 字段 | 教学值 |
|---|---|
| 上游需求 | `TSR_TorquePlausibility` |
| 上游 ASIL | `ASIL_D` |
| 软件分配目标 | `EPS_ControlSoftware` |
| 组件最低开发义务 | `requiredDevelopmentASIL = ASIL_D` |
| 来源锚点 | Part 6 `6.4.1`、`7.4.6` |

只有通用 ECU 分配不足以建立“分配给软件”的前提；模型额外建立了 `TSR_TorquePlausibility allocatedTo EPS_ControlSoftware`。

## SSR-01 转矩范围与变化率校验

```yaml
id: SSR_TorqueRangeCheck
statement: 软件应校验助力转矩指令的允许范围与变化率；检测到越界时进入项目定义的抑制路径并记录诊断状态。
derivedFrom: TSR_TorquePlausibility
asil: ASIL_D
allocatedTo: SWU_TorqueMonitor
requiredDevelopmentASIL: ASIL_D
reviewStatus: Draft
verificationCaseIds:
  - UVC-TM-BOUNDARY-001
  - UVC-TM-RATE-002
sourceAnchors:
  - 6-6.4.1
  - 6-7.4.6
```

待项目补充：允许范围、变化率阈值、去抖策略、故障反应时间、降级接口、标定数据所有者和安全分析依据。

## SSR-02 传感器输入合理性检查

```yaml
id: SSR_InputPlausibility
statement: 软件应检查转矩传感器输入及其诊断状态；检测到项目定义的失效条件时进入受控降级路径。
derivedFrom: TSR_TorquePlausibility
asil: ASIL_D
allocatedTo: SWU_PlausibilityCheck
requiredDevelopmentASIL: ASIL_D
reviewStatus: Draft
verificationCaseIds:
  - UVC-PC-DIAG-001
  - UVC-PC-STALE-002
sourceAnchors:
  - 6-6.4.1
  - 6-7.4.6
```

待项目补充：输入通道、更新周期、陈旧数据判据、诊断标志可信条件、故障反应时间和降级后的车辆级可接受行为。

## 语义责任分离

| 内容 | 类型 | 本样例状态 |
|---|---|---|
| SSR 从已分配给软件的 TSR 派生 | 标准约束的结构化解释 | 已建模，专家复核待定 |
| SSR 分层分配到软件单元 | 标准约束的结构化解释 | 已建模，专家复核待定 |
| ASIL D | 合成教学实例 | 不可用于项目放行 |
| 具体阈值和故障反应 | 项目工程决策 | 未定义 |
| 用例 ID 与追溯字段 | 本体工程 house policy | 已规划，未执行 |

## 评审门槛

- 每条 SSR 有唯一标识、可审查陈述、上游 TSR、ASIL 和软件分配目标。
- 上游 TSR 明确分配到 `SoftwareComponent`，而不是只分配到通用 `Element`。
- 软件组件/单元的 `requiredDevelopmentASIL` 取受控值，且不低于任何已分配安全需求。
- 需求验证用例、环境、预期结果和证据位置已规划。
- 需求批准、验证执行和报告签署分别记录，不能由 SHACL 全绿代替。
