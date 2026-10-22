# 有来源的文本规则推导

`kernel reason --input REQUEST.json [--output RESULT.json]` 通过生成的 `SemanticReasonPort` 调用同一个 Rust 文本规则执行器。规则可以保留原 `ForwardRuleDTO` 的 `rule_id`、`text`、`confidence` 和 `priority`；推导结果能回答“这一结论先由哪条规则产生、依据哪些前提”。它不证明前提真实，也不代替当前 Project 的工程审查和采用决定。

```json
{
  "schema": "semantica.reason-request/1",
  "facts": ["Loaded(part)"],
  "rules": [
    {
      "rule_id": "source-review-required",
      "text": "IF Loaded(?x) THEN NeedsReview(?x)",
      "confidence": 0.8,
      "priority": 10
    },
    "IF NeedsReview(?x) THEN PendingAcceptance(?x)"
  ],
  "max_iterations": 50
}
```

`facts` 可以是多行文本或有序字符串数组；数组每项必须归一化为一条非空、非注释事实。`rules` 可以是多行文本或混合字符串与规则对象的有序数组。对象必须有 `rule_id` 和 `text`，仅省略 `confidence`、`priority` 时使用原默认 `1.0`、`0`，未知字段拒绝。结构化 JSON 对象是原 `ForwardRuleDTO` 的显式传输表示，不是运行 Python 对象。

`priority` 必须是 JSON 整数，允许现有输入预算内的任意精度；`true`、`1.0` 或 `1e0` 不作整数接纳。按数值降序稳定排序，同优先级保持输入顺序。只有字符串规则消耗 `rule_1`、`rule_2` 等默认编号，显式 ID 不增加计数。重复 ID 或解析后相同的条件/结论均拒绝，不静默合并。规则 ID 不允许前后空白。

`confidence` 必须是有限的 `[0,1]` 数值，布尔值拒绝。它保留作者的元数据含义，不是测量可靠度、工程合格概率或事实过滤门槛。同一轮多个规则导出相同结论时，保留首条规则的 ID 和 confidence，并按原顺序合并前提；不计算新的组合概率。

成功结果外层为 `semantica.reason-result/1`，含整个原始类型化请求的 `request_sha256` 和 `data`。摘要在默认补齐、规则排序之前计算；对象键采用既有 canonical JSON 规则，数组顺序及任意整数值保留。`data` 保留原 `semantica.textual-forward-chain.v1` 的 `input_facts`、排序后的规则、推导详情、`all_facts`、`max_iterations` 和 `complete`。执行首先按原轮数推导，再执行原收敛探针；还有新事实可推出则拒绝报告 complete。

该入口沿现有 CLI 的实际编译身份检查、私有输入读取与输出规则执行。完整类型化请求先以既有 ordered JSON 序列化核对 1 MiB 上限，再计算完整 canonical 摘要。原执行器的 4096 事实、256 规则、每条 4096 字节文本、每规则 32 条件、1000 轮及匹配/绑定/工作内存预算仍适用；超界和失败通过 `SemanticRuntimeFailure` 返回。它不创建 Dataset、Project、registry 或模型服务，也不改变现有数据会话的版本和缓存。

教材 BookRunner 的原 `semantica-fact`／`semantica-rule` 字符串资产包装委托同一个执行器，保留资产摘要、原输出包装和既有十项固定预期。它继续按原事实和规则的 UTF-8 原文合计核 1 MiB；新端口按完整 JSON 请求核 1 MiB，不把新运输开销套入旧资产边界。旧 MCP `run_reasoning` 继续使用原 `Reasoner.infer_facts` 默认 50 轮与 `derived_facts` 响应；旧 `kernel rules` 继续其原有一元 Horn 规则合同。这两条入口不借本次增量改变语言或完成含义。

正本为 `specs/modules/semantic-rule-metadata.toml`，由 `specs/engineering.toml` 登记；锁定 ggen 生成中性 DTO/port，`semantica-runtime::reasoner` 执行，`oe-kernel::rules` 与 CLI 只负责接入。原始参照为锁定 Semantica `0.6.5+oe.7` 的 `ForwardRuleDTO`、`SemanticRuntime.reason` 和 `Reasoner`。40 项结构化元数据用例的预期从该原始源码独立推导，不是原 Python 运行观测；任意大整数与资源边界另有独立测试。原字符串 oracles 分别保留和核对。本项不宣称完整 Semantica、Python 自定义处理器、Unicode 变量名、SWRL、OWL/DL、否定、算术内建或工程采用已完成。
