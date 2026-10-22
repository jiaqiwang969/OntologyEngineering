# 原JSON文件的字节解码

工程本体规定含义 → ggen生成合同 → Rust Semantica执行 → RRSI根据反馈检验改进。资料原值进入本体之前需要保留真实内容及失败；不能把损失性转换后的值当作来源原值。

`semantica-json::original_json_bytes::decode_original_bytes` 是原导入行为的显式技术函数，探测 UTF-8/16/32 和 BOM，返回包含孤立代理码点的 `Vec<u32>`。它不强迫输入成为 Rust String。编码错误保留 codec、字节区间及原因；UTF-8-sig 的错误位置相对去掉 BOM 的字节，UTF-16/32 相对原字节，按原运行时验证。

宿主显式提供输入字节与输出码点上限，分别在处理前和追加码点前检查。限额表示逻辑数量，不声称限制全部 RSS；不新建服务预算、Project 或来源授权。旧 `strict_json` 与有类型 Envelope 准入保持原规则，不能把此解码器换成合同或凭据 JSON 的入口。

设计约束及验证关联由 specs 正本和真实 ggen 维护，算法为手写 Rust；没有生产 Python 依赖。1550 份实际原运行时观测覆盖成功码点与编码错误。另冻结了 454 份原 `json.loads` 语法结果供下一阶段使用，尚未实现对应 Rust 语法解析，不能将它们计作解析已通过。

下一依赖为码点序列 → 原 JSON 语法 → NativeValue → 同一 DatasetSession 的图/索引变更 → 原 HTTP 导入。CSV、原对象行为、独立接收、完整平台和工程采用分别验收。这些开发回归只检验本次改法有无破坏既有行为，不构成 RRSI 收益或未见迁移证据。

后续已新增[原JSON语法候选](ORIGINAL-JSON-PARSER.md)，覆盖字节/码点到NativeValue及生成合同；本文此前的阶段边界保留历史含义。图/索引和HTTP导入仍待接通。
