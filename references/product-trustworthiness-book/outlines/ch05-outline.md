---
contract_version: 1
chapter: ch05
target_hanzi: 45000
section_budgets:
  - heading: "台架前那十几秒沉默"
    hanzi: 3000
  - heading: "从一句承诺到一套系统方案"
    hanzi: 5000
  - heading: "一条关系怎样把等级和责任送下去"
    hanzi: 5500
  - heading: "架构不是框图：它要经得起反问"
    hanzi: 5500
  - heading: "接口不是一张信号表"
    hanzi: 5000
  - heading: "安全机制与危害之间的限时赛"
    hanzi: 5000
  - heading: "测试层层向上，结论不许跳级"
    hanzi: 5500
  - heading: "让一条追溯链长出牙齿：本体化实践"
    hanzi: 5500
  - heading: "同一条 EPS 链的九站旅程"
    hanzi: 5000
consumes_state_ids: [EPS-S04]
produces_state_ids: [EPS-S05]
first_teaches: [system-development-and-timing]
ontology_mapping_shape: traceability-chain
source_anchors:
  - id: "4-6.4.7"
    part: 4
    clause: "4-6.4.7"
    artifact: "structured/mineru/ISO-26262-2018/part-04-system-level-product-development/native-full/ISO 26262-4-2018/auto/ISO 26262-4-2018_content_list_v2.json"
    pdf_page: 20
    block: 3
    bbox: [57, 250, 534, 267]
  - id: "10-4.4.1"
    part: 10
    clause: "10-4.4.1"
    artifact: "structured/mineru/ISO-26262-2018/part-10-guidelines/native-full/ISO 26262-10-2018/auto/ISO 26262-10-2018_content_list_v2.json"
    pdf_page: 16
    block: 4
    bbox: [57, 607, 236, 624]
  - id: "10-7"
    part: 10
    clause: "10-7"
    artifact: "structured/mineru/ISO-26262-2018/part-10-guidelines/native-full/ISO 26262-10-2018/auto/ISO 26262-10-2018_content_list_v2.json"
    pdf_page: 27
    block: 6
    bbox: [112, 517, 877, 552]
  - id: "10-12.2"
    part: 10
    clause: "10-12.2"
    artifact: "structured/mineru/ISO-26262-2018/part-10-guidelines/native-full/ISO 26262-10-2018/auto/ISO 26262-10-2018_content_list_v2.json"
    pdf_page: 71
    block: 0
    bbox: [114, 98, 678, 116]
planned_outputs:
  - functional-safety-book/ch05-system-development/chapter.md
gate_count_policy: runtime-derived
question_count_policy: learning-objective-driven
figure_policy: engineering-need-driven
---
# 第5章 系统层开发：让安全目标落到架构与接口

## 章级要素

- 导读承诺：从台架因 HSI 无判据而停下的现场出发，将 SG/FSC 派生为 FSR/TSR，保持 ASIL 与责任追溯，并区分系统设计验证、Clause 7 集成验证与 Clause 8 安全确认。
- 失败故事：需求在硬件、软件两端都存在，接口却没有共同承诺；“覆盖率 100%”也没有给测试工程师一个可执行判据。
- 候选工程图：系统开发递归、SG→FSR→TSR 翻译、FTTI/FHTI 时间轴、三层集成四幅图，各承担一个正文无法仅靠名称说明的观察任务。
- EPS 状态：`EPS-S04` → `EPS-S05`；当前只有 TSC/HSI、时间框架、Clause 7/8 计划骨架和显式欠账，不得将 Planned/Draft 写成已执行或已发布。
- 首讲：TSR、TSC、HSI、FTTI/FHTI 家族、V 模型集成三层。

## 节级分配表

| 节 | 字数 | 供字素材 | 形态 |
|---|---:|---|---|
| 台架前那十几秒沉默 | 3000 | `chapter.md`、`integration-test-strategy.md` | 现场矛盾 |
| 从一句承诺到一套系统方案 | 5000 | `tsc-architecture-analysis.md`、Part 4 p14–22、Part 10 §7 | 翻译与反问 |
| 一条关系怎样把等级和责任送下去 | 5500 | `abox-eps-system.ttl`、`abox-{bms,aeb}-system.ttl` | EPS 主链；副案例限界 |
| 架构不是框图：它要经得起反问 | 5500 | `tsc-architecture-analysis.md`、Part 4 §6.4.3–§6.4.8 | 设计改变与交接 |
| 接口不是一张信号表 | 5000 | `abox-eps-system.ttl`、Part 4 §6.4.7、Annex B | 接口失败判例 |
| 安全机制与危害之间的限时赛 | 5000 | `ftti-timing.md`、Part 1 时间词条、Part 10 §4.4/§12 | 最坏时间账 |
| 测试层层向上，结论不许跳级 | 5500 | `integration-test-strategy.md`、Part 4 Clause 7/8 | 证据轴＋机械后备变式 |
| 让一条追溯链长出牙齿：本体化实践 | 5500 | 三份 EPS ABox、CQ、Shapes、fixtures | CQ＋局部闭包＋单因反例 |
| 同一条 EPS 链的九站旅程 | 5000 | 当前正文、ch06/ch07 输入契约 | 同一现场回收＋分叉 |

上述字数是早期规划上限的分配，不是完成判据；正文是否完成以问题链、来源、工程对象、机器回归和人类验收共同判断。

## 台架前那十几秒沉默

用“需求在两端都存在，但接口上没有责任”的失败开场，先讲漏洞，再引入追溯链。

## 从一句承诺到一套系统方案

把 TSC 拆为 TSR 集、相应架构与适用性理由，不把一份文档名称当成内容完整证据。

## 一条关系怎样把等级和责任送下去

主线走查 EPS 两条 FSR/TSR，再用 BMS/AEB 最小链证明 TBox/Shape 可复用；不声称副线需求集已完整。

## 架构不是框图：它要经得起反问

区分“已有分配节点”、“有架构实现”与“分析已执行”，将 EPS 报告模板保持为空。

## 接口不是一张信号表

用双端、信号、运行模式、时序、资源和诊断使用做完整性走查，不把最小草稿当完整 HSI。

## 安全机制与危害之间的限时赛

对 Part 10 的时标模型做算术走查，区分标准示例和 EPS 未验证占位值。

## 测试层层向上，结论不许跳级

按 HW–SW、系统、车辆三层记录输入、覆盖、方法、环境和报告；再只撤掉“机械后备可用”这一项，预测 Clause 7 范围、Clause 8 评估面和上游主张怎样重开。

## 让一条追溯链长出牙齿：本体化实践

用 `CQ-CH05-01` 检查 `derivedFrom`/`allocatedTo` 纵向链，以 `CQ-CH05-03` 区分架构实现、分析活动与报告状态，以 `CQ-CH05-06` 回查时间模型来源，以 `CQ-CH05-07` 比较集成方法画像；再用 HSI 双端、报告归属和方法矩阵反例测试闭世界交付边界。查询目标与预期结果只以 `eval/eval-cases.yaml` 中对应 oracle 为准，结果再回溯 `ontology/source-anchors-part4.ttl` 和 `ontology/source-anchors-part10.ttl`。

## 同一条 EPS 链的九站旅程

桥尾句：“需求到了硬件，随机失效怎么算。”同时标注 ch07 从同一 `EPS-S05` 分叉。

## 练习配置

覆盖 TSC 内容辨析、FSR/TSR 派生、ASIL 继承、分配与架构区分、HSI 双端补全、时间预算手算、三层策略选择，动手题通过删除一个端点或报告关系观察精确 Shape。

## 重写处置

旧稿的条款目录、大型方法表和 CQ/文件注册表不再作为正文骨架；它们的完整契约留在 README 与机读正本。候选正文已按“台架矛盾→翻译→追溯→架构→接口→时间→分层收证→机器拒绝→回到台架”完成结构性重写，仍须经过同树门禁、成书视觉复核、专家判断和用户明确验收。分析、验证与报告继续保持其真实的 Planned/Draft/NotRun/NotPerformed 状态。
