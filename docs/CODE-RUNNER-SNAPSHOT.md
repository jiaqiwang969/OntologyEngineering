# 代码 runner 的原 RDFLib 快照调用

工程本体规定含义，spec 定义共享输入，真实 ggen 生成 `CodeRdfLibSnapshotInput`，Rust runner 调用同一中性 `DatasetSession`；固定原输入与预期检验输出，不能由生成或执行成功授予采用。

`local-semantic-replay/1` 新增 `rdf.snapshot-rdflib/1`。输入严格四字段：`data` 是原 UTF8 RDF 字节（可为空），`format` 只能为 `ntriples` 或 `nquads`，`graph_name` 必须显式为 null 或合法绝对 IRI，`created_at` 是观察时间正文。NQ 数据自带图上下文，不能另给图覆盖。空观察时间沿原端口用 host UTC，固定比较须冻结明确时间。

runner 创建一个现有 `DatasetSession`，只调用 `load_rdflib`、`snapshot_rdflib`。结果是已有中性合同 `SemanticRdfLibDatasetSnapshot` 的完整原 v2 身份、内容摘要、原 n3（无末尾换行）、计数、修订和时间。plain/显式 xsd:string、原词法、blank 对称和原归一由现算法承担，不复制 RDFLib 或另建数据所有者；RDFC、查询、SHACL、Datalog 四个既有能力的合同和派发保持。

返回仍用 `CodeRunnerObservation`：执行成功只覆盖该次声明能力，解析/输入失败保留 failed、null 结果和空 coverage，未实现的能力仍 unsupported。`engineering_effectiveness_not_evaluated` 原未知项保留。最大 1MiB 调用帧、同 Dataset 的输入/四元组/算法与字面量资源条件保持；总截止时间和输出预算沿现外部固定 driver 的 ProcessTree 执行，runner 不自称独立硬实时或敌对进程隔离。

十二个开发用例在修改候选前，由未改 Semantica `8b2ed123` 的完整 `SemanticRuntime(profile='rdf')` 构造器和 RDFLib 7.6 实际冻结完整快照。十份既有原预期重跑字节不变，另两份原 NT 覆盖无末尾换行的语言 literal 和显式图 dateTime。真正前驱十二次返回 unsupported，是保留的 RED。此目录测试只带合成输入及原完整预期，预期不传给生产 runner。

每个接收头须自行真实 ggen、构建、进程对照及同 Dataset/RDFC 消费者回归。十二项不替代原六项 oracle、十九项协议、二十二项业务或 13,494 全库范围，不签编译来源、工程采用或 whole-code 发布资格。本轮仅新增执行能力，版本选择、实际后续使用、反馈、恢复仍待同一既有 owner 接线；没有 policy/4 或第二登记台账。
