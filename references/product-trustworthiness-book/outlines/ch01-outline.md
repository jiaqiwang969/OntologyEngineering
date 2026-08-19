---
contract_version: 7
chapter: ch01
executable_package_id: semantica.chapter_packages.vol2.ch01
executable_authority: semantica_only_no_book_fallback
package_status: partial
release_status: blocked
chapter_title: "为什么‘全绿’不等于产品可信"
chapter_role: book_hinge_question_chapter
mirror_answer_chapter: ch11
problem_contract_id: PTW-PC-01
target_hanzi: 7300
status: authored_package_partial_release_blocked
book_source: references/product-trustworthiness-book/ch01-introduction/chapter.md
source_binding_authority: semantica_package_manifest
section_budgets:
  - heading: "六盏绿灯，为什么没人敢签字"
    hanzi: 1550
  - heading: "一句能签的话，长什么样"
    hanzi: 1300
  - heading: "报告是工件，支持是关系"
    hanzi: 1150
  - heading: "只改一件事"
    hanzi: 1000
  - heading: "换一个世界，问法还在"
    hanzi: 550
  - heading: "十个断面与一张地图"
    hanzi: 900
  - heading: "散会之后"
    hanzi: 700
consumes_state_ids: []
produces_state_ids:
  - PTW-PC-01
first_teaches:
  - local_pass_is_not_a_merge_operator
  - claim_anatomy_subject_configuration_concern_context_time_assumption_decision_scope
  - explicit_missing_set_over_silent_pass
  - evidence_relation_support_refute_out_of_scope_unknown
  - four_green_layers
  - single_factor_change_reopens_item_by_item
  - question_forms_migrate_answers_do_not
  - trustworthiness_as_scoped_falsifiable_acceptance
  - concern_separation
  - ten_question_chapters_form_one_causal_chain
  - ten_answer_chapters_map_one_to_one_to_questions
  - independent_answer_ontologies_share_method_not_business_facts
  - engineering_ontology_as_semantic_root
  - semantic_fact_and_authority_roots
ontology_mapping_shape: problem_contract_and_book_hinge_only_no_answer_ontology
source_anchors:
  - id: ISO26262-P1-INTRO-EVIDENCE
    part: 1
    clause: Introduction
    artifact: "structured/mineru/ISO-26262-2018/part-01-vocabulary/native-full/ISO 26262-1-2018/auto/ISO 26262-1-2018_content_list_v2.json"
    artifact_sha256: 7978027faf7330e083d6e11a0bd5e854ab78fbbc1ab08cc08e673140db29ca1c
    pdf_page: 6
    block: 3
    bbox: [55, 216, 885, 262]
  - id: ISO26262-P1-SCOPE-MALFUNCTIONING-EE
    part: 1
    clause: Scope
    artifact: "structured/mineru/ISO-26262-2018/part-01-vocabulary/native-full/ISO 26262-1-2018/auto/ISO 26262-1-2018_content_list_v2.json"
    artifact_sha256: 7978027faf7330e083d6e11a0bd5e854ab78fbbc1ab08cc08e673140db29ca1c
    pdf_page: 9
    block: 6
    bbox: [112, 480, 941, 542]
  - id: ISO26262-P1-DEF-3.67
    part: 1
    clause: "3.67"
    artifact: "structured/mineru/ISO-26262-2018/part-01-vocabulary/native-full/ISO 26262-1-2018/auto/ISO 26262-1-2018_content_list_v2.json"
    artifact_sha256: 7978027faf7330e083d6e11a0bd5e854ab78fbbc1ab08cc08e673140db29ca1c
    pdf_page: 22
    block: 7
    bbox: [57, 243, 885, 274]
authoring_and_runtime_bindings:
  - references/product-trustworthiness-book/ch01-introduction/chapter.md
  - references/product-trustworthiness-book/outlines/ch01-outline.md
  - semantica.chapter_packages.vol2.ch01
adjacent_front_matter:
  path: references/product-trustworthiness-book/front-matter/preface.md
  role: authorial_origin_enterprise_ontology_loop_and_book_motivation
  excluded_from_ch01_case: true
  enterprise_ontology_loop:
    semantic_center: federated_enterprise_engineering_ontology_system
    participating_activities: [sales, r_and_d, test, quality, manufacturing, service, employee_learning]
    writeback_rule: candidate_then_validate_then_accept
    prohibited_reading: agent_as_truth_or_decision_center
figure_contract:
  id: ch01-fig01-green-without-claim
  registry: handbook/book-figure-plan.yaml
  figure_type: evidence-structure
  status: placed_and_consumed_in_chapter
  current_disposition: chapter_in_text
  current_reader_placement: before_section_1_2
  asset: handbook/figures-imagegen/ch01-fig01-green-without-claim-v04.png
  asset_sha256: e5b0af12f4728943cc7dac9c0d076f1007e3a5dcc1ef2c9feed4de22c8abdbab
gate_count_policy: runtime_derived
question_count_policy: learning_objective_driven
figure_policy: exactly_one_in_text_figure_observed_and_consumed
---

# 第 1 章 为什么“全绿”不等于产品可信

## 章级角色

前言负责讲作者与雨刷企业相遇、"模型一更新就被吃掉"的生存判断以及本书为何诞生；
第 1 章不再承担作者缘起。本章使用 EPS 六盏绿灯合成评审故事作为全书总铰链：
让"局部全绿即可放行"的自然判断完整发生并撞碎，逐部件重建一条可反驳的完整主张，
建立证据与主张的四种关系和四层绿色，用软件单因变式演示逐项重开，
用 ENV-01 证明问法可迁移而答案不可迁移，最后展开前十问与后十答的地图。

## 纵向问题链（contract_version 7 起）

| 节 | 核心动作 | 必须到达的判断 | 节尾交出的问题 |
|---|---|---|---|
| 1.1 | 让 NaturalJudgment-01 完整发生，三个自然补救（汇总表/全部重测/领导拍板）逐一失效 | 绿色是结论的颜色不是主语；六绿主语各异不能相加 | 一句能签的话长什么样 |
| 1.2 | 把"可以交付了"逐部件重建为完整主张（主语/配置/关注/情境时间/假设/决定范围+缺件清单） | 不能被反驳的话没有资格被签署；可信是有范围、可撤销的接受 | 报告与主张是什么关系 |
| 1.3 | 软件绿卡射程推演；六卡对主张四分类（支持/超出范围/反驳/未知）；四层绿色 | 报告是工件，支持是关系；缺记录默认为未知不是假 | 软件升版后关系还剩多少 |
| 1.4 | SW1.8.3→SW1.8.4 单因变式；两个默认（全保留/全作废）失效；逐卡重开或有理由保留 | 变化不把旧证据变假，它改变证据与主张的关系 | 问法是否只属于汽车 |
| 1.5 | ENV-01 六绿复现同一僵局；问法迁移、结论禁运 | 可迁移的是问法，不可迁移的是答案 | 裂缝还有哪些断面 |
| 1.6 | 十断面一段展开 + 十问十答镜像表 + 三种权威分立与 Agent 边界 | 后十章共享方法不共享业务事实；本体/事实/决定三源分立 | ——（地图收拢） |
| 1.7 | 散会任务分派回到人物；同名争论交接 ch02；兑现导读期票；读者三动作 | ch02 只接收身份与词义问题 | 同一个名字是不是同一个东西 |

## 案例与来源边界

- `EPS-RC17`（H3.2/SW1.8.3/C41/D7/V12）、台架配置（P07+SW1.8.4-rc2+C42）、人物
  （陈工/小唐/郑工/小林）与冬试返工事故均为合成教学材料，不对应真实项目。
- 六项绿色结果的活动、范围与 `does_not_establish` 只以源锁定 Semantica ch01 package
  的 contract/CQ/oracle 为机器正本；书稿保留解释，不保留执行副本。
- 软件单因变式用于证明"全部保留"和"全部作废"都不是合法默认值；台架卡在 1.2 与 1.4
  各消费一次（对不上表 / 接近不等于等于）。
- `ENV-01` 只证明问法可跨出汽车复现，不继承 EPS、汽车分级语义或功能安全结论。
- 汽车功能安全只是一条严格纵线，不是全部关注的总本体；关注并列不折总分。
- 前言雨刷企业缘起由独立用户来源约束，不得被混写成 ch01 的真实 EPS 案例。
- 内部编号、精确条款定位、哈希和来源坐标只进入合同与来源账，不回流正文。

## 图像处置

图 1-1 已在正文 1.1 的观察任务之后放置，并在进入 1.2 前消费：六张局部绿卡的
母线都停在中央主张边界之前。它只表达“局部 PASS 不会自动合并成产品级放行主张”，
不表示下游十个本体已经完成，也不构成 Semantica package 的 release 证据。

## 章末交接

本章只向第 2 章交出"同一个名称是否指向同一对象和同一概念"这一问题。
Semantica ch01 package 当前为 `partial`、release `blocked`；第 2 章的书稿存在不改变
这一状态，也不得作为 ch01 release 的回退入口。
