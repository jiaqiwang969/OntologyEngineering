# 《产品可信工程》来源地图（第二卷）

第二卷是《工程本体论》的实战续篇：前十章按功能安全生命周期讲传统工程，后十章
把同一批困难逐章本体化。贯穿案例 EPS-RC17、跨域投影件 ENV-01、全部人物、事故和
数值均为合成教学材料。

- 书源根：`references/product-trustworthiness-book/`
- 执行正本：`semantica.chapter_packages.vol2.ch01` … `ch20`
- 规范 domain package：`semantica.chapter_packages.vol2.normative`
- 源码/wheel 身份：`runtime/semantica-source-lock.json`

书是来源“石头”。第二卷的 ontology、CQ、SHACL、SPARQL、case、rule、contract、
version、PROV 与 receipt 全部由 Semantica built-in packages 拥有；不存在 OE-local
工程正本或 normative 平行目录。

## 章节地图（ch N ↔ ch N+10）

| 章 | 书源 | 主题与可检索要点 | Semantica package |
|---|---|---|---|
| 前言 | `front-matter/preface.md` | 三问：谁指挥、谁记得、谁有权改写 | 不单独执行 |
| ch01 | `ch01-introduction/chapter.md` | 六绿僵局、主张七部件、证据关系、四层绿色 | `semantica.chapter_packages.vol2.ch01` |
| ch02 | `ch02-concepts-terminology/chapter.md` | 相关项/系统/元素、故障→错误→失效、ASIL 风险语言 | `semantica.chapter_packages.vol2.ch02` |
| ch03 | `ch03-safety-management/chapter.md` | 六职责、独立性、确认措施、安全文化 | `semantica.chapter_packages.vol2.ch03` |
| ch04 | `ch04-concept-hara/chapter.md` | HARA、危害事件、S/E/C 证据与查表纪律 | `semantica.chapter_packages.vol2.ch04` |
| ch05 | `ch05-system-development/chapter.md` | 目标派生、HSI、时间预算、逐层集成 | `semantica.chapter_packages.vol2.ch05` |
| ch06 | `ch06-hardware-development/chapter.md` | FIT 底账、SPFM/LFM/PMHF 与分母纪律 | `semantica.chapter_packages.vol2.ch06` |
| ch07 | `ch07-software-development/chapter.md` | V 模型、覆盖率边界、差异分析与保质期 | `semantica.chapter_packages.vol2.ch07` |
| ch08 | `ch08-asil-decomposition-dfa/chapter.md` | ASIL 分解、共因、独立性、DFA | `semantica.chapter_packages.vol2.ch08` |
| ch09 | `ch09-production-operation/chapter.md` | 设计/批次/单件身份、等效结论、现场追回 | `semantica.chapter_packages.vol2.ch09` |
| ch10 | `ch10-supporting-processes/chapter.md` | 主张-论证-证据、安全案例、TCL、纸面极限 | `semantica.chapter_packages.vol2.ch10` |
| ch11 | `ch11-claim-ontology/chapter.md` | 可信主张本体、缺件即拒、Unknown 与局部完整性 | `semantica.chapter_packages.vol2.ch11` |
| ch12 | `ch12-identity-ontology/chapter.md` | 三族身份判据、Same/Different/Unknown、撞名闸 | `semantica.chapter_packages.vol2.ch12` |
| ch13 | `ch13-governance-ontology/chapter.md` | 签名关系、组织距离、授权角色与有效期 | `semantica.chapter_packages.vol2.ch13` |
| ch14 | `ch14-context-hazard-ontology/chapter.md` | 情境危害、表格对象化、换情境重开 | `semantica.chapter_packages.vol2.ch14` |
| ch15 | `ch15-requirements-ontology/chapter.md` | 五要素需求、理由边、查询式重开清单 | `semantica.chapter_packages.vol2.ch15` |
| ch16 | `ch16-measurement-ontology/chapter.md` | 数字六条边、分母名单、复算与出处 | `semantica.chapter_packages.vol2.ch16` |
| ch17 | `ch17-change-ontology/chapter.md` | 快照、变更边、旧结论世界、保留/作废理由 | `semantica.chapter_packages.vol2.ch17` |
| ch18 | `ch18-dependency-ontology/chapter.md` | 依赖闭包、共享点、排除事实与独立性 | `semantica.chapter_packages.vol2.ch18` |
| ch19 | `ch19-field-ontology/chapter.md` | 单件事件账、代换、设计评估与反查 | `semantica.chapter_packages.vol2.ch19` |
| ch20 | `ch20-assurance-ontology/chapter.md` | 活安全案例、快照绑定、发布保证、终章三问 | `semantica.chapter_packages.vol2.ch20` |
| 附录 | `appendices/` | 半导体走查、车辆适配、术语表、方法表 | 按关联 package |
| 命题索引 | `propositions-index.md` | 全书命题单行索引与章节出处 | 不作为单独 package |
| 成书 | `handbook/产品可信工程-全书.pdf` | 最终图文版式与页码 | 不作为运行资产 |

## 使用纪律

- **案例边界**：EPS-RC17、ENV-01、人物、事故与数字不得作为真实产品事实。
- **条款边界**：正文和 normative package 是转述/派生语义，不是 ISO 原文。精确
  条款、表格、模态或引文必须回用户合法持有的受控来源核对。
- **来源位置**：受控源可由 `ONTOLOGY_ENGINEERING_AUTHORING_ROOT` 指向，但不得把
  其绝对路径、原文或受限抽取件写进公共书仓或公开 receipt。
- **权威边界**：书解释知识；Semantica 执行声明的语义；受控活动产生事实；有权人
  决定接受、认证、风险承担和发布。

## 问答与执行

先查书：

```bash
python3 scripts/search_ontology_sources.py --scope book \
  "ASIL decomposition independence DFA"
```

再按需运行 package：

```bash
bash runtime/setup_runtime.sh
runtime/.venv/bin/semantica package show \
  semantica.chapter_packages.vol2.ch18 --json
runtime/.venv/bin/python demos/vol2_ch18_independence_meet.py
```

回答应同时给出书源路径和 package/scenario 状态。一次 package 运行只能佐证其声明的
oracle；release verdict 仍可能因缺失证据、不支持能力或 provenance 绑定不完整而
blocked。不要把自动绿色解释为 ISO 合规、产品安全、认证或发布授权。

## 人物与叙事索引

陈工（项目负责人）· 小唐（系统工程师）· 郑工（台架）· 小林（软件）·
小蔡（后十章本体建设者）· 方工（安全经理）· 吴工（硬件）· 梁工（供应商）·
老何（产线）。这些名字只帮助检索合成教学故事，不指向真实个人。
