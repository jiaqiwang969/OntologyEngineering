# 第2章：本体论基础理论

## 本章任务

本章建立类、属性、公理、实例、TBox/ABox/RBox、描述逻辑和一阶逻辑的共同语言，
并区分开放世界、封闭世界、单调、非单调与默认推理。重点不是背符号，而是知道
一条知识主张在何种语义前提下成立，以及证据缺失为什么不能自动等价为否定。

## Semantica 绑定

- package：`semantica.chapter_packages.vol1.ch02`
- package status：`partial`
- release status：`blocked`
- payload：ontology `partial`；CQ/SPARQL/case `native`；rules `adapter`；shapes `absent`
- 可复算场景：`OE-V1-CH02-SCN-REASONING-MODES-001`
- exact oracle：受限正向链结论，以及同一数据在 OWA 证据查询与 CWA/NAF 查询下的不同布尔结果
- 不支持：完整 DL/tableau、默认逻辑、一般非单调推理；这些请求必须 fail closed

原书的 `core-concepts`、`description-logic`、`first-order-logic`、
`reasoning-examples` 已作为教学 case 迁入包。执行场景另绑定受限事实、规则、
Turtle 数据与两条 SPARQL ASK 查询；它们不是对整章推理能力的无限承诺。

## 关键概念

- **类（Class）**：领域概念的集合性刻画，不等同于编程语言中的实现继承。
- **属性（Property）**：对象属性连接实体，数据属性连接实体与字面量；domain/range 会触发分类，不是表单校验。
- **公理（Axiom）**：对子类、等价、不相交、基数与限制的逻辑承诺。
- **实例（Individual）**：ABox 中的个体；OWL 默认不采用唯一名假设。
- **Unknown**：开放世界下“未证真也未证伪”的有效状态，不能被静默压成 false。

## 符号速查

| 符号 | 含义 |
|---|---|
| ⊑ / ≡ | 子类 / 等价类 |
| ∀ / ∃ | 全称限制 / 存在限制 |
| ∧ / ∨ / ¬ | 与 / 或 / 非 |
| ⊓ / ⊔ | 类交 / 类并 |

## 复算与边界检查

`SEMANTICA_RUNTIME_COMMIT` 与 `SEMANTICA_RUNTIME_SHA256` 必须由受控发布流程绑定到
当前实际运行的 commit 和精确 wheel/工件；缺失或错配时命令应失败关闭。

```bash
semantica package run semantica.chapter_packages.vol1.ch02 \
  --runtime-commit "$SEMANTICA_RUNTIME_COMMIT" \
  --runtime-artifact-sha256 "$SEMANTICA_RUNTIME_SHA256" \
  --scenario-id OE-V1-CH02-SCN-REASONING-MODES-001 --json
semantica package verify semantica.chapter_packages.vol1.ch02 \
  --runtime-commit "$SEMANTICA_RUNTIME_COMMIT" \
  --runtime-artifact-sha256 "$SEMANTICA_RUNTIME_SHA256" \
  --scenario-id OE-V1-CH02-SCN-REASONING-MODES-001 --json
```

预期是场景 oracle 可被复算，而发布验证仍因包级缺口与收据状态被阻断；两者不可合并报道。
