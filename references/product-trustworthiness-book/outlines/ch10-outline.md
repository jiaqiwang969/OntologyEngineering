---
contract_version: 1
chapter: ch10
executable_package_id: semantica.chapter_packages.vol2.ch10
executable_authority: semantica_only_no_book_fallback
package_status: partial
release_status: blocked
target_hanzi: 40000
section_budgets:
  - heading: "五张材料都写着 PASS，为什么 B-2026-04 仍不能签？"
    hanzi: 3500
  - heading: "DIA 已经签字，CR-0412 的整车确认和供应商 FSA 到底归谁？"
    hanzi: 5000
  - heading: "“收紧转矩合理性阈值”凭什么算一条可交付的安全需求？"
    hanzi: 4500
  - heading: "回归报告写着 PASS，它验证的是哪版对象？"
    hanzi: 4500
  - heading: "从 B-2026-03 到 B-2026-04，哪些结论必须重开？"
    hanzi: 5000
  - heading: "同一品牌、同一张旧证书，为什么不能覆盖新的工具用法？"
    hanzi: 5500
  - heading: "旧组件用了十年，究竟能走哪条复用通道？"
    hanzi: 5000
  - heading: "本体化实践：机器能否证明五张材料终于指向同一个快照？"
    hanzi: 4000
  - heading: "回到放行会，哪些窄结论可以签？"
    hanzi: 3000
consumes_state_ids: [GOV-S03, EPS-S05]
produces_state_ids: [SUP-S10]
first_teaches: [supporting-processes-and-tool-confidence]
ontology_mapping_shape: cross-part-reuse
source_anchors:
  - id: "8-5.4.3"
    part: 8
    clause: "8-5.4.3"
    artifact: "structured/mineru/ISO-26262-2018/part-08-supporting-processes/native-full/ISO 26262-8-2018/auto/ISO 26262-8-2018_content_list_v2.json"
    pdf_page: 16
    block: 7
    bbox: [57, 324, 561, 342]
  - id: "8-6.4.2"
    part: 8
    clause: "8-6.4.2"
    artifact: "structured/mineru/ISO-26262-2018/part-08-supporting-processes/native-full/ISO 26262-8-2018/auto/ISO 26262-8-2018_content_list_v2.json"
    pdf_page: 21
    block: 6
    bbox: [114, 576, 633, 593]
  - id: "8-8.4.1.4"
    part: 8
    clause: "8-8.4.1.4"
    artifact: "structured/mineru/ISO-26262-2018/part-08-supporting-processes/native-full/ISO 26262-8-2018/auto/ISO 26262-8-2018_content_list_v2.json"
    pdf_page: 27
    block: 0
    bbox: [112, 98, 568, 115]
  - id: "8-9.4.1"
    part: 8
    clause: "8-9.4.1"
    artifact: "structured/mineru/ISO-26262-2018/part-08-supporting-processes/native-full/ISO 26262-8-2018/auto/ISO 26262-8-2018_content_list_v2.json"
    pdf_page: 30
    block: 1
    bbox: [57, 131, 309, 148]
  - id: "8-11.4.1"
    part: 8
    clause: "8-11.4.1"
    artifact: "structured/mineru/ISO-26262-2018/part-08-supporting-processes/native-full/ISO 26262-8-2018/auto/ISO 26262-8-2018_content_list_v2.json"
    pdf_page: 37
    block: 1
    bbox: [114, 131, 366, 148]
  - id: "8-Table-3"
    part: 8
    clause: "8-Table 3"
    artifact: "structured/mineru/ISO-26262-2018/part-08-supporting-processes/native-full/ISO 26262-8-2018/auto/ISO 26262-8-2018_content_list_v2.json"
    pdf_page: 39
    block: 17
    bbox: [304, 827, 749, 902]
  - id: "10-9"
    part: 10
    clause: "10-9"
    artifact: "structured/mineru/ISO-26262-2018/part-10-guidelines/native-full/ISO 26262-10-2018/auto/ISO 26262-10-2018_content_list_v2.json"
    pdf_page: 57
    block: 0
    bbox: [114, 98, 448, 116]
  - id: "10-10"
    part: 10
    clause: "10-10"
    artifact: "structured/mineru/ISO-26262-2018/part-10-guidelines/native-full/ISO 26262-10-2018/auto/ISO 26262-10-2018_content_list_v2.json"
    pdf_page: 65
    block: 14
    bbox: [115, 765, 539, 783]
  - id: "10-13"
    part: 10
    clause: "10-13"
    artifact: "structured/mineru/ISO-26262-2018/part-10-guidelines/native-full/ISO 26262-10-2018/auto/ISO 26262-10-2018_content_list_v2.json"
    pdf_page: 82
    block: 0
    bbox: [57, 98, 613, 116]
planned_outputs:
  - references/product-trustworthiness-book/ch10-supporting-processes/chapter.md
gate_count_policy: runtime-derived
question_count_policy: learning-objective-driven
figure_policy: engineering-need-driven
---
# 第10章：支撑过程，让跨阶段证据可信又可复用

## 章级理解合同

- 唯一冲突：现场问题触发 `CR-0412`，供应商收紧
  `TSR_TorquePlausibility` 并提交新基线 `B-2026-04`。放行会上，签署的 DIA、需求评审
  PASS、回归报告 PASS、代码生成器既有资格材料和旧组件运行历史看起来都是绿的，却分别指向
  旧责任范围、含混或旧版需求、旧基线、旧 tool usage 和旧配置历史。它们没有共同指向同一对象、
  同一版本、同一使用边界、同一责任与同一次决定，因而不能拼成一次放行。
- 中心区分：支撑过程不替产品制造安全；它防止证据跨组织、跨阶段、跨版本和跨项目流动时换掉
  主语、对象或边界。签名、PASS、证书和历史都只能支持与其身份和范围同宽的结论。
- 贯穿对象：同一个 `CR-0412`、`TSR_TorquePlausibility`、`B-2026-04`、回归报告和代码生成
  usage 贯穿全章；软件组件复用只作为该变更包中的第二条输入，不另开平行故事。
- 状态：`GOV-S03` + `EPS-S05` → `SUP-S10`。横切支撑契约不推进 EPS 生命周期成熟度；本章
  结束时仍只能交带 Hold 的候选快照。

## 九节问题链

| 节 | 消费 | 产出与留问 |
|---|---|---|
| 10.1 五张材料都写着 PASS，为什么 `B-2026-04` 仍不能签？ | `CR-0412`、五张绿材料与签字请求 | 六项身份错位和 Review Hold；先问责任由谁承担 |
| 10.2 DIA 已经签字，`CR-0412` 的整车确认和供应商 FSA 到底归谁？ | 分布式开发边界、旧 DIA 与变更后的任务 | 责任/信息/活动/执行活性接口卡；再问交给对方的需求是否可执行 |
| 10.3 “收紧转矩合理性阈值”凭什么算一条可交付的安全需求？ | 需求草案、ASIL、上游来源与接口 | 可判定需求及三向追溯；再问 PASS 验的是哪版对象 |
| 10.4 回归报告写着 PASS，它验证的是哪版对象？ | 需求基线、验证计划/规格/执行/评价 | 版本绑定的验证结论；再问对象变化后结论是否仍成立 |
| 10.5 从 `B-2026-03` 到 `B-2026-04`，哪些结论必须重开？ | 旧基线、变更请求、影响与验证路径 | 授权决定、新基线与文档身份；再问工具 usage 是否仍受旧资格覆盖 |
| 10.6 同一品牌、同一张旧证书，为什么不能覆盖新的工具用法？ | 新旧代码生成 usage、输出检查路径与旧资格材料 | 适用性→TI/TD→TCL→方法表→边界化证据；再问旧组件历史能否复用 |
| 10.7 旧组件用了十年，究竟能走哪条复用通道？ | 精确候选对象、用途变化、既有证据与现场历史 | Clause 12/13/14、SEooC 与工具历史的边界对照；再问机器能否核对快照一致性 |
| 10.8 本体化实践：机器能否证明五张材料终于指向同一个快照？ | Semantica ch10 contract/CQ/scenario/oracle | 已建工具子域/未建配置与 DIA 分账；再问签字人还能批准什么 |
| 10.9 回到放行会，哪些窄结论可以签？ | 全章 Hold、当前仓库事实与 `SUP-S10` | 窄裁决、单因变式、文件入口和 ch11 组装之门 |

## 保留与迁移

保留旧稿中来源正确且教学价值高的材料：DIA 无主项、坏需求改写、七要素用例、`CR-0412`
变更链、usage/TI/TD 对照、复用三问、Table 3/4/5 的机器合同和所有诚实边界。旧稿的第二个独立
PiU 故事、条款鸟瞰、新词地图和十一/十/九/七/六项逐条库存不再充当骨架。唯一
机器合同在 Semantica ch10 package；当前为 `partial`、release `blocked`，书稿不保存
第二套规则、数据、查询、完整目录或运行入口。

## 图的冲突落点

- 图 10-1 只在责任冲突处画多层供应边界与 DIA 活性，不把十一项内容压进一张总览图。
- 图 10-2 用于需求身份和派生关系，不承担 Clause 6 全目录。
- 图 10-3/10-4 连续回答新旧 usage 为何产生不同适用性、TI/TD/TCL 和资格方法路由。
- 图 10-5 在复用评审处按“对象—证据—变化触发”分流，不把通道画成强弱等级。

## 本体化实践的验收边界

Semantica ch10 package 已登记的问题与场景可检查 Table 3 六格、两个教学工具 usage 的
TI/TD/TCL，以及 Table 4/5 的 2 表/8 方法/32 单元。package 当前不能证明 §11.4.1
工具清单完整、rationale 工程真实或资格活动已执行；DIA、需求质量、配置/变更链与
五张材料的共同快照仍未对象化。机器只能对登记范围签结构一致，不能替
`B-2026-04` 放行；release 继续 `blocked`。

## 练习配置

每次只改一个事实：把整车确认留成无主项、删掉需求阈值或时间预算、让 PASS 报告指向旧基线、
把变更请求的基线引用换错、保持工具名却更换 usage、把 TI2×TD3 错标 TCL1、把旧组件现场小时
移给新配置，或把 Planned 资格材料写成 Completed。答题必须指出哪一条身份、责任、需求、验证、
变更、工具或复用主张被重开，以及哪些既有证据仍可保留。

## 五张材料都写着 PASS，为什么 B-2026-04 仍不能签？

从放行会直接入场，把五张真实绿材料还原成“责任方—对象—版本/配置—环境/usage—准则—时点”的
关系主张。用统一改名和哈希相同两个反事实证明共同快照不是目录或文件内容同一，形成六类 Hold 与
书稿层 `SUP-S10` 候选接口记号。

## DIA 已经签字，CR-0412 的整车确认和供应商 FSA 到底归谁？

让整车厂、一级供应商和二级组件供应商沿同一个变更交换信息与证据，区分签名、责任结构、现实能力
和接口执行活性。把 Clause 5 的 DIA 内容组织成决定/执行、边界交换、评价/检查权三类断口，并用
无 A、有 A 但无能力两个变式量出本书责任门禁的宽度。

## “收紧转矩合理性阈值”凭什么算一条可交付的安全需求？

先让含混句子遭到供应商四问，再拆成可判断的合成需求；随后用时间预算冲突、测量能力不足和状态
跨越语义证明“数字齐全”仍不等于正确。单条质量、集合质量、三向追溯以及 Table 1/2 的独立方法
选择依次进入，不另造平行案例。

## 回归报告写着 PASS，它验证的是哪版对象？

让 186 条旧回归撞上新需求和新标定，把 PASS 展开为计划—规格—执行—报告的版本关系。完整保留
首次 71 ms FAIL、实现修改与第二次 58 ms PASS，以时间语法说明后一次结果不能覆盖前一次；再以
HIL verification 和整车 safety validation 分账收束。

## 从 B-2026-03 到 B-2026-04，哪些结论必须重开？

通过“同源码哈希、不同重建二进制”的失败确定配置项与创建条件边界，再让 `CR-0412` 走完请求、
影响、授权、实施、验证、文档和项目策略下的新基线。变化后的旧证据分为可保留、需复核适用性和需
重新建立三类，不按文件数量或改动行数裁决。

## 同一品牌、同一张旧证书，为什么不能覆盖新的工具用法？

先逐字段比较 U_old 与 U_new，再把 Clause 11 入口判断与进入后的 TD 评价做成成对边界案例；之后
按 TI/TD 查 Table 3，并为本次 TCL3/ASIL D 教学案真正选择一次有理由的方法组合。资格计划、执行
和限定报告仍保持 NotPerformed，不让方法路由冒充完成资格。

## 旧组件用了十年，究竟能走哪条复用通道？

以同一 `FixedPointMath 3.1.2` 字节、两种证据谱系展示 Clause 12 与 SEooC 分叉，再将 Clause 13、
PiU 和工具使用历史按对象与任务分账。PiU 用 specimen 筛选、服务期合计、Table 6/7、一个事件和
特定根因小时重置给出可重算教学账，同时保持 Table 7 的 Note 示例属性。

## 本体化实践：机器能否证明五张材料终于指向同一个快照？

从 Semantica ch10 contract 走到 CQ、scenario、oracle 与 receipt，说明单盏机器绿灯的
证明边界；把已登记工具子域与尚未对象化的 `CR-0412` 最小关系明确分开。结果区分
Satisfied、Violation、Unknown 与 Not modelled，但不把书稿策略冒充 ISO 术语或发布结论。

## 回到放行会，哪些窄结论可以签？

用 Accept evidence、Hold release、Return for action 三种动词逐份裁决五张绿材料，给每个 Return
写具名主语和关闭证据。完成七个单因变式后，以 BMS 更换现成电流传感器和诊断阈值做冷迁移，证明
共同快照方法能够脱离 EPS 表面词汇并把带 Hold 的候选交给第11章组装。
