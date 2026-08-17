---
contract_version: 2
chapter: ch02
title: "同一句“可靠”，为什么可能在说不同的事"
target_hanzi: 12000
section_budgets:
  - heading: "白板上只有一句话，会议里却有六个世界"
    hanzi: 1200
  - heading: "名称不是身份，编号也不是"
    hanzi: 1800
  - heading: "功能、行为、关注、度量、状态和角色，不是一棵树"
    hanzi: 1700
  - heading: "三个“故障”，不是一条必然流水线"
    hanzi: 1500
  - heading: "同一个“100 ms”，也可能不是同一种时间"
    hanzi: 1000
  - heading: "把“可靠”搬到 ENV-01"
    hanzi: 1800
  - heading: "一个词落到多个工程世界"
    hanzi: 900
  - heading: "冻结问题，不提前替第 12 章作答"
    hanzi: 2100
consumes_state_ids: [PTW-PC-01-handoff-same-name-dispute]
produces_state_ids: [PTW-PC-02-candidate]
first_teaches: [identity-and-category-boundaries]
ontology_mapping_shape: problem-contract-only
mirror_answer_chapter: ch12
source_anchors:
  - id: "1-3.41"
    term: element
    part: 1
    artifact: "structured/mineru/ISO-26262-2018/part-01-vocabulary/native-full/ISO 26262-1-2018/auto/ISO 26262-1-2018_content_list_v2.json"
    pdf_page: 16
    block: 23
    bbox: [57, 687, 99, 701]
  - id: "1-3.46"
    term: error
    part: 1
    artifact: "structured/mineru/ISO-26262-2018/part-01-vocabulary/native-full/ISO 26262-1-2018/auto/ISO 26262-1-2018_content_list_v2.json"
    pdf_page: 18
    block: 0
    bbox: [57, 98, 102, 112]
  - id: "1-3.50"
    term: failure
    part: 1
    artifact: "structured/mineru/ISO-26262-2018/part-01-vocabulary/native-full/ISO 26262-1-2018/auto/ISO 26262-1-2018_content_list_v2.json"
    pdf_page: 18
    block: 19
    bbox: [58, 587, 100, 601]
  - id: "1-3.54"
    term: fault
    part: 1
    artifact: "structured/mineru/ISO-26262-2018/part-01-vocabulary/native-full/ISO 26262-1-2018/auto/ISO 26262-1-2018_content_list_v2.json"
    pdf_page: 19
    block: 0
    bbox: [114, 98, 159, 112]
  - id: "1-3.55/56/59/61"
    term: FDTI_FHTI_FRTI_FTTI
    part: 1
    artifact: "structured/mineru/ISO-26262-2018/part-01-vocabulary/native-full/ISO 26262-1-2018/auto/ISO 26262-1-2018_content_list_v2.json"
    pdf_pages: [19, 20]
  - id: "1-3.84"
    term: item
    part: 1
    artifact: "structured/mineru/ISO-26262-2018/part-01-vocabulary/native-full/ISO 26262-1-2018/auto/ISO 26262-1-2018_content_list_v2.json"
    pdf_page: 24
    block: 4
    bbox: [58, 178, 100, 192]
  - id: "1-3.131"
    term: safe_state
    part: 1
    artifact: "structured/mineru/ISO-26262-2018/part-01-vocabulary/native-full/ISO 26262-1-2018/auto/ISO 26262-1-2018_content_list_v2.json"
    pdf_page: 29
    block: 23
    bbox: [114, 690, 168, 706]
  - id: "1-3.163"
    term: system
    part: 1
    artifact: "structured/mineru/ISO-26262-2018/part-01-vocabulary/native-full/ISO 26262-1-2018/auto/ISO 26262-1-2018_content_list_v2.json"
    pdf_page: 33
    block: 21
    bbox: [115, 565, 168, 580]
  - id: "10-4.2"
    part: 10
    artifact: "structured/mineru/ISO-26262-2018/part-10-guidelines/native-full/ISO 26262-10-2018/auto/ISO 26262-10-2018_content_list_v2.json"
    pdf_page: 14
    block: 1
    bbox: [55, 203, 739, 221]
  - id: "10-4.3.1"
    part: 10
    artifact: "structured/mineru/ISO-26262-2018/part-10-guidelines/native-full/ISO 26262-10-2018/auto/ISO 26262-10-2018_content_list_v2.json"
    pdf_page: 15
    block: 2
    bbox: [114, 469, 531, 485]
  - id: "10-4.4"
    part: 10
    artifact: "structured/mineru/ISO-26262-2018/part-10-guidelines/native-full/ISO 26262-10-2018/auto/ISO 26262-10-2018_content_list_v2.json"
    pdf_page: 16
    block: 3
    bbox: [57, 574, 601, 592]
planned_outputs:
  - functional-safety-book/ch02-concepts-terminology/chapter.md
  - functional-safety-book/ch02-concepts-terminology/problem-contract.yaml
  - functional-safety-book/ch02-concepts-terminology/SOURCE-AUDIT.md
  - functional-safety-book/ch02-concepts-terminology/README.md
  - functional-safety-book/ch02-concepts-terminology/examples/core-terms.txt
  - functional-safety-book/ch02-concepts-terminology/examples/fault-error-failure.txt
  - functional-safety-book/ch02-concepts-terminology/examples/asil-explained.txt
  - handbook/figures-imagegen/ch02-fig01-one-word-many-worlds-*.png
gate_count_policy: runtime-derived
question_count_policy: exactly-three-mirror-cq-drafts
figure_policy: exactly-one-imagegen-teaching-figure
---
# 第 2 章 同一句“可靠”，为什么可能在说不同的事

## 章级承诺

- 写作组织：承接 ch01 白板上的 `EPS-RC17`，同一句“可靠”贯穿全章，不恢复术语抽屉。
- 自然错误：同一术语表和同名 ID 被误当成同一对象/含义，异名记录被误当成不同对象。
- 本体纪律：先问类/个体、身份判据、关系、state/role，再冻结问题；不提前交付 ch12 TBox。
- ISO 纪律：Part 1 术语精确保留，Part 10 只作资料性解释；跨层传播与时间关系保持条件和主体。
- 迁移纪律：ENV-01 只共享拆问方法，不继承 EPS 个体、ASIL 或 SafetyGoal。
- 图文纪律：正文稳定后生成一幅 ImageGen 概念图，图前提出观察任务、图后消费结论。

## 2.1 白板上只有一句话，会议里却有六个世界

让硬件、软件、整车、制造、服务和项目经理对同一句 `EPS-RC17 可靠` 作出不同但诚实的解释，
暴露主语、concern、时间与决定范围同时缺失。

## 2.2 名称不是身份，编号也不是

用 item/system/element 与 artifact/physical individual 建立对象范畴；说明同名异物、异名同物、
类型相容的身份判据和证据不足时的 Unknown。

## 2.3 功能、行为、关注、度量、状态和角色，不是一棵树

把 function/behavior、concern/measure、essential type/state/role 分开；用关系代替错误继承，
给一句“可靠”补出最小防误读变量。

## 2.4 三个“故障”，不是一条必然流水线

在 R17→ADC→非预期助力的合成路径中保留 fault/error/failure 的并列定义、观察边界、条件传播与 Unknown。

## 2.5 同一个“100 ms”，也可能不是同一种时间

用 FDTI/FHTI/FRTI/FTTI 说明同单位同数值不等于同 measure；safe state 是 failure 语境中的 state，
不是永久产品类型或关断同义词。

## 2.6 把“可靠”搬到 ENV-01

分开准确度、漂移稳定性、可靠性、可用性、可维护性和数据完整性；用校准系数与网络重启两个单因变式检验边界。

## 2.7 一个词落到多个工程世界

图 2-1 展示同一“可靠”标签如何落到车辆功能、系统、组件、软件工件、运行行为和 DUT 角色；
视觉邻近不执行身份合并。

## 2.8 冻结问题，不提前替第 12 章作答

冻结 `PTW-PC-02`、Same/Different/Unknown、三条 CQ 与镜像验收，随后以“谁有权确认身份合并”交给 ch03。

## 验收边界

- H1/H2 与本大纲一致，每节消费上一节留下的歧义或对象。
- `PTW-PC-02` 恰含三条镜像 CQ，且不包含 ch12 的 OWL/SHACL/SPARQL 实现。
- 同名不得自动 Same，异名不得自动 Different，证据不足必须保持 Unknown。
- Fault/Error/Failure 保持并列，跨层关系保留条件；Part 10 示例不得提升为普遍公理。
- ENV-01 六种 concern 不互相推出，不出现 ASIL、SafetyGoal 或 EPS 个体泄漏。
- 图 2-1 必须经 ImageGen、语义/视觉/打印/权利复核、正文观察与消费、同树 PDF QA 后才可关闭图文项。
- 章节最多进入 candidate；独立人工冷读、用户章级接受和出版放行未发生时不得标为 accepted。
