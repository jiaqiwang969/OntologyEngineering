# 私有 Source UPDATE 执行接续

发布接续：下文历史“默认关闭”属于独立库默认配置或原轮次，不代表 RC.5 CLI 的实际依赖图。RC.5 显式编入 source-session 所需两个 feature；运行准入仍按请求版本，公共查询 `/5` 继续拒绝。编译范围检查与行为/平台接收见[发布范围](RELEASE-SCOPE.md)，旧 RC.4 原门槛保留。

工程本体仍由规范定义对象、关系、条件和证据含义，经真实 ggen 生成共享合同，由 Rust 执行；Project 保持工程决定的唯一来源。本页只说明底层 RDFLib 兼容能力的私有实现范围，不授予工程采用或正式版本资格。

`rdflib760-nullable-internal` 默认关闭。私有 DatasetUpdateV3 复用现有 DatasetSession、SourceDatasetState、PreparedSourceUpdate、来源对象与表达式/上下文 owner。旧会话和公开 profile 保持原含义；不能把 V3 试验路径称为完整新查询版本。

UPDATE 接收显式来源字节和已观察的缺失，由原加载 owner 核验内容身份与工作限额；不会自行访问网络或文件。已由来源类型核实的原错误保留 module/type/message；尚未接收的格式和类型仍标为未知。执行到达阶段、部分结果、缓存和 revision 分别保留。命名图枚举保留 source 对象身份，未作 QUERY 分析的 UPDATE OPTIONAL/UNION 继续使用现有执行器。

原库的具体行为要按其实际层次解释。例如单独 false Literal 在翻译时被真假判断省掉，与执行器收到一个假表达式是两件事；UNION 保留重复行，外围非 lazy Join 仍可在物化右侧时去重。原库兼容结论不能代替工程规则的正确性。

私有受控构造输入仅用于实际空白节点创建点，不输入查询答案、不改全局随机状态、不添加虚构的分配。原库临时 prologue Graph 与 Rust 前端的不同要单独记录。预提交输入、原库观察和 Rust 接收分别固定身份；新条件下的通过不追认缺少随机输入记录的旧失败。

实际验收仍要求完整动作、状态、来源、错误、缓存、snapshot 和有序 SELECT；不能通过删除失败、结果重排或事后选择随机值取得一致。源码导出的预期、原库真实观察、软件控制和工程任务反馈分别标记。私有材料通过既有固定身份读取器提供，原件和运行证据留在 sources/ 与 var/，不进入公开分发。

当前通过范围从维护回执核实。完整原行为、平台接收、公开入口启用以及原 19 模块、19 协议、22 owner、13494 范围身份与 63 项工程目标继续分别验收；本页不声明完整 Semantica 或 RRSI 收益成立。


## Source 保存、重读与差异增量

2026-10-14 私有接续复用现有 DatasetUpdateV3 / SourceDatasetState，增加显式 NT/NQ source writer 与 source snapshot diff。工程本体规定身份域，ggen 生成 source 参数/结果合同，Rust 在同一状态上执行。原 native serialize、RDFC diff、v2-only snapshot 输出和 require_native_complete 保持各自含义；新参数 DTO 接受已知 v1/v2 只为原 diff 输入语义，不能改变旧输出资格。其他原格式、平台和消费者继续属于完整目标。

Writer 按原 context-major 与同 owner 的实际 set-copy 读取；默认图必须显式指定，scan(None) 是 union。原 NT 对象转义不同于 snapshot n3，保留显式 xsd:string、language 大小写、空图注册及缓存/revision 行为。source diff 先按原 Python splitlines/count/sorted/SHA 检查，再比算法；带实际换行的 snapshot 可能因原算法缺陷而拒绝。这是原行为接续，不是工程正确性证明。

冻结原41动作与独立8动作分开登记；原库49动作已执行并完成有限独立原件审查，包含35 completed、11 original_error、3 caller transport。新增 Rust 行为、所有原失败及平台接收分别看私有维护回执。接收器必须先执行全部49动作、保存/重读自己的实际字节，再读 oracle。实现作者已曝光原件，属于 development_replay；不能当成未曝光方法收益。真实执行、当前源码接收、正式版本选择与工程采用分开，不从本段文档推断通过。


显式本地 source 会话新增接续见 [source-session 合同](SEMANTIC-SOURCE-SESSION.md)：`kernel source-session` 将同一原 owner 的 load/query/update/clear/snapshot、实际初始及失败后状态接入有界同程序 worker。原18逐项对照、源码/程序及实际CLI资格须按新源回执核验；本段不继承旧A2未启用该feature时的平台资格，不关闭旧884两项历史失配或完整Semantica范围。
