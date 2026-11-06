# OntologyEngineering v2.0.0-rc.9 — 大型 Project 工作区（格式 /2–/6）

**工程本体可以直接承接长期、大型的 Project：来源越积越多、提交链越来越长时，显式选择更高的工作区格式，旧记录原样保留。**

这是 v2.0.0 的第 9 个整合候选（预发布，重构中）。rc.8 增加了读懂已有工程图纸；rc.9 让工程本体自己的 Project 管理程序（workspace owner）支持此前只在独立研发仓库中实现的工作区格式 `/2`–`/6`。一个已经积累了数百次提交、上千个来源的 Project，也能由安装好的 `oev2` 直接恢复、开始阶段和登记结果。

## 这一版有什么

| 内容 | 位置 |
|---|---|
| 工作区提交请求与提交记录格式 `/2`–`/6`，任务视图 `/3`–`/6` | `specs/modules/unified-system.toml`、`specs/foundation/engineering-v1.toml` |
| 无损来源缓存：只在压缩更小时用 zlib，原件逐字节核对 | `crates/oe-engineering/src/source_cache.rs` |
| 按格式分派的来源上限与历史预算、提交前检查 | `crates/oe-engineering/src/project_workspace/` |
| 历史预算类失败按原因公开 | `crates/oe-cli/src/v2.rs` |
| 格式说明 | [项目文件体系](PROJECT-WORKSPACE.md) |

## 各格式的上限

| 提交请求格式 | 来源路径数 | 来源载荷 | 单个原件 | 历史（每次恢复与提交都自 HEAD 起完整读取） | 任务视图 |
|---|---|---|---|---|---|
| `/1` | 256 | 原始字节合计 64 MiB | 16 MiB | 1024 个提交、64 MiB | `/1` |
| `/2` | 1024 | 原始字节合计 64 MiB | 16 MiB | 1024 个提交、64 MiB | `/3` |
| `/3` | 1024 | 无损缓存载荷 64 MiB | 16 MiB | 1024 个提交、64 MiB | `/4` |
| `/4` | 2048 | 无损缓存载荷 128 MiB | 16 MiB | 1024 个提交、64 MiB | `/5` |
| `/5` | 4096 | 无损缓存载荷 256 MiB | 16 MiB | 1024 个提交、64 MiB | `/6` |
| `/6` | 4096 | 无损缓存载荷 256 MiB | 16 MiB | 1024 个提交、256 MiB 提交字节、256 MiB 归档字节 | `/6` |

任何格式都不是进程内存上限。`/3`–`/6` 的恢复输出带 `source_budget`，`/6` 另带 `history_budget`（实际提交数、字节与上限）。

## 规则

- 格式由每次新提交显式选择。已提交的记录不重写，旧格式的含义与上限不变。
- 恢复按 HEAD 自身的格式取上限；提交按新请求的格式校验整条父链。所以 `/6` 请求可以接在历史已超过 64 MiB 的 `/5` HEAD 之后，而 `/5` 请求不能承载这样的历史，降级绕不过上限。
- 提交在写任何文件之前核对：父链字节加上新提交的确切字节不超过新格式的历史预算，序号不超过 1024；否则拒绝，HEAD 不动。此前的版本只核对父链，可能写出之后读不回来的 HEAD。
- `workspace_history_byte_budget`、`workspace_history_byte_limit`、`workspace_history_archive_budget`、`workspace_history_chain_budget_or_cycle` 四个历史预算原因直接报出，其余失败的公开规则不变。
- 旧程序遇到不认识的格式会拒绝，不会误读或改写。

升级一个工作区：用新格式写下一次提交，例如请求的 `schema` 写 `oev2.workspace-commit-request/6`，其余内容照常；之后的恢复即按新格式计。

读图路线经同一管理程序打开工作区，因此不再只限 `/1`。在大型 Project 上读图前，图纸须先登记为该 Project 的证据；本版没有在大型 Project 上做端到端读图验证。

## 验证

- 锁定真实 ggen 重生比对 `consistent`、`drift=[]`，经验库合同 `matched`，八个程序按锁定依赖构建。
- 测试：工作区单元测试（含历史预算的精确边界与提交前拒绝）、来源缓存、6 个工作区与任务视图集成测试（含真实超过 64 MiB 的历史：`/5` 拒绝，`/6` 接受并冷恢复，其上 `/5` 拒绝）、CLI 失败原因、读图、义务与经验库运行时测试均通过；既有的需私有原件或显式运行的案例保持 ignored，不计通过。
- 迁移对照：在一个积累了 262 次提交、约 72 MB 历史和 1200 多个来源的长期 Project（不随仓库分发）的克隆副本上，本版 `oev2` 与此前独立维护的管理程序对同一 HEAD 的完整恢复输出逐字节相同；本版写入的 `/6` 提交，两者读出也逐字节相同。rc.9 的发布复核见 [验证记录](UNIFIED-VERIFICATION.md)。

## 已知限制

- 每次恢复与提交都完整读取历史，而每个提交都带完整来源清单与事件，历史随提交数近似平方增长；`/6` 是有限余量。不重读全部历史的检查点机制尚未实现。
- 256 MiB 上限由常量与注入预算的精确边界测试核对，没有用真实的 256 MiB 历史测试。
- 读图仍只支持 ASCII DXF 的有限剖面与 macOS 上的隔离 worker，其余限制同 [rc.8 发布说明](RELEASE-NOTES-v2.0.0-rc.8.md)。

## 安装与升级

```sh
cd "$HOME/OntologyEngineering"
git fetch --tags && git checkout v2.0.0-rc.9
sh scripts/install.sh
"$HOME/OntologyEngineering/var/install/bin/oev2" --root "$HOME/OntologyEngineering" --version
```

安装器保留上一安装为 `var/install-previous.*`，私有数据目录 `var/` 与 `sources/` 不受影响。已有工作区无需迁移即可继续使用原格式。

本版不授予工程采用，项目的全部根验收仍按各自证据判断。
