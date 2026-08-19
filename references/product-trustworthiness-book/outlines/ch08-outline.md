---
contract_version: 1
chapter: ch08
executable_package_id: semantica.chapter_packages.vol2.ch08
executable_authority: semantica_only_no_book_fallback
package_status: partial
release_status: blocked
target_hanzi: 35000
section_budgets:
  - heading: "C(D)+A(D) 已命中，为什么评审仍拒绝签字？"
    hanzi: 3000
  - heading: "两条候选支路会合后，我们究竟在分解什么？"
    hanzi: 4000
  - heading: "两条需求都写了“检测”，为什么还不叫冗余？"
    hanzi: 4000
  - heading: "组合查表合法，括号里的 D 又在提醒什么？"
    hanzi: 4000
  - heading: "开发等级降了，哪些义务没有跟着降？"
    hanzi: 4500
  - heading: "FFI 已列为开放项，为什么还不是独立性证据？"
    hanzi: 4000
  - heading: "一份 DFA 怎样把“独立”变成可反驳结论？"
    hanzi: 4500
  - heading: "安全分析有数值，为什么仍不能替 DFA 签字？"
    hanzi: 3500
  - heading: "本体化实践：机器与评审人最后各能批准什么？"
    hanzi: 3500
consumes_state_ids: [EPS-S06-HW, EPS-S07-SW]
produces_state_ids: [EPS-S08]
first_teaches: [asil-decomposition-and-dfa]
ontology_mapping_shape: exception-encoding
source_anchors:
  - id: "9-5.4.9"
    part: 9
    clause: "9-5.4.9"
    artifact: "structured/mineru/ISO-26262-2018/part-09-asil-and-safety-analyses/native-full/ISO 26262-9-2018/auto/ISO 26262-9-2018_content_list_v2.json"
    pdf_page: 14
    block: 13
    bbox: [55, 707, 884, 753]
  - id: "10-11.1"
    part: 10
    clause: "10-11.1"
    artifact: "structured/mineru/ISO-26262-2018/part-10-guidelines/native-full/ISO 26262-10-2018/auto/ISO 26262-10-2018_content_list_v2.json"
    pdf_page: 67
    block: 4
    bbox: [114, 252, 467, 268]
  - id: "10-12.2.6"
    part: 10
    clause: "10-12.2.6"
    artifact: "structured/mineru/ISO-26262-2018/part-10-guidelines/native-full/ISO 26262-10-2018/auto/ISO 26262-10-2018_content_list_v2.json"
    pdf_page: 78
    block: 0
    bbox: [57, 98, 485, 115]
planned_outputs:
  - references/product-trustworthiness-book/ch08-asil-decomposition-dfa/chapter.md
gate_count_policy: runtime-derived
question_count_policy: learning-objective-driven
figure_policy: engineering-need-driven
---
# 第8章：ASIL 分解与相关失效分析

## 章级理解合同

- 唯一冲突：联合评审把 `EPS-S06-HW` 的三个绿色数字、`EPS-S07-SW` 的两条 SSR/两个软件单元与 §5.4.9 的 `C(D)+A(D)` 基准拼在一起，要求把 `Decomp_DetectAssist` 从 Draft 改成 Approved；评审人指出，组合命中只证明一种等级结构可用，不能证明分解对象正确、两个分支各自符合初始需求、充分独立、义务配置正确或 DFA 已完成。
- 三个等级身份：括号内始终记录安全目标 ASIL；每一次分解都有自己紧邻的“分解前 ASIL”；分解产出按“分解后 ASIL”配置相应开发。三者在首次 D 级分解时可能碰巧同现为 D，二次分解时必须分账。
- 章节动作：从两条带开放项的软硬件候选状态中固定一条初始需求与候选架构，核对冗余语义、组合和标记，建立开发/硬件度量/集成与确认/独立性证据的义务账，区分共存、FFI 与独立性，用同一共享时间基与供电疑点走完安全分析→DFA→措施→验证→复审，最后让机器与人分别签窄结论。
- 章际关系：ch08 不把 ch06/ch07 的候选绿灯改写成已完成事实；输出 `EPS-S08` 仍为带开放项的分解提案。ch09 只消费与制造、安装、服务和现场有关的已识别发起者，不接受“分成两路所以控制项翻倍”的机械推导。

## 九节问题链

| 节 | 消费 | 产出与留问 |
|---|---|---|
| 8.1 C(D)+A(D) 已命中，为什么评审仍拒绝签字？ | 两条候选交接与两块查询绿屏 | 五项 Review Hold；先问分解对象 |
| 8.2 两条候选支路会合后，我们究竟在分解什么？ | `EPS-S06-HW`、`EPS-S07-SW`、初始 FSR 与候选架构 | 分解输入卡和边界；再问两条产出是否真冗余 |
| 8.3 两条需求都写了“检测”，为什么还不叫冗余？ | 初始需求、两条产出与各自执行路径 | 独立符合的功能预言机及看门狗反例；再问等级组合 |
| 8.4 组合查表合法，括号里的 D 又在提醒什么？ | 单次分解对象与两条功能上可行的产出 | 基准/higher 方案、精确标记和三等级身份；再问义务分账 |
| 8.5 开发等级降了，哪些义务没有跟着降？ | 组合、分解层级和当前证据状态 | 开发、硬件度量、集成/后续活动与独立性措施账；再问 FFI/独立性 |
| 8.6 FFI 已列为开放项，为什么还不是独立性证据？ | 共处架构、不同等级与共享资源 | 共存、FFI、技术独立性三者边界；再问 DFA 怎样举证 |
| 8.7 一份 DFA 怎样把“独立”变成可反驳结论？ | 安全分析线索、九主题、耦合因子与模式 | 可信原因、措施、理由、深度、验证和两种合规结论分支；再问信号从哪里来 |
| 8.8 安全分析有数值，为什么仍不能替 DFA 签字？ | ch06 数值候选、FTA/FMEA 线索与同一共因 | 定性/定量、归纳/演绎的任务边界，安全分析→DFA→设计/测试回路；再问机器放行范围 |
| 8.9 本体化实践：机器与评审人最后各能批准什么？ | Semantica ch08 contract/CQ/scenario/oracle 与全章 Hold | `EPS-S08` 候选、单因变式、窄签字与 ch09 交接 |

## 保留与迁移

正文保留 Clause 5 的分解合同、八个基准与 higher-ASIL 规则、括号标记、Clause 6
共存判据、Clause 7 的九主题与 Annex C 七类耦合因子、Clause 8 安全分析边界及
EPS Draft 教学状态。Candidate/Approved 两层合同与正反场景只在 Semantica ch08 package
执行；package 当前为 `partial`、release `blocked`，书稿不保存第二套规则、数据或 runner。

## 练习配置

练习只改一个事实：监控通道失去独立关断能力、二次分解把括号从 D 错写成 C、两路改为共用时间基、低等级软件与高等级软件改为同一内存池、DFA 状态从 Planned 偷改 Completed、一个可信原因缺措施验证、或把系统性共因硬填概率。答题必须指出哪一条功能预言机、等级身份、义务、DFA 结论、机器门禁或下游状态重开，以及哪些既有事实仍可保留。

## C(D)+A(D) 已命中，为什么评审仍拒绝签字？

用“受控基准查询正确 + EPS 结构查询正确”两块绿屏制造冲突。五项 Hold 分别是：精确分解对象与架构、每个分支独立符合初始需求、三类等级及义务配置、充分独立/DFA、执行与确认状态。暂停的是 Approved 和降级信用，不禁止受控探索或补证。

## 两条候选支路会合后，我们究竟在分解什么？

固定 `FSR_DetectUnintendedAssist` 这一条 ASIL D 初始安全需求及其所在层级架构；说明 ch06/ch07 交来的对象只是候选输入，两个软件单元或两组硬件数值都不会自动变成分解产出。每次分解以一条初始需求和一个明确架构层为边界。

## 两条需求都写了“检测”，为什么还不叫冗余？

以“检测并抑制非预期助力”为可观察预言机，逐路遮蔽另一通道，核对感知、判断、反应和执行权；同时解释每条产出功能上独立符合初始需求，与两条较低等级通道组合取得目标风险降低并不矛盾。用简单看门狗反例拆掉“监控存在即冗余”。

## 组合查表合法，括号里的 D 又在提醒什么？

先验证 EPS 的 C(D)+A(D)，再引出八个基准、无序逐分支 higher-ASIL 变体、QM(x) 身份和多次分解。明确括号记录安全目标 ASIL，不充当每个局部分解边界的唯一义务参数。

## 开发等级降了，哪些义务没有跟着降？

按活动对象分账：相应系统/软件/硬件开发最低按分解后 ASIL；硬件架构度量和随机硬件失效目标不因分解改变；发生分解的设计层级上，集成与后续活动按该次分解前 ASIL；独立性证据及控制可信相关原因的措施按相应初始需求配置。软件层分解还须在系统层验证充分独立。

## FFI 已列为开放项，为什么还不是独立性证据？

用同一 ECU/处理器上的低高等级共处与两条分解通道作对照。Clause 6 的 FFI 关注低等级子元素是否级联伤害高等级子元素；Clause 5 的技术独立性同时要求双向级联与共因边界。就高是默认，维持较低等级需要证据；FFI 是独立性的一部分，不是同义词。

## 一份 DFA 怎样把“独立”变成可反驳结论？

从安全分析的割集和重复失效模式取线索，按运行场景/模式及九主题逐项评估，再用 Annex C 七类资料性耦合因子补完整性。以共用源时间戳、供电和刷写路径为疑点，形成“潜在相关失效→可信性/影响理由→预防根因、控制影响或降低耦合→实现/验证→DFA 验证报告”的闭环；当前 EPS 仍停在 Planned/Draft/Undetermined。

## 安全分析有数值，为什么仍不能替 DFA 签字？

FTA/FMEA 给 DFA 找候选发起者，定量分析补充随机硬件失效目标，不能给系统性共因编造发生概率。Clause 8 的结果必须表明需求是否满足；不满足时派生措施并进入产品开发，必要时补测试、更新 HARA，并独立验证分析。三颗绿数没有当前计算工件，也没有承担这些活动。

## 本体化实践：机器与评审人最后各能批准什么？

Semantica ch08 CQ registry 的基准问题只核对八个基准登记，状态问题只返回 EPS 的产出、
标记、分配与占位状态。Candidate/Approved 场景的精确义务只以 package contract/oracle
为准；即使场景通过也不证明理由真实或物理独立。章末维持 Draft，输出带开放项的
`EPS-S08`，且 release 继续 `blocked`。
