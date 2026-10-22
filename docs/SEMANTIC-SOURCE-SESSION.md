# 显式本地 source 会话

Windows 输入路径接续：原公共 stdin owner 误拒绝 Windows 盘符前缀，导致库测试通过而安装后 host 在创建 worker 前失败。候选修复让普通盘符和 `\\?\` 本地盘符路径沿原 ProcessTree 输入 owner 执行；UNC、设备路径、ADS、路径回退和不合法文件名仍在文件访问前拒绝。原摘要/字节数、快照、前后元数据复核、输入版本、输出预算和调用者 deadline 保持，未认证敌对瞬时替换下的完整物理文件身份。实际 Windows 子进程反例、修复回归与安装后 `/1–3` 接收分别留证；原失败不因新代码存在而改签。当前状态与原失败分别核对私有维护记录。

发布接续：下文历史“默认关闭”属于独立库默认配置或原轮次，不代表 RC.5 CLI 的实际依赖图。RC.5 显式编入 source-session 所需两个 feature；运行准入仍按请求版本，公共查询 `/5` 继续拒绝。编译范围检查与行为/平台接收见[发布范围](RELEASE-SCOPE.md)，旧 RC.4 原门槛保留。

`kernel source-session --input REQUEST.json [--output RECEIPT.json]` 在同一现有 Rust `DatasetSession` 上按声明顺序执行 `load/query/update/clear/snapshot`。请求为 `semantica.source-session-request/1`，显式 profile 为 `semantica.rdflib760-source-session/1`。这是新增本地入口；源码存在不等于当前安装程序已生成、构建或接收通过。实际可用性核当前程序的维护回执。

请求包含 `created_at`、`cache_queries`、`resources` 与 `actions`。每份资源的 `binding` 使用既有 `SemanticSourceBinding`，核 UTF-8 原字节长度和 SHA256；`source_id` 是显式资源 IRI，不是文件或网络读取授权。每个动作有唯一 `id`，其 `operation` 与参数互斥。资源、动作和 binding 名重复整单拒绝；所有准入检查先于创建会话及任何动作。

`load` 引用已声明资源，使用原六种 RDF 语法与显式 base/graph；`query/update` 使用原查询文字、namespaces 和 typed bindings，保留原术语、空值、行序与调用者 blank identity。`clear` 直接调用既有 `clear_rdflib(graph_name)`，`null` 的范围按原 owner 执行，不由 UPDATE 字符串模拟。`snapshot` 使用原算法及请求时间。无显式 UUID 答案或第二语义实现。

UPDATE 的 LOAD/USING 仅能访问本请求声明且经 SHA 核验的 Turtle 字节。未知 IRI 即使 SILENT 仍拒绝，不访问网络/文件，也不伪造“已观察不存在”。每个 UPDATE 重新建立原资源加载预算。没有 registry、Project 写入、工程采用或自动部署权限。

动作失败保留原错误链及可用的原异常身份。`backend_phase` 来自原 owner 的实际 reached 分支，不能由错误消息猜测。已执行的前缀可能改变图，revision 不变也不证明回滚。首个动作错误后，后续 load/update/clear 标 skipped，声明的 query/snapshot 继续读取同一真实会话；捕获预算耗尽后尾部全 skipped 并返回 partial。进程超时、信号、输出不完整和来源变化另作运输失败，不能当语义通过。

结果保留真实 initial 及每个已执行动作的 contexts、quads、revision、query cache 计数和 snapshot cache 标记。这一观察机械复用原 `visit_observations`，包括其原有 default-context 登记作用；只排序状态的展示副本，查询行不排序。失败后的附加 snapshot 身份观察在 clone 上执行，不暖起 live 缓存。观察失败与空图分别表示；skipped 没有伪造状态。结果还绑定完整请求与资源 binding 列表摘要，loader 请求轨迹不能替代授权。

预算分别声明：最多32动作、128资源、每动作128 namespaces/bindings；原资源总字节16MiB、原 dataset100000 quads/16MiB、原查询/更新1MiB及原 options/结果限制继续生效。新请求全部已解码字符串32MiB，JSON运输193MiB容纳转义扩张；动作结果捕获32MiB、worker总输出64MiB。超限有明确拒绝或 partial，不能截断后返回成功。当前程序启动独立同程序 worker，使用既有进程树监督、单个60秒绝对deadline及完整流记录；清理与源复核可能超过该时限，不声称硬实时返回。父入口前后核原输入、编译来源、生成合同和实际程序身份。

验收包括独立9请求44动作（正常缓存、部分后端失败、未知LOAD SILENT/USING、源身份及重复ID拒绝），预算/接收封套反例，以及原默认构造两案完整18项：真实 initial +16动作，包括clear与末尾空snapshot。原18输入来自已有159项私有接收目录，先执行全部动作再读原答案，并额外核 Nzz精确身份和新节点UUIDv4域；不把原件嵌入分发或把此前14项诊断当18项通过。实际生产CLI及每个平台须在新源构建后独立接收。

`source-session-local-v1` 只为该显式入口启用既有内部 source owner；kernel 依赖显式启用这一 feature。普通 DatasetSession 和公共查询 `/1–4` 默认语义保持，内部 owner 不因此获得完整公开 API 等价资格。原884中的UP0078/UP0726历史失配、原19模块/19协议/22owners/13494条及11工作包63目标均保留；服务、SDK、前端、WASM、providers和工程实物验收继续分别推进。

## 显式 /2：同一有序动作流中的导出与比较

`semantica.source-session-request/2` 与 profile `semantica.rdflib760-source-session/2` 在原有同一会话中接入 `serialize` 和 `diff`。旧 `/1` 的 DTO、五类动作、失败和输出含义保留。两版经借用适配进入同一个执行循环和同一个 `DatasetSession`，没有另建存储引擎或隐式第二会话。

- `serialize_request` 使用已有 `SemanticSourceSerializeRequest`，保留 NT/NQ 原格式别名、图选择、错误、完整文字和 warnings。图选择可能登记原 owner 的空图上下文；成功输出不证明它一定可重新解析。其他格式沿完整 Semantica 待接范围继续推进。
- `diff` 的 `before_snapshot_action_id` 必须指向本请求先前实际成功且完整保留的 `snapshot`；可选 `after_snapshot_action_id` 遵守相同规则。省略后者时调用原 owner 对当前状态的快照比较，包括原有缓存效果。前向、未知、非快照引用在整个请求准入时拒绝，失败或未保留的快照不替换成空图或预期值。快照完整性、算法比较和错误顺序仍由既有 `source_storage` 承担。
- 动作按声明顺序执行，允许 `load → snapshot → serialize → update → diff → clear → serialize`。任一实际动作失败后停止后续 `load/update/clear`，继续声明的只读观察；总捕获预算耗尽后全部后续动作跳过。原 32 动作、32 MiB 解码、16 MiB 来源、193 MiB 请求、32 MiB 累计步骤、64 MiB 输出及 60 秒子进程期限保留，新增参数与返回结果也计入。
- worker 不取得任意路径或网络权限。主机明确保存并重新读取实际导出字节，以实际 SHA/bytes 建立下一请求资源；新请求构造新会话，不恢复旧 revision/cache。跨请求持久会话、完整存储格式与服务/SDK/WASM 等平台继续单独验收。

入口仍为 `kernel source-session --input ...`，按显式 schema 分派，新 worker 和执行回执使用 `/2`。当前源码、编译合同、请求、进程原流和前后程序身份必须实际接收。编译、输出或传输完成不能替代语义完成或工程采用。

本增量须接收新顺序/引用/失败/字节预算反例，同时完整接续旧 `/1`、默认18、原 storage49、catalog159 与 update884 原件。原 UP0078/UP0726 两项缺少同次独立随机输入的失配不因此关闭；新 /2 不宣称重放全部原49多会话及任意 snapshot 参数域。原49的任意 DTO 错误、跨会话比较及原传输继续由其既有 owner 与独立接收覆盖，公开 /2 的有意安全准入范围单独列明。当前分发、Windows/原生资格及完整19模块/19协议/22责任/13494范围仍以实际接收为准。


## 显式 storage 结果关联合同 `/3`

`semantica.source-session-request/3` 与
`semantica.rdflib760-source-session/3` 继续使用原有七种有序动作和同一个
`DatasetSession`。CLI 仍是 `oev2 --root <skill> kernel source-session --input <request>`；
`/1`、`/2` 的字段、接收范围和历史回执保留。

`/3` 的 `storage_witnesses` 按已完成的 storage 动作顺序关联 `action_id`。
显式 diff 使用原先成功保留的 snapshot；隐式 current diff 保留该动作本来就会计算的
完整 snapshot，并在原状态观察过程中保留 typed quads/contexts。接收器用共享的原观察
投影核对完整状态（含空 context 和 DTO 转换错误），再复用原纯 snapshot 算法与
完整 diff 算法核对结果。它不重放 load、query 或 update，也不创建第二个 Dataset。

serialize 的见证来自原 writer 同一次遍历，保存有序的完整 typed rows。原 formatter
同时用于生成与接收，逐字核对 text、空白和 warnings；完整选中 quads 与实际
`source_state` 比较，不能用相同条数替代内容。Variable、relative IRI、blank、
原 datatype/language 拼写与 default/named graph 保持原域。覆盖比较可排序观察副本，
writer 输出次序不排序。

见证计入原 32 MiB 累计结果捕获，整个 worker 输出仍限 64 MiB；32 动作、32 MiB
解码输入、底层 16 MiB 数据以及 host 60 秒总期限均不提高。接收复用原 canonicalization
工作上限，并检查总截止时间。超限保留 partial/失败与已发生状态，不发布截断见证为
成功，也不推断失败回滚。观测失败有明确错误时保留未验证状态，不补造空图或 typed facts。

这验证的是绑定到同一次实际 worker 执行的结果关系。攻击者同时重写全部结果、状态和
见证，不能仅靠本地 JSON 自洽性获得来源真实性；实际程序、原始进程流、输入/来源前后
身份以及独立原库预期仍是必要证据。原 49 storage、884 update、完整 18 constructor
接续以及 UP0078/UP0726 两项历史失败均保留。有限反例拒绝不表示完整 Semantica、
工程采用或完整 19/19/22/13494 范围完成。


`/3` 的生产 host 将最初建立的绝对截止时间贯穿接收封套与关系核验，
不在 worker 返回后另给60秒；独立库调用可显式提供自己的有限截止时间。
隐式当前快照的 `created_at` 必须与非空请求值逐值相同，保留原显式字符串域。
底层快照 owner 的空时间参数仍取真实 RFC3339 观察值，并核格式；
这不改变现有 `/1`、`/2`、`/3` 整批请求要求非空 `created_at` 的准入。

Explicit `/4` is a separate source candidate described in [the phased state and Turtle contract](SEMANTIC-SOURCE-SESSION4.md). It uses `kernel source-session` with a version4 request; current installed availability, platform receiving and formal selection require current maintenance receipts. Existing `/1–3` meanings and original failures remain.
