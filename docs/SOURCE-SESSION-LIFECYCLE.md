# 有源连续动作与观察时机

公共 Turtle 接入前，先在同一 DatasetSession 收齐剩余动作的状态变化。快照、差异比较、SELECT/ASK/CONSTRUCT 辅助方法、绑定构造早期失败与原 DATA-v2 受控历史沿既有 owner 执行，原 source-session `/1–3` 准入不扩展。

原版读取 Dataset.contexts 时，在缺少默认上下文的情况下会初始化 Dataset 的命名空间管理器。它属于状态观察的效果；Rust 在真实默认上下文登记位置调用已有命名空间 owner，保持此前四元组观察失败的先后关系。命名空间 witness 自身仍是只读捕获。

QUERY 和 UPDATE 共用实际初始值构造。查询缓存命中先返回；未命中时构造绑定，进入后端后才登记隐式前缀。构造错误保留原 builtins 身份；空查询和辅助方法类型不匹配分别保留 SemanticInputError 与 UnsupportedQueryTypeError。辅助类型检查位于查询和缓存写入之后，不假造回滚。

生成的命名空间状态沿原合同复用。新增内部 capture_until / validate_namespace_witness_until 消耗调用者的绝对期限，并在复制和计字节时检查；超时与资源拒绝保留本地错误，不授予工程结论。

独立冻结25组136动作，另9组38动作刻意不插入 source-state 观察，以区分动作与观察效应。原版 diff 的 as_dict 包含 changed 属性；最初 dataclasses.asdict 漏读该属性的原件与失败保留，修正观察器后重新向原版取证，不删除 Rust 字段或改验收答案。实际通过、二进制与平台回执见私有维护记录。

公共 Turtle 仍需完整结果、原始遍历与命名空间因果见证、同源 host/CLI 和新根安装。原 Turtle writer 使用真实 Memory 索引顺序、literal 值缓存及排序；只凭四元组集合与前缀状态不足以证明输出全文。结构检查不证明执行真实性。正式选择按当前维护状态；原63项、CAD16、完整 Semantica 和最终计量继续保留。
