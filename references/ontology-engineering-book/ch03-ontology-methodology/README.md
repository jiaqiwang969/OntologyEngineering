# 第3章：本体构建方法论

## 本章任务

本章把“如何设计本体”改写为一条可审查的生命周期：目的与范围 → 能力问题（CQ）→
概念化 → 形式化 → 场景与 oracle → 回归 → 版本、溯源和发布判定。Ontology 101、
METHONTOLOGY、NeOn 与 OntoClean 是不同粒度的方法资源；它们不能只停留在文档清单，
但也不能在没有执行器时假称已自动化。

## Semantica 绑定

- package：`semantica.chapter_packages.vol1.ch03`
- package status：`partial`
- release status：`blocked`
- 已迁入：CQ 教学材料、Ontology 101/METHONTOLOGY/OntoClean 工程规则、章合同、
  CQ 注册表、CQ1 查询、正例与单因反例
- 原生场景：`OE-V1-CH03-SCN-CQ-ACCEPTANCE-001`
- 未完成：Ontology 101/METHONTOLOGY 阶段门禁 runner、OntoClean 刚性/同一性/
  统一性/依赖性检查器、包级发布收据

## 方法论不是四套运行时

| 方法 | 书中作用 | Semantica 中的状态 |
|---|---|---|
| Ontology 101 | 轻量七步迭代 | 规则材料已入包，阶段门禁未实现 |
| METHONTOLOGY | 生命周期与概念化产物 | 规则材料已入包，阶段门禁未实现 |
| NeOn | 复用、对齐和网络化场景 | 书中方法说明，未声明专用 runner |
| OntoClean | 类层次元属性审查 | 检查规则已入包，自动检查器缺失 |
| CQ 回归 | 范围与验收 | CQ1 的正例/单因反例与精确多重集 oracle 已原生绑定 |

一个 CQ 只有在问题、查询、输入夹具与预期结果同时固定后才成为可执行验收；
“能返回一行”不够，必须比较精确绑定、行数与单因反例。

## 复算

先由受控发布流程把实际 runtime commit 与精确 wheel/工件 SHA-256 分别写入
`SEMANTICA_RUNTIME_COMMIT`、`SEMANTICA_RUNTIME_SHA256`；不得用空值或示例摘要替代。

```bash
semantica package run semantica.chapter_packages.vol1.ch03 \
  --runtime-commit "$SEMANTICA_RUNTIME_COMMIT" \
  --runtime-artifact-sha256 "$SEMANTICA_RUNTIME_SHA256" \
  --scenario-id OE-V1-CH03-SCN-CQ-ACCEPTANCE-001 --json
```

该场景只证明 CQ1 在声明夹具上符合 oracle，不证明方法论全部自动化，也不授予本体发布权限。

## 核心纪律

- 先写可判定的问题，再选择概念与语言。
- 正例、单因反例、歧义例和上一发布版回归必须分开保存。
- 复用外部本体前审查许可、维护状态、语义承诺和版本锁。
- 任何语义变更都要经过 snapshot/diff、来源记录和显式发布判定。
