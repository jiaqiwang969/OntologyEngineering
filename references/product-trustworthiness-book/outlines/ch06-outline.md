---
contract_version: 1
chapter: ch06
target_hanzi: 35000
section_budgets:
  - heading: "三盏绿灯，为什么评审人不签字？"
    hanzi: 3000
  - heading: "第5章究竟把什么硬件责任交了过来？"
    hanzi: 3500
  - heading: "有监控，为什么还不能给故障分类？"
    hanzi: 4000
  - heading: "一行 FMEDA 要交出哪些证据？"
    hanzi: 4000
  - heading: "95% 和 99.4% 是同一个结论吗？"
    hanzi: 4500
  - heading: "加了 U7，为什么只到 95.48%？"
    hanzi: 4500
  - heading: "SPFM/LFM 过线，为什么仍不能宣称安全目标达成？"
    hanzi: 4000
  - heading: "本体化实践：怎样让 99.4% 变成可审的数字？"
    hanzi: 4000
  - heading: "评审最后能签什么？"
    hanzi: 3500
consumes_state_ids: [EPS-S05]
produces_state_ids: [EPS-S06-HW]
first_teaches: [hardware-failure-metrics]
ontology_mapping_shape: numerical-honesty
source_anchors:
  - id: "5-8.4.5"
    part: 5
    clause: "5-8.4.5"
    artifact: "structured/mineru/ISO-26262-2018/part-05-hardware-level-development/native-full/ISO 26262-5-2018/auto/ISO 26262-5-2018_content_list_v2.json"
    pdf_page: 26
    block: 0
    bbox: [55, 98, 884, 145]
  - id: "5-8.4.8"
    part: 5
    clause: "5-8.4.8"
    artifact: "structured/mineru/ISO-26262-2018/part-05-hardware-level-development/native-full/ISO 26262-5-2018/auto/ISO 26262-5-2018_content_list_v2.json"
    pdf_page: 27
    block: 5
    bbox: [110, 395, 941, 428]
  - id: "5-9.4.1.1"
    part: 5
    clause: "5-9.4.1.1"
    artifact: "structured/mineru/ISO-26262-2018/part-05-hardware-level-development/native-full/ISO 26262-5-2018/auto/ISO 26262-5-2018_content_list_v2.json"
    pdf_page: 29
    block: 12
    bbox: [112, 442, 939, 473]
  - id: "10-8.1"
    part: 10
    clause: "10-8.1"
    artifact: "structured/mineru/ISO-26262-2018/part-10-guidelines/native-full/ISO 26262-10-2018/auto/ISO 26262-10-2018_content_list_v2.json"
    pdf_page: 29
    block: 12
    bbox: [114, 495, 583, 512]
  - id: "10-12.3"
    part: 10
    clause: "10-12.3"
    artifact: "structured/mineru/ISO-26262-2018/part-10-guidelines/native-full/ISO 26262-10-2018/auto/ISO 26262-10-2018_content_list_v2.json"
    pdf_page: 79
    block: 0
    bbox: [114, 98, 702, 116]
  - id: "10-Annex A"
    part: 10
    clause: "10-Annex A"
    artifact: "structured/mineru/ISO-26262-2018/part-10-guidelines/native-full/ISO 26262-10-2018/auto/ISO 26262-10-2018_content_list_v2.json"
    pdf_page: 86
    block: 0
    bbox: [389, 98, 551, 143]
planned_outputs:
  - functional-safety-book/ch06-hardware-development/chapter.md
gate_count_policy: runtime-derived
question_count_policy: learning-objective-driven
figure_policy: engineering-need-driven
---
# 第6章：硬件层开发，99.4% 为什么还不能签字

## 章级理解合同

- 唯一冲突：评审界面显示 ECU SPFM 99.4%、ECU LFM 92.0%、电机 SPFM 99.1% 三盏绿灯，验证评审人因三条记录只有比较链、没有计算链而拒签。
- 数字身份：`95.00%` 是加 U7 前 `Σλ=400 FIT` 合成基线；`95.48%` 是同一基线加入 3 FIT U7 后、在全部分类/时序/独立性假设成立时的重算；`99.4%` 是当前 ABox 中尚无计算工件连边的教学记录。
- 章节动作：固定范围与 HSR/HSI 责任，逐模式分类，审一行 FMEDA，完成前后手算，撤回超时或相关失效条件下的覆盖信用，区分 Clause 8 与 Clause 9，再让机器只检查其能负责的局部合同。
- 章际关系：第6章与第7章均直接消费 `EPS-S05`，不是硬件把 `EPS-S06-HW` 串行交给软件；两支在第8章会合。

## 九节问题链

| 节 | 消费 | 产出与留问 |
|---|---|---|
| 6.1 三盏绿灯，为什么评审人不签字？ | 三条 ABox 记录、开场签字页 | 四项 Review Hold；先问分析范围 |
| 6.2 第5章究竟把什么硬件责任交了过来？ | `EPS-S05`、TSR/HSI 分配 | SG1 硬件范围卡；再问 2 FIT 分类 |
| 6.3 有监控，为什么还不能给故障分类？ | 范围卡、未监控供电路径 | 五类 λ 与 FTTI/MPFDTI；再问每行来源 |
| 6.4 一行 FMEDA 要交出哪些证据？ | 分类与两只时钟 | 可聚合的 400 FIT 基线；再问 95 与 99.4 身份 |
| 6.5 95% 和 99.4% 是同一个结论吗？ | 400 FIT 聚合、项目目标、ABox | 数字身份账；再问 U7 的真实增量 |
| 6.6 加了 U7，为什么只到 95.48%？ | 基线与 U7 变更 | 403 FIT 条件账及单因变式；再问 SG 结论 |
| 6.7 SPFM/LFM 过线，为什么仍不能宣称安全目标达成？ | Clause 8 结果 | Clause 9、验证与系统性证据边界；再问机器合同 |
| 6.8 本体化实践：怎样让 99.4% 变成可审的数字？ | 参考/项目/记录三层 | 当前能证与不能证的状态账；再问签字范围 |
| 6.9 评审最后能签什么？ | 全章产物 | `EPS-S06-HW` 候选与开放项；回到软件平行支路 |

## 保留与迁移

正文保留五类 λ、FTTI/MPFDTI、一行 FMEDA、400/403 FIT 手算、U7 自身故障、Annex H 逻辑、Clause 8/9 边界、三层数值模型与一个真实 CQ。长数据源目录、供应商加权长例、完整 EEC 逐格走查、PMHF 暴露周期清单、全量 TTL/SPARQL/fixture 导航均迁往 README、examples、ISO 回读矩阵或附录，不再充当叙事骨架。

## 练习配置

练习围绕单因迁移：改 U7 检测时间、自检周期、独立性、失效率或 ABox 达成值，要求指出哪一笔信用撤回、哪个 Shape 报警，以及机器报警仍不能证明什么。练习不以“列出十项知识点”代替迁移判断。

## 三盏绿灯，为什么评审人不签字？

直接进入 EPS 硬件度量评审；三条记录均越过当前目标，但只有比较链，没有计算链。产出范围、来源、机制时序和 Clause 9 四项 Review Hold。

## 第5章究竟把什么硬件责任交了过来？

从 `EPS-S05` 固定硬件/软件平行分叉、TSR 分配、HSR/HSI 与需求追溯下界，形成 SG1 硬件范围卡；配置与 FTTI/MPFDTI 保持待绑定/TBD。

## 有监控，为什么还不能给故障分类？

沿同一条 2 FIT 供电路径先后经过无机制和加 U7 两种设计，建立相对安全目标的五类 λ、FTTI 反事实条件与 MPFDTI 条件性边界。

## 一行 FMEDA 要交出哪些证据？

从对象版本、失效率来源、任务剖面、模式分布、机制、DC、时序、独立性、五类去向和评审状态审一行，最后才允许聚合成加 U7 前的 400 FIT 基线。

## 95% 和 99.4% 是同一个结论吗？

手算 SPFM 95.00% 与 LFM 94.74%，再把参考目标、项目选择、合成计算与 ABox 教学记录分开；99.4% 不得成为 95% 的修改后版本。

## 加了 U7，为什么只到 95.48%？

U7 明确为 400 FIT 之外的 3 FIT 增量；逐项记录原路径迁移与机制自身安全/检出/潜伏贡献，完成 403 FIT 算账并用超时、超 MPFDTI 和独立性失效做单因变式。

## SPFM/LFM 过线，为什么仍不能宣称安全目标达成？

区分 Clause 8 相对架构度量与 Clause 9 绝对风险；PMHF/EEC 仅在 Clause 9 内替代，部件级闸门、验证评审和系统性故障证据仍各自存在。

## 本体化实践：怎样让 99.4% 变成可审的数字？

用 `CQ-CH06-02` 展示参考—项目—记录三层与当前精确答案；再用 expected-set/anti-join 的缺口解释开放世界下 Motor LFM 缺失为什么不会自动变红。

## 评审最后能签什么？

只允许数据质量复核人在 Review Hold/会议纪要上确认教学结构和算术，不冒充 §8.4.9 正式验证接受；输出带开放项的 `EPS-S06-HW` 候选，再回到 ch07 平行支路。
