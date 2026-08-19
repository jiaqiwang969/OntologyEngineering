# 第 1 章：为什么“全绿”不等于产品可信

正文见 [chapter.md](chapter.md)。本章保留主张、证据射程、版本与重开条件的可读论述、术语、方法和
合成教学案例；机器可执行语义不在本目录。

## 唯一执行绑定

- package id：`semantica.chapter_packages.vol2.ch01`
- primary scenario：`semantica.vol2.ch01.scenario.primary`
- manifest 状态：`partial`
- release 状态：`blocked`
- 唯一资产位置：Semantica package registry

本体、CQ 注册表、SPARQL、SHACL、正反案例、工程规则、exact oracle、manifest、
版本、PROV、receipt 与 release verdict 全部由上述包持有。本章不存在本地
`examples/`、`ontology/`、`eval/`、fixture、runner 或 fallback；旧路径只在
Semantica migration ledger 中作为哈希来源保留。

```bash
# 从 ontology-engineering skill 根运行
runtime/.venv/bin/python scripts/semantic_engagement.py discover
runtime/.venv/bin/python scripts/semantic_engagement.py run \
  --binding /path/to/package-binding.json \
  --task /path/to/task-envelope.json \
  --scenario semantica.vol2.ch01.scenario.primary
runtime/.venv/bin/python scripts/semantic_engagement.py open \
  --binding /path/to/workspace-binding.json \
  --task /path/to/task-envelope.json \
  --workspace /path/to/semantica-managed-registry
```

统一入口自动核验并注入项目 source lock 的 Semantica commit、版本与 wheel SHA-256。
`package-binding.json`、`workspace-binding.json` 与 `task-envelope.json` 必须按
[`semantic-engagement-contract.md`](../../semantic-engagement-contract.md) 建立。
`propose/commit/verify/history/promote` 继续引用该合同定义的 delta、candidate 与
gate-evidence fixtures，参数以各子命令 `--help` 为准。原生
`semantica package show/run/verify` 仅供底层 runner/manifest 诊断，不是主运行路径。报告时将
书中依据、scenario oracle 与独立 release verdict 分开；不得把 `partial`、
`blocked`、未运行检查或 unsupported 能力改写成通过。局部绿色只能按其主语解释，不能相加成产品可信。

ISO 术语或模态需要机器核对时，只能查询
`semantica.chapter_packages.vol2.normative` 中**已登记**的范围；该包目前只承诺
manifest 声明的部分工程释义/教学映射。未登记的分册、条文或表必须报告
unsupported/blocked 并回到合法持有的原文，不得补造；该包也不是官方解释、
合规意见或认证证据。
