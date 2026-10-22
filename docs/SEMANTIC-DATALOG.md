# 多变量规则推导的接入

需要保留连续动作及失败后的实际状态时，使用另行声明的[连续推理入口](SEMANTIC-DATALOG-SESSION.md)。本页两个一次性入口的事务和结果含义保持。

`kernel datalog --input REQUEST.json [--output RESULT.json]` 通过生成的 `SemanticDatalogPort` 调用已有 Rust `DatalogReasoner`。它可以沿规则推导多级关系，例如“图纸依赖装配、装配依赖零件”产生“零件变化后图纸也需重查”的候选关系。输入关系是否真实、条件是否适用以及是否改变工程决定，仍由同一 Project 和原审查负责。

```json
{
  "schema": "semantica.datalog-request/1",
  "facts": ["depends(drawing, assembly)", "depends(assembly, part)"],
  "rules": [
    "affected(X, Y) :- depends(X, Y).",
    "affected(X, Y) :- depends(X, Z), affected(Z, Y)."
  ],
  "query": {"pattern": "affected(?object, part)", "bindings": {}},
  "budget": {
    "max_iterations": 1024,
    "max_match_attempts": 10000000,
    "max_facts": 100000,
    "max_query_rows": 10000
  }
}
```

示例返回 `assembly`、`drawing` 两个绑定。`closure` 保存输入及全部派生事实的谓词和参数，`seed_fact_count`、`derived_fact_count` 分开计数。`request_sha256` 绑定完整类型化请求，包含原数组顺序和预算；它不证明前提具有工程依据。`iterations` 与 `inference_match_attempts` 是本次 Rust 推导的实际计数，不包括查询、运输或实测时间，也不宣称与原 Python 的执行轮数相同。

事实支持原有字符串和有限字典形式。`input_inserted` 与每个原输入对应；`false` 表示重复或原接口不认识的字典被跳过，这两者不被伪装为新增事实。当前不接受复杂字典项、函数项、否定、算术内建或不安全规则。已有 ContextGraph 导入继续由同一库内 owner 执行，本次 stateless 请求不建立第二份持久图。

查询保留原变量名与预绑定语义。`query` 缺省或为 `null` 表示没有查询；查询结果中的空数组表示空投影或无匹配。尤其完全没有变量的查询，按原接口返回 `[]`，不能把它用作 ASK 或据此否定关系。

上述预算是硬上限，调用方只能降低。完整请求最多 1 MiB、4096 条事实输入和 256 条规则；最多执行一次查询。匹配次数分别约束推导与该次查询，不声称是二者合计或绝对耗时上限。原单项、参数、工作集和安全规则继续生效。输出按含换行的完整 pretty JSON 限 16 MiB；超限、未收敛或查询失败均返回错误，不截断事实、返回部分成功或更新 Project。

正本 `specs/modules/semantic-rule-metadata.toml` 经锁定 ggen 生成 DTO/port；`semantica-runtime::datalog_port` 只做边界包装，推导仍在原 `datalog.rs`。`oe-kernel::rules` 与 CLI 沿现有编译身份和私有输出规则接入。旧 `kernel rules`、`kernel reason`、SPARQL 与 source-session 的语言和判据各自保留。

18 个原始场景先冻结预期，再运行锁定 Semantica 0.6.5+oe.7 核对；原始列表、错误与传输日志保留，集合比较仅用于 Datalog 原有事实/绑定集合。新的入口、资源边界、实际二进制、平台和安装分别取证。源码支持不代表已正式选用；以维护回执为准。完整 Semantica、工程采用、原生 CAD、制造/最终计量和 RRSI 收益继续单独验收。

## 从上下文图接续

`kernel datalog-graph --input REQUEST.json [--output RESULT.json]` 使用独立的 `semantica.datalog-graph-request/1`。它保留上述 `facts`、`rules`、`query`、`budget` 字段，另要求 `graph`，其中 `nodes` 和 `edges` 都是字典记录数组。例如：

```json
"graph": {
  "nodes": [{"id": "part", "type": "part"}],
  "edges": [
    {"source": "drawing", "target": "assembly", "type": "depends"},
    {"source": "assembly", "target": "part", "type": "depends"}
  ]
}
```

这是原 `ContextGraph.find_nodes/find_edges` 记录的不可变快照；调用方提供完整记录与适用依据。入口不读取图句柄、文件路径、服务器或 Python 对象。先添加 `facts`，再由原 `DatalogReasoner::load_from_graph` 按边、节点顺序导入，最后执行同一规则与查询。未知字典仍跳过，重复事实仍去重。原字典语义将空格换为下划线并转小写，可能合并不同标签；必须回到原输入和 Project 核对象身份，不能从合并后的术语自行认定工程对象相同。

结果 `semantica.datalog-graph-result/1` 保留完整闭包、查询和原工作量字段，新增 `graph_node_count`、`graph_edge_count` 与 `graph_facts_added`。前两项计全部输入记录，后一项只计图相对已添加 `facts` 新增的唯一事实。`input_inserted` 仅对应 `facts`，`seed_fact_count` 包括图导入，`derived_fact_count` 才计后续推导。完整请求摘要在上述归一化之前计算，记录顺序、跳过项和预算也参与绑定。

两种入口沿同一推导实现。图入口将 `facts + nodes + edges` 合计限制为4096条，完整请求仍为1MiB，规则256条、完整输出16MiB和原预算保持。图数组只接受字典，复杂字典术语仍按既有限定范围拒绝。失败丢弃该次私有引擎，不返回部分成功；原 Python 图导入异常时可能留下已插入的事实，这一原观察保留，本无状态入口不声称复刻持久对象的异常后状态。18组新增原始场景含16个成功与2个失败，不能作为完整 ContextGraph、动态对象适配或全部 Semantica 等价的证明。


## 数值作为推理符号

字典中的数值按原 Python `str` 形成符号，再执行已有的小写和空格转换；Rust 复用 `semantica-json::python_str`。例如 `0.00001` 得到 `1e-05`，`1e15` 得到 `1000000000000000.0`，整数 `5` 与浮点 `5.0` 是不同符号，`-0.0` 保留负号；任意精度整数不先转浮点。原始字符串事实不会自动做数值转换。它们的匹配是符号相等，不承担算术、公差或单位换算。

两种 `/1` 入口都使用这一规则；JSON admission 仍只接受有限浮点，有限下溢可以成为正负零。数值出现在 SPO、节点或边字段时共用同一格式实现；字段优先级和原零值跳过条件保持。术语字节上限仍在转换后检查，复杂项继续拒绝。请求摘要依旧绑定推理前的完整类型化输入，不重新签发旧回执。

本修复在修改前冻结了60组原始数值请求并运行锁定原版：原实现有12组因小数/指数写法而漏匹配或未去重。旧失败和原始输出保留，修复的接收范围以本轮维护记录、程序摘要及实际 Mac/Windows 验收为准。该修复保持 `/1` 的原数值含义与传输结构；修复前版本的数值结果不能追认为已经修正，需用当前执行身份重新核验。
