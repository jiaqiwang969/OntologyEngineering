# 接收方初始化与单独采用

使用 `scripts/method_bootstrap.py`，在明确授权的新目录重建冻结方法包的精确版本链。
本版运输数据覆盖 `engineering-judgment-intake` 和 `engineering-evidence-methods`；不宣称能初始化任意方法包或恢复
任意 Semantica runtime。需要先按分发说明安装锁定运行时。

初始化保留方法作者的 source authority，接收方重新执行案例并生成自己的任务、
commit/promote 授权和原生回执。作者身份表示方法来源；原组织的批准和项目事实不会
随包转移。脚本不认证操作者身份，授权依据由调用方的实际任务或组织控制面提供。

## 准备可审阅计划

在 skill 根目录执行；工作目录必须尚不存在：

```bash
runtime/.venv/bin/python scripts/method_bootstrap.py prepare \
  --directory /path/to/new-method-workspace \
  --project judgment-intake-maintenance --domain discrete-manufacturing \
  --actor recipient-operator --fact-authority recipient-controlled-records \
  --evidence-root evidence:recipient
```

上面的项目名用于包内合成演示；真实使用须填写对应项目身份。`prepare` 生成计划、
初始方法库绑定和项目绑定模板，不创建原生 registry。检查输出中的计划摘要及范围。

调用方把现有授权依据写到私有 JSON 文件；下面的占位值必须换成实际输出与记录。
这份文件不代表人的数字签名，也不新增超出当前任务的权限。

```json
{
  "schema": "ontology-engineering.method-bootstrap-authorization/v1",
  "action": "initialize-frozen-methods",
  "actor_id": "recipient-operator",
  "plan_sha256": "<prepare 返回的 plan_sha256>",
  "reason": "<当前任务或组织授权依据及适用范围>",
  "issued_at": "<带时区时间>"
}
```

```bash
runtime/.venv/bin/python scripts/method_bootstrap.py apply \
  --directory /path/to/new-method-workspace --authorization /private/initialize.json
```

每个版本先运行真实作者案例，再通过根 `semantic_engagement` 的
`open/propose/commit/verify/promote`。两类 committed-subject gate 由 Semantica 内部执行
并推导，不导入外部 PASS。每次原生晋升只产生 successor binding；初始化授权同时明确
允许另存本地方法库绑定，以便继续下一版。旧绑定、原生事件和来源始终保留。

输出的 `proposed-project-binding.json` 尚未采用。项目绑定使用接收方事实源，支持审查
和非权威候选；由于当前 native workspace 合同要求候选 prefix，也包含 `propose`。
它没有 `commit/verify/promote` 权限，不能因此接受工程事实或放行产品。

## 单独采用并运行项目审查

另立授权 JSON，`action` 改为 `adopt-project-binding`，保留同一个 `plan_sha256`，
增加 `binding_sha256`（取 `apply` 输出），并记录本次采用理由和时间。

```bash
runtime/.venv/bin/python scripts/method_bootstrap.py adopt \
  --directory /path/to/new-method-workspace --authorization /private/adopt.json
runtime/.venv/bin/python scripts/judgment_review.py plan \
  --input examples/judgment_intake/cad-review-plan.json \
  --evidence-root examples/judgment_intake \
  --binding /path/to/new-method-workspace/project-binding.json \
  --workspace /path/to/new-method-workspace/registry \
  --actor recipient-operator --output /path/to/new-review-output
```

合成演示会查出旧版本报告和未重审报价的缺口；出现 `blocked` 是对应检查结果，
不能为了展示绿色而删去反例。`clear` 仍只表示有界记录检查，模型质量和实物结论另验。

## 中断与配置变化

同一初始化使用原计划和原授权重试。脚本核对原生事件里的 exact context，保留原任务，
已完成动作不会被新时间或新请求冒充。原生晋升的跨账本恢复仍由 Semantica 处理。
本地辅助日志不作为完成凭据；不复制或修改 CAS、events、registry。

代码、运输包或 runtime 改变时，旧计划明确阻断。保留旧目录和执行记录，在另一个目录
建立新计划，或使用经过验证的冻结环境恢复；不在恢复中混用版本。
当前支持范围为 POSIX 本地文件系统与已锁定运行时；Windows 原生执行未验收。

## 复用工程证据方法

初始化时加 `--bundle engineering-evidence-methods`，在另一个目录重建已有的 19 项
工程证据方法包，再按相同步骤单独采用。判断模式与专门证据方法保留各自精确包绑定，
可以属于同一个项目；不能把一个包的 PASS 用作另一个包的执行回执。

单项源记录使用统一审查入口，例：

```bash
runtime/.venv/bin/python scripts/judgment_review.py record \
  --bundle engineering-evidence-methods \
  --input examples/judgment_intake/method-review/quote-as-settled-price.json \
  --evidence-root examples/judgment_intake/method-review \
  --binding /path/to/evidence-method-workspace/project-binding.json \
  --workspace /path/to/evidence-method-workspace/registry \
  --actor recipient-operator --output /path/to/new-cost-review
```

[组合反例清单](../examples/judgment_intake/method-review/cases.json)覆盖数量及成本口径、
同源转发、共同模型依赖、重放冒充新采集和放宽判据冒充改善。
这些是有出处的合成输入和开发者参考，不是实际账单、独立准确率标签或物理测量。
