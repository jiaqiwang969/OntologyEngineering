# 历史运行时与当前 Rust 执行边界

工程本体论规定含义，ggen 生成合同，Rust 能力库执行，RRSI 根据固定判据检验反馈。
当前执行身份由 Rust 来源、生成合同、工具和实际产物共同建立。历史 Python archive/RECORD 身份仍可由 Rust 解析；它不证明当前程序执行。

隔离复现发现 `historical-contract-tests` 曾允许 public probe/build/preflight/install 重新走 Python 运行时。实际 probe canary 被执行，四个固定 retirement 预期均未满足。修复后，不论该 feature 是否开启，这些公开入口都直接返回原 retirement error，不调用外部程序，也不创建 cache、输出或 runtime。

Python 探针程序和旧 wheel 构建执行已从 production 实现删除。历史安装恢复代码仅在 `#[cfg(test)]` 的私有 Rust unit-test 模块中编译，用于保留原恢复算法的 fixture；feature 不能把它加入正式库。原始源码只读 Git 检查、ZIP/RECORD/PEP 610 身份解析和有界 archive 运输仍保留。

这项验收只证明旧运行时不能被重新启用。它不将被退役的 Python build/probe 原测试改称 Rust 等价，也不证明完整 Semantica、教材采用、CAD 原生验收或工程闭环完成。固定全代码迁移清单及未知项继续保留。
