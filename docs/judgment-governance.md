# 项目断言采用与模式迁移

此入口处理受控记录：来源字节、事实审阅、采用决定和 Semantica 审查分别保留。
Jev 台账继续只保存候选。采用入口不读取模型分数，也不接收外填的语义 PASS。
本版语义包为 `semantica.engineering.judgment-intake@0.2.2`；接收方先按
[方法初始化](method-bootstrap.md)执行锁定的五步原生版本链，再单独采用项目 binding。

## 断言采用

`scripts/judgment_admission.py` 默认只审查；只有显式传入 `--authorization`，且事实
审阅、原生检查、权限与精确输入全部匹配，才写出已采用项目记录及 ABox 投影。
项目 binding 的决定权威必须显式具备 `adopt-project-assertion` 范围。方法库的
commit/promote 权限不包含项目事实采用；这里不自动扩宽已有 binding。

请求合同 `ontology-engineering.assertion-admission/v1` 有以下字段：

| 字段 | 含义 |
| --- | --- |
| `admission_id`、`project_id` | 本次记录与项目的明确身份 |
| `assertion` | `id / statement / subject_revision / scope / kind`；kind 为 `reported_statement` 或 `engineering_claim` |
| `source` | 与批量输入共用的来源描述：`id / path / sha256 / media_type / selection / lineage_group / context_status` |
| `required_methods` | 本次受控审阅要求的方法 ID；工程主张不得为空 |
| `records` | 与同一主张、范围和修订对应的方法记录，每项为 `{path, sha256}` |
| `review_decision` | 事实审阅文件的 `{path, sha256}`；准备或只读缺口审查时可以为 null |

本版 `records` 使用判断包内的方法，例如身份、条件和变化检查。其他工程证据包
仍通过既有 `judgment_review.py record --bundle …` 入口执行，不把包间结果冒充此处
的自动义务覆盖。必查清单来自受控审阅范围，系统不会宣称已自动发现全部工程义务。

先准备请求并导出精确审阅对象：

```bash
runtime/.venv/bin/python scripts/judgment_admission.py \
  --subject-only --request /project/request.json --evidence-root /project \
  --binding /project/binding.json --output /project/review-subject.json
```

事实审阅合同为 `ontology-engineering.assertion-review-decision/v1`，包含：
`review_id / subject_sha256 / assertion_sha256 / source_sha256 / scope / subject_revision /
authority_id / authority_scope / origin / decision / source_relation / applicability_review /
reason / issued_at`。`subject_sha256` 来自上述导出；它包含来源选择、断言、必查方法、
方法记录和 binding 摘要。此处 `source_sha256` 是完整来源描述的摘要，原文件摘要
另外保留在 `source.sha256`；因此更换同一文件内的选择范围也会使旧审阅失效。

`origin=explicit_control_plane_decision` 表示调用方受控审阅，`decision` 可为
`accept / reject / pending`，`source_relation` 为 `supports / contradicts / not_established`。
事实权威和非空权限范围必须对应 binding 的事实权威。记录中的名字不构成身份认证，
调用方负责审阅人身份与委派依据，不能把模型生成的 JSON 自称为外部审阅。

`reported_statement` 的 statement 必须逐字等于所选来源，审阅范围为 `source_event_only`；
它只接受这段来源陈述。`engineering_claim` 需要 `controlled_engineering_review` 及
已声明义务的实际原生检查。原文“尚未试验”不能被引用成“试验通过”；只有来源哈希
正确不能越过这一区分。本文的合成案例没有独立审阅标签或实物试验结论。

将审阅文件的引用写回请求后，运行只读审查：

```bash
runtime/.venv/bin/python scripts/judgment_admission.py \
  --request /project/request.json --evidence-root /project --binding /project/binding.json \
  --workspace /project/registry --actor project-controller --output /project/review-001
```

`eligible_for_separate_adoption` 仍未采用。需要采用时，使用新的输出目录并另加
`--authorization /project/adopt.json`。采用合同
`ontology-engineering.assertion-adoption-authorization/v1` 明确包含
`authorization_id / action / actor_id / authority_id / binding_sha256 / request_sha256 /
review_decision_sha256 / reason / issued_at`，action 为 `adopt-project-assertion`。
它绑定最终请求文件和事实审阅文件的字节摘要，并在采用时重新执行原生审查。

结果保留 `adopted-assertion.json`、来源与决定、必要检查，以及 `adopted-abox.ttl`
及其投影依据。ABox 表达当前受控主张及依据角色；采用不是认证、产品放行或 TBox
晋升。旧目录拒绝覆盖；发生来源、配置或方法变化时另立记录并重审依赖。

## 模式拆分与合并

本版保留五个原模式及其问题。新增 `P-CLAIM-REPORT`、`P-CLAIM-ENGINEERING` 和
`P-CLAIM-COMPOSITION`，分别对应来源陈述、工程支持和显式选定变体的组合检查。
它们都有独立的 query、shape、CQ 和案例；拆分与组合不重定义旧 `P-CLAIM`。

`scripts/judgment_evolution.py subject` 导出确切迁移对象；`review` 使用同一
`semantic_engagement.review` 原生入口检查映射。调用示例：

```bash
runtime/.venv/bin/python scripts/judgment_evolution.py review \
  --mapping /project/mapping.json --evidence-root /project --binding /project/binding.json \
  --workspace /project/registry --actor project-controller --output /project/mapping-review
```

映射合同 `ontology-engineering.pattern-migration-review/v1` 包含
`mapping_id / project_id / source_version / target_version / kind / relation / sources /
targets / review_decision`。sources/targets 是已定义模式的 ID 列表；目标必须是当前绑定
版本。split 使用 `decomposes_review`，merge 使用 `composes_review`；两个变体在同一
版本内组合，也必须保留不同模式身份并明确审阅。

审阅合同 `ontology-engineering.pattern-migration-decision/v1` 包含
`review_id / subject_sha256 / actor_id / authority_id / action / state / reason / issued_at`。
subject 摘要取自 `subject` 输出；action 为 `review-pattern-migration`，state 为
`reviewed` 时才表示已经审阅。决定权威须有对应权限范围。

工具从锁定的原生版本链读取旧、新定义，逐项对比目录项、输入合同、query 与 shape
摘要。Semantica 检查声明的映射义务；传入 `equivalent` 或 `sameAs` 仍不能得到等价
结论。输出的 `clear` 不自动迁移项目应用，也不证明逻辑保守扩展。应用选择哪个变体、
在什么范围重新采用，需要自己的来源和决定。
