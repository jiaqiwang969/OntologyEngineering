---
name: leantoken
description: Retrieve bounded source context and locate implementations or tests in an explicitly selected local repository using LeanToken.
---

Use `leantoken` pinned to 0.1.28. Always pass `--root /absolute/repo`. Never index the home directory or infer an authorized global MCP workspace from the shell startup directory.

Read `leantoken --help` and the command help for exact contracts. Run `leantoken --root REPO --json index` before first direct CLI retrieval, then `context --task TEXT --budget 2200`. Start with one context request; make another search only for a named evidence gap. Confirm repository identity and generation in `status` and the result; paths are repository relative.

After edits run `index` explicitly, then verify a changed range and the new generation. `--consistency indexed_generation` preserves the indexed generation boundary, but read can return live file bytes with that older generation; inspect live_bytes_read/content hashes and explicitly reindex after changes. Do not equate generation alone with the returned bytes. CLI 0.1.28 defaults to reconcile_working_tree for context/read, unlike the generic MCP default documented in README.

Save invocation, fixed binary identity, exact outputs, generation, scope, failures and unknowns. Retrieval is not test execution or engineering adoption. No persistent service is required. Sources: https://github.com/morluto/leantoken/tree/v0.1.28 and its README.md/docs/usage.md; this is a locally maintained skill, not an official managed skill.

## 与当前 v2 Project 接续

本工具承担：源码范围与新鲜度；定位结果须回读原文件，不能代替功能测试。

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
