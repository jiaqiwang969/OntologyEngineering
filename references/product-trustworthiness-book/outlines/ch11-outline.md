---
contract_version: 1
chapter: ch11
outline_identity: historical_capstone_supplement_not_current_ch11
executable_successor_package_id: semantica.chapter_packages.vol2.ch20
executable_authority: semantica_only_no_book_fallback
package_status: partial
release_status: blocked
target_hanzi: 30000
section_budgets:
  - heading: "11.1 子门禁已经是绿的，为什么报告还是不能签？"
    hanzi: 3000
  - heading: "11.2 这次究竟验的是哪一包字节？"
    hanzi: 4000
  - heading: "11.3 三个案例都进了包，为什么不能写“同样成熟”？"
    hanzi: 3500
  - heading: "11.4 一条追溯链到底闭到了哪里？"
    hanzi: 3500
  - heading: "11.5 七件候选证据为什么仍关不上一个 Claim？"
    hanzi: 3500
  - heading: "11.6 本体化实践：一处单因错误，门禁究竟会不会精确咬住？"
    hanzi: 3000
  - heading: "11.7 怎样让别人复现的是同一个结论，而不只是同一份数据？"
    hanzi: 4000
  - heading: "11.8 收口发现缺口，谁有权修改什么？"
    hanzi: 3000
  - heading: "11.9 回到放行桌，最终能签下哪一句话？"
    hanzi: 2500
consumes_state_ids: [GOV-S03, EPS-S09, SUP-S10]
produces_state_ids: [EPS-S11]
first_teaches: []
ontology_mapping_shape: six-shape-assembly
source_anchors:
  - id: "10-5.3.2"
    part: 10
    clause: "10-5.3.2"
    artifact: "structured/mineru/ISO-26262-2018/part-10-guidelines/native-full/ISO 26262-10-2018/auto/ISO 26262-10-2018_content_list_v2.json"
    pdf_page: 23
    block: 3
    bbox: [114, 325, 467, 342]
  - id: "10-5.3.2-incremental"
    part: 10
    clause: "10-5.3.2"
    artifact: "structured/mineru/ISO-26262-2018/part-10-guidelines/native-full/ISO 26262-10-2018/auto/ISO 26262-10-2018_content_list_v2.json"
    pdf_page: 23
    block: 4
    bbox: [112, 351, 941, 382]
  - id: "10-5.3.2-planning-note"
    part: 10
    clause: "10-5.3.2"
    artifact: "structured/mineru/ISO-26262-2018/part-10-guidelines/native-full/ISO 26262-10-2018/auto/ISO 26262-10-2018_content_list_v2.json"
    pdf_page: 23
    block: 5
    bbox: [112, 391, 941, 420]
  - id: "10-5.3.2-example-milestones"
    part: 10
    clause: "10-5.3.2"
    artifact: "structured/mineru/ISO-26262-2018/part-10-guidelines/native-full/ISO 26262-10-2018/auto/ISO 26262-10-2018_content_list_v2.json"
    pdf_page: 23
    block: 6
    bbox: [112, 429, 942, 504]
  - id: "10-5.3.2-confirmation-review"
    part: 10
    clause: "10-5.3.2"
    artifact: "structured/mineru/ISO-26262-2018/part-10-guidelines/native-full/ISO 26262-10-2018/auto/ISO 26262-10-2018_content_list_v2.json"
    pdf_page: 23
    block: 7
    bbox: [112, 512, 806, 527]
  - id: "10-5.3.2-modification-impact"
    part: 10
    clause: "10-5.3.2"
    artifact: "structured/mineru/ISO-26262-2018/part-10-guidelines/native-full/ISO 26262-10-2018/auto/ISO 26262-10-2018_content_list_v2.json"
    pdf_page: 23
    block: 8
    bbox: [112, 537, 941, 568]
planned_outputs:
  - references/product-trustworthiness-book/ch11-capstone-three-items/chapter.md
gate_count_policy: runtime-derived
question_count_policy: learning-objective-driven
figure_policy: engineering-need-driven
abox_policy: assemble-frozen-upstream-only
---
# 第11章 综合收口：一份绿了一半的报告，最终能签哪句话

本提纲对应已归档的旧“第 11 章”总装案例，不是现行可信主张本体章。历史整书报告、
旧 bundle manifest 与漂移快照继续作为书面案例；其执行后继只在
`semantica.chapter_packages.vol2.ch20` 的 `legacy-capstone-*` 资产中。九节按下列问题链连续推进：

```text
子门禁绿了，为什么总报告仍不能签？
  → 这次究竟验的是哪一包字节？
  → 三案例同载，为什么不等于同样成熟？
  → 一条追溯链到底闭到哪里？
  → 七件候选证据为什么仍关不上 Claim？
  → 单因错误能否被窄门禁精确命中？
  → 怎样复现同一个结论而非同一份数据？
  → 收口发现缺口，谁有权修改？
  → 历史、当前与未来候选分别能签哪句话？
```

全章目标约 30,000 汉字。`GOV-S03 + EPS-S09 + SUP-S10 → EPS-S11` 只是书稿接口记号，不是产品状态。旧 manifest 是历史冻结和漂移比较基线，当前不得为取得绿色演示而重冻。Clause 8 安全确认对象由 ch05 对象责任边界维护，其进入 Claim/Evidence 的角色和状态受 ch03 CAE 治理；ch11 只读查询，不创建、批准或升级这些上游事实。

## 11.1 子门禁已经是绿的，为什么报告还是不能签？

从历史报告的 `capstone=0`、`knowledge-model=1`、`overall=failed` 开场，让“一个子门禁绿色即可签章”的自然判断完整失败；再把历史运行、重写期诊断快照、capstone 子合同和常规整书/出版 release 合同排成坐标格。产物是一句可被反驳的待签主张：对象、检查器、运行结果和未决项必须同时绑定。

预算：约 3,000 汉字。主要素材为历史 JSON、当前失败关闭结果和 §11.1 四格对照。

## 11.2 这次究竟验的是哪一包字节？

解释旧 manifest 的 41 个输入、逐文件 SHA-256、bundle/manifest 哈希分工，以及十处
字节漂移为何只说明“不是旧包”。旧 capstone 的首处不匹配失败关闭作为历史事实保留；
现行复核必须通过 Semantica ch20，不得重建旧脚本或“自动重冻→继续验收”。

预算：约 4,000 汉字。产物是本次被检查对象的精确身份与失败关闭状态。

## 11.3 三个案例都进了包，为什么不能写“同样成熟”？

区分文件被选入、RDF 可解析、查询可达、窄门禁通过与工程证据充分。用 `CQ-CH11-01` 精确统计三案例已分配的 FSR/TSR，用 `CQ-CH11-02` 找到经危害事件归属到 Item 的 ASIL D 安全目标；当前 oracle 分别锁定 AEB/BMS/EPS=2/2/6，以及 AEB/EPS 两条 ASIL D 归属。BMS/AEB 主目标已进入 `GATE-CH11-01`，但链只到 TSR；次要目标未标记，不在该窄门禁范围。旧 manifest 已漂移，因此这些开发态合同不能被写成当前冻结包绿色结果。

预算：约 3,500 汉字。产物是带范围和空缺的三案例成熟度对照。

## 11.4 一条追溯链到底闭到了哪里？

沿 EPS 的 HE→SG→FSR→TSR→SSR→SWU 主链走查，并把分解、HSI、硬件度量和生产定义作为异构支路分账。`GATE-CH11-01` 只检查已进入系统层的非 QM 目标至少有一个经 `derivedFrom+` 可达、已 `allocatedTo` 且属于安全需求子类闭包的后代；它不要求每一条路径闭合。删除全部相关分配边的单因变式负责量出这条 GATE 的宽度。图 11-2 放在 BMS/AEB 第四跳无 SSR 的归因之后，明确“结构较浅”与“窄 GATE 通过”可以同时成立。

预算：约 3,500 汉字。产物是可指出终点、支路、缺口和机器射程的追溯判断。

## 11.5 七件候选证据为什么仍关不上一个 Claim？

把 HARA、FSR、TSR、硬件度量、分解候选、Clause 8 安全确认规范和报告模板挂到开放 Claim 下，逐件区分“可作为候选输入”与“已被接受”。`CQ-CH11-03` 精确绑定规范、活动、结果、评价、报告和 Claim 的状态：Draft/EvidenceCandidate、Planned、NotRun、NotPerformed 与 ClaimOpen 不能互相升级。Clause 8 对象由 ch05 维护，CAE 候选角色与状态治理回 ch03；ch11 只能只读消费和报告。

预算：约 3,500 汉字。产物是七件材料各自能支持多宽，以及 Claim 仍开放的具体原因。

## 11.6 本体化实践：一处单因错误，门禁究竟会不会精确咬住？

对比直接 FSR 存在性 Shape 与 `GATE-CH11-01` 的不同射程，用完整临时副本执行“基线→只删分配边→恢复”三步，不触碰共享正本和旧 manifest。契约测试可以证明哈希、路径、快照、查询一致性、Clause 8 状态与受控 fixture 的局部行为；重写期快照即使这些测试绿色，capstone 仍因旧 manifest 漂移而在装载前失败。

预算：约 3,000 汉字。产物是红灯精确命中的事实，以及它不能证明的工程结论。

## 11.7 怎样让别人复现的是同一个结论，而不只是同一份数据？

把数据包、验证器、运行环境和报告四层身份同时写进结论；旧 runner 名称只解释历史
报告的分层边界。现行执行由 Semantica ch20 的单一 runner 完成，并把 scenario oracle
与 release verdict 分开。最后以“只换验证器、数据不变”的变式说明旧绿必须重议。

预算：约 4,000 汉字。产物是含输入身份、检查器身份、当次结果、未决项和边界的最小可复核报告。

## 11.8 收口发现缺口，谁有权修改什么？

收口人的默认动作只有只读定位、登记缺口、退回责任章。事实责任人、规则责任人、集成人、冻结者和收口评审人可以由同一人兼任，但动作与接受因果不能合并。当前漂移保持为 Hold；Clause 8/CAE 状态问题退回 ch05/ch03 上游边界，不能由 ch11 为关 Claim 临场改写。只有上游接纳、规则影响复核和受控集成完成后，冻结者才可显式形成下一版 manifest。

预算：约 3,000 汉字。产物是一张带对象、合同、责任边界、关闭证据和禁止补丁的缺口单。

## 11.9 回到放行桌，最终能签下哪一句话？

分开裁决历史报告、重写期诊断快照和未来新候选：历史可以保留“子门禁 0、knowledge-model 1、总状态 failed、release 未运行”的事实；快照只能签旧 manifest 漂移导致失败关闭及检查器契约测试结果；未来候选必须重新绑定对象、检查器、结果、未决项和接受决定。练习要求读者在不重冻的前提下按快照身份写收口报告，并用 Clause 8 状态变式、分配边变式和验证器变式说明哪些结论重开。

预算：约 2,500 汉字。结尾回到全书人机边界：机器签已编码合同的当次结果，工程责任人签对象、证据和接受决定。
