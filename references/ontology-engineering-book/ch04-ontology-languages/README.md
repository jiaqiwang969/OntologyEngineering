# 第4章：本体描述语言与工具 / Chapter 4: Ontology Description Languages and Tools

## 本章内容

本章介绍工程本体论中常用的描述语言和工具，包括：
- RDF (Resource Description Framework) 及其序列化格式
- RDFS (RDF Schema)
- OWL (Web Ontology Language)
- SPARQL 查询语言
- Protégé 本体编辑器

## 文件说明

| 文件 | 内容 |
|------|------|
| `rdf-turtle-examples.ttl` | RDF Turtle格式示例（制造领域） |
| `rdf-xml-examples.rdf` | RDF/XML格式示例 |
| `rdfs-examples.ttl` | RDFS模式定义示例 |
| `owl-classes.owl` | OWL类层次与约束（Manchester语法） |
| `owl-properties.owl` | OWL属性定义示例 |
| `sparql-queries.sparql` | SPARQL查询示例集 |
| `property-restrictions.txt` | OWL属性约束说明（∃/∀限制） |
| `manufacturing.owl` | 制造本体完整示例（配合4.5.3节Protégé教程） |

## 语言对比

| 语言 | 表达能力 | 主要用途 |
|------|----------|----------|
| RDF | 三元组事实 | 数据表示 |
| RDFS | 类与属性定义 | 基础模式 |
| OWL DL | 完整描述逻辑 | 工程本体 |
| SPARQL | 图查询 | 知识检索 |

## Protégé快速入门

1. 安装插件：Tools → Plugins，安装 OntoGraf（可视化）、SWRL Tab（规则）、SPARQL Query（查询）
2. 新建本体：File → New，在 Active Ontology 视图设置 IRI
3. 导入本体：Active Ontology → Ontology imports，输入IRI或本地文件
