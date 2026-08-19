---
contract_version: 1
chapter: ch09
executable_package_id: semantica.chapter_packages.vol2.ch09
executable_authority: semantica_only_no_book_fallback
package_status: partial
release_status: blocked
target_hanzi: 30000
section_budgets:
  - heading: "字段已经填齐，为什么 EPS-CAL-B4 还不能签字？"
    hanzi: 3000
  - heading: "从开发交到产线的，到底不能少哪些东西？"
    hanzi: 3500
  - heading: "“正确版本”如此关键，为什么它不是车上的安全机制？"
    hanzi: 3000
  - heading: "一行读回比对，怎样才会变成可执行的生产控制？"
    hanzi: 4000
  - heading: "读回 B3 之后，是恢复符合、处置产品，还是修改定义？"
    hanzi: 3500
  - heading: "一个 PASS，凭什么能绑定到这一颗 ECU？"
    hanzi: 3500
  - heading: "车辆离开工厂后，谁继续守住同一个安全前提？"
    hanzi: 3000
  - heading: "现场数据什么时候才算进入了行动闭环？"
    hanzi: 3000
  - heading: "本体化实践：机器与签字人最后各能批准什么？"
    hanzi: 3500
consumes_state_ids: [EPS-S08]
produces_state_ids: [EPS-S09]
first_teaches: [production-and-field-lifecycle]
ontology_mapping_shape: closed-loop-process
source_anchors:
  - id: "1-3.147"
    part: 1
    clause: "1-3.147"
    artifact: "structured/mineru/ISO-26262-2018/part-01-vocabulary/native-full/ISO 26262-1-2018/auto/ISO 26262-1-2018_content_list_v2.json"
    pdf_page: 31
    block: 26
    bbox: [114, 708, 430, 724]
  - id: "7-5.4.1.3"
    part: 7
    clause: "7-5.4.1.3"
    artifact: "structured/mineru/ISO-26262-2018/part-07-production-operation-service-decommissioning/native-full/ISO 26262-7-2018/auto/ISO 26262-7-2018_content_list_v2.json"
    pdf_page: 13
    block: 24
    bbox: [112, 770, 939, 801]
  - id: "7-6.4.1.1"
    part: 7
    clause: "7-6.4.1.1"
    artifact: "structured/mineru/ISO-26262-2018/part-07-production-operation-service-decommissioning/native-full/ISO 26262-7-2018/auto/ISO 26262-7-2018_content_list_v2.json"
    pdf_page: 17
    block: 10
    bbox: [110, 384, 941, 416]
  - id: "7-7.4.1.1"
    part: 7
    clause: "7-7.4.1.1"
    artifact: "structured/mineru/ISO-26262-2018/part-07-production-operation-service-decommissioning/native-full/ISO 26262-7-2018/auto/ISO 26262-7-2018_content_list_v2.json"
    pdf_page: 19
    block: 8
    bbox: [112, 346, 941, 376]
  - id: "10-14"
    part: 10
    clause: "10-14"
    artifact: "structured/mineru/ISO-26262-2018/part-10-guidelines/native-full/ISO 26262-10-2018/auto/ISO 26262-10-2018_content_list_v2.json"
    pdf_page: 83
    block: 4
    bbox: [114, 663, 650, 682]
planned_outputs:
  - references/product-trustworthiness-book/ch09-production-operation/chapter.md
gate_count_policy: runtime-derived
question_count_policy: learning-objective-driven
figure_policy: engineering-need-driven
---
# 第9章：生产、运行、服务与报废的真实世界闭环

## 章级理解合同

- 唯一冲突：`EPS-S09` 放行会上，`ProductionControlPlan_Draft` 已有两项安全相关特殊特性、两个控制步骤及顺序、方法、资源和判据；桌面走查却故意把 `EPS-CAL-B3` 填到 `EPS-CAL-B4` 的目标行。管理者要求“字段齐全就签字”，审核员要求证明这是受控定义，以及本批次已按已批准配置执行、可追溯并能进入现场闭环。
- 中心区分：定义说“应怎样做”，执行说“这一次做了什么”，报告说“对哪个对象、在什么时候、得到什么结果”。三者不互相派生，字段齐全、`Approved` 标签或机器绿灯都不能代替现实执行。
- 贯穿对象：`EPS-CAL-B4` 不是一个孤立字符串。它从开发基线和安全需求进入特殊特性与控制定义，在生产中与读回实际值及已批准配置比对，再通过对象标识、控制报告、服务刷写和现场数据守住同一前提。
- 证据边界：当前 EPS 仅有 Draft 计划、两个控制步骤定义和 Draft 现场观测说明；图中登记的 `ProductionExecution` 实例为 0，现场数据→分析→行动的内部对象仍缺。章末只能交出带 Hold 的 `EPS-S09` 候选，不作量产或合规声明。

## 九节问题链

| 节 | 消费 | 产出与留问 |
|---|---|---|
| 9.1 字段已经填齐，为什么 `EPS-CAL-B4` 还不能签字？ | Draft 计划、桌面 B3 反例与放行请求 | 五项 Review Hold；先问开发须交什么 |
| 9.2 从开发交到产线的，到底不能少哪些东西？ | `EPS-S08`、Part 7 Clause 5 前提和发布配置 | 开发→生产接口卡与特殊特性生命线；再问它与安全机制的边界 |
| 9.3 “正确版本”如此关键，为什么它不是车上的安全机制？ | Part 1 §3.147、B4 配置与运行机制 | 产品/过程特性与运行技术方案分账；再问如何落到控制定义 |
| 9.4 一行读回比对，怎样才会变成可执行的生产控制？ | 特殊特性、控制计划五要素与 5.4.1.3 | 受控目标—读回实际—判据—资源—顺序的定义卡；再问 B3 红行应启动哪类决定 |
| 9.5 读回 B3 之后，是恢复符合、处置产品，还是修改定义？ | 6.4.1.2 偏差四步、6.4.1.6 两个配置分支与 Part 8 变更桥 | 三类决定不混用，偏差处理必须同时照顾过程与嫌疑产品；再问执行证据如何绑定对象 |
| 9.6 一个 PASS，凭什么能绑定到这一颗 ECU？ | 6.3.1、6.4.1.1、6.4.1.5、追溯标识和目标基线 | 定义/执行/报告/放行四层证据链及当前准入门禁缺口；再问离厂后谁接手 |
| 9.7 车辆离开工厂后，谁继续守住同一个安全前提？ | 服务刷写、用户信息、报废/救援说明与 7.4.1.2 | 配置纪律穿越生命周期，说明与执行记录仍分层；再问现场数据何时成为行动 |
| 9.8 现场数据什么时候才算进入了行动闭环？ | 7.4.1.1 的提供—分析—触发行动 | 闭环与变更/PiU 接口，并暴露 EPS 当前仅有占位挂接；再问机器能守哪些边界 |
| 9.9 本体化实践：机器与签字人最后各能批准什么？ | Semantica ch09 contract/CQ/scenario/oracle 与全章 Hold | `EPS-S09` 候选、单因变式、窄签字与 ch10 支撑过程接力 |

## 保留与迁移

正文保留 Part 7 的规划→生产→运行/服务/报废主线、特殊特性与安全机制分界、控制
计划五要素、控制报告三要素、偏差四步、已批准配置/授权偏差两分支、现场监控三段
和 EPS Draft 教学状态。六类机器合同只在 Semantica ch09 package 执行；当前为
`partial`、release `blocked`，书稿不保存第二套规则、数据、查询或运行入口。

## 图的冲突落点

- 图 9-1 放在“从开发交什么”的冲突中，只画特殊特性从识别、规定控制到生产监控的生命线，并明示 Part 10 Clause 14 为资料性指南。
- 图 9-2 放在 B3 红行的决策分叉处，以 6.4.1.2 四步闭环为主干，额外画出“恢复符合”与“修改已批准定义”的边界。
- 图 9-3 放在“有售后数据为什么还不是现场闭环”的冲突中，以提供—分析—触发行动三段及其断点为主视图。

## 本体化实践的验收边界

Semantica ch09 CQ registry 分别询问特殊特性—控制—需求追溯、安全目标到生产控制的
路径、Draft 计划步骤及声明图中的 `ProductionExecution` 计数；计数不得被写成真实执行。
场景的精确义务与已知缺口只以源锁定 package manifest/oracle 为准，其中 §6.4.1.1
准入闭环仍是开放项，release 因而保持 `blocked`。

## 练习配置

练习只改一个事实：把 Draft 计划涂成 Approved，把 B4 目标改成 B3，删掉读回对象标识，新建一个无报告的 Completed 执行，给未批准配置添一个无责任人的“已授权”标签，或创建一个无行动的现场问题。答题必须指出哪一条定义、执行、报告、配置、现场或发布主张被重开，以及哪些既有事实仍可保留。

## 字段已经填齐，为什么 EPS-CAL-B4 还不能签字？

用 `ProductionControlPlan_Draft` 的字段齐全与桌面 B4/B3 红行制造冲突，把一个“签字”
拆成控制定义、生产准备和某颗产品通过三种主张；每项 Hold 都写明证据关闭条件，
不把声明图中的 0 个执行实例改写成现实世界的 0 次执行。

## 从开发交到产线的，到底不能少哪些东西？

把裸版本号展开为适用配置、需求/假设来源、特殊特性、控制与放行基线；用下游消费测试和交接回读区分文件传输与语义接口，并让生产/服务规划发现的安全需求回流开发。

## “正确版本”如此关键，为什么它不是车上的安全机制？

用时间、载体和偏差三个反事实分开安全相关特殊特性、生产控制方法和车载安全机制；再以扭矩、BMS 标定与 AEB 安装角度变式验证边界，明确 Part 10 Clause 14 只作资料性连续性指南。

## 一行读回比对，怎样才会变成可执行的生产控制？

先在规划期走查目标下发、包选择、通信、对象绑定、读回和判定失效，再把 B4 控制写成有顺序、目标来源、方法、必要资源、判据、反应与记录的合同；最后分开定义、执行和报告。

## 读回 B3 之后，是恢复符合、处置产品，还是修改定义？

沿 §6.4.1.2 的四步分析过程失效、安全影响、过程/产品双向措施和有效性；把恢复既定符合、嫌疑产品处置、责任人授权配置偏差与修改受控定义分账，并用换班、工具升级与重复返工说明“维持”。

## 一个 PASS，凭什么能绑定到这一颗 ECU？

从日期、对象标识和结果三项底线，扩展到序列化产品、控制执行、目标/实际配置、定义/工具版本与返工历史；用缓存串件和双错相消说明字段存在不等于报告与现实同一。

## 车辆离开工厂后，谁继续守住同一个安全前提？

完整走查 V17 更换 ECU、取适用配置、临时失能、刷写读回、换件关系和服务记录；再处理用户信息、报废/救援适用性与二手件再进入，保持规划说明与一次执行分层。

## 现场数据什么时候才算进入了行动闭环？

以低速助力迟滞工单走完提供可分析数据、分析并允许多种结果、对已识别问题触发行动；
说明数据偏差、分析节奏、车辆与过程双向行动、PiU 接口，以及 Semantica ch09 当前
package 尚未覆盖独立分析对象的断口。

## 本体化实践：机器与签字人最后各能批准什么？

以 Semantica ch09 已登记场景区分开放世界与局部闭世界、直接要求/本书投影/EPS
策略，并用单因反例量出已编码合同宽度。最终只签与证据同宽的 `EPS-S09` 候选；
scenario 通过不覆盖独立的 blocked release verdict。
