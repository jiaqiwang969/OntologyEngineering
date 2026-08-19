# 第7章：知识图谱构建与管理

## 本章任务

本体提供 TBox/RBox 语义骨架，知识图谱将受来源约束的 ABox 数据装入同一 RDF Dataset。
本章讨论数据接入、IRI 铸造、实体解析、质量门禁、存储投影、版本迁移和运行监控。
“图里有三元组”不等于数据合格；开放世界推理与封闭式质量校验必须显式分层。

## Semantica 绑定

- package：`semantica.chapter_packages.vol1.ch07`
- package status：`partial`
- release status：`blocked`
- payload：CQ、SHACL、SPARQL、case、engineering rules `native`；ontology `partial`
- 原生场景：`OE-V1-CH07-SCN-SHACL-QUALITY-001`
- exact oracle：正例 conforms 且 0 violation；缺序列号单因反例不 conforms 且恰有
  1 个包含“序列号”的 violation
- 未完成：领域 mutation 原子事务合同，以及 Property Graph 与 canonical RDF Dataset
  的统一事务语义

## 包内执行资产

`kg-quality-shacl` 是唯一 shape 正本；`positive` 与
`single-fault-missing-serial` 是受控夹具；`cq-missing-serial` 是对应查询。
书中显示的 Turtle 只是该资产的教学节选，不应另装 pySHACL 对书目录中的副本执行。

## 数据层次与权威

| 层 | 内容 | 权威与门禁 |
|---|---|---|
| TBox/RBox | 类、属性、公理 | 包 manifest、哈希、profile 与评审 |
| ABox | 实体、事件、测量、关系 | 业务来源、时间与来源图；Semantica 不创造现实事实 |
| Shapes | 完整性、格式和跨字段约束 | `SemanticRuntime.validate` 与 exact oracle |
| Projection | 属性图、搜索或分析副本 | 可重建的派生视图，不反向成为语义正本 |

## 复算

先由受控发布流程把实际 runtime commit 与精确 wheel/工件 SHA-256 分别写入
`SEMANTICA_RUNTIME_COMMIT`、`SEMANTICA_RUNTIME_SHA256`；缺失或错配必须失败关闭。

```bash
semantica package run semantica.chapter_packages.vol1.ch07 \
  --runtime-commit "$SEMANTICA_RUNTIME_COMMIT" \
  --runtime-artifact-sha256 "$SEMANTICA_RUNTIME_SHA256" \
  --scenario-id OE-V1-CH07-SCN-SHACL-QUALITY-001 --json
```

场景通过不代表数据写入获得授权；校验者、事实提供者与有权提交 mutation 的执行者
必须保持为不同责任面。
