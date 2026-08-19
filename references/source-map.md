# 《工程本体论》来源地图（第一卷）

本地图只回答两件事：**书里的依据在哪里**，以及需要执行佐证时应调用哪个
Semantica built-in package。两者不能混为一谈。

- 书源根：`references/ontology-engineering-book/`
- 执行正本：`semantica.chapter_packages.vol1.ch01` … `ch09`
- 构建身份：`runtime/semantica-source-lock.json`

第一卷是 9 章“石头”；ontology、CQ、query、shape、case、rule、contract、version、
PROV 与 receipt 是 Semantica package 的内容，不在书根下维护可执行副本。

## 章节地图

| 章 | 书源（回答依据） | 主题与检索要点 | Semantica package（运行佐证） |
|---|---|---|---|
| 总览 | `README.md` | 全书范围、章序与学习路线 | registry/policy |
| ch01 | `ch01-introduction/README.md`, `handbook/chapters/ch01.tex` | 从哲学本体到工程本体、AI 时代定位、路线图 | `semantica.chapter_packages.vol1.ch01` |
| ch02 | `ch02-ontology-foundations/README.md`, `handbook/chapters/ch02.tex` | 类/关系/实例、一阶逻辑、描述逻辑、推理基础、OWA/CWA | `semantica.chapter_packages.vol1.ch02` |
| ch03 | `ch03-ontology-methodology/README.md`, `handbook/chapters/ch03.tex` | Ontology 101、METHONTOLOGY、CQ、OntoClean | `semantica.chapter_packages.vol1.ch03` |
| ch04 | `ch04-ontology-languages/README.md`, `handbook/chapters/ch04.tex` | RDF、RDFS、OWL、Turtle、限制、SPARQL | `semantica.chapter_packages.vol1.ch04` |
| ch05 | `ch05-reasoning/README.md`, `handbook/chapters/ch05.tex` | 描述逻辑、规则、SWRL、时序/概率推理边界 | `semantica.chapter_packages.vol1.ch05` |
| ch06 | `ch06-applications/README.md`, `handbook/chapters/ch06.tex` | 自动驾驶、BIM、航空 FMEA、制造调度 | `semantica.chapter_packages.vol1.ch06` |
| ch07 | `ch07-knowledge-graph/README.md`, `handbook/chapters/ch07.tex` | KG 构建、实体消歧、存储/查询、SHACL 质量 | `semantica.chapter_packages.vol1.ch07` |
| ch08 | `ch08-ontology-llm/README.md`, `handbook/chapters/ch08.tex` | GraphRAG、Text2SPARQL、幻觉控制、ontology-guided Agent | `semantica.chapter_packages.vol1.ch08` |
| ch09 | `ch09-capstone-manufacturing/README.md`, `handbook/chapters/ch09.tex` | 制造本体综合案例、查询服务、推理与交付 | `semantica.chapter_packages.vol1.ch09` |
| 术语 | `handbook/chapters/appB-glossary.tex` | 中英文术语对齐 | 按相关章节 package |
| 成书 | `handbook/工程本体论-全书.pdf` | 需要页式、图文布局或最终排版时 | 不作为运行资产 |

章节 `README.md` 和 handbook source 是书内依据。不要再引用已经迁入 Semantica 的
旧章级 companion assets 或实现目录。需要读取迁移后的资产时，用 package
manifest/asset ID；需要执行时，用 package/scenario ID。

## 检索流程

```bash
python3 scripts/search_ontology_sources.py --scope book \
  "能力问题 competency question"
python3 scripts/search_ontology_sources.py --scope book --json \
  "OntoClean 刚性 统一性"
```

推荐双语查询：

- `本体构建 方法论 Ontology 101 METHONTOLOGY`
- `能力问题 competency question acceptance test`
- `OntoClean 刚性 统一性 依赖性 身份`
- `描述逻辑 description logic DL reasoning`
- `RDF RDFS OWL SPARQL Turtle`
- `SHACL 质量 校验 constraint validation`
- `GraphRAG Text2SPARQL hallucination control`
- `ontology guided agent provenance routing`

回答时优先给章名和书源路径。书中未覆盖时，要明确说“第一卷没有给出该结论”，
再把通用知识标为补充推论。

## 执行佐证

```bash
bash runtime/setup_runtime.sh
runtime/.venv/bin/semantica package show \
  semantica.chapter_packages.vol1.ch03 --json
runtime/.venv/bin/python demos/vol1_ch03_cq_acceptance.py
```

执行报告的 package/scenario/oracle/receipt 是代码证据，不能替代书源引用。完整 9 章
各有独立 manifest 与状态；某章的绿色结果不能替另一章生成 receipt。若包需要完整
DL/tableau、一般 SWRL 内建、非单调、时序或概率推理而运行时不支持，必须报告
blocked，不得把书中理论覆盖误报为代码能力。

## 可选外部案例

CauchyX PDE Agent 和 CAD Agent 只可作为明确标注的 applied example，不是第一卷书源。
有授权本地 checkout 时，分别使用 `--scope pde` 或 `--scope cad` 检索。实际求解、CAD
修改或设备执行应切换到对应技能；本 skill 的语义通过不授予外部执行权限。
