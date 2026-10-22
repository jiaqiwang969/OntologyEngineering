# SHACL 检查的停用边界

每个 shape 和它引用的每条 `sh:sparql` 检查各自拥有停用状态。Rust Semantica 在既有全图参数预检后，按 `sh:deactivated` 的布尔值决定是否执行该条检查；`true` 与 `"1"^^xsd:boolean` 跳过，`false`、`"0"^^xsd:boolean` 和缺省值继续执行。

例如旧载荷检查停用后，不应继续生成违反项。同一 shape 仍启用的其他载荷检查和 Core 约束继续报告完整发现；停用状态不批准工程采用，也不替代原 Project 的必要检查。非法布尔值或重复停用值仍在全图预检拒绝，包括没有目标节点的输入。活动查询的语法错误仍为失败。停用查询仍保留原有 select 必填、唯一及 literal 检查；未扩大成全图 SELECT 语法校验。

原 RC.1 仅检查整个 shape 的停用，漏掉 SPARQL constraint 自身的状态。本次在同一个 `semantica-runtime::shacl` owner 修正，复用原数据图、报告及 kernel 入口；没有新增语义引擎或改动报告 DTO。规范正本是 `specs/modules/semantic-shacl-activation.toml`，派生检查关系由锁定 ggen 生成。

独立依据是 [W3C SHACL 2017 §5.3](https://www.w3.org/TR/2017/REC-shacl-20170720/#sparql-constraints-validation)。十六组预先固定的合成输入保存在 `crates/semantica-runtime/tests/fixtures/shacl-sparql-deactivation.json`，覆盖停用、活动对照、父级、同级、Core 发现以及拒绝情况。Rust 测试比较完整 finding 和参数错误；CLI 保留其既有脱敏失败/unknown 合同。停用且查询本身不合语法的输入不作为规范通过案例。

实际新版本构建、原失败、安装与接收范围由 `var/maintenance` 回执记录。此次有限修复不代表完整 SHACL、原库行为全等或完整 Semantica 已完成。下一阶段继续补原结构化规则输入、明确选图及原查询值表示的接续合同，保留原冻结预期和差异。
