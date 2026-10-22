# 项目接续样例与字段合同

这里是合成支架案例，没有真实企业、原始报告、账号或现场试验。用户可以说：“支架从 A 改到 B，温度新增 60°C，供应商报告换版；哪些决定要重审？”agent 整理对象、条件、来源和依赖，在 `var/projects/` 保存新修订；用户不用填写 JSON。

`history-cases/` 的七组 before/after 与 `oracles.json` 来自首次独立验证前固定的输入和预期，保留原字节。完全不变是负对照；对象、温度、来源各有单因素反例；三项变化有保留旧采用和主动转为待审两个版本。60°C 是新增绝对温度工况，室温具体数值未知；没有补造载荷、材料、允差或试验结果。

## 数据格式

编辑项目格式为严格 JSON `oev2.project/1`，拒绝未知字段。下表是 agent 整理要求；项目校验仅检查身份与结构，内容真实性及采用仍需证据。

| 字段 | 填写要求与含义 | Rust 映射与验收 |
|---|---|---|
| `schema` | 固定 `oev2.project/1` | `String`，精确格式检查 |
| `project_id` | 非空、跨修订稳定的项目身份 | `String`；接续不能换项目 |
| `revision` | 非空记录修订，不覆盖旧记录 | `String`；空修订拒绝 |
| `purpose` | 当前问题和目的；保留假设与未知 | `String`，非空 |
| `objects` | 对象 ID → 当前非空修订；同名对象不能隐式合并 | `BTreeMap<String,String>`；决定引用须存在 |
| `conditions` | 条件 ID → 有单位/适用范围的描述；未知要明示 | `BTreeMap<String,String>`；空描述拒绝；尚非结构化量纲校验 |
| `evidence` | 稳定来源 ID → 当前 SHA-256，64 位十六进制 | `BTreeMap<String,String>`；摘要是值，不要求摘要本身作为键 |
| `decisions` | 下表的决定列表；保留历史和当前结论 | `Vec<Decision>`；ID 唯一 |
| `dependencies` | 受影响目标 ID → 其依赖 ID 集合 | `BTreeMap<String,BTreeSet<String>>`；按前后两个基线求闭包，未声明关系未知 |

| 决定字段 | 填写要求 | 验收依据 |
|---|---|---|
| `id` / `object_id` | 非空稳定决定 ID / 已登记对象 ID | 重复或不存在对象拒绝 |
| `object_revision` | 该决定原本适用的非空修订 | 与当前对象不同且仍采用时标记陈旧 |
| `condition_scope` | 非空适用条件，未知不得伪造为已知 | 条件或范围变化要求重审；描述不等于证明适用 |
| `source_sha256` | 该决定所依据版本的 SHA-256 | 格式校验；失去当前来源绑定时撤销新鲜采用资格 |
| `conclusion` | `supported` / `refuted` / `unknown` | 历史支持与当前未知分别保存 |
| `adopted` | 当前工作记录是否沿用采用 | 仅 `supported` 且有非空依据可设 true；变化后仍需新审查 |
| `basis` | 来源/条件/前置决定 ID，无重复或空字符串；采用时须非空 | 缺采用依据拒绝；依赖改变影响后续决定 |

`adopted=true` 是输入中的采用记录，并非本工具签发的工程批准。当前 DTO 不承载新试验或新审查的完整证明；受影响决定不得借输入里改写的 supported 消除重审要求。

## 实际命令

从 skill 根执行；`--output` 使用新的路径，旧记录不可覆盖。

```sh
var/builds/rust-target/debug/oev2 project validate --input examples/v2-project/history-cases/unchanged-stable-source-ids.before.json
var/builds/rust-target/debug/oev2 project reconcile --before examples/v2-project/history-cases/temperature-only-retained-adoption.before.json --after examples/v2-project/history-cases/temperature-only-retained-adoption.after.json --output var/projects/bracket-temperature-impact-001.json
```

后一个命令应退出 1，并列出 `adopt-bracket`、`use-bracket` 的陈旧采用。退出 0 只表示本命令检查范围内没有继续保留陈旧采用，不是工程通过。Rust 测试 `project_history` 直接比较七组原预期，并保留历史输入身份；独立复核记录保存在本机私有维护记录中。
