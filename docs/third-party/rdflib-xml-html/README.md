# 私有 XMLLiteral / HTML 源码候选

两项均有纯 Rust 算法，入口是 `pub(crate) fn normalize(datatype: &str, text: &str) -> anyhow::Result<String>`。转换器解析失败时按原 RDFLib 行为保留完整输入词法；未知 datatype 才是调用错误。没有 Python 运行时、网络模型请求、业务 stub 或占位成功。

实际需复制的 Rust 源码只有相邻的 `rdflib_xml_html.rs` 和 `rdflib_xml_name_ranges.rs`。前者内部以 `#[path = "rdflib_xml_name_ranges.rs"] mod xml_names` 引入后者。`driver.rs` 是私有比较程序，不是运行时模块。HTML 移植部分的 MIT 全文已在算法源码顶部，`NOTICE.md` 和 `LICENSES/` 保留其来源及其余许可证。

XML 实现包含包裹后的 fragment 解析、Expat 名称和 namespace 校验、attribute 空白与字符引用区别、namespace 声明先于普通 attributes、minidom escaping / comment / CDATA / PI / empty-element 写法，以及原 `_writeXML` 额外的 wrapper-empty 剥除。原输入 `<rdflibtoplevelelement/>` 的结果为空。名称表是通过真原 `rdflib.term._parseXML` 对整个 BMP scalar alphabet 的 PI target 身份观察生成的完整字符类别数据，不是用例键查表；数据、脚本、二进制来源 SHA 均保留。补充字符可作内容/字符引用，不能作此锁定 Expat 的名称字符，亦由 [Expat 2.7.3 官方源码](https://github.com/libexpat/libexpat/blob/R_2_7_3/expat/lib/xmltok.c) 的四字节名称分类器确认。

HTML 使用真实 html5ever tokenizer/tree builder、自有 DOM 和按 html5rdf 1.2.1 默认参数移植的 serializer/optional-tags。兼容层处理旧 command/template/rb/rtc/search/dialog 分支、Unicode BOM、命名引用的严格错误策略和 foreign CDATA。内部 tag aliases 含 NUL，不能与有效原输入的普通 tag 混淆；foreign command 亦保留独立于 param 的栈身份。错误策略保留所有 parser errors，仅去掉精确 `Non-space table text`，因为原 `InTableTextPhase.flushCharacters` 对此执行 foster parenting 而不报错。并没有通配忽略 parser errors。

新增 direct parser 依赖候选为 `html5ever = "=0.35.0"`。现有 `quick-xml = "=0.37.5"` 和 anyhow 仍使用基线坐标。html5ever 的 [官方 manifest](https://docs.rs/crate/html5ever/0.35.0/source/Cargo.toml) 声明 MIT OR Apache-2.0、MSRV 1.70；实际完整私有闭包已在 Rust 1.95.0 编译。私有 Cargo.lock 有 44 个外部坐标，其中相对只读 349 基线增加 19 个；候选并集为 368。坐标、checksum、实际 cargo metadata、许可证来源、原/候选 lock SHA 在 `DEPENDENCY-CANDIDATE.json`、`ACTUAL-CARGO-METADATA-1.95.json` 和 `LICENSES.json`。没有改 T/R10 的锁，也不能把私有 44 坐标锁冒充 R11 完整闭包。

mac 0.1.1 与 match_token 0.35.0 发布包未带单独 license 文件，实际发布的 manifest 声明已保存；mac 的 README 声明亦保存。match_token 的共享仓库许可证另从其 `.cargo_vcs_info.json` 的精确 commit 获取。没有替 mac 推断或补造版权文本。

四组原输入/预期在各自 Rust 执行前冻结，均由真完整原 `SemanticRuntime(profile='rdf')` constructor、load 和 snapshot 得到，另有真 Literal 的词法结果。原 oracle 均用只读 venv、Python `-B`，禁用 socket 网络；已有原预期未修改、未重签。

| 固定组 | XML | HTML | Rust 1.95.0 locked/offline |
| --- | ---: | ---: | ---: |
| MAIN | 57 | 82 | 139/139 |
| EXTRA | 43 | 40 | 83/83 |
| POLICY | 16 | 110 | 126/126 |
| BINDING | 0 | 10 | 10/10 |

最终同源终端输出为 `ACTUAL-FINAL-V2-RUST-1.95-*.json` 与对应 log，运行命令保留在 `run_final.sh`。原 unimplemented baseline 0/139、首次 137/139、Unicode/旧语法边界 48/83、CDATA 回归 138/139、alias 身份边界 5/10 的真实 RED 均保留；最后一项的修复前源码/哈希也已保存。早期未在执行时冻结的源码字节 SHA 不被补签成资格。

**有限资格是 358/358 词法 String 对照。** 原完整 constructor 的 mutation/snapshot 预期虽已冻结，Rust driver 尚未执行统一 constructor、dataset snapshot 或 revision/source binding 对照。剩余全域 parser 差异、Python recursion/call-stack 影响、任意资源极限仍为 unknown。R11 必须 source-only 合入后重新生成、编译、锁定完整依赖闭包并验证完整消费路径；这里的通过状态不继承为 T/U/F 或 R11 资格。source identity 和 adoption 状态均为 unknown。

此 pure helper 不实施宿主资源预算。Root 计划在 DatasetSession 调用边界前置 XML/HTML UTF8 byte 与结构输入限制，后置输出限制；资源拒绝必须由调用边界直接返回 RustErr，不能进入此 helper 的 parse-error 词法保留分支。`walk_html` 是递归 DOM walker，HTML5 合成节点使输入 `<` 数量不能充当精确 DOM depth；这里没有证明宿主界的全域递归/资源上界，也没有继承宿主 owner 原子性或预算测试资格。
