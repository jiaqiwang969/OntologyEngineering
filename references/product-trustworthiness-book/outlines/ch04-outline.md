---
contract_version: 2
chapter: ch04
executable_package_id: semantica.chapter_packages.vol2.ch04
executable_authority: semantica_only_no_book_fallback
package_status: partial
release_status: blocked
chapter_contract_id: PTW-PC-04
main_title: "先决定担心什么：从使用情境到安全目标"
subtitle: "HARA 纵向样板"
status: rewritten_working_copy
paired_answer_chapter: ch14
pair_contract_id: PTW-OAC-14
pair_dependency: contract_only_no_runtime_graph_dependency
target_hanzi: 18000
planned_outputs:
  - references/product-trustworthiness-book/ch04-concept-hara/chapter.md
  - semantica.chapter_packages.vol2.ch04 / source-problem-contract
final_figure_id: ch04-fig01-context-changes-consequence
figure_policy: imagegen_final_puml_draft_only
---

# 第 4 章施工提纲

## 章级问题

当同一对象的同一偏差出现在不同情境中时，团队如何识别正在被评价的 scenario、
concern 与 consequence，并在不预支方案的情况下写出 objective？

## 问题链

| 节 | 读者先相信什么 | 本节怎样使旧判断失效 | 交给下一节 |
|---|---|---|---|
| 4.1 先画措施，为什么看起来如此合理 | 故障清单、等级和措施已经足够 | 白板方案无法回答在何种处境、担心何种损失 | `NaturalJudgment-04` |
| 4.2 同一失常，换个处境就不是同一件事 | 失常名称自带后果和等级 | 只换 context，高速与维修的后果链分开 | `ContextSplit-04` |
| 4.3 你到底在担心哪一种损失 | 危险、后果、目标和方案可以写在一句话里 | 分开 subject/context/scenario/concern/consequence/objective/assumption | `DistinctionCard-04` |
| 4.4 HARA 如何把安全问题压成一条可审查的纵线 | HARA 从查表开始 | 先稳定 Item，再走失常、Hazard、Operational Situation、Hazardous Event 与 Consequence | `EPS-HARA-CaseFile-Draft` |
| 4.5 三把尺子回答三种不同的问题 | S/E/C 是三个印象分 | 伤害、使用剖面和人因证据分别立论，再查 Table 4 | `EPS-Classification-Draft` |
| 4.6 等级之后，目标仍然不能先写成方案 | ASIL D 会自动推出双传感器和关断 | Safety Goal 先写功能意图，方案回到候选位置 | `EPS-SafetyGoal-Draft` 与 `FSC-InputBoundary` |
| 4.7 把同一问法搬到 ENV-01 | HARA 语言可以换名移植到任何产品问题 | 保留通用问法，拒绝复制 S/E/C、ASIL、SafetyGoal | `ENV01-ScenarioObjective-Draft` |
| 4.8 把问题冻结给 ch14 | 做一张知识图谱就算回答 | 先冻结 CQ、精确答案形式与不能推出的结论 | `ProblemContract PTW-PC-04` |

## 主案例与单因变式

主案例始终是 EPS 非预期助力。单因变式保持分析对象和偏差不变，只更换受控情境身份：

- 高速道路：道路使用者安全 concern，可进入 EPS HARA 纵线；
- 静止维修工位：服务连续性 concern；车辆静止且无人处于运动包络内，不复制高速事件的 ASIL 或 Safety Goal。

ENV-01 是迁移反例，不是第二条平行 HARA：仓储监测与校准台检查形成两个 measurement-stability
目标，现实规格、漂移时序、不确定度和现场证据保持 `Unknown`。

## 来源边界

Part 3 Clause 5/6 与 Table 1—4 是 EPS 纵线的主要受控来源；Part 1 3.77 固定 Hazardous Event
身份；Annex B 与 Part 10 只作资料性校准。Table 4 的 `S3 × E1 × C3 → ASIL A` 脚注和
Clause 6.4.3.11 的许可性 QM 论证必须保留，不能改写为自动降级。

## 章节边界

- FSC 的规范身份仍在 Part 3 Clause 7，但展开教学移交第 5 章。
- 本体类、IRI、关系方向、查询和约束实现由第 14 章独立决定。
- 第 4 章不声称现实等级、目标充分性、实现有效、验证完成、标准符合或产品放行。
