# RUNBOOK DELTA — 供 `references/fusion-execution.md` 与调用方指南直接采用的文本

下面三段是可直接粘贴的成稿。段 A 替换/扩写 `references/fusion-execution.md`
的“已知易错点”与“调用方式”；段 B 追加到
`references/runbooks/fusion-mcp-caller-guide.md` 的“调用方必须遵守的规则”；
段 C 是新增的“结果分类法”小节，两处都应引用。

---

## A. `references/fusion-execution.md` —— 替换「调用方式」输出说明 + 「已知易错点」两条

### A1. 替换第 42–43 行（输出说明）

> 输出为 JSON：`classification`（见下表）+ `payload`（Fusion 的内层结果原样）+
> `raw_text`（`content[0].text` 原文，永远保留）+ `inner`（脚本 stdout 末行解析出的对象）。
> 退出码：**0 = 已送达**（还要读 `classification` 才知道成没成）；**2 = 传输/会话失败或上游超时**；
> **3 = 用法错误**；**4 = 传输说成功但没有可用结果**（`no_output` / `response_truncated`）；
> **5 = 命中 `--fail-on` 指定的分类**。
> 失败时 stderr 额外带 `--- cell stderr (tail) ---`，cell 自己的启动错误就在那里
> （例如 `timeout_seconds must be finite and between 0 and 300`）。
>
> **不要再自己解析 `payload`。** 用 `scripts/fusion_ops.py` 里已测的
> `call_read` / `call_execute_script`，它们返回同一套分类。

### A2. 替换第 144–147 行（非 ASCII 字面量）——新规则 1

> - **非 ASCII 字面量由工具转义，不许人手写。**
>   `fusion_mcp_execute` 脚本里任何非 ASCII 字面量（中文路径、中文文档名、注释）
>   在到达 Fusion 之前必须已经是 ASCII-only 的 Python Unicode 转义
>   （例如 `"无标题"`），回读时再拿 Fusion 返回的原生 Unicode 比对。
>   原因是链路的最后一段（cell → Fusion 的 loopback HTTP）发的是**裸 UTF-8** body 且
>   `Content-Type` 不带 `charset`；Fusion 侧一旦不按 UTF-8 解码，脚本字面量就变成 mojibake，
>   出现“界面名称明明对、脚本精确比对却失败”“STEP 路径明明在、
>   `createSTEPImportOptions` 报 `3 : The selected file does not exist.`”。
>   **做法**：所有脚本都经 `scripts/fusion_ops.py` 发出
>   （`call_execute_script` 会自动调用 `ascii_safe_script`），
>   或先手工跑一次 `python3 scripts/fusion_ops.py escape <script.py>`。
>   该转换只改字符串字面量与注释，并用 AST 相等性证明语义未变；
>   遇到非 ASCII 标识符或 raw f-string 会**拒绝**而不是硬改。
>   `plan.json` 看起来是纯 ASCII（`json.dumps` 默认 `ensure_ascii=True`）**不代表已经安全**，
>   MCP SDK 在下一跳就会把码点还原成裸 UTF-8。

### A3. 新增一条「已知易错点」——新规则 2

> - **脚本源代码单行不得超过 2000 字符（多行字面量规则）。**
>   2026-09-06 18:21，一个 10 KB 脚本因为 `PARAMS` 被写成一条约 8 KB 的单行，
>   Fusion 返回 `success=true`、`message=""`、AppLog 记 `duration=29ms`，
>   **一条参数都没建**；同一脚本改成 `json.dumps(indent=1)`（最长行 253 字符）后正常跑完 42 s。
>   我们这一侧（caller → cell → HTTP）经实测对该脚本字节无损，
>   所以截断发生在 Fusion 内置 `mcp_execute_script` 里，不可控。
>   **做法**：大字面量一律用 `fusion_ops.render_script(template, **subs)` 生成
>   （占位符 `{{NAME}}`，自动折行 + 自动 ASCII 转义），
>   或自己 `json.dumps(..., indent=1)`。`fusion_ops` 会在发出前本地拒绝超长行。
>   **推论**：`success=true` 但脚本 stdout 为空 = **脚本没跑**，不是“跑成功但没打印”。
>   `fusion_call.py` 现在对此退出码 4（`no_output`），必须当硬失败处理，
>   并且在做任何后续动作之前先独立回读一次文档状态。

---

## B. 追加到 `references/runbooks/fusion-mcp-caller-guide.md` 的「调用方必须遵守的规则」

> 7. **一律走 `scripts/fusion_ops.py`，不要再手写驱动逻辑。**
>    2026-09-05/06 的四个驱动各自重写了：guard-transient 重试、`message` → 末行 JSON 解析、
>    文档名/lineage 身份校验、“脚本无输出”检测、ASCII 转义、精确文档搜索 ——
>    四份实现四种缺陷。现在只有一份、带测试：
>
>    ```python
>    from fusion_ops import FusionOps, RetryPolicy
>    ops = FusionOps(receipts_dir=OUT, retry=RetryPolicy(attempts=6, delay_seconds=45.0))
>    ops.assert_active_document(NAME, LINEAGE)          # 身份先于一切写操作
>    outcome = ops.call_execute_script(script_path, stem="A8_parameters")
>    if outcome.classification != "delivered_ok":
>        raise SystemExit(outcome.message)              # 绝不把非 delivered_ok 当成功
>    ```
>
>    也可当 CLI 用：`fusion_ops.py script A8.py --receipt-dir . --stem A8_parameters`、
>    `fusion_ops.py search-exact <NAME>`、`fusion_ops.py escape <script.py>`、
>    `fusion_ops.py classify <report.json>`（离线复判历史报告）。
>
> 8. **重试只允许发生在 upstream 之前的守卫拒绝上，且必须是全新的一次性 cell。**
>    唯一可重试的分类是 `guard_transient`（例如
>    `Fusion module-loading auxiliary is not uniquely stable`、窗口 census 类拒绝），
>    默认上限 6 次、间隔 45 s，可配置。
>    `upstream_timeout`、`transport_failure`、`inner_script_error`、`no_output`
>    **一律不得重试**：上游可能已经提交，重发就是二次写入。
>    `RetryPolicy` 在构造期就拒绝把这些分类放进可重试集合，写不出来。
>    超时之后的纪律不变：同一 Fusion PID 不得再收到任何 MCP 请求，
>    包括只读确认、健康探针、save/close/quit 与新建/删除 session。
>
> 9. **`document/search` 是模糊匹配，必须做精确过滤。**
>    2026-09-06 19:31 实测：搜 `FA04_V019_TROLLEY_6P84P_W850_REBUILD_V0_1`
>    返回 `Found 1 document(s)`，而那一条是 `FA04_V018_TROLLEY_6P84P_REBUILD_V0_1`。
>    信 `count` 就会把工作做进错的文档。用 `ops.exact_document_search(name)`
>    （按 `name` 逐字节相等过滤），命中数不是 1 就 HOLD。
>    写操作之前再用 `ops.assert_active_document(name, lineage)` 证明活动文档身份；
>    需要切换时用 `ops.activate_document(name, lineage)`：
>    激活与回读是两个独立的一次性 cell，中间在 MCP 之外 settle。
>
> 10. **失败报告必须留证。**
>     `ops.call(..., stem="A8_parameters")` 配合 `receipts_dir` 会写全
>     `.args.json` / `.py` / `.result.json` / `.stderr` / `.exit` 五件套，
>     其中 `.result.json` 含 `classification`、`raw_text`、`cell_stderr_tail`。
>     以前 `A6_import_step.stderr` 只有 129 字节的 `"call": {}`，真因全丢；
>     现在 cell 的启动错误会原样落盘。

---

## C. 新增小节：结果分类法（两处引用同一张表）

> ### 结果分类法（`fusion_ops.TAXONOMY`）
>
> 每次调用都会得到 **恰好一个** `classification`，并且始终保留原文 `raw_text`：
>
> | classification | 含义 | 退出码 | 允许重试 | 驱动应做什么 |
> |---|---|---|---|---|
> | `delivered_ok` | 送达且内层 `success=true` | 0 | — | 继续 |
> | `inner_business_error` | 脚本/工具跑完，报告业务性失败 | 0 | 否 | 读 `inner`，按业务分支 |
> | `inner_script_error` | 脚本抛异常（带 `trace`）或 Fusion 报 `success=false` | 0 | 否 | 修脚本；文档状态可能已部分改变，先回读 |
> | `no_output` | **`success=true` 但脚本无输出 / 末行不是 JSON** | **4** | 否 | 当硬失败；先独立回读，再改脚本形状（长行！） |
> | `response_truncated` | 响应超过 `max_response_bytes` | 4 | 否 | 提高 `--max-response-bytes` 或缩小查询 |
> | `guard_transient` | upstream 之前的守卫拒绝（窗口 census 等） | 0 | **是** | 等 45 s，起一个全新 cell 重试，上限 6 次 |
> | `guard_rejected` | 其它 `isError=true` 的拒绝（含静态脚本拒绝） | 0 | 否 | 读 `raw_text`；改脚本或改环境，不要重试 |
> | `upstream_timeout` | 已发出的请求超时 → **歧义** | 2 | 否 | 冻结日志，同 PID 不再发任何 MCP 请求，走歧义分诊 |
> | `transport_failure` | cell 启动/传输失败 | 2 | 否 | 读 `cell_stderr_tail`（真因在那里） |
>
> 退出码 0 **不等于成功**，只等于“已送达”。判断成功的唯一依据是
> `classification == "delivered_ok"`。
> 想让某些已送达的分类也硬停，用 `--fail-on guard_rejected,inner_script_error`（退出码 5）。

---

## D. 部署备注

- 新文件 `scripts/fusion_ops.py` 与打过补丁的 `scripts/fusion_call.py` **必须成对部署**：
  `fusion_call.py` 会从自己所在目录导入 `fusion_ops`。
- `fusion_ops.py` 只用标准库，在系统 `python3`（本机 3.13）与 skill venv（3.12）下都可运行。
- 旧驱动无需立刻改：所有“已送达”的分类退出码仍是 0，
  只有 `no_output` / `response_truncated` 从 0 变成 4 —— 这正是要修的行为。
- `fusion_ops.outcome_from_caller` 兼容未打补丁的旧 `fusion_call.py`
  （从 `payload` 反推分类），可以分批灰度。
