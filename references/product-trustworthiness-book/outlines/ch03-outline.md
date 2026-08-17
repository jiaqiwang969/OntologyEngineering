---
contract_version: 1
chapter: ch03
target_hanzi: 40000
section_budgets:
  - heading: "绿表到了签字页，为什么没人敢签"
    hanzi: 4000
  - heading: "没有坏人，责任为什么消失"
    hanzi: 4500
  - heading: "任命了安全经理，为什么仍没有合格判断者"
    hanzi: 4000
  - heading: "计划写了，为什么三年没有发生"
    hanzi: 5000
  - heading: "“换个人看”为什么不等于独立"
    hanzi: 5000
  - heading: "两百份报告，为什么还不是安全档案"
    hanzi: 5500
  - heading: "评估接受了，谁来决定放行"
    hanzi: 4500
  - heading: "本体化实践：机器能抓住哪一种责任空洞"
    hanzi: 4000
  - heading: "回到第一次 HARA，谁守住它"
    hanzi: 3500
consumes_state_ids: [EPS-S00]
produces_state_ids: [GOV-S03]
first_teaches: [safety-management-and-safety-case]
ontology_mapping_shape: controlled-catalog-human-boundary
source_anchors:
  - id: "2-5.4.2.1"
    part: 2
    clause: "2-5.4.2.1"
    artifact: "structured/mineru/ISO-26262-2018/part-02-management-of-functional-safety/native-full/ISO 26262-2-2018/auto/ISO 26262-2-2018_content_list_v2.json"
    pdf_page: 17
    block: 13
    bbox: [112, 535, 941, 565]
  - id: "10-5.3.1"
    part: 10
    clause: "10-5.3.1"
    artifact: "structured/mineru/ISO-26262-2018/part-10-guidelines/native-full/ISO 26262-10-2018/auto/ISO 26262-10-2018_content_list_v2.json"
    pdf_page: 22
    block: 4
    bbox: [57, 312, 376, 330]
planned_outputs:
  - functional-safety-book/ch03-safety-management/chapter.md
gate_count_policy: runtime-derived
question_count_policy: learning-objective-driven
figure_policy: engineering-need-driven
---
# 第3章 安全管理：谁能为安全签字

## 章级要素

- 导读承诺：从一张全绿但不能签字的放行表出发，把文化、能力、计划、独立性、安全档案、FSA 与放行责任接成一条可追问的问题链。
- 失败故事：安全档案确认评审长期停在 `Planned`；客户与供应商两份计划各自成立、接口无人认领；绿灯只证明局部记录，不能证明合格的人已经针对正确版本作出判断。
- 候选工程图：Safety Case 的 Claim–Argument–Evidence AND 状态合同，以及安全档案、确认措施、渐进 FSA 并行汇拢到放行证据包的关系图。
- EPS 状态：`EPS-S00` → `GOV-S03`；Safety Case=Draft、SG1 Claim=Open、Evidence=Candidate、确认评审=Planned。
- 首讲：safety culture、confirmation measure/I0–I3、safety case/Claim/Evidence；DIA 只提名，操作深讲保留给 ch10。

## 节级分配表

| 节 | 字数 | 供字素材 | 形态 |
|---|---:|---|---|
| 绿表到了签字页，为什么没人敢签 | 4000 | Part 2 §6.4.13、EPS 教学状态 | 失败故事与坏表 |
| 没有坏人，责任为什么消失 | 4500 | Annex B、`examples/safety-culture.md` | 文化压力测试 |
| 任命了安全经理，为什么仍没有合格判断者 | 4000 | Part 2 §5.4.4、§6.4.2 | 角色/能力双门槛 |
| 计划写了，为什么三年没有发生 | 5000 | §6.4.3–6.4.7、`safety-plan-template.md` | 变更、剪裁、DIA 走查 |
| “换个人看”为什么不等于独立 | 5000 | Table 1、`confirmation-independence.ttl` | 矩阵与组织关系判定 |
| 两百份报告，为什么还不是安全档案 | 5500 | `safety-case-skeleton.txt`、Part 10 §5.3 | CAE 贯穿案例 |
| 评估接受了，谁来决定放行 | 4500 | §6.4.12–6.4.13、Clause 7 | 并行汇拢与权力分离 |
| 本体化实践：机器能抓住哪一种责任空洞 | 4000 | CQ/GATE、Shapes、fixtures | 受控目录与证明边界 |
| 回到第一次 HARA，谁守住它 | 3500 | `README.md`、ch04 输入契约 | 状态卡、练习与桥接 |

## 绿表到了签字页，为什么没人敢签

先让读者在一张责任表上找到空洞，再引入组织、计划、确认和放行的分层责任。

## 没有坏人，责任为什么消失

逐条走查文化判据，区分“自动可检查的结构”与“必须由组织证据支撑的行为”。

## 任命了安全经理，为什么仍没有合格判断者

把能力缺口、培训、资源配置、任命和复查连成循环，不把“有证书”等同于胜任。

## 计划写了，为什么三年没有发生

用一次供应商变更演示计划为何是受控活文件，要求每个裁剪都留下依据与影响。

## “换个人看”为什么不等于独立

从工作产物、ASIL 和确认对象三个维度查 Table 1，说清独立性是工程关系，不是人名旁的标签。

## 两百份报告，为什么还不是安全档案

用 EPS 的 Open Claim 与 Candidate Evidence 走查反例：有证据节点不代表证据已被评估或主张已闭合。Part 10 §5.3.1 仅用于解释 Safety Case 的理解边界，不把资料性指南改写成新的放行要求。

## 评估接受了，谁来决定放行

把开发时的管理骨架接到生产、运行、服务和变更，但把具体闭环操作留给 ch09。

## 本体化实践：机器能抓住哪一种责任空洞

以 `CQ-CH03-03` 查询 Safety Case、Claim 与确认措施的状态边界，以 `CQ-CH03-04` 核对确认措施目录，以 `CQ-CH03-05` 遍历安全文化判据到本书机制的映射；再运行相应单因反例，并明确自动化不得替代人类判断与批准。查询目标与预期结果只以 `eval/eval-cases.yaml` 中对应 oracle 为准，结果再回溯 `ontology/source-anchors-part2.ttl`。

## 回到第一次 HARA，谁守住它

桥尾句：“组织就绪，第一件技术工作是问危害。”

## 练习配置

覆盖文化判例、能力闭环、裁剪理由、确认独立性查表、CAE 状态判断、放行责任辨析，动手题要求删除一个独立性字段并解释 Shape 报告。

## 现稿处置

`README.md` 保留模块索引；四个 examples 按上表编织；`chapter.md` 从零撰写，必须吸收 `study-part9-part2.md` 的能力、剪裁、计划、FSA 与放行差集。
