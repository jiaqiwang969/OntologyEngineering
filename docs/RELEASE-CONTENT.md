# 发布内容检查的责任

工程本体论规定对象、来源、条件与证据的含义 → ggen 生成合同 → Rust Semantica 检查实际内容 → RRSI 在固定目标和判据下比较改进。这里的检查结果不能改变工程采用条件。

`release_validation` 实现锁定原 `SemanticRuntime.verify_release` 的十五项内容检查；原 runner 增加两项后按检查名称合并并排序。同名检查后值覆盖前值。报告、PROV 记录校验、顺序、前驱链、完整 bundle 摘要和 receipt bindings 由同一 `execution_provenance` 算法处理。

`verify_legacy_content_data` 接受历史 DTO 数据，仅比较内容。独立预期来自未修改的原 Semantica 实际 constructor、RDF、包加载、runner 和发布检查的 65 组运行；输入、完整回执和判定保持原值。测试投影保留原资产字节及 SHA，未读取新 Rust 输出来生成预期。原 RDFLib 快照和新 RDFC 快照保留各自算法身份，这些数据测试不证明旧快照算法已迁移。

`verify_rust_content` 从当前唯一 `DatasetSession` 借用注册包并取得实际 RDFC 快照，使用显式 Rust source profile。它不伪造历史 Git 标识、不创建并行 registry 或 Dataset。来源值匹配属于内容对照，真实执行仍须同一 runner 的保留记录；编译来源认证、完整库迁移与工程采用分别取证。

`book_runner::release_content` 把这些检查连接到同一 `BookRunnerState` 的实际保留执行。独立 profile `semantica.release-content-retained-rust/1` 在实际报告上解决已实现的有界内容检查，生成完整 PROV，再读取私有保留的包声明和场景结果进行 17 项检查。原 `run` 输出及其历史 capability profile 保持原值。`verify_release_content` 从私有原回执重建整个观察，拒绝换实例、数据变化、来源观察变化或自行重算摘要的伪造。

这一入口可以得到内容 `complete`，同时保留 `runner_execution=retained`、`runtime_origin/source_authenticity/engineering_adoption=unknown`、`formal_release=not_published`。真实执行和内容验收不受无关的永久 unknown 阻挡，正式发布与工程采用仍按各自证据判断。

原数据判据允许格式合法的不同 runtime version/artifact 值，也允许重新计算完整摘要后的部分元数据变化。Rust 保留这些原判据；生产入口另核真实实例、当前来源及输入身份，不能将自报的 `passed` 当执行资格。内容 `complete` 不表示全库、平台或机械项目完成。

本模块的规范、实际 ggen 生成、锁定编译、原预期回归和独立反例须逐项绑定当前源码后验收。隔离副本的证据不继承到正式根或其他改动后的源码。
