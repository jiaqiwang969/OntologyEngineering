# 生产控制计划（安全相关内容）：EPS 教学草稿

```text
reviewStatus: Draft
ruleBasis: ISO structure + EPS project-specific teaching strategy
productionExecutionStatus: not asserted
```

本文件只示范生产控制计划的定义结构，不代表任何工厂的量产计划、设备能力、批准配置、一次执行或控制报告。正文桌面走查中的 `EPS-CAL-B4`/`EPS-CAL-B3` 不在主 ABox，也不在本例中冒充目标、读回或 PASS 实例。工作产物锚点为 7-5.5.2；术语边界来自 Part 1 §3.147，控制规划锚点包括 7-5.4.1.1/5.4.1.3/5.4.1.5/5.4.1.6。

## 1. 安全相关特殊特性

| 特性 ID | 特性 | 所属元素 | 追溯需求 | 来源边界 |
|---|---|---|---|---|
| `SSC_TorqueSensorCalibration` | 转矩传感器末端标定 | `TorqueSensor` | `HSI_TorqueSignal`（HSI 规格内接口需求） | 术语边界来自 Part 1 §3.147，Part 7 负责生产规划；EOL 标定为教学项目选择 |
| `SSC_ECUSoftwareVersion` | ECU 软件/标定数据版本正确性 | `EPS_ECU` | `SSR_TorqueRangeCheck` | 术语边界来自 Part 1 §3.147；正确版本烧录程序来自 5.4.1.3；具体校验方式不是通用强制方法 |

本书用 `derivedFromRequirement` 接回需求链，并要求“控制步骤或显式处置理由”。这是 `BookHousePolicy`，用于避免开放世界下的遗漏，不是把 5.4.1.5 改写成逐项控制义务。

## 2. 控制步骤定义

| 顺序 | 步骤 ID | 方法（教学） | 测试设备 | 工具 | 判据（教学） | 特性 |
|---:|---|---|---|---|---|---|
| 10 | `CM_EOLCalibrationCheck` | 施加规定输入，读取并比较标定结果 | `EOL_TestBench` | `CalibrationApplication` | 落入项目公差带，否则拒收 | `SSC_TorqueSensorCalibration` |
| 20 | `CM_SoftwareChecksum` | 烧录后读回标识与校验值，与教学目标清单比较 | `ProgrammingStation` | `ProgrammingStation` | 与教学 BOM 目标一致，否则拒收 | `SSC_ECUSoftwareVersion` |

顺序、方法、必要设备、工具和测试判据是 5.4.1.6 的结构要求。表中的 EOL、checksum、read-back、BOM、公差值和拒收方式均为 `ProjectSpecificStrategy`。Checksum/read-back 在标准中是示例，不应推广成每个项目必须采用的方法。

## 3. 本体映射

```text
EPS_ProductionProcessDefinition_Draft
  hasControlStepDefinition -> CM_EOLCalibrationCheck
  hasControlStepDefinition -> CM_SoftwareChecksum

ProductionControlPlan_Draft [reviewStatus Draft]
  documents -> EPS_ProductionProcessDefinition_Draft

CM_* [ProductionControlStepDefinition]
  partOfPlan -> ProductionControlPlan_Draft
  controls -> SSC_*
  controlSequenceIndex / controlMethod
  requiresTestEquipment / requiresControlTool
  controlCriterion
```

## 4. 尚不存在的执行证据

| 执行对象 | 本教学 ABox 中的状态 |
|---|---|
| `ProductionExecution` | 无实例 |
| `ProductionControlExecution` | 无实例 |
| `ControlMeasuresReport` | 无实例 |
| `ReleaseForProductionReport` / 批准配置 | 无实例 |
| `ProductionDeviation` | 无实例 |

因此不能从本计划得出“生产已执行”“控制已通过”或“配置已获放行”。独立 fixtures 只核对当前已登记正反分支的实际放行/拒绝行为是否与 oracle 一致，不是量产记录，也不表示未登记分支已被覆盖。

## 5. 真实项目未决项

- 判据数值、公差带和测量不确定度论证；
- 设备校准、过程能力和人员能力证据；
- 过程失效及措施有效性分析；
- 计划评审、批准、配置基线和偏差授权；
- 控制报告的受控编号、保存期和责任接口。
