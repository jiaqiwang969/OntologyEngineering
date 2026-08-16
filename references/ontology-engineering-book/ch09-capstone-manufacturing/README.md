# 第9章：综合实战案例：智能制造知识管理系统
# Chapter 9: Capstone - Intelligent Manufacturing Knowledge Management System

## 项目概述

本章是全书综合实战案例，实现一个完整的 **智能制造知识管理系统**，
涵盖本体设计、Java/Jena查询服务、Python推理引擎的完整技术栈。

## 系统架构

```
智能制造知识管理系统
├── 本体层 (Ontology Layer)
│   ├── manufacturing.owl    # 核心制造本体（Manchester语法）
│   └── sparql-queries.sparql # 标准查询集
├── 查询服务 (Query Service)
│   ├── OntologyManager.java # 本体加载与管理（Apache Jena）
│   └── QueryService.java    # SPARQL查询服务
└── 推理引擎 (Reasoning Engine)
    └── reasoner.py          # 基于owlready2的推理与冲突检测
```

## 技术栈

| 层次 | 技术 | 版本 |
|------|------|------|
| 本体语言 | OWL Manchester Syntax | OWL 2 DL |
| Java查询 | Apache Jena | 4.10.0+ |
| Python推理 | owlready2 + Pellet | 0.46+ |
| 规则引擎 | SWRL + swrlb | - |

## 快速开始

### 运行Java查询服务

Maven依赖：
```xml
<dependency>
  <groupId>org.apache.jena</groupId>
  <artifactId>apache-jena-libs</artifactId>
  <version>4.10.0</version>
  <type>pom</type>
</dependency>
```

```bash
mvn compile
java -cp target/classes:target/dependency/* QueryService
```

### 运行Python推理引擎
```bash
pip install owlready2
python src/reasoner.py
```

## 系统核心能力

| CQ | 问题 | 实现方式 |
|----|------|----------|
| CQ1 | 哪些设备可以加工铝合金？ | SPARQL查询 |
| CQ2 | 当前哪些设备空闲？ | SPARQL + hasStatus |
| CQ3 | 任务T001存在哪些冲突？ | SWRL规则推理 |
| CQ4 | 产品P001推荐什么工艺？ | 相似性规则 |
| CQ5 | 工单W001的工序顺序？ | 传递性属性推理 |

## 开发技术说明

### Apache Jena
- `OntModel`: 支持OWL推理的本体模型
- `ModelFactory.createOntologyModel(OntModelSpec.OWL_DL_MEM_RULE_INF)`: 创建带内置推理器的模型
- `QueryExecutionFactory.create(sparql, model)`: 执行SPARQL查询

### owlready2
- `get_ontology(path).load()`: 加载本体
- `sync_reasoner_pellet()`: 运行Pellet推理器
- `onto.search(type=SomeClass)`: 查询特定类的实例
