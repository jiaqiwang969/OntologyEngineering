# 当前 Peekaboo 浏览器接入

当前 live 默认是 `peekaboo_background`。旧 `chrome_apple_events` 和 `browser_harness` 任务仍可解析并作为离线历史输入，当前 CLI/领域连接入口在任何传输派发前拒绝它们。任务 JSON 不恢复旧授权；没有自动 AppleScript、扩展、Harness 或全局键鼠后备。

`oe-browser` 当前提供页面与 AX 观察、来源/连接核验和已有观察结果的检查。供应商入口为 `session-check/inspect/archive/download-observed-ap203`，分别核账号状态、实际页面材料和已观察到的官方下载来源；不包含模型选动作或自动获取循环。当前 DOM 连接必须绑定显式 Peekaboo `tab_id`（当前 worker 的正整数 DevTools page ID；0 会走 selected-page 后备，因此拒绝）、同一 worker session/原生 Chrome 进程实例及明确的原页 URL。旧 AppleEvents 账号回执不能被新后端消费。

`runtime/peekaboo/bridge.mjs` 是第三方 Peekaboo MCP 的薄 Node 传输边界，复用独立 misumi-incad 的后台页/活动应用机制；任务、供应商与账号业务仍由原 Rust owner 执行。它只允许检查连接、创建后台页、绑定精确页、执行 Rust 持有的程序、目标截图、固定账号导航和自有页清理。不暴露任意 JavaScript CLI、Cookie/存储读取、登录、浏览器设置或全局输入。

`provider-lock.json` 固定当前 Peekaboo/Node 二进制与 Chrome DevTools MCP 原 `McpPage.js` 的 SHA。`focus-policy.mjs` 只在接受的原字节上，将唯一 `emulateFocusedPage(true)` 改为 `false`，在内存中加载并留下自有加载回执；安装文件不修改。每次操作再核 provider 原文件和实际活动 bundle/PID；Rust读取目标URL/origin/document focus事实，在派发前判断并持有原子DOM guard。第三方页面路由、默认配置及dialog handler也固定SHA；读取明确关闭自动接受网页对话框，不修改用户浏览器。真实 `document.hasFocus()==false` 与实际应用样本分别取证；不能以焦点模拟或有限采样证明任意间隙都未变焦。

后台配置沿原配置协议，明确写 `backend: "peekaboo_background"` 和已有的绝对 `peekaboo_socket`。其他传输选项拒绝。socket 由任务拥有，目录建议在显式新建的 0700 私有 `/private/tmp` 路径，避免 AF_UNIX 长度上限；不可换掉已有 socket。

```text
oev2 browser transport-start --backend-config var/backend.json --output var/transport-start-01
oev2 browser connection-check --backend-config var/backend.json --output var/connection-check-01
oev2 browser observe --task var/readonly-task.json --backend-config var/backend.json --output var/observation-01
```

启动只创建本工具的传输进程，返回 `worker_started/connection_verified=false`。连接检查必须实际 `list_pages` 后返回 ready；它只连接已有 Chrome，不替用户开启调试、不重启、不处理许可弹窗。未连接/许可未知/provider 变化/用户换应用或目标页获焦点均保留错误或未知，已可能派发的操作不重试。所有 backend 输入、来源、过程与终态证据留在本根 `var/`。

`browser observe` 无模型/网页动作，只观察指定当前页或显式任务拥有的后台页；当前 CLI 没有 `browser run`。账号字段仅在内存分类和摘要，日志不写账号值；账户导航只到固定官网账号页，不提交登录。

DOM 快照实现保存在 `runtime/browser-observer/`，来源锁、原 MIT 许可证和上游提交一并保留。其中上游 `browser-use/jev-ultrafast` 只贡献锁定的纯 DOM 快照代码，不引入其模型客户端、Agent 或服务；来源署名不表示该服务仍为依赖。源码匹配只证明观察器身份，不能替代实际浏览器连接、账号或工程验收。

本增量不宣称整个 misumi-incad 已内置 Rust。独立工具的目录/文件闭包、展示、队列/取消与恢复接续保持各自责任和待验状态，已有原文件不重下载。受控协议测试只证明编排/拒绝；实际后台只读、真实账号/供应商下载、CAD 原生保存重开和工程采用各有独立回执。

当前默认 `browser observe/connection-check/transport-start` 在没有显式CDP endpoint时走Rust的Peekaboo CLI后台AX窗口清单，并返回原窗口数据、活动应用前后及Chrome原生进程绑定；不启动Node/MCP，DOM明确not_available。`browser ax-observe --window-id <实际正常窗口ID>` 使用 `see --pid ... --window-id ... --mode window --no-web-focus --json --no-remote`，存当前AX原件/PNG及共享ArtifactReference；没有正常窗口时保留not_observed，不建窗口、不切焦点，不以截图核验DOM/账号/网站来源。

DOM另由配置显式 `cdp_endpoint: "http://127.0.0.1:<既有端口>"` 提供前提。Rust在任何Node/Peekaboo MCP/socket动作前，核现有Chrome PID/原生start、该PID独占loopback LISTEN及无代理无重定向的`/json/version`、同host/port的WS；不读取profile目录/DevToolsActivePort/Cookie。合法现存endpoint仍支持原DOM合同；缺失或错配明确拒绝。薄桥仅接受Rust传入的已核endpoint并显式交Peekaboo browserUrl，拒任意继承URL、isolate/headless与默认autoConnect profile发现。旧真实未ready/profile寻找错误回执保留旧软件身份，不能被新AX/源码通过改签。
