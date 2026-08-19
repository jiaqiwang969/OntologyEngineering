---
contract_version: 1
chapter: ch07
executable_package_id: semantica.chapter_packages.vol2.ch07
executable_authority: semantica_only_no_book_fallback
package_status: partial
release_status: blocked
target_hanzi: 40000
section_budgets:
  - heading: "十个“++”，为什么评审人仍按下暂停？"
    hanzi: 3000
  - heading: "第5章究竟把什么软件责任交了过来？"
    hanzi: 3500
  - heading: "两条 SSR 都有 ID，为什么测试预言机仍是空的？"
    hanzi: 4500
  - heading: "拆成两个软件单元，为什么仍没有免于干扰？"
    hanzi: 5500
  - heading: "“++”到底要求你做什么？"
    hanzi: 3500
  - heading: "手写 C 路线该怎样形成一组方法？"
    hanzi: 5500
  - heading: "四个 Planned 用例，离验证结论还有多远？"
    hanzi: 6000
  - heading: "本体化实践：机器能证明十个“++”的哪一层？"
    hanzi: 4500
  - heading: "评审最后能批准什么？"
    hanzi: 4000
consumes_state_ids: [EPS-S05]
produces_state_ids: [EPS-S07-SW]
first_teaches: [software-development-and-method-tables]
ontology_mapping_shape: method-table
source_anchors:
  - id: "6-Table-2"
    part: 6
    clause: "6-Table 2"
    artifact: "structured/mineru/ISO-26262-2018/part-06-software-level-development/native-full/ISO 26262-6-2018/auto/ISO 26262-6-2018_content_list_v2.json"
    pdf_page: 19
    block: 14
    bbox: [115, 554, 937, 755]
  - id: "10-9.2.4"
    part: 10
    clause: "10-9.2.4"
    artifact: "structured/mineru/ISO-26262-2018/part-10-guidelines/native-full/ISO 26262-10-2018/auto/ISO 26262-10-2018_content_list_v2.json"
    pdf_page: 63
    block: 6
    bbox: [114, 400, 875, 418]
  - id: "10-12.4"
    part: 10
    clause: "10-12.4"
    artifact: "structured/mineru/ISO-26262-2018/part-10-guidelines/native-full/ISO 26262-10-2018/auto/ISO 26262-10-2018_content_list_v2.json"
    pdf_page: 81
    block: 8
    bbox: [114, 643, 435, 659]
planned_outputs:
  - references/product-trustworthiness-book/ch07-software-development/chapter.md
gate_count_policy: runtime-derived
question_count_policy: learning-objective-driven
figure_policy: engineering-need-driven
---
# 第7章：软件层开发，十个“++”为什么还不能进集成

## 章级理解合同

- 唯一冲突：`CQ-CH07-05` 正确返回 ASIL D 下 Table 7 的十个 `++` 方法，负责人据此申请进入集成；评审人因软件输入、项目化 SSR、适用性/组合理由和执行证据均缺而暂停。
- 三层身份：方法表只给标准推荐画像；项目选择还需适用性、组合理由和评审；验证结论还需受控执行、实际结果、异常处置与报告。三层不得互相冒充。
- 章节动作：固定 `EPS-S05` 软件入口，精化两条 SSR，识别架构/FFI 欠账，正确读取 §4.3 与 Table 7/8/9，形成一条隔离的手写 C 组合推演，沿单元/集成/嵌入式软件三级活动收证，最后只让机器签其能证明的局部合同。
- 章际关系：ch07 与 ch06 均直接消费 `EPS-S05`；`EPS-S07-SW` 与 `EPS-S06-HW` 到 ch08 才会合，两个软件单元不能因画成两框就预先获得分解或独立性信用。

## 九节问题链

| 节 | 消费 | 产出与留问 |
|---|---|---|
| 7.1 十个“++”，为什么评审人仍按下暂停？ | 推荐查询与 Planned 状态查询 | 四项项目 Review Hold；先问软件入口 |
| 7.2 第5章究竟把什么软件责任交了过来？ | `EPS-S05`、TSR/HSI/时序欠账 | 软件入口卡；再问 SSR 是否可执行 |
| 7.3 两条 SSR 都有 ID，为什么测试预言机仍是空的？ | TSR 与 HSI | 两条 Draft SSR 的参数/模式/数据缺口；再问单元独立性 |
| 7.4 拆成两个软件单元，为什么仍没有免于干扰？ | SSR 分配与架构骨架 | ASIL 义务、FFI 主张和可验证单元输入；再问方法表 |
| 7.5 “++”到底要求你做什么？ | §4.3、Table 7/8/9 | 推荐/适用性/组合规则；再问项目组合 |
| 7.6 手写 C 路线该怎样形成一组方法？ | 失效假说与方法候选 | 离线适用性账、六证据组合理由与单因变式；再问执行 |
| 7.7 四个 Planned 用例，离验证结论还有多远？ | Draft 规格与用例 | 单元/集成/嵌入式证据边界及项目阶段门；再问机器能力 |
| 7.8 本体化实践：机器能证明十个“++”的哪一层？ | Semantica ch07 contract/CQ/scenario/oracle | 推荐/选择/证据三层与模型缺口；再问签字范围 |
| 7.9 评审最后能批准什么？ | 全章状态 | `EPS-S07-SW` 候选与开放项；交给 ch08 会合 |

## 保留与迁移

正文保留软件入口责任、两条 SSR 与两个软件单元、`requiredDevelopmentASIL`、FFI
五通道、§4.3 连续/备选规则、Table 7 十个 `++`、手写 C 组合理由、四个 Planned
用例、三级验证和三层方法表边界。机器合同只存在于 Semantica ch07 package；当前为
`partial`、release `blocked`。package 中已登记的表覆盖与未覆盖项必须由 manifest/oracle
诚实报告，正文不得补出第二套可执行目录或把摘要当作同等级来源治理。

## 练习配置

练习围绕单因迁移：新增行为模型、改变目标处理器、补一条启动模式 SSR、把 Planned
偷改 Completed、删除软件分配前提或改变跨表查询范围。机器部分只运行已登记的
Semantica ch07 场景并读取 receipt；书稿不复制约束，也不提供替代入口。

## 十个“++”，为什么评审人仍按下暂停？

用推荐查询与 Planned/Draft 状态查询构成两块同时正确却不能相加的绿屏，开出对象、SSR、组合和执行四项 Hold，并明确“暂停进入正式证据批次”是本教学项目门；探索性集成可以受控进行，但不能倒写为 ISO 刚性阶段门或验证结论。

## 第5章究竟把什么软件责任交了过来？

固定 ch06/ch07 从 `EPS-S05` 平行分叉，形成安全目标、TSR、HSI、架构和时间欠账的软件入口卡；以同一异常的 `t0–t3` 时间线走查硬件提供、软件使用和联合验证责任，已有分配只提供继续派生的合法上游。

## 两条 SSR 都有 ID，为什么测试预言机仍是空的？

让输入合理性与转矩范围检查一进一出夹住传播路径，同时揭示范围、变化率、新鲜度、诊断可信条件、模式、HSI、配置与标定仍未形成批准预言机；规范性 Annex C 的数据 ASIL、选定组合验证和发布验证不能被“文件已入库”替代，C.4.10/C.4.11 的改变检出与生成/应用责任只归于标定数据。

## 拆成两个软件单元，为什么仍没有免于干扰？

区分单元粒度、最高 ASIL 义务、ASIL 分解和 FFI；保留 §7.4.9 b) 的“专用硬件特性或等效手段”，用共享任务无界重试走完“故障传播→设计措施→分析→集成故障注入→重开条件”，让五条干扰通道和集成证据保持 Open。

## “++”到底要求你做什么？

按条目号先分连续/备选，再读 `++/+/o`；用负责人“十个都必须”和“任选一个就够”两次反驳拆开条目结构、推荐度、适用前提与条款证据目标。Table 7 是备选组，推荐度不是选择或结果，背靠背在无模型的手写 C 路线中是不适用而非偏离。

## 手写 C 路线该怎样形成一组方法？

在与项目状态隔离的反事实里，按失效假说形成适用性账，并把方法组合映射到 §9.4.2 六类证据；输入变化会重开判断，正文推演不生成项目选择对象。

## 四个 Planned 用例，离验证结论还有多远？

用四个用例暴露预言机和结果缺口，让同一个源时间戳/接收时间戳缺陷横跨 Clause 9 单元验证、Clause 10 软件集成与验证、Clause 11 嵌入式软件测试，说明受控结果复用、内部阶段门与标准活动身份的边界。

## 本体化实践：机器能证明十个“++”的哪一层？

Semantica ch07 的 CQ registry 分别询问追溯、状态、推荐画像和条款模态；package 的
局部闭世界场景只守声明范围内的转录与状态一致性，不评方法充分性或证据真实性。
精确覆盖集合只以源锁定 manifest/oracle 为准；当前 `partial` 与 `blocked` 状态不得由
正文中的表格数量覆盖。

## 评审最后能批准什么？

把十个对勾改成推荐、需求、架构、组合、单元验证和后续活动六类状态；输出带开放项的 `EPS-S07-SW`，只允许与实际证据等宽的签字。
