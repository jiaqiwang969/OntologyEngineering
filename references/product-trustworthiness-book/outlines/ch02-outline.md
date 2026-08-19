---
contract_version: 2
chapter: ch02
executable_package_id: semantica.chapter_packages.vol2.ch02
executable_authority: semantica_only_no_book_fallback
package_status: partial
release_status: blocked
title: "同一句“可靠”，为什么可能在说不同的事"
target_hanzi: 12000
section_budgets:
  - heading: "白板上的争论，这个行业早就吵过了"
    hanzi: 1800
  - heading: "相关项、系统、元素：分析边界的名字"
    hanzi: 1700
  - heading: "故障、错误、失效：一条链，三个词"
    hanzi: 1500
  - heading: "四个一百毫秒"
    hanzi: 1000
  - heading: "ASIL：风险的语言，不是荣誉的语言"
    hanzi: 900
  - heading: "换一个世界：ENV-01 需要自己的术语表"
    hanzi: 1800
  - heading: "纸上语言到不了的三个地方"
    hanzi: 3300
consumes_state_ids: [PTW-PC-01-handoff-same-name-dispute]
produces_state_ids: [PTW-PC-02-candidate]
first_teaches: [identity-and-category-boundaries]
ontology_mapping_shape: semantica-package-only
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
authoring_and_runtime_bindings:
  - references/product-trustworthiness-book/ch02-concepts-terminology/chapter.md
  - references/product-trustworthiness-book/outlines/ch02-outline.md
  - semantica.chapter_packages.vol2.ch02
  - handbook/figures-imagegen/ch02-fig01-fault-error-failure-chain-v01.png
gate_count_policy: runtime-derived
question_count_policy: exactly-three-mirror-cq-drafts
figure_policy: exactly-one-imagegen-teaching-figure
figure_contract:
  id: ch02-fig01-fault-error-failure-chain
  registry: handbook/book-figure-plan.yaml
  status: placed_and_consumed_in_chapter
  current_reader_placement: end_of_section_2_3
  asset: handbook/figures-imagegen/ch02-fig01-fault-error-failure-chain-v01.png
  asset_sha256: d3804b5e621339d5aa03e90b6422eab833fab2e9e841b43338c8ce9f17a29baa
---
# 第 2 章 同一句“可靠”，为什么可能在说不同的事

## 章级承诺

- 写作组织：承接 ch01 白板上的 `EPS-RC17`，同一句“可靠”贯穿全章，不恢复术语抽屉。
- 自然错误：同一术语表和同名 ID 被误当成同一对象/含义，异名记录被误当成不同对象。
- 本体纪律：先问类/个体、身份判据、关系、state/role，再冻结问题；不提前交付 ch12 TBox。
- ISO 纪律：Part 1 术语精确保留，Part 10 只作资料性解释；跨层传播与时间关系保持条件和主体。
- 迁移纪律：ENV-01 只共享拆问方法，不继承 EPS 个体、ASIL 或 SafetyGoal。
- 图文纪律：正文稳定后生成一幅 ImageGen 概念图，图前提出观察任务、图后消费结论。

## 2.1 白板上的争论，这个行业早就吵过了

让硬件、软件、整车、制造、服务和项目经理对同一句 `EPS-RC17 可靠` 作出不同但诚实
的解释，并用受控术语说明同名争论为什么会把对象、关注、时间与决定范围混在一起。

## 2.2 相关项、系统、元素：分析边界的名字

用 item/system/element 建立分析边界，说明这些名字不是零件户籍；同名异物、异名同物
和证据不足时的 Unknown 留给后续身份本体回答。

## 2.3 故障、错误、失效：一条链，三个词

在 R17→ADC→非预期助力的合成路径中保留 fault/error/failure 的并列定义、观察边界、
条件传播与 Unknown。图 2-1 就放在本节命题之后，由图前观察任务和图后边界共同消费。

## 2.4 四个一百毫秒

用 FDTI/FHTI/FRTI/FTTI 说明同单位同数值不等于同 measure；safe state 是 failure 语境中的 state，
不是永久产品类型或关断同义词。

## 2.5 ASIL：风险的语言，不是荣誉的语言

把分级重新绑定到危害事件及其风险语境，不把 ASIL 当作组件、产品或团队的等级头衔。

## 2.6 换一个世界：ENV-01 需要自己的术语表

分开准确度、漂移稳定性、可靠性、可用性、可维护性和数据完整性；用校准系数与网络重启两个单因变式检验边界。

## 2.7 纸上语言到不了的三个地方

冻结纸面语言无法自行处理的身份、活动时效与授权问题；不提前替第 12/13 章作答，
也不把术语一致误写成对象、事实或决定已经一致。

## 验收边界

- H1/H2 与本大纲一致，每节消费上一节留下的歧义或对象。
- `PTW-PC-02` 恰含三条镜像 CQ，且不包含 ch12 的 OWL/SHACL/SPARQL 实现。
- 同名不得自动 Same，异名不得自动 Different，证据不足必须保持 Unknown。
- Fault/Error/Failure 保持并列，跨层关系保留条件；Part 10 示例不得提升为普遍公理。
- ENV-01 六种 concern 不互相推出，不出现 ASIL、SafetyGoal 或 EPS 个体泄漏。
- 图 2-1 的资产、位置、观察任务与图后边界必须和当前正文逐字对齐；图文 QA 不构成 package release。
- 章节最多进入 candidate；独立人工冷读、用户章级接受和出版放行未发生时不得标为 accepted。
