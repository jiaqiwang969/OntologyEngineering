# 本地治理策略与证据边界

当前实现按锁定包与生成的 `PackageState` 执行 candidate → proposed → committed → regression_passed → release_complete → promoted。`published` 明确拒绝：外部发布核验尚未实现。阶段连续、内容摘要和回归通过不能单独证明发布授权或工程采用。

`kernel transition` 的 event 是请求。请求中的 `authority_scope` 不能授权自身；执行器另读 `var/state/governance-authority.json`。策略与账本位于私有 `var/`，不分发。Unix 策略要求权限 600；当前策略是本地控制的信任边界，没有声称密码学签名或组织授权已核验。

## 策略字段

| 字段 | 格式与用途 | 实际执行检查 |
|---|---|---|
| `schema` | `oev2.governance-policy/1` | 精确格式、严格 JSON |
| `ledger` | 本根 `var/` 下账本路径 | 路径约束，与 CLI 目标一致 |
| `bundle` | `runtime/semantic-bundles.json` 中的包身份 | 读取锁定包及原资产摘要 |
| `candidate_sha256` | 包归档 SHA-256 | 必须等于当前包字节身份 |
| `package_binding_sha256` | 规范 JSON `{package_id, package_version, archive_sha256}` 的 SHA-256 | 由当前包重算，禁止版本或基线偷换 |
| `actor_scopes` | actor ID → 允许的目标阶段集合 | 策略与请求权限取交集；独立验证者不能是上一提交者 |
| `required_evidence` | 目标阶段 → 必需证据摘要集合 | 非空；请求完整提供；逐文件回读核对 SHA-256 |
| `release_artifacts` | `{path, sha256, role}` 列表 | Release/Promote 核对实际产物；至少 package_archive、license、interface、origin 且路径不重复 |

证据文件位于账本 `evidence/<sha256>.json`。不得把任意 JSON 的命名或某个 `status: passed` 当作已经验证。proposal/conflict 证据的领域内容目前由本地策略选择，完整冲突分析与 registry 治理尚待迁移。

## 事件与历史

事件绑定 candidate、package、actor、from/to、前驱和证据。执行器验证整条既有事件链：名称/摘要、根、前驱、连续状态、唯一后继和同一策略/包基线。策略变化需另建候选基线，不能在旧链上静默换权限。策略摘要由执行器写入新事件；旧链不会重新署名。

Regression/Release/Promote 实际重新执行包内不变的独立预期，并要求提交的证据与新执行的语义合同一致：引擎/版本/可执行摘要、包、输入、oracle、各案例执行与判定。SELECT 原预期按结果多重集比较；无序行及匿名 shape 结点标签不要求字节等同。实际新回归原记录另存，原 oracle 未修改。单纯提交序列化的通过不能获得晋升。

行为依据为 `crates/oe-kernel/tests/governance.rs`，包含自授权、伪造通过、跳级、权限换版、重复后继、缺发布产物和外部发布反例。它只验证本地治理合同；真实组织授权、行业 registry、章节/书源治理与外部发布继续保持未闭合。
