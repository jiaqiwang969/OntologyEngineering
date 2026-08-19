# 工程本体论 — Engineering Ontology: Principles and Practice

## 关于本书 / About This Book

**《工程本体论》** 系统讲述如何把本体论理论用于工程领域，覆盖概念化、
描述逻辑、RDF/RDFS/OWL、SPARQL、SHACL、推理、知识图谱以及
ontology-guided LLM/Agent。书中的概念、方法、案例叙事与习题是稳定的“石头”；
与这些内容对应的本体、能力问题（CQ）、形状、查询、案例、工程规则、执行合同、
版本与溯源收据，统一由 **Semantica** 承载和执行。

本目录是书稿，不是第二套语义运行库。删除或改写本目录中的文字不会绕过
Semantica 的包注册、能力声明、精确 oracle 与发布门禁；反过来，某个包场景运行
成功也只是在其声明边界内复算书中主张，不会自动把整章标记为完整或可发布。

## 唯一执行语义 / Sole Executable Semantics

- 九章分别绑定 `vol1.ch01` 至 `vol1.ch09`；完整包名使用下文所列统一前缀。
- RDF/OWL、CQ、SPARQL、SHACL、事实、规则、案例、合同和场景只在
  Semantica 的 allowlisted built-in package 中保留可执行正本。
- 本书中的代码框是教学节选；其历史来源路径用于溯源，构建时通过迁移账解析到
  Semantica 包资产，不是可从本目录直接运行的副本。
- Python、CLI 与 MCP 都必须委托同一个 `SemanticPackageRunner`；未知包、未知场景、
  缺失资产、哈希不符或超出能力范围时 fail closed。
- `run` 的场景 oracle 与 `verify` 的发布判定必须分开报告。当前各章 manifest 的
  `release_status` 均为 `blocked`，不得把教学片段或部分场景通过写成整章“全绿”。

## 书稿结构 / Book Structure

```text
ontology-engineering-book/
├── ch01-introduction/             # 哲学到工程的转向；包 vol1.ch01
├── ch02-ontology-foundations/     # 基础理论；包 vol1.ch02
├── ch03-ontology-methodology/     # 构建方法论；包 vol1.ch03
├── ch04-ontology-languages/       # RDF/RDFS/OWL/SPARQL；包 vol1.ch04
├── ch05-reasoning/                # 推理机制；包 vol1.ch05
├── ch06-applications/             # 工程案例；包 vol1.ch06
├── ch07-knowledge-graph/          # 知识图谱与 SHACL；包 vol1.ch07
├── ch08-ontology-llm/             # 本体 × LLM/Agent；包 vol1.ch08
├── ch09-capstone-manufacturing/   # 制造业综合案例；包 vol1.ch09
├── resources/                     # 阅读与选型线索，不是自动信任清单
└── handbook/                      # XeLaTeX 作者源、受控生成片段与成书 PDF
```

第 2、4、5、6、9 章的教学材料来自原书稿；第 1、3、7、8 章包含按全书脉络补写的
材料。所有已迁移材料在 Semantica manifest 中保留 `source_anchor`、哈希与
`derivation`，以区分原文、结构化提取、适配器和新增执行合同。

## 九章执行落点 / Package Map

下表以 `vol1.chNN` 缩写包名；其完整前缀均为
`semantica.chapter_packages.`。

| 章 | 书中主题 | Semantica 包 | 当前边界摘要 |
|---|---|---|---|
| Ch01 | 工程本体论的目的与范围 | `vol1.ch01` | CQ/案例已入包；概念化边界 runner 与 oracle 缺失 |
| Ch02 | DL、FOL、开放/封闭世界 | `vol1.ch02` | 受限正向规则与 OWA/CWA 场景可复算；完整 DL、默认/非单调逻辑不支持 |
| Ch03 | CQ、Ontology 101、METHONTOLOGY、OntoClean | `vol1.ch03` | CQ1 正/单因反例可执行；阶段门禁与 OntoClean 检查器缺失 |
| Ch04 | RDF/RDFS/OWL/SPARQL | `vol1.ch04` | RDF Dataset 与查询原生；Manchester 解析、章内 shapes、SERVICE 离线执行不完整 |
| Ch05 | DL、SWRL、时态与概率推理 | `vol1.ch05` | 受限正向规则适配可复算；SWRL built-ins、DL tableau、时态、概率、默认逻辑 fail closed |
| Ch06 | 四类工程应用 | `vol1.ch06` | 案例文本已迁入；尚无领域 RDF/OWL、SHACL、查询与 exact oracle |
| Ch07 | 图谱流水线、对齐、SHACL | `vol1.ch07` | SHACL 正/单因反例原生；属性图统一事务与领域 mutation 合同未完成 |
| Ch08 | 幻觉控制、GraphRAG、Text2SPARQL、Agent | `vol1.ch08` | 规则与案例已入包；动作路由、模型/prompt/快照端到端合同仍阻断 |
| Ch09 | 制造业综合案例 | `vol1.ch09` | CQ1 精确 oracle 原生；Manchester/SWRL 仅保留，CQ2–CQ7 与端到端推理未假绿 |

以上状态以安装版本中的 manifest 为准；书中的表是发布时快照，不替代机器可读清单。

## 阅读与复算 / Read and Corroborate

先读正文，再查看并运行对应包：

`ONTOLOGY_ENGINEERING_ROOT` 指向同时含两卷书源的受控 checkout。另两个环境变量
必须由构建/发布流程写入：一个是所运行 Semantica 的 commit 身份，另一个是该精确
wheel 或运行工件的 SHA-256；空值、占位值或错配值都会 fail closed。

```bash
semantica package list --volume vol1 --json
semantica package show semantica.chapter_packages.vol1.ch03 --json
semantica package verify-books \
  --book-root "$ONTOLOGY_ENGINEERING_ROOT" --volume vol1 --json
semantica package run semantica.chapter_packages.vol1.ch03 \
  --runtime-commit "$SEMANTICA_RUNTIME_COMMIT" \
  --runtime-artifact-sha256 "$SEMANTICA_RUNTIME_SHA256" \
  --scenario-id OE-V1-CH03-SCN-CQ-ACCEPTANCE-001 --json
semantica package verify semantica.chapter_packages.vol1.ch03 \
  --runtime-commit "$SEMANTICA_RUNTIME_COMMIT" \
  --runtime-artifact-sha256 "$SEMANTICA_RUNTIME_SHA256" \
  --scenario-id OE-V1-CH03-SCN-CQ-ACCEPTANCE-001 --json
```

不要在本书目录另装 Jena、owlready2、pySHACL 或 RDFLib 来构造平行答案。
Protégé、Jena、RDF4J、HermiT、Pellet、pySHACL 等仍可作为语言生态与历史实现的
学习对象；但本书所声称的可复算结果只能以 source-locked Semantica 包的运行收据为准。

## 权利与责任边界 / Rights and Authority

本书正文与教学案例仅供学习参考，版权归原书作者所有。外部标准、本体与软件各自
受其许可约束。Semantica 的语义一致性、查询或形状校验结果不等于现实事实、合规
结论、设备操作授权或发布批准；事实权威与决策权威必须由受控来源和具名责任人承担。

---
*Derived from 《工程本体论》书稿一审0513 正.docx; executable semantics bound to Semantica.*
