# 执行 provenance 的 Rust 接续

工程本体规定记录的含义与权限边界，ggen 从 specs 正本生成共享 Entry、Bundle、Report、Receipt 和 Observation；`semantica-runtime::execution_provenance` 执行内容算法。后续 RRSI 使用这些实际输入、过程与输出证据检验候选，不能从内容摘要相等推断改善收益或工程采用。

原 `ProvenanceEntry` 的全部39个序列化字段保留。原 Python dataclass 不校验类型注解，纯 JSON 接续合同以 JSON 值保存原默认、null、布尔和数字类型。显式时间可固定数据；默认墙钟只在构造边界解析。原非 JSON Python 对象不由该合同覆盖。

单条校验和执行原 `integrity.compute_checksum` 字符串拼接算法，保留 dict 与 Entry 对象在 null/default 上的差别；没有改为整对象摘要。`entity_id`、metadata、sequence 等原算法排除字段由完整 bundle canonical JSON 的 SHA256 绑定。Bundle 逐条检查原 checksum、从1开始的 sequence 和 previous_checksum，再核完整内容摘要。删除、换序、修改绑定和未重算内容的字段变更分别可检出。重新计算全部公开摘要本身不能证明来源或执行真实性。

纯 builder 接受声明的数据和 report 状态，只产生可校验内容。JSON入口先检查原 DTO 构造条件：非空 bundle/record、非空 report kind、原三种 status、规范摘要和 receipt schema；严格字节解码另拒绝重复键、非有限值和深度超限。单条完整性、Bundle完整性、Receipt完整性与 bundle/receipt binding相符各有检查函数。

`book_runner::execution_provenance` 只读取同一 `BookRunnerState` 内实际 `run` 留下的记录，要求原 receipt、当前同一 Dataset 快照和 runtime observation 相符。它生成完整 bundle，移除新回执中已实现的 `provenance.bundle` 未覆盖项；旧 `run` 结果与历史观察保持原含义。新输出使用 `semantica.execution-provenance-rust-observation/1`，Rust source 仍为 `semantica.rust-source/` 加64位摘要，不能冒充旧 Git revision。该端口不创建第二 Project、registry、history、model pool 或 budget。

完整 bundle 是后续发布检查所需的一项内容算法；编译来源批准、来源真实性、release、行业晋升和工程采用仍分别取证。原 manager/sqlite 持久化、跨平台和完整原库13494范围继续保持，不能由本项测试关闭。原 source-span/RDF 投影模块保留其原范围。

独立预期来自 pinned 原 schemas、integrity 与 lifecycle 的未改实际运行，开发使用固定输入和28组原反例。另用当前 BookRunner 的真实查询和原正反例验证同实例 producer；它们属于明确范围的行为对照，不是工程项目、独立泛化或全库完成资格。

新增47组原默认/truthiness/数字/序号边界及19组原构造器边界均使用未改原运行预期。Python strip的U+001C–001F与原JSON时间类型分别保留；动态默认时钟只核类型和同一bundle/records时间关系，不冒充固定时间字节资格。
