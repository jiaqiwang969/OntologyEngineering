# 第6章：硬件层开发 —— 支撑材料与证据边界

本章正文围绕一场 EPS 硬件度量拒签评审展开：三条教学记录都满足当前已编码的数值比较，但 `99.4%` 没有计算工件、范围、版本、分子分母和验证评审连边，因此不能写成 FMEDA 结果或整项硬件达标。

> 案例边界：本目录内的失效率、覆盖率、度量值和 U7 均为合成教学数据。当前知识模型只证明已编码对象、来源关系和局部比较合同；真实项目必须用受控设计、安全分析、项目任务剖面、失效率证据、时序/独立性论证以及验证评审重建。

## 三个数字身份

| 数字 | 身份 | 可以支持 | 不能支持 |
|---:|---|---|---|
| 95.00% | 加 U7 前、`Σλ=400 FIT` 四元素合成 FMEDA 聚合的 SPFM | 公式与分类聚合教学 | 当前 EPS ECU 的 FMEDA 结果 |
| 95.48% | 同一基线增加 3 FIT U7 后，在全部覆盖、时序与独立性假设成立时的 SPFM | 单因设计变式 | U7 已实现、已验证或项目已达标 |
| 99.4% | `abox-eps-hardware.ttl` 中 `ECU_SPFM` 的 TeachingExample 记录 | 目标血缘与 `got >= need` 比较 | 与 95.00% 构成前后链、真实计算链或放行结论 |

U7 加入后的统一教学账为：

```text
原 2 FIT 供电路径：1.8 → λ_MPF,DP；0.2 → λ_RF
U7 自身 3 FIT：1.35 → λ_S；1.485 → λ_MPF,DP；0.165 → λ_MPF,L

Σλ = 403
SPFM = 1 − 18.2/403 = 95.48%
LFM  = 1 − 20.165/(403−18.2) = 94.76%
```

本算例假定检测 U7 的既有控制器诊断路径已经包含在原“控制器 200 FIT”聚合中，不增加 403 FIT 分母；这只是离线前提。若该路径检测 U7 超过 MPFDTI，U7 危险侧 1.65 FIT 全部潜伏，LFM 约为 94.37%；若检测与反应超过 FTTI 或元素层最大故障处理时间，原路径 1.8 FIT 不得继续领取覆盖信用，必须返回逐模式分类，不能只机械替换一个数。

## 章际责任

`EPS-S05` 同时进入两条平行支路：

- ch06：ECU 硬件、传感/供电/驱动路径、`AssistMotor` 与 `TSR_MotorCurrentLimit` 的硬件实现；
- ch07：`EPS_ControlSoftware`、`TSR_TorquePlausibility` 的软件实现与 HSI 软件端。

`TSR_TorquePlausibility` 连接 `EPS_ECU + EPS_ControlSoftware`；由它派生的 HSI 需求连接 `TorqueSensor + EPS_ControlSoftware`。ch07 不消费 `EPS-S06-HW`；两支在 ch08 的分解、共存与相关失效分析中会合。

## 交付物

| 产物 | 用途与边界 |
|---|---|
| `chapter.md` | 九节连续问题链：拒签→范围/HSR→分类/时钟→FMEDA 行→数字身份→U7 前后账→Clause 8/9→数值本体→有限签字 |
| `examples/metrics-walkthrough.md` | 可独立重算的 400/403 FIT 支撑算例、U7 单因变式与多元素目标说明 |
| `../../ontology/hardware-metric-targets.ttl` | Table 4/5/6 参考目标；百分比参考值有闭世界目录，PMHF 用尾数/指数/单位结构化 |
| `../../ontology/abox-eps-hardware.ttl` | 两个项目目标与三条教学记录；当前故意保留计算链、范围完整性、电机 LFM 和评审证据缺口 |
| `../../ontology/source-anchors-part5.ttl` | Part 5 条款、表、式和限定性 fragment 的真实页/block/bbox 来源 |
| `../../ontology/shapes.shacl.ttl` | 参考目录、项目目标、记录结构与数值比较的局部闭世界合同 |
| `../../eval/eval-cases.yaml` | `CQ-CH06-*`、`GATE-CH06-*` 的精确查询与 oracle |
| `../../eval/fixtures/invalid-hw-*.ttl` | 无目标、错种类、空理由、弱化目标、不达标和伪造参考值等单因反例 |

## Clause 8 与 Clause 9 的边界

- SPFM/LFM 是按所考虑安全目标进行的架构度量，范围和五类失效率必须先成立；它们不替代 Clause 9。
- Clause 9 内 PMHF 与 EEC 是替代路线；选择其中一条不删除 Clause 8、部件级共同门槛、HSR/设计验证或结果验证评审。
- PMHF 是相关项运行寿命内平均每运行小时的安全目标违背概率，不等同于某一元件普通失效率。定量分析还需架构、每个 part 的 SPF/RF/MPF 估算率、DC 和多点暴露时长。
- EEC 在硬件 part 层逐个评价单点、残余与可信双点失效原因；聚合百分数不能替代个体过堂。
- 多组织开发可在 DIA 中分配目标或约定路线；共享硬件不能按信号数量机械均分预算。

## Annex H 的窄门

§8.4.8 c) 的资料性解释不是“机制自身会自动暴露”。检出型机制揭示的是预期功能中的潜伏故障；机制自身未覆盖的故障反而成为剩余潜伏贡献。仅控制故障效应、却不检出或上报的滤波/遮蔽机制，其所覆盖故障仍可能潜伏。

## 数值本体化

三层关系如下：

```text
ISOReferenceMetricTarget
        ↑ derivedFromMetricTarget
ProjectPercentMetricTarget
        ↑ usesMetricTarget
HardwareElementMetric（当前为教学记录）
```

当前 `CQ-CH06-02` 返回 `ECU_LFM`、`ECU_SPFM`、`Motor_SPFM`，只表示三条已有记录满足当前目标比较。要把 `99.4%` 升级为可审计算结果，至少还需对象化计算工件/版本、范围/配置、分子分母、FMEDA 来源、机制时序与独立性、评审状态、元素组合和 Clause 9 结果。

PMHF Table 6 的 OCR 曾把 `<10⁻⁸ h⁻¹` 损坏成 `<10-8 h-1`。模型不保存受损算式字符串，而保存比较符、尾数、指数和单位，并以 `ocrCorrected` 留下校订依据。正确目标值也不能冒充 PMHF 已算出。

## 当前开放项

- `MetricAchievement` 尚无完整计算工件、范围、配置、分子分母与评审合同；
- 当前没有 ECU/电机真实 FMEDA，也没有相关项级 SPFM/LFM 组合结论；
- 电机 LFM 的“不适用/尚未分析/记录遗漏”状态尚未对象化；
- FTTI、最大故障处理时间、MPFDTI 与覆盖信用尚未形成完整可执行 Shape；
- PMHF 只对象化参考目标，未对象化定量计算链；
- `EPS-S06-HW` 仍是候选交接状态，不是发布输入。

## 运行

```bash
.venv/bin/python eval/run_eval.py
```

若只做教学变异，应在完整临时副本中一次只改一个因素，并在运行后核对共享正本哈希未变化。Shape 报警只证明相应局部合同被触发，不评价 FMEDA 技术充分性或功能安全达成。
