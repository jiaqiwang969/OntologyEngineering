# Source span、PROV 与来源谱系

这是完整 Semantica Rust 迁移的一个新协议切片，profile 为 `semantica.prov-source-span/1`。它把来源原字节、UTF-8 片段、提炼活动、派生、修订和撤回接入现有 DatasetSession。它没有关闭整个 provenance/context，也没有授予工程采用。

共享 17 类 DTO 与 `SemanticProvenancePort` 的正本是 `specs/engineering.toml`，实际 ggen 生成唯一 `semantica-contracts` 类型与结构测试。中性实现位于 `semantica-runtime::provenance`，不依赖任何 `oe-*`；工程来源适配位于 `oe-kernel::provenance`，委派既有 `oe-knowledge::conversation::verify_entry`。Project、Harness、learning、包治理继续保持原权威，未创建新的项目或采用账本。

## 调用与可信范围

- `verify_source_span` 接收真实源字节，核完整 SHA/长度、UTF-8 边界、半开 byte range、片段 SHA 和可选 quote。`VerifiedSourceSpan` 私有字段、无 Deserialize，公开 `SemanticSourceSpanProof` 只是回执。prepare/load 不接受 proof 或 passed 声明，每次核 actual source data。
- `prepare_records` 完整检查所有来源与不可变记录，建立 OxRDF terms，经唯一 dataset RDFC10 snapshot 得到规范 NQuads。`load_records` 在同一 `&mut DatasetSession` 借用内准备，然后只调用一次既有 `load_batch`；错误保留 graph、revision、query/snapshot cache。
- 每个新 record/subject IRI 不可复用。来源、agent、activity 可以按相同 subject quads 重用；同 IRI 改内容拒绝。修订使用新的 record/subject 并明确 `revision_of`，不能覆盖旧来源。新 source Entity 可以同时承载明确 parser activity，但只有一份 RDF 身份。
- `record_view` 和 `source_view` 返回限定图中原对象、绑定片段/原来源、直接派生/修订引用及撤回活动；`not_found` 单独报告。它们不是旧 get_lineage 的 metadata 合并视图，也没有隐含全部祖先完整性。
- `lineage_view`/`descendants_view` 只遍历明确的 `derived_from`/`revision_of` IRI 边。max_levels=1 只含 root；深度/节点/边预算留下 frontier，缺失引用另列；有 gap 就 partial。Kahn 检查区分实际循环与会合 DAG。cycle 只说明已枚举子图，partial 未遍历部分不能推断没有循环；来源边永远不验证工程物理因果。
- `prepare_invalidation`/`load_invalidation` 增加独立 activity、actor、reason 与可选带时区时间。撤回保留原实体和来源，不能把 plain source Entity 冒充 record，也不执行删除、purge、tombstone 或 Project 决定更新。

原活动/主体/角色和时间均是来源声明；不赋予审批权限或历史真实性。时间给出时要求明确时区并检查 activity start ≤ end，缺时间保持缺失。confidence 为有限 0..1，metadata 原值保留；metadata 中 status=passed/adopted 或 confidence=1 不改变 `engineering_status=unknown`，原 Project 仍需正常审查和治理。

## 聊天来源适配

`prepare_entry_provenance(path, entry, graph_id, agent_id, subject_id)` 先委派原 verify_entry，读取同一原记录字节并核 raw SHA，再重核选定 visible block。中性库不解析 Codex/Claude JSONL，也不访问用户目录。

原记录和解码后的 Entry.text 有各自 source IRI、SHA 和 byte_count。可见文本 span 的 [0,text.len()) 只属于解码后的 UTF-8 来源。parser activity 显式使用原记录，解码文本实体 `derived_from` 原记录；提炼实体另派生自解码文本。原档记录没有全文 quote，不把未选块或隐藏推理输出到 RDF。实际原记录字节只留在调用者的私有准备请求中，不能直接作为模型上下文或公开分发资产。

## 预算与身份

本切片固定硬上限：来源 64、records/agents/activities 4096、每项关系/片段 4096、graph/output 100000 quads、总请求和输出 16MiB、metadata 累计100000 nodes/depth96。来源谱系 levels≤96、nodes≤10000、edges≤100000；请求可设更低上限，不能扩大硬上限。输入及 metadata 序列化先计字节/结构，再构造规范副本，预算错误不变成完成。

来源完整性 profile 为 `semantica.source-utf8-span-sha256/1`。record 内容摘要 profile 为 `semantica.provenance-content-sha256/1`，绑定 typed record 和已核 source-span 回执。它是本次投影输出身份，未在 RDF 添加旧 checksum 或全局审计 hash-chain，不能冒认旧 checksum 或跨活动来源完整性。NQuads 内容身份继续唯一 `semantica.rdfc-1.0-sha256/1`；生成回执、来源一致、RDF 合法和工程采用分别取证。

## 固定验收与差异

实现前冻结的19独立预期 SHA `dddd9041e8b6dec75252cd2e7348d4b98d26f7a451adb11316d8819fe3a37394` 原字节保留。24 ground quads 3294 bytes 的 oracle SHA `3db1708c07b505d35b0b628974b01a3ad4ba8b320d1a3b8c78f9e592787b703b` 经真实投影、parse/load 和 session snapshot 对照；它不能替代 blank-node 标准夹具或全部 PROV Constraints。原JSON转义/改原记录/改block、自报 proof、批次失败缓存、跨图、DAG/循环、深度/预算、重复发布、身份/quote、撤回和 Project adopted 原字节另有实际反例。

旧 manager 部分失败、可变主键、char chunk index、source location、metadata 祖先覆盖、legacy checksum、entity 上 qualifiedAssociation 等协议尚未 exact 适配。新原子批次、byte span、活动 qualifiedAssociation 和独立 digest profile 是显式差异，不把新 profile 通过重签为旧 API 等价。旧 SQLite persistence/savepoint、完整 memory/chain/integrity/audit/export、ContextGraph retraction/purge、AgentMemory/检索/Decision/Policy/图分析及可选模型/数据库仍在完整迁移范围。本切片不更新 whole scope 中的迁移完成状态。

对 RRSI 的作用是保存可核来源与活动、修订和反例。经验候选仍进入当前 Harness/learning 的同一冻结与审查；全代码候选仍须同一固定 driver/oracle 下 baseline/candidate 各自实际 ggen→source/build/test→独立比较→批准选择→实际反馈→恢复。这块不新增或绕过现 learning guard，也不把来源投影成功计成改进收益。
