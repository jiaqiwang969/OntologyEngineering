# 报告输入字段说明

本文件说明 `ontology-engineering.manufacturing-report/v1` 的完整输入接口。它是供记录投影使用的字段契约，不是工程本体规则或 Semantica binding。先准备来源与对象，再写记录和视图；不需要阅读生成器源码才能构造输入。

## 通用约定

- 文本字段填写非空字符串；未知信息用明确文字说明，例如“版次待确认”。金额和数量另按下面的 `null` 规则。
- 各集合的 `id` 为 1 至 64 位 ASCII 字符：首位字母，其余字母、数字、下划线或连字符。集合内不重复；对象、记录、关系、变化和费用行五类 ID 之间也不能重名。来源和视图有各自的 ID 空间。
- 引用字段是无重复的 ID 数组，引用的条目必须已经列入本输入。除视图选择的 `items` 可以为空外，下述来源、对象和影响引用都需要至少一项。
- 日期是展示字符串，建议 `YYYY-MM-DD`。报告版次和对象版次是各自独立的字符串，不用它们推断工程状态。
- 没有 `cost` 可省略或写 `null`；没有变化时 `changes` 可省略或写 `[]`。其他顶层字段须提供，暂时没有内容的集合可以写 `[]`，但 `views` 至少一项。

## 顶层与文档

| 字段 | 类型及要求 |
| --- | --- |
| `format` | 固定字符串 `ontology-engineering.manufacturing-report/v1` |
| `semantic_status` | 当前固定为 `not_executed_for_this_snapshot`；生成器不导入工程语义回执 |
| `document` | 下表列出的文档对象 |
| `sources`, `objects`, `records`, `relations`, `views` | 对应条目数组 |
| `changes` | 变化条目数组，选填 |
| `cost` | 一个同口径成本情景，选填 |

`document` 全部必填：

| 字段 | 含义 |
| --- | --- |
| `id` | 稳定文档 ID，遵守上述 ID 格式 |
| `revision` | 本次文档版次 |
| `title`, `date`, `audience` | 标题、日期与读者 |
| `project_state` | 当前项目或实物状态 |
| `evidence_scope` | 本报告实际使用的证据范围 |
| `decision` | 本轮需要回答的问题或支持的决定 |
| `footer` | 页脚文字，按实际采用/分发状态填写 |

## 来源、对象、记录与关系

| 集合 | 每条必填字段 | 填写规则 |
| --- | --- | --- |
| `sources` | `id`, `title`, `locator`, `excerpt`, `basis` | 题名、原件位置、收到的摘录、依据类型；转述或合成资料必须注明，不假装已看原件 |
| `objects` | `id`, `name`, `revision`, `state`, `source_ids` | 对象名称、版次与状态；来源引用 `sources` |
| `records` | `id`, `title`, `kind`, `statement`, `nature`, `status`, `conditions`, `source_ids`, `object_ids` | 类型、表述、事实/解释/提议等性质、状态和成立条件；分别引用来源与对象 |
| `relations` | `id`, `from`, `to`, `relation`, `reason`, `source_ids` | 两端只能是本输入的对象或记录 ID；文字写清关系含义、理由及来源 |
| `changes` | `id`, `before`, `after`, `reason`, `affected_ids`, `source_ids` | 新旧表述与原因；`affected_ids` 目前只能引用对象或记录，费用变化可关联其成本记录，不直接填费用行 ID |

`kind`、`nature`、`status` 等是依证据填写的展示文字，没有自动认定“已通过”的枚举规则。条件不因有来源引用而自动成立；冲突和未决范围要在记录中明写。

## 费用情景

`cost` 必填字段：

| 字段 | 类型及含义 |
| --- | --- |
| `currency`, `tax_basis` | 字符串；同一币种、含税/未税与适用范围 |
| `quantity` | 正整数，或 `null` 表示数量未知 |
| `quantity_basis` | 数量是订单件数、良品件数还是其他含义 |
| `scope`, `assumptions` | 所含/未含费用、假设、来源局限和已含项关系 |
| `source_ids` | 此口径的来源 ID 数组 |
| `lines` | 费用行数组；只对明确列出的行计算 |

每条费用行包含以下全部字段：

| 字段 | 类型及含义 |
| --- | --- |
| `id`, `name`, `nature` | 稳定费用 ID、名称、该金额是估算/实测/口头声明等性质 |
| `basis` | 仅 `per_unit`（按件）、`per_batch`（按批）、`one_time`（一次性） |
| `count` | 正整数；按件时必须为 1，按批或一次性时表示发生次数 |
| `value` | 十进制金额字符串或整数；`null` 表示未知。不要传浮点数、布尔值、NaN 或 Infinity |
| `object_ids`, `source_ids` | 对象和来源 ID 数组 |

按件费用乘情景数量；其他费用乘自身次数。金额或所需数量未知，该项不计入小计并列为未计入项。零表示数值已经明确为零，不能用来代替未知。金额显示到两位小数；需要特殊舍入、税费、良率或复杂摊销时先在项目核算工具中按受控口径处理，避免把展示计算当正式结算。

已含准备费或包装不能再重复列为新增金额。口头金额有内部矛盾时，写清条件和澄清问题，计算出来也只是条件算术。

## 视图

每个 `views` 条目必填 `id`, `title`, `question`, `kind`, `update_triggers`。`question` 是本节回答的问题；`update_triggers` 是新证据到来后何时重查的文字说明。

| `kind` | `items` 与呈现范围 |
| --- | --- |
| `records` | 必填 `items`，明确选择记录 ID；`[]` 显示尚无记录 |
| `relations` | 必填 `items`，明确选择关系 ID；`[]` 显示尚无关系记录 |
| `cost` | 不填 `items`；显示本情景全部费用，没有情景则显示未提供 |
| `changes` | 不填 `items`；显示本快照全部变化，没有变化则明确说明 |

工艺流程图使用独立的 [图形模板](process-flow-templates.md)，当前不支持在这里填写 `kind=flowchart`。

## 最小可运行输入

把下面内容另存到项目目录的 `report-input.json`，即可使用 [报告生成命令](report-design.md)。这是合成询问，尚无成本情景：

```json
{
  "format": "ontology-engineering.manufacturing-report/v1",
  "semantic_status": "not_executed_for_this_snapshot",
  "document": {
    "id": "DEMO", "revision": "R01", "title": "合成询问记录",
    "date": "2031-01-01", "audience": "业务与技术",
    "project_state": "资料待补", "evidence_scope": "一条合成询问",
    "decision": "目前还缺什么资料？", "footer": "教学示例"
  },
  "sources": [{
    "id": "S1", "title": "首次询问", "locator": "合成对话第1条",
    "excerpt": "想了解一个机械组件的制造方案，图纸尚未提供。",
    "basis": "合成教学输入"
  }],
  "objects": [{
    "id": "P1", "name": "待询机械组件", "revision": "待确认",
    "state": "尚未收到图纸", "source_ids": ["S1"]
  }],
  "records": [{
    "id": "R1", "title": "先明确对象", "kind": "资料缺口",
    "statement": "取得图纸、用途和数量后再选择路线。",
    "nature": "工作建议", "status": "待接续",
    "conditions": "本轮没有制造或价格结论。",
    "source_ids": ["S1"], "object_ids": ["P1"]
  }],
  "relations": [],
  "views": [{
    "id": "V1", "title": "本轮判断与接续", "question": "下一步需要什么？",
    "kind": "records", "items": ["R1"],
    "update_triggers": "收到图纸、用途或数量后更新。"
  }]
}
```
