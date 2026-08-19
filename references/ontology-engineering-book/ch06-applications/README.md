# 第6章：工程领域应用案例

## 本章任务

本章用智能制造调度、自动驾驶场景、BIM 合规检查与航空航天 FMEA 四类案例，说明
本体如何参与约束检查、语义集成、决策支持和合规审计。这些案例用于迁移方法和风险
模式，不应被读成真实项目绩效、现实产品结论或已获得行业批准的系统。

## Semantica 绑定

- package：`semantica.chapter_packages.vol1.ch06`
- package status：`absent`
- release status：`blocked`
- 已迁入：四类案例文本、章合同、CQ 与场景注册表
- 未完成：案例 RDF/OWL 形式化、SHACL、SPARQL、正例/单因反例、exact oracle 与
  端到端场景绑定
- 声明场景：`OE-V1-CH06-SCN-END-TO-END-001`，其 status 与 oracle 均为 `absent`

因此，本章中的规则片段、流程和数值只能作为教学规格或示意，不得写成“Semantica
已经证明四个案例可运行”。对该场景的执行请求应因缺少绑定而 fail closed。

## 四类应用模式

| 案例 | 主要语义问题 | 走向可执行还缺什么 |
|---|---|---|
| 制造调度 | 设备能力、状态、任务与冲突 | 领域本体、数据夹具、规则 profile、精确冲突 oracle |
| 自动驾驶 | 场景实体、空间/时序关系、风险状态 | 时点/窗口语义、实时预算、正反例和安全责任接口 |
| BIM 合规 | IFC 对齐、条款转述、可判定检查 | 合法受控条款来源、规则映射、适用域与证据链 |
| 航空 FMEA | 故障模式、原因、影响与传播 | 受控型号事实、传播规则、覆盖与审查基线 |

## 统一语义介入入口

从 ontology-engineering skill 根运行。先只读发现，再按
[`semantic-engagement-contract.md`](../../semantic-engagement-contract.md) 建立精确
package/workspace binding 与 task envelope：

```bash
runtime/.venv/bin/python scripts/semantic_engagement.py discover
runtime/.venv/bin/python scripts/semantic_engagement.py run \
  --binding /path/to/package-binding.json \
  --task /path/to/task-envelope.json \
  --scenario OE-V1-CH06-SCN-END-TO-END-001
runtime/.venv/bin/python scripts/semantic_engagement.py open \
  --binding /path/to/workspace-binding.json \
  --task /path/to/task-envelope.json \
  --workspace /path/to/semantica-managed-registry
```

该场景为 `absent`，所以 `run` 必须失败关闭并保留 blocker；`open` 只允许把受控项目证据
送入可审查的学习回路，不自动补造或晋升行业本体。书提供迁移方法，受控工程记录提供
事实，Semantica 是唯一可执行语义；现实绩效、风险、合规和发布由有权人决定。原生
`semantica package ...` 只供底层 runner/manifest 诊断，不是主运行路径。

下一步不是在书旁复制四套本体，而是在 Semantica ch06 包内逐个补齐最小可判定场景，
并让每个场景都有独立的正例、单因反例与 oracle。
