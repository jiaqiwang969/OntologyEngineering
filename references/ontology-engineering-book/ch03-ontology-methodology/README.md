# 第3章：本体构建方法论 / Chapter 3: Ontology Engineering Methodologies

## 本章内容

本章回答"如何系统地设计一个本体"，介绍：
- 能力问题（Competency Questions, CQ）驱动的需求定义
- Ontology Development 101 七步法
- METHONTOLOGY 生命周期方法
- NeOn 场景化方法概览
- OntoClean 本体质量评估（元属性标注）

## 文件说明

| 文件 | 内容 |
|------|------|
| `competency-questions.txt` | 制造领域能力问题设计：CQ→本体元素→验证查询 |
| `ontology-101-process.txt` | Ontology 101 七步法完整走查（制造本体） |
| `methontology-lifecycle.txt` | METHONTOLOGY 生命周期与概念化产物 |
| `ontoclean-evaluation.txt` | OntoClean 元属性标注与建模错误检查 |

## 方法论对比

| 方法论 | 提出时间 | 特点 | 适用场景 |
|--------|----------|------|----------|
| Ontology 101 | 2001 | 七步迭代，轻量易上手 | 教学、中小型本体 |
| METHONTOLOGY | 1997 | 完整生命周期+产物模板 | 企业级、需文档化交付 |
| NeOn | 2009 | 九种场景，强调复用 | 网络化、多本体集成 |
| OntoClean | 2002 | 元属性形式化评估 | 类层次质量审查 |

## 关键概念

- **能力问题 (CQ)**: 本体建成后必须能回答的问题，是范围定义与验收标准
- **概念化 (Conceptualization)**: 从术语表到概念分类树、关系表的中间产物
- **刚性 (Rigidity)**: OntoClean 元属性，区分本质类型与角色/状态
- **本体复用 (Reuse)**: 优先采用顶层本体（BFO/DOLCE）与领域本体（IOF），见附录A
