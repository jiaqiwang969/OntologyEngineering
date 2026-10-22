# 私有验收输入与公开构建

公开源码不包含项目原件、SDK、教材或旧私有试验载荷。此前把 14 个 Rust 测试文件中的 54 处私有 `include_str!` / `include_bytes!` 改为共享运行时读取器，登记了 41 项固定输入。本轮另将 `semantica-json/tests/python_representation.rs` 的两处旧相对路径运行时读取接入同一读取器；当前共 15 个消费者文件、56 处运行时调用、43 项固定输入。这两处原本就不是编译时 include，不能把历史数量改称为56处 include。原测试和断言保留；缺少必要输入时明确失败，不计作跳过或通过。

执行这些私有验收时，显式设置 `OE_PRIVATE_TEST_FIXTURE_ROOT` 为包含登记相对路径的绝对目录。读取器只接收固定目录中的固定条目，并核大小与 SHA256，向消费者返回同一份已核字节。未知条目、缺件、改变、符号链接、非普通文件和超限均拒绝。不能把任意同名 JSON 放进目录当作原验收材料。

Unix 实现通过逐级目录句柄及 `openat` 定位，打开时禁止跟随符号链接并设非阻塞；打开后检查文件类型，读取前后核对路径和文件身份。FIFO 等替换不得等待写入端。根目录、路径深度及载荷字节有固定预算。这不保证任意故障文件系统的墙钟期限。Windows 接续实现见下文；其它非 Unix/Windows 平台仍明确报告不支持。公开构建和各平台生产功能的资格另行核验。

公开合成检查可直接运行：

```sh
cargo test --locked --offline -p semantica-runtime --test private_test_fixtures
cargo test --locked --offline -p semantica-numeric
```

真实原件验收例如：

```sh
OE_PRIVATE_TEST_FIXTURE_ROOT=/absolute/verified-input-root \
  cargo test --locked --offline -p semantica-runtime \
  --test rdflib_literal_consumer --test rdflib_snapshot
```

`refinery_subject_parity` 等完整历史验收还需要其原合同指定的 checkpoint、额外反例或 SDK 源包，不能只提供这 43 项就宣称整组通过。`cargo check --workspace --all-targets` 只证明所选平台的目标可编译；某一轮是否在禁止读取私有资料的环境里实际完成、使用了哪些缓存和工具链，均由该轮接收回执记录。完整工程目标和未执行验收继续保留。


本轮追加的两份 Unicode 原件只放在显式私有根，公开源码仅保留固定身份与加载器：

| 私有根内相对路径 | 字节数 | SHA256 |
|---|---:|---|
| `ORIGINAL-PYTHON-UNICODE-CLASSIFICATION.bin` | 1114112 | `b79eaa6372a6d75fcd37311560beff4e26aa3b1c2f1c5f7a721a9b00e52bde10` |
| `ORIGINAL-PYTHON-UNICODE-DECIMAL.json` | 23002 | `2cbf99cb55b4046965919a6467dca68736620b7d94b363050b20297cc26416b3` |

原分类测试仍比较 `0x110000` 个槽位中全部合法 Unicode scalar 的 printable/whitespace，原数值测试仍检查全部680个已登记十进制数字。第三项对象顺序、转义、strip 与浮点显示的合成测试也保留原断言。缺件恢复只恢复相同字节，不继承原 inode 或旧执行资格；本轮没有重跑 Python 生成另一份答案。

```sh
OE_PRIVATE_TEST_FIXTURE_ROOT=/absolute/verified-input-root \
  cargo test --locked --offline -p semantica-json --test python_representation
```

这组测试中的私有读取不提供默认父目录回退；未设置环境或根内缺件应失败。合成项可以单独、不提供私有根运行：

```sh
cargo test --locked --offline -p semantica-json --test python_representation \
  ordered_native_text_and_escaped_representations_share_numeric_owner -- --exact
```

共享读取器和固定身份目录是公开测试源码，其 SHA 实际纳入编译来源闭包及变更/缺失拒绝检查；两份原件没有进入公开分发。依赖 `sha2` 与 Unix `libc` 仅用于测试加载与身份核验。公开所有目标的编译检查、缺私有根的失败对照、显式根下的完整原件回归是三种不同证据，必须分别执行并记录。

## 来源查询与会话原件的分发接续

当前候选另将 30 个测试消费者中的 91 处编译时文本/字节嵌入改为同一读取器。固定目录增加 86 项来源查询、会话和加载原件，总计 129 项；旧 43 项的标识、大小、摘要和读取语义保持。新增标识以 `release-inputs-03/` 开头，后接原源码相对路径，方便根据旧记录定位。运行时根须实际包含本次测试使用的固定标识；不存在默认父目录或旧工作区回退。

86 项包括 66 个带私人来源路径的原件和 20 个 gzip/base64 封装原件。封装不提供公开分发资格；后者的完整封装字节、解码身份和原解码检查均保留。测试使用原输入、原 SHA、原案例成员与原断言，没有重新生成预期。原 `QUERY-NUMERIC-PROVENANCE.json` 完整记录也移到私有根，公开同名文件仅保留该原件的固定身份引用；源码与许可证继续分发。

原 `include_str!` 所需的长期借用在测试内通过 `text_ref!` 保存：每个显式调用点首次读取时验证原根和固定字节，将同一已验证字符串保留到该测试进程结束。失败不缓存为成功；换根、换输入的检查使用新进程，原文件读取器的逐级目录句柄、大小/SHA、拒绝符号链接和变化检查不变。它仅为测试输入存储，不进入生产语义执行。

公开构建绑定读取器、固定目录、消费者及公开来源引用，私有执行另绑定本轮实际输入和结果。公开编译不需要这些原件；执行原件验收时缺失、替换或超限仍失败。当前是否已完成公开构建、全部受影响验收、分发和搬迁接收，以维护回执为准。完整 Semantica、平台与工程验收范围继续保留。


## Windows 等效私有输入接收

Windows 测试读取器沿同一固定目录，使用原生 NtCreateFile 的 RootDirectory 单分量相对打开，禁止重解析后检查磁盘、目录/普通文件类型；随后核完整128位文件ID和卷身份。返回前重开元数据，分别核根目录、父目录、当前叶及原打开句柄，并检查大小、写入/变化时间。消费者取得的仍是实际哈希验证的同一份字节。此处复用同一私有输入职责，不借用供应商身份凭据的 ACL 规则。

支持显式本地盘符绝对根和 VerbatimDisk 形式；拒绝 UNC、设备命名空间、数据流、父遍历、DOS别名和尾点/尾空格。系统必须支持完整 FileIdInfo，取不到身份则拒绝，不降级为仅摘要判断。仍无任意故障文件系统的墙钟期限保证。协议依据：[NtCreateFile](https://learn.microsoft.com/en-us/windows/win32/api/winternl/nf-winternl-ntcreatefile)、[FileIdInfo](https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-file_id_info)。

Windows 合成控制实际创建 junction、符号链接和替换文件，检验其拒绝；权限不足不能跳过后计通过。`private_test_fixture_catalog` 是另行显式提供原件的验收，逐件读取129个原始身份；缺原件仍失败。目录读取、原语义消费者验收、公开无私有根编译以及完整 Windows 发行各自记录。2686个新增原件相关测试及旧 Unicode 等指定消费保留原输入和预期，不能通过修改换行或答案适配通过。

同一已打开句柄另经 NtQueryVolumeInformationFile 的 FileFsDeviceInformation 核操作系统报告的本地 disk 类型；拒绝 remote 标志，查询失败或不完整也拒绝。该核查不保证物理介质位置；位标志合成反例不冒充真实映射网络盘试验。
