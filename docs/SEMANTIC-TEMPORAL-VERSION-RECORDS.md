# 有源版本记录

`oev2 kernel temporal-version-records --input REQUEST --output RESULT` 将原 Semantica `TemporalVersionManager` 的三项记录操作接到既有 Rust 语义 owner：`create_version`、`validate_snapshot`、`migrate_snapshot`。它服务于来源版本调查；同一 Project 仍维护工程事实、依赖和重审，`versions` 仍负责受控软件/方法版本的登记与选用。

规范正本为 `specs/modules/semantic-rule-metadata.toml`，共享请求、结果和端口由锁定 ggen 生成。实现位于 `semantica-runtime::temporal_version_records`，经原 kernel 与正式语义执行权限进入 CLI。实际构建、平台、新根安装和正式选用分别核对维护状态与对应回执；本文说明合同范围。

请求的 `schema` 为 `semantica.temporal-version-records-request/1`，`operation` 选择上述操作，`arguments` 保存原关键字参数。`create_version` 要求 `graph`；另两项要求且只接收 `snapshot`。所有操作显式给出 `clock_local`，格式为 `YYYY-MM-DDTHH:MM:SS`，有非零微秒时加六位小数。这是原 `datetime.now().isoformat()` 的本地无时区回放值，不读取主机时钟，也不推断 UTC、证据有效期或工程时点。

- **生成记录**：按原 Python 真值规则回退 timestamp、version_label 与 metadata；保留 entities、relationships 的即时复制内容。原代码接受的字典集合也原样保留；不会偷偷转成数组或回退到 nodes/edges。真值 timestamp 可以是不合法日期。原方法只返回记录，不保存到 storage。
- **检查格式**：只核 format_version、label、timestamp、author、description、checksum 是否为字符串，entities、relationships 是否为数组。不会验证版本号、时间、邮箱、校验和、数组成员或工程事实。类型齐全而内容错误的记录仍返回 true；这项原行为不能成为采用依据。
- **迁移旧格式**：复制全部内容；已有字符串 `format_version="1.0"` 时直接返回。其他版本写入 1.0，并为缺少的必要字段补 null，保留已有值及额外字段。迁移后的记录仍可能格式检查失败。

结果的 `schema` 为 `semantica.temporal-version-records-result/1`，profile 为 `semantica.source-temporal-version-records/1`，绑定完整请求摘要与本地时钟，分别返回原结果、原异常及实际时钟回退次数。格式检查的 false 是一次完成的检查。原 get/copy/membership 异常保留类型、文字及发生前的回退次数，失败不输出局部记录。整个请求或完整输出超限则整次拒绝。限制为输入 1 MiB、完整 pretty JSON 加换行 16 MiB，原限额保持。整数另沿原 CPython 3.13.11 JSON 入口限制：每个整数最多 4300 位十进制数字，不计负号；嵌套和未使用字段也先检查。超过该范围在操作和时钟回退之前整次拒绝，复用现有 Rust 原 JSON 解析模块的位数判据；其他 JSON 入口的含义保持。

独立预期先于原版执行和 Rust 实现冻结，覆盖 102 个即时结果/失败、8 个整数入口边界，以及原浅复制、深复制、无 storage 调用的观察。原 `create_version` 在 Python 进程中与输入共享嵌套对象和真值 metadata；JSON 入口只承诺调用结束时的序列化内容，跨请求对象别名不属于该 profile。请求和结果使用独立值，后续修改输入不会隐式修改已保存证据。

当前 Project 投影用于开发回放时保持原文件、未知日期、来源与必要验收。记录生成和格式迁移不新增工程批准，也不证明 RRSI 收益。`compare_versions` 的集合顺序与重复身份、带校验和的 create_snapshot、事实修订与存储恢复仍需独立接续；三项记录操作通过不能称完整版本管理或完整 Semantica 已完成。
