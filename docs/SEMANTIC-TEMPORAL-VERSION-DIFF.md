# 有源版本差异

`oev2 kernel temporal-version-diff --input REQUEST --output RESULT` 比较两个来源图版本，保留新增、删除、修改、字段变化与原异常。输入可以是即时快照，也可以是标签；标签从请求显式携带的 `stored_versions` 读入，并记录实际查找顺序。这是原内存存储的只读状态交接，存储写入、SQLite、校验和、修订和恢复仍由后续原接口接续。同一 Project 的工程依赖与受控软件版本不由该比较替代。

请求 schema 为 `semantica.temporal-version-diff-request/1`，包含 `version1`、`version2`、字典 `stored_versions`、`hash_seed`、`comparison_metrics` 和 `options`。后两项沿原方法不影响比较，仍绑定到输入摘要。原标签查找先于图读取；标签缺少或对应值为假时保留原 ValidationError，第二次查找和后续比较不伪造执行。

`hash_seed` 为0至4294967295，明确选择原 CPython3.13.11、64位、小端、SipHash13 的启动种子。Rust 使用中性的 `semantica-json::python_set`，复现 Unicode 内存宽度、数值哈希、集合探测/扩容及交并差；结果数组和字段变化顺序按该种子产生，不排序原件。不同种子可能产生不同顺序，结果绑定实际种子，不宣称恢复未知原进程的随机种子。运行不需要 Python。算法来源与许可见 [CPython原源](https://github.com/python/cpython/tree/v3.13.11) 和 [许可](third-party/CPYTHON-3.13.11-LICENSE.txt)。

重复的 Python 相等 ID 保留首个键代表与最后内容；交集的代表按原集合大小和遍历决定。缺少 ID 使用原字符串索引，显式 null 不替换。关系使用原 source/type/target 拼接键，保留分隔符碰撞及显式关系 ID 被忽略的行为。字段缺失与 null 可使记录被判修改而 changes 为空；原缺陷和差异不偷偷修正。

整个请求最多1MiB，每个整数最多4300位，每个来源集合16384项，共享哈希/相等/探测预算16777216次，集合表最多1048576槽，完整 pretty JSON 结果含换行最多16MiB。超限整次拒绝，不返回截断比较；原业务异常作为完整失败结果保留。JSON profile覆盖有限JSON值，任意Python对象及可变别名仍需各自合同。

独立原版预期在实现前冻结：54项内容/错误/标签案例乘5种子；共享集合基础为97组输入乘5种子及28个标量哈希。旧32项比较调查和原记录102项预期不修改。实际平台、安装、正式选用与工程采用分别核维护回执；本能力用于开发回放，不证明工程质量或RRSI收益。规范正本为 `specs/modules/semantic-rule-metadata.toml`，共享合同须由锁定ggen实际生成；Mac和Windows使用同一Rust实现，平台接收单独取证。
