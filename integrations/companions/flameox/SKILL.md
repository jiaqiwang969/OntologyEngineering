---
name: flameox
description: Analyze runtime artifacts or capture explicitly authorized local workloads with Flameox, preserving native evidence, budgets, replay, and cleanup.
---
# Flameox runtime evidence

Locally maintained supplement sourced from https://github.com/morluto/flameox/tree/v0.2.9 (installed PyPI flameox 0.2.9); consult the installed `flameox mcp inspect --tool NAME` for exact schemas. Use `flameox`; MCP arguments are `mcp serve`.

1. Prefer read-only analysis of existing explicit absolute artifacts when it answers the question. Capture runs trusted local programs with your permissions; use it only within the user's current task authorization.
2. Inspect the named operation schema. For capture bind exact argv, absolute cwd, selected provider, baseline/candidate identities, input set, and a behavior oracle. Do not convert shell text into an implicit execution target.
3. Freeze three separate budgets: analyzer `limits`, each workload's `target.budget`, and entire experiment deadline. The small acceptance workload uses 30 seconds and 536870912 bytes; these are not defaults for large analyses. Record memory sampling limits.
4. Save original result bytes, evidence ID, environment, inputs, failed/partial attempts, and conditions. Observation, derived metrics, and causal inference remain distinct; profiles alone do not prove speedup or correctness.
5. Preserve evidence before process exit. Reopen with the same `FLAMEOX_DATA_DIR`, call `inspect_evidence`, and pass `analysis_sources` unchanged to the named analysis tool. Replay must not call capture. Submit exact `next_page` handoffs for pagination.
6. On timeout retain partial evidence and cleanup receipts. Check owned child PIDs have exited; terminate only task-owned leftovers. Close stdio and release runtime scratch. Do not add persistent services or restart loops. Retain immutable evidence and raw artifacts.

Save deployment evidence and receipts in your own private tool directory; previous installations do not qualify this installation. Tool handoff manifests are not native v2 contracts. Supply facts/evidence to Project via its existing contracts; memory records process and actual later use.

## 与当前 v2 Project 接续

本工具承担：采集证据是否足够支撑运行观察；保留部分结果、预算、重放身份和清退。

当任务使用 v2 时，调用前绑定实际业务对象、修订、条件和待决定事项；调用后按 `$ONTOLOGY_ENGINEERING_ROOT/integrations/bindings/WORKFLOW.md` 接收原生证据、独立核查、声明依赖并执行原生变更审查。工具版本不代替业务对象版本，安装验收不自动成为业务证据。此节是本地补充，上游 Skill 原文身份保留。

## Portable OntologyEngineering entry

For an OntologyEngineering task, resolve the user's absolute
`ONTOLOGY_ENGINEERING_ROOT` and read `integrations/bindings/WORKFLOW.md` and
`references/professional-capabilities.md` under that root. Keep the selected
Project, object revision, conditions and pending decision. Use the portable
`verify_candidate.py`, `reconcile_reviewed.py` and `resume_memory.py` there when
that workflow calls for them; native Rust remains the semantic owner.
Return native results through `oev2 project tool-result`, and actual process and
usage feedback through `oev2 memory`. Registration does not grant adoption.
This project-maintained addition requires no hosted judgment service.
