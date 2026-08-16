# 第7章：知识图谱构建与管理 / Chapter 7: Knowledge Graph Construction and Management

## 本章内容

本体定义了知识的"骨架"（TBox），知识图谱在其上填充大规模实例数据（ABox）。
本章讲述从企业数据源到可用知识图谱的工程全流程：
- 构建流水线：数据接入 → 实体/关系抽取 → 知识融合 → 入库
- 实体对齐与消歧：同一设备的多种叫法如何归一
- 数据质量：用 SHACL 形状约束做自动化校验
- 存储与查询：三元组库与属性图数据库的选型

## 文件说明

| 文件 | 内容 |
|------|------|
| `kg-construction-pipeline.txt` | 从台账/工艺文件/维修日志到知识图谱的流水线 |
| `entity-resolution.txt` | 实体对齐与消歧：规则、相似度、冲突消解 |
| `kg-quality-shacl.ttl` | SHACL 数据质量校验形状（可直接用 pySHACL 运行） |
| `kg-storage-query.txt` | 三元组库 vs 属性图选型；SPARQL 与 Cypher 对照 |

## 本体与知识图谱的关系

| 层次 | 内容 | 规模 | 维护者 |
|------|------|------|--------|
| 本体 (TBox) | 类、属性、公理 | 数十~数百概念 | 本体工程师+专家 |
| 知识图谱 (TBox+ABox) | 本体 + 海量实例与关系 | 百万~十亿三元组 | 自动化管道 |

## 技术栈

| 环节 | 代表工具 |
|------|----------|
| 实体/关系抽取 | 规则模板、NER模型、LLM抽取（需校验，见第8章） |
| 质量校验 | SHACL (pySHACL / TopBraid)、ShEx |
| 三元组存储 | Jena TDB2 + Fuseki、GraphDB、Virtuoso |
| 属性图存储 | Neo4j、NebulaGraph |
