# 显式命名图 SHACL 检查

`kernel validate-named --input REQUEST.json [--output RESULT.json]` 显式选择一张命名图，沿唯一 DatasetSession 和原 SHACL 检查器执行。它不把默认图或相邻命名图的数据合并进来；某图缺少必要属性时，别处同一对象的属性不能替它通过。

请求为 `semantica.named-graph-validation-request/1`：`data` 是完整 TriG/N-Quads 原文，`data_sha256` 是原文 UTF-8 SHA256，`dataset_format` 必须为 `trig` 或 `nquads`；`shapes` 是完整 Turtle 形状原文并有独立 `shapes_sha256`；`graph_name` 是必填的绝对 IRI。没有隐式格式、默认选图、IRI 修复或图名覆盖加载。

合法但没有三元组的图名产生空视图。`sh:targetNode` 的必要属性检查仍执行；`sh:targetClass` 可能没有目标。结果保留 `selected_quad_count`、原 report 的 `focus_count` 和全部 findings；空覆盖下 conforms 不是工程证据。形状的结构预检、约束停用规则及原不支持表达的拒绝仍执行。

结果 `semantica.named-graph-validation-result/1` 绑定完整请求 canonical JSON SHA、原 data/shapes SHA、图名，以及实际 `revision_before`/`revision_after`。新端口先创建其本次数据会话并加载原数据，再作不可变验证，两次 revision 都是加载之后的版本。直接调用 `DatasetSession::validate_named_graph` 借用现存会话，不修改原四元组、版本、包登记或缓存，不调用 RDFC 快照，不重命名已加载的 blank node。临时默认图只是只读评价视图，不是新事实库。

解码后的 data 和 shapes 仍各限 16 MiB，图名使用既有 MAX_BYTES 的显式 16 MiB 界；完整默认 Dataset 的 100000 四元组及 16 MiB 序列化预算在选图前检查。context50k 不自动扩大此 SHACL 范围。原 SHACL 访问次数、路径步数、嵌套深度、本地查询和拒绝条件不变。

新 JSON 运输上限为 `6 × (16 MiB + 16 MiB + 16 MiB) + 4096`，即 288 MiB + 4 KiB；六倍覆盖 JSON 转义，另保留固定 envelope 空间。它使两份原先各合法 16 MiB 的原件可同时运输，不扩大任何解码后或评价预算。新 CLI 使用这一专用读取上限，再复用原严格 JSON 和文件 owner；旧 `json_input` 及 `kernel validate --data --shapes` 不变。验证不符合时仍保存完整报告并 exit 1，解析、来源或预算错误不返回 conforms。

原 Python `SemanticRuntime.validate(graph_name=...)` 是本次显式选择的来源；其 `graph_name=None` 将整个 Dataset 交给 pyshacl，含义不能擅自改成默认图或 union。现有 Rust 默认 `validate_dataset` 遇命名图仍拒绝；BookRunner 原步骤没有向原 runtime 传 graph_name，当前 promoted review 的 partial/unknown 反例继续保留。未来接入两者需要明确的新合同，不从旧忽略字段激活新行为。

正本为 `specs/modules/semantic-named-graph-validation.toml`；ggen 生成中性合同，RustSemanticRuntime、DatasetSession 与现有 SHACL owner 实际执行。新单图用例预期从锁定原源码独立推导，不能称为原 Python 实跑观测。完整 Semantica、完整 SHACL、多种形状格式、原全部推理选项、无显式选择的整 Dataset 语义及工程采用继续分别验收。
