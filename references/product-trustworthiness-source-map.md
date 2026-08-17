# 《产品可信工程》来源地图（本 skill 第二卷 · 实战卷）

本卷是《工程本体论》的实战续篇：前十章按 ISO 26262 生命周期讲透 AI 之前的
传统功能安全最佳实践，后十章用工程本体论把同一套工程逐章本体化（AI 之后的世界）。
贯穿案例：合成教学案例 EPS-RC17（电动助力转向候选，配置 H3.2/SW1.8.3/C41/D7/V12）
与跨域投影件 ENV-01（环境监测单元）。全部人物、事故、数字均为合成教学材料。

工程正本（本体/SHACL 门禁/来源锚点/问题合同）仍在 `/Users/jqwang/143-工程规范`；
本卷是冻结的可读知识快照。精确 ISO 条款坐标查该仓库的 SOURCE-AUDIT 与 ontology/。

## 章节地图（问答镜像：ch N ↔ ch N+10）

| 章 | 文件 | 主题与可检索要点 |
|---|---|---|
| 前言 | front-matter/preface.md | 雨刷企业缘起；"模型一更新就被吃掉"生存判断；三问（谁指挥/谁记得/谁有权改写） |
| ch01 | ch01-introduction/chapter.md | 六绿僵局；主张七部件（主语/配置/关注/情境/时间/假设/决定范围）；证据四关系；四层绿色；单因变式 |
| ch02 | ch02-concepts-terminology/chapter.md | 术语体系=停战协议；相关项/系统/元素；故障→错误→失效链；四个一百毫秒；ASIL 是风险的语言 |
| ch03 | ch03-safety-management/chapter.md | 六职责（做/产证/挑战/评审/担险/授权）；独立性=利害距离；确认评审/审核/评估三件套；安全文化 |
| ch04 | ch04-concept-hara/chapter.md | HARA 纵向样板；情境拆分；危害事件=危害×运行情形；S/E/C 各绑证据；查表纪律 |
| ch05 | ch05-system-development/chapter.md | 目标派生五要素；接缝合同（HSI）；时间预算账本；逐层集成收账 |
| ch06 | ch06-hardware-development/chapter.md | 数字要能作证；400 FIT 底账；SPFM/LFM 除法与分母；PMHF 出处纪律 |
| ch07 | ch07-software-development/chapter.md | 软件无失效率靠过程；V 模型两侧对质；覆盖率边界；差异分析=标注保质期 |
| ch08 | ch08-asil-decomposition-dfa/chapter.md | 分解拆开承诺不拆工作量；共因藏在缝里；独立是有分母的结论；DFA |
| ch09 | ch09-production-operation/chapter.md | 三层身份（设计定义/批次/单件）；"等效"是设计结论；2100 台追回事故 |
| ch10 | ch10-supporting-processes/chapter.md | 回到放行桌；档案≠案例；主张-论证-证据；TCL；纸面三极限 |
| ch11 | ch11-claim-ontology/chapter.md | 可信主张本体；三元组/类与实例首讲；缺件即拒；Unknown 三值与局部完整性 |
| ch12 | ch12-identity-ontology/chapter.md | 对象与同一本体；三族判据（履历/内容/配置）；Same/Different/Unknown；撞名闸 |
| ch13 | ch13-governance-ontology/chapter.md | 治理本体；签名还原成关系；组织距离可计算；授权带日期 |
| ch14 | ch14-context-hazard-ontology/chapter.md | 情境危害本体；表格站成对象；换情境重开清单；空椅子 |
| ch15 | ch15-requirements-ontology/chapter.md | 需求追溯本体；五要素缺项即拒；连线带理由；重开由查询返回 |
| ch16 | ch16-measurement-ontology/chapter.md | 测量证据本体；数字六条边；分母是名单；复制的数过不了夜 |
| ch17 | ch17-change-ontology/chapter.md | 版本变化本体；快照与变更边；每张通过交代自己的世界；两种痛快被拦 |
| ch18 | ch18-dependency-ontology/chapter.md | 依赖本体；缝成为一等对象；闭包查共享点；排除理由押着事实 |
| ch19 | ch19-field-ontology/chapter.md | 制造现场本体；单件事件账；代换须挂设计评估；683 台反查 |
| ch20 | ch20-assurance-ontology/chapter.md | 发布保证本体；活的安全案例；模型升级本体存活；终章三问 |
| 附录 A–D | appendices/ | 半导体指南走查；摩托车/卡客车适配；受控术语表；28 张方法表速查 |
| 路线图 | ch01-introduction/examples/book-roadmap.txt | 20 章依赖图、三条阅读路径、最短可用路径 |
| 图谱 | handbook/book-figure-plan.yaml | 全书章首图与编号图的提示词计划 |
| 成书 | handbook/产品可信工程-全书.pdf | 345 页图文排版成品（含封面与章首艺术图） |

## 人物与事故索引（叙事检索用）

陈工（项目负责人）· 小唐（系统工程师，"还在等什么"弧光）· 郑工（台架，冬试事故）·
小林（软件，编译器升级十一天事故）· 小蔡（新人→后十章本体建设者主角线）·
方工（安全经理，评审人=作者旧案）· 吴工（硬件，"没有出处的数字不许过夜"）·
梁工（转矩传感器供应商，2ms 典型值事故 / 电源树共因）· 老何（产线，2100 台代换料追回）。

## 使用纪律

- 案例边界：EPS-RC17 与 ENV-01 均为合成教学材料，不得作为真实产品结论引用。
- 条款纪律：正文为 ISO 自然语言转述；需要精确条款/表格坐标时，指向仓库
  `/Users/jqwang/143-工程规范` 的来源账，不要凭记忆报条款号。
- 两卷分工：概念/方法/语言/推理问题优先第一卷；功能安全实践、产品可信、
  标准本体化示范优先第二卷；"如何把一部规范本体化"用第二卷后十章做样例。
