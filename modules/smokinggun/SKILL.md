---
name: smokinggun
description: Use SmokingGun to find complexity candidates and organize imported optimization evidence.
---

# SmokingGun

Use SmokingGun as a local evidence boundary for complexity and performance work. It scans repository source and context, imports external findings, and compares immutable measurement artifacts. It does not execute repository workloads or edit source files.

## Start with a scan

Run the first action from the repository under review:

```bash
smokinggun scan .
```

The default runtime source profile suppresses candidates from tests, documentation, examples, and fixtures while
keeping those files visible in repository inventory. Use `smokinggun scan . --source-profile all` when the task calls
for candidates from every supported source file, or `--only <path>` to inspect one auxiliary area explicitly.

If the `smokinggun` command is unavailable, do not treat that as a scan with no findings. Tell the user that SmokingGun could not be run and ask for authorization to use one of these recovery paths:

```bash
npx --yes --package=smokinggun -- smokinggun scan .
```

This is a one-command bootstrap: it may contact the npm registry, but it does not alter the repository or create a global installation. After the user authorizes it, run the exact command from the repository under review.

Or, for a reusable installation:

```bash
npm install --global smokinggun
smokinggun scan .
```

Do not install npm packages implicitly. If either recovery path fails, report the exact command and its diagnostic, keep SmokingGun coverage as unknown, and suggest checking Node 22.18+ and registry access. Do not substitute a failed or unavailable scan with an unsupported performance claim.

## Interpret evidence carefully

Keep these categories separate in every report:

- Static findings are observations from source, repository context, or imported artifacts.
- Estimates are theoretical consequences of stated assumptions, such as a likely repeated lookup inside a loop.
- Measurements are empirical results for a declared benchmark plan and runtime environment.
- Behavioral checks establish whether a proposed change preserves observed behavior; they do not prove all behavior is preserved.
- Unknowns include missing coverage, unresolved types, unavailable tools, unrun benchmarks, and unverified assumptions.

A finding is a candidate, not proof of a universal speedup or asymptotic bound. Missing or failed coverage must remain visible and must not be reported as clean coverage.

## Investigate and compare measurements

Create a durable investigation bundle when it will help organize evidence:

```bash
smokinggun investigate <path>
```

SmokingGun does not execute repository workloads. Produce benchmark and profile artifacts with the repository's existing tools, then import or compare those immutable artifacts through SmokingGun.

Keep baseline and candidate measurements tied to their benchmark plan, subject digest, shared input-set digest, executable, environment, artifacts, and behavior checks. Use `smokinggun compare <baseline> <candidate>` only after checking that both sides are comparable. Report what was not measured and which assumptions remain unresolved.

## Safety and recovery

Static scans are read-only, offline, and non-executing. SmokingGun does not apply automatic source edits. Do not modify source, configuration, dependencies, or generated artifacts unless the user explicitly asks for that change.

Use machine-readable output when handing results to another tool:

```bash
smokinggun scan . --format json
smokinggun scan . --format markdown
smokinggun scan . --format sarif --output smokinggun.sarif
```

Operational CLI diagnostics belong on stderr. Evidence and coverage diagnostics remain fields in the single JSON or SARIF document on stdout. Preserve nonzero exit codes and incomplete coverage when deciding whether a result is usable.

## Studio supplement

Read [LOCAL.md](LOCAL.md) for pinned runtime and handoff rules.

## 与当前 v2 Project 接续

本工具承担：候选测量与性能采用；静态依据、真实测量、行为保持和采用判据分别核对。

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
