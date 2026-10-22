# 第一个完整 Project：经验、数学工具、决定与反馈

本例让同一 Project 依次完成：公开经验回交 → 明确数学问题 → 冻结上下文 → 实际 Jacobian 求解 → 独立复核 → 登记结果 → 更新决定 → 保存使用反馈 → 新 session 回读。已在 rc.6 的实际本机安装中走通；发布后的空环境重装另行验收。

经验原文是公开合成的“离散观察不能证明连续路径”讨论。数学输入是 `2x+y=5, x-y=1`，仅用于演示工具结果接续。两者共享系统案例 Project，但数学结果不支持连续路径安全、物理适用性或方法收益。全过程不执行工程采用。

## 1. 使用本次安装和公开输入

先按 [README](../README.md) 安装本系统，再按 [工具安装](TOOL-INTEGRATIONS.md) 安装锁定的 Jacobian 并应用匹配版本的 diff。确认当前 Agent 读到了本仓库 `SKILL.md` 和 Jacobian skill。下面采用 Unix shell；Windows 的路径、散列和进程记录方式需按本机适配。

```sh
export ONTOLOGY_ENGINEERING_ROOT="$HOME/OntologyEngineering"
export OEV2_BIN="$ONTOLOGY_ENGINEERING_ROOT/var/install/bin/oev2"
export JACOBIAN_BIN="$ONTOLOGY_TOOL_ENVS/jacobian-0.23.0/bin/jacobian"
cd "$ONTOLOGY_ENGINEERING_ROOT"
"$OEV2_BIN" --root "$ONTOLOGY_ENGINEERING_ROOT" --version
```

`JACOBIAN_BIN` 必须是本机实际安装入口。核对 `var/install/INSTALLATION.json` 的根和八程序摘要，不能使用旧安装的回执。用对应 Python 环境的 `importlib.metadata.version("jacobian")` 核版本；记录实际入口、解释器、包版本以及相关运行环境。登记表核入口文件摘要，不代表自动核对整套 Python 环境。

先审阅 [memory 公开案例说明](../modules/engineering-memory/examples/README.md) 与 [Agent 审阅依据](../modules/engineering-memory/examples/AGENT-REVIEW.md)，然后使用新目录运行完整案例：

```sh
rustup run 1.95.0 cargo run --locked -j2 -p engineering-memory-runtime \
  --example agent_memory_loop -- --root "$ONTOLOGY_ENGINEERING_ROOT" \
  --output var/examples/first-memory
```

只有本次 `RESULT.json` 为 `matched`，且 14 次真实 CLI 调用成功，才继续。`--prepare-only` 没有这个资格。保留原始来源、r1/r2、每步回执及专用记忆库。

```sh
export BASE="$ONTOLOGY_ENGINEERING_ROOT/var/examples/first-memory"
export CASE="$ONTOLOGY_ENGINEERING_ROOT/var/examples/first-math"
mkdir "$CASE"
cp integrations/examples/linear-system.json "$CASE/linear-system.json"
shasum -a 256 "$CASE/linear-system.json"
```

以下 JSON 由 Agent 按真实文件准备。`<…>` 表示必须填入的本次值；不能原样提交。本文所称 `Ref(file)` 是 `{ "path": "相对于仓库根的路径", "sha256": "该文件原始字节的实际 SHA256" }`，不是 CLI 语法。工具结果中的文件引用必须使用这种根内相对路径；不填猜测摘要，不复用本文之外的私有回执。

## 2. 在同一 Project 中声明数学问题

以 `$BASE/project-r2.json` 为输入，准备**新的** `$CASE/project-r3.json`。这是 Agent 声明新对象和待决事项，不是结果登记或决定更新。保留旧文件、原 `project_id`、`path` 对象、`inspection` 条件、来源和记忆证据，作以下明确增加：

| 位置 | 新值 |
|---|---|
| `revision` | `r3` |
| `purpose` | 保留原目的，并注明增加精确数学结果回交演示，不推导连续路径结论 |
| `objects.example-loop` | `v1`；本次系统接续案例的根对象 |
| `objects.algebra` | `v1`；数学子案例 |
| `conditions.rational-system` | `QQ 上的 2x+y=5、x-y=1；纯数学教学数据，无物理单位或设备对应` |
| `evidence.linear-system-input` | 本次 `linear-system.json` 的 SHA256 |

向 `decisions` 增加：

```json
{
  "id": "D-linear-solution",
  "object_id": "algebra",
  "object_revision": "v1",
  "condition_scope": "rational-system",
  "source_sha256": "<本次输入摘要>",
  "conclusion": "unknown",
  "adopted": false,
  "basis": []
}
```

`$CASE/object.json` 使用同一输入摘要，注意对象合同的字段是 `revision`：

```json
{"object_id":"algebra","revision":"v1","condition_scope":"rational-system","source_sha256":"<本次输入摘要>"}
```

按 [路线锚定合同](../references/goal-anchoring.md) 准备本次 `route-before.json`：根对象为 `example-loop`；`path`、`algebra` 是该**软件接续案例**的两个组成部分，不能声明代数是路径安全模型。以 `A-memory` 保留经验边界，以 `A-linear-exact` 核验精确求解和结果回交；当前阶段 `S-math` 推进后者。逐字保存本次用户请求，不复制开发者的私人会话。

```sh
"$OEV2_BIN" --root "$ONTOLOGY_ENGINEERING_ROOT" project validate \
  --input "$CASE/project-r3.json"
"$OEV2_BIN" --root "$ONTOLOGY_ENGINEERING_ROOT" project route-audit \
  --input "$CASE/project-r3.json" --anchor "$CASE/route-before.json"
"$OEV2_BIN" --root "$ONTOLOGY_ENGINEERING_ROOT" project context \
  --input "$CASE/project-r3.json" --object "$CASE/object.json" \
  --output "$CASE/context.json"
```

路线为 `anchored` 后继续。`context.json` 是执行前冻结的完整 `ProjectReviewContext`；之后原样嵌入 ToolResult。其 `project_sha256` 是内核计算的审查输入摘要，不应替换为文件摘要。

## 3. 实际求解与独立复核

先发现并检查操作，再运行：

```sh
"$JACOBIAN_BIN" catalog > "$CASE/catalog.json"
"$JACOBIAN_BIN" inspect matrix.rational_linear_system.solve \
  > "$CASE/operation.json"
```

核当前操作是只读、有界方阵的 QQ 精确求解，接受公开输入中的有理数 `num`/`den` 字符串。若版本或合同不同，先停止接续并核 diff。

下面的命令须由 Agent 的进程执行工具记录实际 argv、cwd、UTC 开始/结束时间、退出码与 stdout/stderr 摘要，保存为 `solve-invocation.json`。文件名可不同，但后续引用须一致。

```sh
"$JACOBIAN_BIN" run matrix.rational_linear_system.solve \
  --file "$CASE/linear-system.json" \
  > "$CASE/solve.stdout.json" 2> "$CASE/solve.stderr"
```

只有退出 0 且原生返回 `outcome=UNIQUE`、输入回显与本次输入一致时才检验解。此固定输入的实测返回是 `x=2/1, y=1/1`；不能把这个已知答案当作新调用的输出。

把以下独立检查保存到 `$CASE/recheck.rs`，使用锁定 Rust 编译。它只检查本例整数值有理数的代入和非零行列式；不调用 Jacobian。

```rust
// Independent exact substitution for the owned integer-valued QQ fixture.
// Arguments are a11 a12 b1 a21 a22 b2 x y from the actual input and solver output.
fn main() {
    let args: Vec<i128> = std::env::args().skip(1)
        .map(|s| s.parse::<i128>().expect("expected exact integer"))
        .collect();
    assert_eq!(args.len(), 8, "expected complete input and observed solution");
    let mul = |a:i128,b:i128| a.checked_mul(b).expect("exact multiplication overflow");
    let lhs1 = mul(args[0],args[6]).checked_add(mul(args[1],args[7])).expect("sum overflow");
    let lhs2 = mul(args[3],args[6]).checked_add(mul(args[4],args[7])).expect("sum overflow");
    let determinant = mul(args[0],args[4]).checked_sub(mul(args[1],args[3])).expect("determinant overflow");
    assert_ne!(determinant, 0, "unique solution requires nonzero determinant");
    assert_eq!(lhs1,args[2],"first equation failed");
    assert_eq!(lhs2,args[5],"second equation failed");
    println!("{{\"schema\":\"oev2.exact-substitution-check/1\",\"status\":\"matched\",\"lhs\":[{},{}],\"rhs\":[{},{}],\"determinant\":{},\"scope\":\"exact integer-valued rational system only\",\"engineering_adoption\":false}}", lhs1,lhs2,args[2],args[5],determinant);
}
```

```sh
rustup run 1.95.0 rustc --edition=2024 "$CASE/recheck.rs" -o "$CASE/recheck"
```

参数依次为原输入的 `a11 a12 b1 a21 a22 b2` 和**本次实际返回**的 `x y`。逐一确认分母都是 `1` 后再传整数；否则应换成真正支持该有理数的独立检查，不能截断。

```sh
"$CASE/recheck" 2 1 5 1 -1 1 2 1 > "$CASE/recheck.json"
```

同时保留真实调用记录、源码和程序摘要。只有本次原生解确实是 `2,1` 时，上面这组实参才正确。独立输出应为 `lhs=[5,1]`、`rhs=[5,1]`、`determinant=-3`。保存 `qualification.json`：引用本次输入、操作合同、原生输出、独立检查与身份记录，只对这个明确操作/样例声明已检验。

## 4. 准备本机 registry 和 ToolResult

本例使用 `$CASE/registry.json`，不覆盖已有能力登记。准备 `oev2.capability-registry/1`，其中一项能力为：

```json
{
  "schema":"oev2.capability-registry/1",
  "capabilities":[{
    "name":"jacobian","display":"Jacobian","skill":"jacobian-math",
    "role":"Exact QQ linear system solve","outputs":["Native operation result"],
    "version":"0.23.0","executable":"<本次实际入口的绝对路径>",
    "executable_sha256":"<该入口文件实际摘要>","invocation":["cli"],
    "acceptance":"<qualification.json 的根内相对路径>",
    "acceptance_sha256":"<qualification.json 实际摘要>","status":"qualified",
    "limits":["仅本次声明的 QQ 样例；不覆盖全部后端、物理适用性或性能","入口摘要不覆盖整个解释器环境"]
  }]
}
```

`qualified` 必须来自上一步真实检查；缺检查就保持 `registered_unqualified`，不能用它支持决定。registry 命令只重核登记入口字节；外部资格记录仍需 Agent 审阅。

`$CASE/subject.json` 是实际输入清单：

```json
{"schema":"oev2.subject-manifest/1","files":[{"path":"<linear-system.json 的绝对路径>","sha256":"<实际摘要>"}]}
```

按 [工具结果合同](../references/professional-capabilities.md) 准备 `$CASE/tool-result.json`。完整结构如下；将 `Ref(...)`、上下文和真实执行记录展开为 JSON 对象，不能提交表达式：

| 字段 | 本例实际填写方式 |
|---|---|
| `schema`, `result_id` | `oev2.tool-result/1`，`first-project-jacobian` |
| `project` | 第 2 步 `context.json` 的整个对象 |
| `purpose` | 补足 `A-linear-exact` 的证据缺口 |
| `decisions`, `acceptance`, `route_stage` | `["D-linear-solution"]`、`["A-linear-exact"]`、`S-math` |
| `tool` | `{name:"jacobian",version:"0.23.0",skill:"jacobian-math",invocation:"cli",executable_sha256:<实际摘要>}` |
| `subject` | `{kind:"artifact",identity:"declared QQ matrix and rhs",manifest:Ref(subject.json)}` |
| `inputs` | `{arguments:<真实 argv 去掉程序>,conditions:[<原条件全文>],environment:[<无凭据环境说明>],files:[Ref(linear-system.json)]}` |
| `execution` | `{status:"completed",started_at:<实际UTC RFC3339>,finished_at:<实际UTC>,exit_code:0,cleanup:"Request-owned CLI exited; no service started"}` |
| `records` | 下表的每份原件；每项为 `{record_id,kind,artifact:Ref(file)}` |
| `findings` | 下列两条实际观察 |
| `measurements` | `[]`；本例不作性能比较 |
| `behavior` | 独立检查确实成功才写 `passed` |
| `coverage` | `{covered:["One declared QQ system; exact substitution and nonzero determinant"],not_covered:["Physical validity","Continuous path safety","General backends","Method benefit"]}` |
| `unknowns` | `["原 path 没有连续验证或工程采用","没有对照证明经验收益"]` |

| `record_id` | `kind` | 原件 |
|---|---|---|
| `solver` | `raw_output` | `solve.stdout.json` |
| `invocation` | `log` | 求解的实际进程记录 |
| `stderr` | `log` | `solve.stderr`，空文件也保留真实摘要 |
| `recheck` | `report` | `recheck.json` |
| `recheck-invocation` | `log` | 独立检查的实际进程记录 |
| `recheck-source` | `source_copy` | `recheck.rs` |
| `qualification` | `report` | `qualification.json` |
| `runtime-identity` | `report` | 本次入口/解释器/安装身份记录 |
| `operation-contract` | `report` | `operation.json` |

```json
[
  {"finding_id":"solver-unique","statement":"Native Jacobian returned UNIQUE, x=2/1, y=1/1","kind":"observed","basis":["solver"]},
  {"finding_id":"substitution","statement":"Independent exact substitution gives lhs=[5,1], rhs=[5,1], determinant=-3","kind":"observed","basis":["recheck","recheck-invocation","recheck-source"]}
]
```

这些观察只有在本次原件支持时才成立。内核核绑定、摘要和引用关系，不自动证明陈述的数学语义。

## 5. 经 CLI 登记和更新决定

```sh
"$OEV2_BIN" --root "$ONTOLOGY_ENGINEERING_ROOT" project tool-result registry \
  --registry "$CASE/registry.json"
"$OEV2_BIN" --root "$ONTOLOGY_ENGINEERING_ROOT" project tool-result verify \
  --project "$CASE/project-r3.json" --input "$CASE/tool-result.json" \
  --registry "$CASE/registry.json"
"$OEV2_BIN" --root "$ONTOLOGY_ENGINEERING_ROOT" project tool-result register \
  --project "$CASE/project-r3.json" --input "$CASE/tool-result.json" \
  --registry "$CASE/registry.json" --revision r4 \
  --output "$CASE/project-r4.json" --receipt "$CASE/register-receipt.json"
```

检查 `accepted`、`project_binding=matched`、`subject_status=manifest_matched`、`tool_status=qualified`，以及原件核验数量。r4 只增加证据与依赖，数学决定仍应为 `unknown`、`adopted=false`。

准备 `$CASE/decision.json`，将结果引用替换为实际根内路径和 SHA256：

```json
{
  "schema":"oev2.decision-update/1","decision_id":"D-linear-solution",
  "conclusion":"supported",
  "results":[{"path":"<tool-result.json 的根内相对路径>","sha256":"<实际摘要>"}],
  "findings":["first-project-jacobian#solver-unique","first-project-jacobian#substitution"],
  "rationale":"实际原生解满足原输入的两式，独立计算的行列式非零",
  "claim_scope":"仅此 QQ 方程组的唯一解为 (2,1)；不支持物理或连续路径结论"
}
```

```sh
"$OEV2_BIN" --root "$ONTOLOGY_ENGINEERING_ROOT" project decide \
  --project "$CASE/project-r4.json" --input "$CASE/decision.json" \
  --registry "$CASE/registry.json" --revision r5 \
  --output "$CASE/project-r5.json" --receipt "$CASE/decide-receipt.json"
```

r5 的数学决定应为 `supported`，`adopted` 仍为 `false`。禁止直接编辑结果、依据或采用状态来替代这两步 CLI。

在独立副本做三个负例，使用 `tool-result verify` 核每次退出 1、`rejected`：修改结果中的工具版本；复制并改变一个原始输出字节、引用新副本但保留旧摘要；只改变副本 Project 的数学条件。实测分别得到 `tool:version_mismatch`、`record:solver:tool_result_reference_drift`、`project_binding:condition_changed`。不改成功原件或降低判据。

## 6. 使用反馈回写，再创建新 session

先核原候选在新 Project 下的来源和原对象条件仍有效：

```sh
"$OEV2_BIN" --root "$ONTOLOGY_ENGINEERING_ROOT" project memory-candidate verify \
  --project "$CASE/project-r5.json" --input "$BASE/handoff.json"
"$OEV2_BIN" --root "$ONTOLOGY_ENGINEERING_ROOT" project memory-feedback \
  --project "$CASE/project-r5.json" --input "$BASE/handoff.json" \
  --id jacobian-scope-feedback --outcome no_delta \
  --result "实际求解并独立代入后，数学结果已登记和决定，adopted=false；候选已用于保留覆盖边界，数学结果不证明连续路径。无对照证明收益。" \
  --unknown "原 path 没有连续验证；没有记忆收益或工程采用证明" \
  --output "$CASE/feedback.json"
"$OEV2_BIN" --root "$ONTOLOGY_ENGINEERING_ROOT" memory --home "$BASE/memory" \
  session feedback --input "$CASE/feedback.json"
```

此反馈针对原来的 `sampling-boundary` 使用范围。原经验的历史文字不应被新数学调用改写；新事实保存在新 Project、工具原件和本次反馈中。`no_delta` 不表示“不曾使用”，而是没有证明收益。

从 `$BASE/session-second.json` 准备新的 `$CASE/session-third.json`，保留公开案例条件、查询及原 `path` 对象绑定，更新：

| 字段 | 新值 |
|---|---|
| `id` | 未使用的 `jacobian-followup-context` |
| `session_id` | 未使用的 `public-session-after-jacobian` |
| `task` | 回读同一 Project 的数学结果范围使用反馈；连续路径仍未知 |
| `project.path` | `project-r5.json` 的实际绝对路径 |
| `project.revision`, `project.sha256` | `r5`、r5 文件的实际字节摘要 |

```sh
"$OEV2_BIN" --root "$ONTOLOGY_ENGINEERING_ROOT" memory --home "$BASE/memory" \
  session prepare --input "$CASE/session-third.json"
```

从这次成功响应的 `data.context_sha256` 读取新摘要，保存为 `CONTEXT_SHA`，然后：

```sh
"$OEV2_BIN" --root "$ONTOLOGY_ENGINEERING_ROOT" memory --home "$BASE/memory" \
  session activate --id jacobian-followup-context --context-sha256 "$CONTEXT_SHA"
"$OEV2_BIN" --root "$ONTOLOGY_ENGINEERING_ROOT" memory --home "$BASE/memory" \
  session verify --id jacobian-followup-context
"$OEV2_BIN" --root "$ONTOLOGY_ENGINEERING_ROOT" memory --home "$BASE/memory" \
  session handoff --context-id jacobian-followup-context --context-sha256 "$CONTEXT_SHA" \
  --id sampling-boundary --revision 1
```

独立核对返回的 `data.project` 指向 r5；`data.candidate.feedback` 含 `jacobian-scope-feedback`；完整时间线仍含原错误与纠正，未知项和 `engineering_adoption=not_granted` 仍在。这里是新的 memory session 请求，未冒称客户端生命周期 hook 自动触发。

最后按路线 `/2` 合同把 `A-linear-exact` 关联到 `D-linear-solution` 和 `tool-result:first-project-jacobian`，经 `project route-audit` 核该项 `delivery_verified`。记忆候选本身不能关闭工程验收；其他未闭合项继续保留。保存本次每步参数、原件摘要、真实退出码和局限，下一次运行使用新目录与新 session 身份。
