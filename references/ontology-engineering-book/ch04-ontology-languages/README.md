# 第4章：本体描述语言与工具

## 本章任务

本章介绍 RDF Dataset、RDFS、OWL 2、SPARQL 与 Protégé，强调“语法可读”与
“语义可执行”不是同一件事。RDF/Turtle、RDF/XML、Manchester Syntax 和 SPARQL
可以作为教学文本比较；本书的可复算主张则只经过 Semantica 的包加载、查询和门禁。

## Semantica 绑定

- package：`semantica.chapter_packages.vol1.ch04`
- package status：`partial`
- release status：`blocked`
- payload：ontology/CQ/SPARQL/case `native`；rules `partial`；shapes `absent`
- 场景：`OE-V1-CH04-SCN-OPEN-VS-CLOSED-001`（状态 `partial`）
- 原生部分：无损 RDF Dataset 处理、SPARQL 查询，以及调用 ch07 shape 的跨包校验
- 明示边界：Manchester 资产被保留但当前 runtime 不解析；本章没有自有 shapes；
  图同构 round-trip oracle 未声明；依赖外部端点的 CQ7 `SERVICE` 离线 fail closed

## 包内资产

| 角色 | 资产 |
|---|---|
| ontology / ontology source | `rdf-turtle-examples`、`rdf-xml-examples`、`rdfs-examples`、`manufacturing`、`owl-classes`、`owl-properties` |
| SPARQL | `cq01`–`cq08`、`cq-missing-serial`、`sparql-queries` |
| case | `property-restrictions`、`open-world-data` |
| 跨包 shape | `semantica.chapter_packages.vol1.ch07:kg-quality-shacl` |

Protégé、Jena、RDF4J、GraphDB、RDFLib、owlready2 等仍在正文中作为生态、互操作或
历史实现讨论；它们不构成本书的并行执行入口，也不能替代 Semantica receipt。

## 语言对比

| 层 | 主要用途 | 不应误解为 |
|---|---|---|
| RDF | 图事实与 Dataset | 数据质量约束语言 |
| RDFS | 基础模式与蕴含 | 表单式 domain/range 校验 |
| OWL | 描述逻辑公理 | 完整性约束或封闭世界规则 |
| SPARQL | 查询/更新 RDF Dataset | 自动拥有外部端点与网络授权 |
| SHACL | 数据图形状校验 | OWL 世界语义的替代品 |

## 统一语义介入入口

从 ontology-engineering skill 根运行。统一入口从 source lock 自动核验 Semantica
runtime identity；先发现，再按
[`semantic-engagement-contract.md`](../../semantic-engagement-contract.md) 建立绑定：

```bash
runtime/.venv/bin/python scripts/semantic_engagement.py discover
runtime/.venv/bin/python scripts/semantic_engagement.py run \
  --binding /path/to/package-binding.json \
  --task /path/to/task-envelope.json \
  --scenario OE-V1-CH04-SCN-OPEN-VS-CLOSED-001
runtime/.venv/bin/python scripts/semantic_engagement.py open \
  --binding /path/to/workspace-binding.json \
  --task /path/to/task-envelope.json \
  --workspace /path/to/semantica-managed-registry
```

该场景将开放世界下的“缺少序列号”查询与封闭式 shape 违规并置；跨包依赖和 partial
状态必须保留在结果解释中。书提供语言与方法，受控工程记录提供事实，Semantica 是唯一
可执行语义；网络访问、事实接受、风险与发布由相应有权人决定。`open` 不自动晋升学习
结果。原生 `semantica package ...` 只供底层 runner/manifest 诊断，不是主运行路径。
