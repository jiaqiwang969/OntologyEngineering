# Jev Ultrafast 浏览器工具

工程本体内的可选网页执行工具，源自 [browser-use/jev-ultrafast](https://github.com/browser-use/jev-ultrafast)，
固定 commit `1231850a0bf1a0c0341fe408ef1668dbbfdfac46`，保留 MIT 许可。
入口是 [scripts/jev_browser.py](../scripts/jev_browser.py)，来源及依赖见
[source-lock.json](../runtime/jev-ultrafast/source-lock.json)。执行代码独立于 Semantica。

适用：需要在实际网页中定位、选择、填写和读取结果的有界任务。普通资料检索可以继续使用搜索工具；
用户指定的 Peekaboo 或其他操作方式仍优先。本工具接管一个网页子任务，工程目标、CAD、文件验收和
本体演化仍由对应入口负责。浏览器循环可调用 Jev 多轮，不代表工程本体已有自动目标控制器。

## 准备与调用

从 skill 根解析路径。首次准备只安装专用环境，不启动浏览器或配置远程调试：

```bash
bash runtime/jev-ultrafast/setup.sh
python3 scripts/jev_browser.py doctor
```

`doctor` 验证固定源码、依赖锁和已安装直接依赖；不读取用户页面、不调用模型、不连接 Chrome。
`local_runtime_ready` 只表示本地环境就绪，账号、浏览器连接和真实任务另验。

agent 在项目私有目录准备任务文件，用户无需写 JSON：

```json
{
  "schema": "ontology-engineering.jev-browser-task/v1",
  "url": "https://example.org",
  "goal": "读取 Example Domain 页面的标题与说明，取得页面事实后结束。",
  "text_values": {},
  "max_actions": 15,
  "max_decisions": 30,
  "max_seconds": 120,
  "keep_open": false,
  "required_initial_text": ["Example Domain"]
}
```

```bash
python3 scripts/jev_browser.py run \
  --task /private/project/web-task.json --output /private/project/web-run-001
```

输出目录必须是新的项目私有目录，不得写进 skill。事件、页面和结果使用私有文件权限；页面记录可能
包含账号业务信息，不并入通用 skill。字段按当前页面的准确标签写入 `text_values`；值来自已确认输入。
未知标签或值返回 `field_value_required`，由主 agent 根据实际观察补充，不能猜测身份、型号或参数。
密码字段不在上游动作表中；不把凭据填进目标、文本映射或记录。

Jev 沿现有私有凭据解析器接入，固定使用能力目录中的精确版本。适配器不把真实 key 注入上游环境，
也不需要另配文本模型 key。它使用调用者提供的确定字段值，未启用上游的小模型文本生成。

## 执行与结果

默认 `backend=browser_harness`：上游创建自己的后台标签，不主动激活用户标签。适配器只使用已有的健康 Browser Harness 连接，
覆盖了上游可能启动 Chrome、自动打开设置页的连接行为；缺连接时返回 `browser_connection_required`。
首次连接可能需要配置 Chrome 调试能力；按当前任务的授权和焦点约束单独准备。
不要为任务自动重启 Chrome、换 profile、修改设置或转移用户焦点。
已有账号要求先核对；`required_initial_text` 只是所声明的文字检查，不是通用账号认证器。
具体网站用户放在项目上下文，不固化为通用 skill 的用户身份。

已授权 Chrome Apple Events JavaScript 的任务可显式选择 `backend=chrome_apple_events`：
提供当前实际观察到的整数 `tab_id`、`keep_open=true`，以及按当前页面标签声明的
`allowed_actions`（`kind=click|select|fill` 与 `label`）。可用 `custom_click_selectors` 补充
当前 DOM 中已核实的自定义按钮；不会修改网页角色或标签。该执行器只绑定现有标签，
不创建、激活、导航或关闭标签，也不改变 Chrome 设置、读取浏览器存储或自动登录。
它复用上游的 DOM 观察和 Jev 决策，输入通过普通 DOM 事件执行；须记录为
`chrome_apple_events_dom`，不能称为 Browser Harness/CDP 已连通。

原生 AppleScript 的 `make new tab` 实测可能激活 Chrome，不能用它落实“不转移焦点”。
准备已有任务标签和操作已有任务标签分开取证；用户自行切换窗口时保留观察，不擅自
把焦点抢回。暂时开启的 Apple Events 设置依当前授权恢复，用户自己授权的网站下载权限
不能被当作助手临时设置一并撤销。

结果文件包括 `task.json`、`session.json`、`events.jsonl`、`observation.json`、`result.json`。
每次循环前保存意图；未知执行结果保留断点，当前入口不自动重放或恢复写操作。

| 结果 | 如何处理 |
|---|---|
| `reported_done`、`verification=required` | 模型提出完成；主 agent 用当前子任务的独立检查决定验收 |
| `field_value_required` | 查观察记录，补齐准确标签对应的值；不得猜测 |
| `initial_observation_mismatch` | 初始网页不满足所声明的前置观察，不执行后续动作 |
| `browser_connection_required` | 没有已有健康连接；没有自动启动 Chrome、打开设置页或执行网页动作 |
| `budget_exhausted` | 保存断点；执行完当前调用后检查预算，不承诺硬实时中断 |
| `blocked` | 保留上游停止状态，检查实际页面、文件或作业进展再改路线 |
| `interrupted_review_required` | 可能存在未确认操作，先核对同一会话及产物，不自动重复派发 |

默认结束时关闭本工具创建的标签；若需保留现场或等待下载，任务必须使用 `keep_open=true`。
只处理 `session.json` 所属的标签，核对后关闭；不关闭其他用户标签。发生中断时事件记录用于核对，
不构成跨进程恰好一次执行保证。

下载验收检查实际落盘及型号/格式/完整性；NX 验收沿 cad-agent 的保存、独立重开和几何检查。
米思米采用“账号与完整配置 → 生成 → 落盘 → 文件检查 → NX”的交接。通用入口仍只报告
模型停止；米思米的专用入口
[misumi_acquire.py](../skills/cad-agent/scripts/misumi_acquire.py) 按工程背景和当前实际选项，
先让 Jev 选择资料/格式，再执行并检查新文件。输入包括用途、验收条件、已有资料、
缺口和约束，不能由适配器固定选择 STEP/AP203。三维、二维、规格/手册和采购信息
服务于不同问题；格式选择不代替内容适用性判断。产物和失败原因回到下一轮选择；
资料选择、浏览器派发、文件取得、原生验证和需求接受分别记录。文档链接返回交接，
由主 agent 继续读取/下载并反馈，不冒充已取得。详见 CAD 模块的 MISUMI 指南。
旧 `misumi_browser_download.py` 仅保留为历史 STEP/AP203 对照入口，不作默认流程。
已发起生成后只等待结果，不重复点击；实际格式、型号来源、单位与内容按证据分别检查。
网页显示成功而无新文件时保留失败，检查浏览器的多文件下载权限，不能无限点击或重新登录。
frames、shadow roots、弹窗新标签和复杂控件的覆盖仍受上游限制；账号核验由调用者独立完成。

已有实际 MISUMI 下载证据，仍须按具体记录区分执行器、初始设置、账号和型号。
对照比较保留失败与准备耗时，不能用修复后的少量成功样本声称所有网站无人值守稳定，
也不能把固定流程的速度归因于 Jev。根工程目标仍由本体和用户对齐；网页 DONE 不提升工程或物理验收状态。
