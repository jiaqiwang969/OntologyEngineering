# 通用包输入的 Rust 增量

本增量在独立私有候选开发；未接收为主工作版本。原库全功能迁移仍未闭合，不能按此页推导已经替代全部 Semantica。

通用入口是 `packages::load_package` 与 `DatasetSession::load_package`，输入由真实 ggen 生成的 `SemanticPackageInput` 表达。mapping 和 provider 内 manifest 文件恰选一个；文件支持 JSON/YAML/YML，文件输入使用实际 manifest 父目录，忽略另传 base_path。`load_package_file` 接受显式本地路径并构造中性 FileProvider；库不固定工作室根、目录数量或项目治理。

每项 path/data 按键存在恰选一个。data 文本以 UTF8 原字节读取；native bytes 由 `inline_bytes` 的 typed map 按 asset_id 运输，对应 manifest 空文本 data marker。这是明确 Rust 运输形式，旧 JSON 数组不会被静默改当 bytes。空、NUL 和非UTF8原件可保留。FileProvider 解析实际路径，以包根核对包含关系，允许根内符号链接和不逃根的内部 ../；外部符号链接仍拒绝。JSON 文件只按 JSON 解析，不能以 YAML 回退掩盖坏 JSON。

读取器始终对完整实际字节计算 SHA256。缺少或 null expected SHA 可以加载通用包，其 `source_authenticity` 仍为 unknown；即使声明 SHA 匹配，也不能据此自授来源批准。历史书籍 Catalog 使用同一读取器和准备算法，但要求 expected SHA；v2 ArchiveProvider 的编译期已批准历史 SHA、continuity 一致性和操作末尾重读继续保留。source 缺项不会自动补成通过。

包身份保持原 stage1 metadata 算法：实际 SHA、精确角色、旧格式别名、命名图及实际加载旗标参加 compact/排序/ASCII JSON digest；路径和输入方式不直接参加。同内容从 path 换到内联 bytes 身份相同，改变实际 bytes 会拒绝同 ID/version；不能以缺少声明 SHA 复用旧版本。

格式缺失时资产 DTO 保留 null，实际 RDF 加载默认 Turtle。显式加载 null 为 false，shapes 默认不加载；非 RDF format 原样保留。加载到命名图和 dataset syntax 的限制继续委托唯一 DatasetSession 和现有 RDF parser。新的 JSONLD/TriX alias 识别已经接续，但实际执行仍具体返回尚未迁移，不能将 metadata 成功称为格式执行通过。

DatasetSession 唯一 packages map 保留第一次登记的完整资产及身份；Catalog 不另建 runtime cache。重复 load 先重读并验证全部输入，再返回首次资产、不重放 RDF；clear/update 后仍保留登记，重复包不会恢复被删数据。最后一个 RDF 失败不改变图、revision、cache 或登记；非空新包仅提交一次修订，零 quad 包只登记。源绑定、工程事实、治理采用和发布仍另取证。

原预期与独立空包 digest 在私有 `var/package-input-evidence/ORIGINAL-EXPECTATIONS.json`，日志和源码/生成身份在同目录后续 receipt。JSONLD 的旧 graph_name/命名图处理、TriX parser/serializer、Python 任意对象强制转换和 PyYAML 1.1/1.2 差异继续在全库目标中明确保留；本次没有将这些未验范围签为完成。

