---
contract_version: 1
supplements:
  - supplement: appA-semiconductor
    target_hanzi: 25000
    section_budgets:
      - heading: "导读：芯片不是一只黑盒"
        hanzi: 2000
      - heading: "半导体部件划分、故障模型与 IP 边界"
        hanzi: 3500
      - heading: "基础失效率的来源、假设与计算边界"
        hanzi: 4500
      - heading: "半导体 DFA：DFI 分类与工作流"
        hanzi: 4000
      - heading: "故障注入与验证证据"
        hanzi: 2500
      - heading: "生产、分布式开发与确认接口"
        hanzi: 2000
      - heading: "数字、模拟、PLD、多核与传感器案例"
        hanzi: 4500
      - heading: "本体化实践：数值诚实与依赖链"
        hanzi: 2000
    ontology_mapping_shape: numerical-honesty-plus-dfa
    source_boundary: informative-guidance
    source_anchors:
      - id: "11-4"
        part: 11
        clause: "11-4"
        artifact: "structured/mineru/ISO-26262-2018/part-11-semiconductor-guidelines/native-full/ISO 26262-11-2018/auto/ISO 26262-11-2018_content_list_v2.json"
        pdf_page: 10
        block: 3
        bbox: [55, 187, 584, 205]
      - id: "11-4.5"
        part: 11
        clause: "11-4.5"
        artifact: "structured/mineru/ISO-26262-2018/part-11-semiconductor-guidelines/native-full/ISO 26262-11-2018/auto/ISO 26262-11-2018_content_list_v2.json"
        pdf_page: 14
        block: 0
        bbox: [57, 98, 344, 116]
      - id: "11-4.6"
        part: 11
        clause: "11-4.6"
        artifact: "structured/mineru/ISO-26262-2018/part-11-semiconductor-guidelines/native-full/ISO 26262-11-2018/auto/ISO 26262-11-2018_content_list_v2.json"
        pdf_page: 23
        block: 11
        bbox: [112, 668, 505, 684]
      - id: "11-4.7.5.1"
        part: 11
        clause: "11-4.7.5.1"
        artifact: "structured/mineru/ISO-26262-2018/part-11-semiconductor-guidelines/native-full/ISO 26262-11-2018/auto/ISO 26262-11-2018_content_list_v2.json"
        pdf_page: 53
        block: 10
        bbox: [114, 739, 771, 756]
      - id: "11-4.7.6"
        part: 11
        clause: "11-4.7.6"
        artifact: "structured/mineru/ISO-26262-2018/part-11-semiconductor-guidelines/native-full/ISO 26262-11-2018/auto/ISO 26262-11-2018_content_list_v2.json"
        pdf_page: 59
        block: 2
        bbox: [114, 196, 304, 211]
      - id: "11-4.8"
        part: 11
        clause: "11-4.8"
        artifact: "structured/mineru/ISO-26262-2018/part-11-semiconductor-guidelines/native-full/ISO 26262-11-2018/auto/ISO 26262-11-2018_content_list_v2.json"
        pdf_page: 63
        block: 4
        bbox: [114, 325, 302, 342]
      - id: "11-4.9"
        part: 11
        clause: "11-4.9"
        artifact: "structured/mineru/ISO-26262-2018/part-11-semiconductor-guidelines/native-full/ISO 26262-11-2018/auto/ISO 26262-11-2018_content_list_v2.json"
        pdf_page: 65
        block: 14
        bbox: [114, 633, 410, 650]
      - id: "11-5.1"
        part: 11
        clause: "11-5.1"
        artifact: "structured/mineru/ISO-26262-2018/part-11-semiconductor-guidelines/native-full/ISO 26262-11-2018/auto/ISO 26262-11-2018_content_list_v2.json"
        pdf_page: 68
        block: 6
        bbox: [57, 573, 428, 590]
      - id: "11-5.2"
        part: 11
        clause: "11-5.2"
        artifact: "structured/mineru/ISO-26262-2018/part-11-semiconductor-guidelines/native-full/ISO 26262-11-2018/auto/ISO 26262-11-2018_content_list_v2.json"
        pdf_page: 88
        block: 8
        bbox: [57, 517, 443, 535]
      - id: "11-5.3"
        part: 11
        clause: "11-5.3"
        artifact: "structured/mineru/ISO-26262-2018/part-11-semiconductor-guidelines/native-full/ISO 26262-11-2018/auto/ISO 26262-11-2018_content_list_v2.json"
        pdf_page: 109
        block: 11
        bbox: [114, 567, 428, 585]
      - id: "11-5.4"
        part: 11
        clause: "11-5.4"
        artifact: "structured/mineru/ISO-26262-2018/part-11-semiconductor-guidelines/native-full/ISO 26262-11-2018/auto/ISO 26262-11-2018_content_list_v2.json"
        pdf_page: 124
        block: 13
        bbox: [57, 838, 326, 853]
      - id: "11-5.5"
        part: 11
        clause: "11-5.5"
        artifact: "structured/mineru/ISO-26262-2018/part-11-semiconductor-guidelines/native-full/ISO 26262-11-2018/auto/ISO 26262-11-2018_content_list_v2.json"
        pdf_page: 127
        block: 10
        bbox: [114, 527, 396, 544]
    planned_outputs:
      - functional-safety-book/appA-semiconductor/README.md
      - functional-safety-book/appA-semiconductor/chapter.md
      - ontology/source-anchors-part11.ttl
    gate_count_policy: runtime-derived
    question_count_policy: learning-objective-driven
    figure_policy: engineering-need-driven

  - supplement: appB-motorcycle-truck
    target_hanzi: 10000
    section_budgets:
      - heading: "导读：同一套标准为何需要车型适配"
        hanzi: 1000
      - heading: "适配总则与适用边界"
        hanzi: 1200
      - heading: "安全文化与确认措施的摩托车适配"
        hanzi: 1500
      - heading: "摩托车 HARA 与 MSIL 判定"
        hanzi: 2500
      - heading: "整车集成、测试与安全确认"
        hanzi: 1500
      - heading: "附录判例：严重度、暴露度与可控性"
        hanzi: 1300
      - heading: "本体化实践：变体分级与 T&B 边界索引"
        hanzi: 1000
    ontology_mapping_shape: domain-adaptation-delta
    source_boundary: motorcycle-primary-tb-index-only
    source_anchors:
      - id: "12-4.5"
        part: 12
        clause: "12-4.5"
        artifact: "structured/mineru/ISO-26262-2018/part-12-motorcycle-adaptation/native-full/ISO 26262-12-2018/auto/ISO 26262-12-2018_content_list_v2.json"
        pdf_page: 12
        block: 0
        bbox: [57, 98, 364, 115]
      - id: "12-4.6"
        part: 12
        clause: "12-4.6"
        artifact: "structured/mineru/ISO-26262-2018/part-12-motorcycle-adaptation/native-full/ISO 26262-12-2018/auto/ISO 26262-12-2018_content_list_v2.json"
        pdf_page: 12
        block: 2
        bbox: [57, 172, 615, 190]
      - id: "12-6"
        part: 12
        clause: "12-6"
        artifact: "structured/mineru/ISO-26262-2018/part-12-motorcycle-adaptation/native-full/ISO 26262-12-2018/auto/ISO 26262-12-2018_content_list_v2.json"
        pdf_page: 13
        block: 1
        bbox: [114, 648, 292, 665]
      - id: "12-7"
        part: 12
        clause: "12-7"
        artifact: "structured/mineru/ISO-26262-2018/part-12-motorcycle-adaptation/native-full/ISO 26262-12-2018/auto/ISO 26262-12-2018_content_list_v2.json"
        pdf_page: 14
        block: 11
        bbox: [57, 573, 332, 589]
      - id: "12-8"
        part: 12
        clause: "12-8"
        artifact: "structured/mineru/ISO-26262-2018/part-12-motorcycle-adaptation/native-full/ISO 26262-12-2018/auto/ISO 26262-12-2018_content_list_v2.json"
        pdf_page: 19
        block: 3
        bbox: [114, 763, 521, 782]
      - id: "12-9"
        part: 12
        clause: "12-9"
        artifact: "structured/mineru/ISO-26262-2018/part-12-motorcycle-adaptation/native-full/ISO 26262-12-2018/auto/ISO 26262-12-2018_content_list_v2.json"
        pdf_page: 26
        block: 3
        bbox: [57, 218, 403, 237]
      - id: "12-10"
        part: 12
        clause: "12-10"
        artifact: "structured/mineru/ISO-26262-2018/part-12-motorcycle-adaptation/native-full/ISO 26262-12-2018/auto/ISO 26262-12-2018_content_list_v2.json"
        pdf_page: 28
        block: 3
        bbox: [58, 702, 265, 720]
      - id: "12-Annex-A"
        part: 12
        clause: "12-Annex A"
        artifact: "structured/mineru/ISO-26262-2018/part-12-motorcycle-adaptation/native-full/ISO 26262-12-2018/auto/ISO 26262-12-2018_content_list_v2.json"
        pdf_page: 32
        block: 1
        bbox: [60, 160, 882, 204]
      - id: "12-Annex-B"
        part: 12
        clause: "12-Annex B"
        artifact: "structured/mineru/ISO-26262-2018/part-12-motorcycle-adaptation/native-full/ISO 26262-12-2018/auto/ISO 26262-12-2018_content_list_v2.json"
        pdf_page: 38
        block: 1
        bbox: [142, 160, 798, 184]
      - id: "12-Annex-C"
        part: 12
        clause: "12-Annex C"
        artifact: "structured/mineru/ISO-26262-2018/part-12-motorcycle-adaptation/native-full/ISO 26262-12-2018/auto/ISO 26262-12-2018_content_list_v2.json"
        pdf_page: 46
        block: 1
        bbox: [152, 160, 788, 184]
    planned_outputs:
      - functional-safety-book/appendices/appendix-b-motorcycle-truck.md
      - ontology/source-anchors-part12.ttl
      - ontology/msil-tables.ttl
    gate_count_policy: runtime-derived
    question_count_policy: learning-objective-driven
    figure_policy: engineering-need-driven

  - supplement: appC-glossary
    target_hanzi: 15000
    section_budgets:
      - heading: "术语条目模板与受控来源"
        hanzi: 1200
      - heading: "结构群：从相关项到软件单元"
        hanzi: 2200
      - heading: "因果群：故障、错误、失效与依赖失效"
        hanzi: 2200
      - heading: "风险群：危害事件、S/E/C 与完整性等级"
        hanzi: 2200
      - heading: "需求群：安全目标、需求与工作产物"
        hanzi: 2200
      - heading: "时间群：容忍、检测、处理与运行时间"
        hanzi: 1800
      - heading: "本体化实践：一份词表、多条概念路径"
        hanzi: 1600
      - heading: "字母索引、首讲章与交叉引用"
        hanzi: 1600
    ontology_mapping_shape: controlled-glossary-concept-index
    source_boundary: part1-controlled-terms-no-second-tbox
    inventory_count: 185
    organization_policy: five-concept-groups-plus-alphabetic-index
    source_anchors:
      - id: "1-3"
        part: 1
        clause: "1-3"
        artifact: "structured/mineru/ISO-26262-2018/part-01-vocabulary/native-full/ISO 26262-1-2018/auto/ISO 26262-1-2018_content_list_v2.json"
        pdf_page: 9
        block: 12
        bbox: [114, 804, 374, 822]
      - id: "1-3.185"
        part: 1
        clause: "1-3.185"
        artifact: "structured/mineru/ISO-26262-2018/part-01-vocabulary/native-full/ISO 26262-1-2018/auto/ISO 26262-1-2018_content_list_v2.json"
        pdf_page: 36
        block: 15
        bbox: [57, 508, 110, 523]
    planned_outputs:
      - functional-safety-book/appC-glossary/README.md
      - functional-safety-book/appC-glossary/glossary.md
    gate_count_policy: runtime-derived
    question_count_policy: learning-objective-driven
    figure_policy: engineering-need-driven

  - supplement: appD-method-quick-reference
    target_hanzi: 10000
    section_budgets:
      - heading: "速查卡读法与推荐等级边界"
        hanzi: 1000
      - heading: "Part 4：系统与整车集成的 14 张卡"
        hanzi: 2500
      - heading: "Part 6：软件生命周期的 12 张卡"
        hanzi: 2500
      - heading: "Part 8：TCL3 与 TCL2 的 2 张卡"
        hanzi: 1000
      - heading: "按场景跨表导航"
        hanzi: 1200
      - heading: "人工注释：组合理由与偏离边界"
        hanzi: 1000
      - heading: "本体化实践：从 RDF 导出到版本化卡片"
        hanzi: 800
    ontology_mapping_shape: generated-method-cards
    source_boundary: ontology-derived-human-reviewed-not-compliance
    inventory_count: 28
    inventory_by_part:
      part4: 14
      part6: 12
      part8: 2
    generation_policy: ontology-export-plus-human-annotation
    source_anchors:
      - id: "4-Table-3"
        part: 4
        clause: "4-Table 3"
        artifact: "structured/mineru/ISO-26262-2018/part-04-system-level-product-development/native-full/ISO 26262-4-2018/auto/ISO 26262-4-2018_content_list_v2.json"
        pdf_page: 25
        block: 0
        bbox: [115, 153, 937, 379]
      - id: "4-Table-16"
        part: 4
        clause: "4-Table 16"
        artifact: "structured/mineru/ISO-26262-2018/part-04-system-level-product-development/native-full/ISO 26262-4-2018/auto/ISO 26262-4-2018_content_list_v2.json"
        pdf_page: 32
        block: 1
        bbox: [58, 167, 880, 445]
      - id: "6-Table-2"
        part: 6
        clause: "6-Table 2"
        artifact: "structured/mineru/ISO-26262-2018/part-06-software-level-development/native-full/ISO 26262-6-2018/auto/ISO 26262-6-2018_content_list_v2.json"
        pdf_page: 19
        block: 14
        bbox: [115, 554, 937, 755]
      - id: "6-Table-15"
        part: 6
        clause: "6-Table 15"
        artifact: "structured/mineru/ISO-26262-2018/part-06-software-level-development/native-full/ISO 26262-6-2018/auto/ISO 26262-6-2018_content_list_v2.json"
        pdf_page: 37
        block: 7
        bbox: [115, 683, 937, 894]
      - id: "8-Table-4"
        part: 8
        clause: "8-Table 4"
        artifact: "structured/mineru/ISO-26262-2018/part-08-supporting-processes/native-full/ISO 26262-8-2018/auto/ISO 26262-8-2018_content_list_v2.json"
        pdf_page: 40
        block: 2
        bbox: [60, 215, 880, 372]
      - id: "8-Table-5"
        part: 8
        clause: "8-Table 5"
        artifact: "structured/mineru/ISO-26262-2018/part-08-supporting-processes/native-full/ISO 26262-8-2018/auto/ISO 26262-8-2018_content_list_v2.json"
        pdf_page: 40
        block: 3
        bbox: [58, 410, 880, 568]
    planned_outputs:
      - functional-safety-book/appD-method-quick-reference/README.md
      - functional-safety-book/appD-method-quick-reference/method-cards.md
    gate_count_policy: runtime-derived
    question_count_policy: learning-objective-driven
    figure_policy: engineering-need-driven
---
# 四个附录的写作施工图

这四个附录不是正文遗留内容的堆放区。它们分别承担专题深入、域适配、受控词汇入口和方法选择入口，并继续使用全书同一套来源、本体和门禁契约。

## 附录 A：半导体应用指南

定位：把 Part 11 的资料性半导体指南改造成一条“对象划分→失效率→DFA→故障注入→具体技术案例”的证据链，而不把指南写成新的规范性要求。

| 节 | 字数 | 供字素材 | 形态 |
|---|---:|---|---|
| 导读：芯片不是一只黑盒 | 2000 | Part 11 p10–22；划分、SEooC、IP 与黑盒 IP | 失败故事+边界 |
| 半导体部件划分、故障模型与 IP 边界 | 3500 | §4–§4.5，p10–22 | 概念图+责任表 |
| 基础失效率的来源、假设与计算边界 | 4500 | §4.6，p23–49 | 数值走查 |
| 半导体 DFA：DFI 分类与工作流 | 4000 | §4.7，p49–63；§4.7.5.1 分类、§4.7.6 流程 | 分类表+工作流 |
| 故障注入与验证证据 | 2500 | §4.8，p63–65 | 实验设计 |
| 生产、分布式开发与确认接口 | 2000 | §4.9–§4.12，p65–67 | 接口清单 |
| 数字、模拟、PLD、多核与传感器案例 | 4500 | §5.1–§5.5，p68–140；Annex A–E 作补充 | 技术对照 |
| 本体化实践：数值诚实与依赖链 | 2000 | ch06 数值模式+ch08 DFA 模式+Part 11 锚点 | 可执行知识 |

### 导读：芯片不是一只黑盒

以“整车团队拿到一个只有功能数据手册的黑盒 IP，却需要完成定量分析”开篇，引出划分粒度、使用假设、证据可见性和 DIA 责任。

### 半导体部件划分、故障模型与 IP 边界

从 component/part/subpart/elementary subpart 层次走到 IP 类别、生命周期、工作产物和黑盒集成；术语定义回指 ch02/ch10，不抢首讲权。

### 基础失效率的来源、假设与计算边界

按技术来源、mission profile、永久/瞬态、封装/芯片和非恒定失效率组织。任何密集表格、公式和数值例均需回原 PDF 与 `pdftotext -layout` 校订，并保留 `ocrCorrected`。

### 半导体 DFA：DFI 分类与工作流

先分清级联失效与共因失效，再按 §4.7.5.1 组织 DFI 类别和缓解措施，最后走完 §4.7.6 的 B1–B12 工作流。分析输出保留 `Candidate/Pending`，不因流程模型完整就声称 DFA 已完成。

### 故障注入与验证证据

把目标、故障模型、注入位置、时间、观测量、判定准则和结果连成一个实验对象，防止“执行过故障注入”取代可复现证据。

### 生产、分布式开发与确认接口

对齐 ch03/ch09/ch10 的责任、生产、维修、DIA 和确认措施；附录只写半导体特有接口差异，不复制主章通用过程。

### 数字、模拟、PLD、多核与传感器案例

每类技术使用同一问题框：故障模型是什么、安全机制改变了什么、定量分析需要哪些假设、还有哪些系统性故障需要人类审查。

### 本体化实践：数值诚实与依赖链

复用 ch06 的结构化数值+单位+范围+校订留痕和 ch08 的 DFI→故障场景→缓解措施→验证状态链。附录 A 新增半导体 ABox 时仍复用共享 TBox，不自造平行概念。

**素材账与边界**：Part 11 p10–140 可支撑主体，Annex A–E 只在有明确教学任务时选用。Part 11 是 informative guidance；本附录不得用其取代 Parts 2–9 的适用要求。

## 附录 B：摩托车适配与 T&B 边界索引

定位：Part 12 的主体是摩托车适配。卡车、客车、挂车和半挂车（T&B）在本附录中只建跨 Part 条款索引和边界说明，不伪造一部与摩托车等量的 Part 12 指南。

| 节 | 字数 | 供字素材 | 形态 |
|---|---:|---|---|
| 导读：同一套标准为何需要车型适配 | 1000 | Part 12 p9–12 | 差异故事 |
| 适配总则与适用边界 | 1200 | §4.5/§4.6+§5，p12 | 适用性差分 |
| 安全文化与确认措施的摩托车适配 | 1500 | §6–§7，p13–19 | 变体要求对照 |
| 摩托车 HARA 与 MSIL 判定 | 2500 | §8，p19–26 | 判定表 |
| 整车集成、测试与安全确认 | 1500 | §9–§10，p26–31 | 流程对照 |
| 附录判例：严重度、暴露度与可控性 | 1300 | Annex B/C，p38–50 | 判例导读 |
| 本体化实践：变体分级与 T&B 边界索引 | 1000 | 共享 TBox+Part 12 delta+T&B 条款索引 | 变体模型 |

### 导读：同一套标准为何需要车型适配

用“把乘用车的可控性判断直接套到两轮车”的失败场景说明车身动力学、骑手行为和专家骑手评估为何需要 delta，而基础生命周期仍来自 Parts 2–9。

### 适配总则与适用边界

说清 §4.5 的替代关系、§5 的适配范围及完整基线。§4.6 只说明 T&B 特有内容会被显式标记；它不是一套完整的卡客车适配流程。

### 安全文化与确认措施的摩托车适配

只写 Part 12 相对 ch03 的差分：MSIL 引起的表格解读、确认独立性和适配后的工作产物。

### 摩托车 HARA 与 MSIL 判定

复用 ch04 的 HazardousEvent、Severity、Exposure、Controllability 和 SafetyGoal 类，仅将分级体系、判定表和可控性评估建模为摩托车变体。不把 ASIL 与 MSIL 建成简单同义词。

### 整车集成、测试与安全确认

把摩托车层的集成目标、测试方法、确认环境、执行与评价串成可追溯链，并回指 ch05 的通用系统集成模式。

### 附录判例：严重度、暴露度与可控性

用 Annex B/C 训练“证据如何支撑分类”，不把示例直接复制为项目结论。

### 本体化实践：变体分级与 T&B 边界索引

建立 `base provision -> adaptation delta -> vehicle-domain applicability` 链，使同一条查询能返回基线要求、摩托车替代条款和不适用理由。T&B 仅生成跨 Parts 1–9 的特有条款索引与待补证据清单。

**素材账与边界**：Part 12 p12–31 是规范性摩托车适配主体，p32–50 是资料性附录。仅依 Part 12 不足以写成完整 T&B 指南；后续扩展必须先建 Parts 1–9 的 T&B 条款覆盖账本。

## 附录 C：185 词术语表与概念网索引

定位：以 Part 1 §3.1–§3.185 为唯一受控词条集，正文按 ch02 的五条概念路径组织，字母序只做入口索引。附录不复制第二套 TBox。

| 节 | 字数 | 供字素材 | 形态 |
|---|---:|---|---|
| 术语条目模板与受控来源 | 1200 | Part 1 §3，p9–36 | 使用说明 |
| 结构群：从相关项到软件单元 | 2200 | ch02 结构群+Part 1 词条 | 概念路径 |
| 因果群：故障、错误、失效与依赖失效 | 2200 | ch02 因果群+Part 1 词条 | 概念路径 |
| 风险群：危害事件、S/E/C 与完整性等级 | 2200 | ch02/ch04+Part 1 词条 | 概念路径 |
| 需求群：安全目标、需求与工作产物 | 2200 | ch02/ch05+Part 1 词条 | 概念路径 |
| 时间群：容忍、检测、处理与运行时间 | 1800 | ch02/ch05+Part 1 词条 | 时间线 |
| 本体化实践：一份词表、多条概念路径 | 1600 | `fsafety-tbox.ttl`+术语来源锚点 | 查询生成 |
| 字母索引、首讲章与交叉引用 | 1600 | 全书首讲表+章节键 | 出版索引 |

### 术语条目模板与受控来源

每个条目固定包含编号、英文标题、中文重述、稳定概念 ID、所属概念路径、相关词、首讲章、来源坐标和校订状态。中文重述不是未授权的逐字翻译。

### 结构群：从相关项到软件单元

用部分-整体、实现和分配关系把 item/system/element/component/hardware part/software unit 等词连成可导航结构。

### 因果群：故障、错误、失效与依赖失效

以 fault→error→failure 为主链，旁接随机/系统性、永久/瞬态、单点/潜伏/多点及级联/共因关系。

### 风险群：危害事件、S/E/C 与完整性等级

从 harm/hazard/operational situation/hazardous event 走向 risk、ASIL/MSIL 和 unreasonable risk，把同名但适用域不同的分级对象分开。

### 需求群：安全目标、需求与工作产物

把 safety goal、functional/technical/hardware/software safety requirement、work product、verification 和 confirmation measure 按层级与活动关系导航，不把词典顺序冒充派生顺序。

### 时间群：容忍、检测、处理与运行时间

将 FTTI、FDTI、FHTI、FRTI、EOTI/EOTTI 等术语挂到统一事件时间线，并标出不能简单等同的边界。

### 本体化实践：一份词表、多条概念路径

受控条目仅保留一份；五群是可多值标注的语义导航路径，字母表是排序视图。用 SPARQL 生成分群表和章节回指，避免手工维护三份会漂移的清单。

### 字母索引、首讲章与交叉引用

附录末保留字母序全量入口，每个词条反向指向首讲章和操作章，使读者既能查词，也能回到完整工程语境。

**素材账与边界**：Part 1 §3 的 185 个条目为完整性分母；当前 TBox 只锚定了其中一部分，不得以 TBox 当前实例数替代 185 词的出版覆盖账。

## 附录 D：28 张方法表速查

定位：从已校订的方法表本体生成 28 张基础卡，再由人工补写适用场景、组合理由和偏离边界。卡片是选法入口，不是“选了 `++` 就合规”的打勾清单。

| 节 | 字数 | 供字素材 | 形态 |
|---|---:|---|---|
| 速查卡读法与推荐等级边界 | 1000 | §4.3 读表规则+主章选法边界 | 使用说明 |
| Part 4：系统与整车集成的 14 张卡 | 2500 | `system-integration-method-tables.ttl` | 卡片组 |
| Part 6：软件生命周期的 12 张卡 | 2500 | `sw-method-tables.ttl` | 卡片组 |
| Part 8：TCL3 与 TCL2 的 2 张卡 | 1000 | `tool-qualification-tables.ttl` | 卡片组 |
| 按场景跨表导航 | 1200 | 主章工程问题+方法类型 | 选择树 |
| 人工注释：组合理由与偏离边界 | 1000 | ch05/ch07/ch10 现有边界声明 | 决策记录 |
| 本体化实践：从 RDF 导出到版本化卡片 | 800 | 三个 MethodTableSet+来源锚点+子矩阵 | 生成流水线 |

### 速查卡读法与推荐等级边界

先解释表的活动目标、行方法、ASIL 列、`++/+/o` 和连续/备选关系，再说明“推荐度”、“项目选择”与“验证完成证据”是三层对象。

### Part 4：系统与整车集成的 14 张卡

从 Table 3 的测试用例推导走到硬软件、系统和整车三层的需求实现、性能/时序、接口与稳健性验证。

### Part 6：软件生命周期的 12 张卡

覆盖架构表示、架构验证、单元设计、单元验证、软件集成和嵌入式软件测试；卡片回指 ch07 的完整教学语境。

### Part 8：TCL3 与 TCL2 的 2 张卡

保留两张资格认定方法表的差异，并强制先回指 TI/TD→TCL 判定，不允许跳过使用场景直接选资格认定方法。

### 按场景跨表导航

以“我要证明什么”而非“我在第几张表”导航：需求正确实现、时序/性能、接口、稳健性、结构覆盖、故障注入和工具置信度分别返回候选卡。

### 人工注释：组合理由与偏离边界

自动导出只能证明表格转录与来源一致。项目为什么选这组方法、替换高推荐度方法的理由是什么、证据是否充分，仍由负责人记录与审查。

### 本体化实践：从 RDF 导出到版本化卡片

查询 `MethodTableSet -> MethodTable -> VerificationMethod -> MethodRecommendation` 与来源锚点，生成带输入哈希的基础卡。人工注释与自动字段分层存储，重新导出时不覆盖审查结论。

**生成流程**：加载三个受控方法表模块→运行来源转录门禁→校验 Part 4/6/8 的 `14/12/2` 库存→导出基础卡→合并人工注释→生成差异报告。任何表、行、ASIL 单元或来源坐标变化都使出版快照失效，必须重新人工复核。
