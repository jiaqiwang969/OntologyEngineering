# 原JSON语法到有类型合同

工程本体规定含义 → ggen生成合同 → Rust Semantica执行 → RRSI检验反馈。资料内容在进入工程对象、条件和来源关系之前，先保存其真实值及解析失败，避免把解码或转换失真当作工程事实。

显式 `semantica-json::original_json::parse_original_json` 复用原字节解码，从码点直接产生 NativeValue。普通合同/凭据仍走旧 strict_json。当前原行为档是CPython 3.13.11默认十进制整数4300位限制，非所有解释器配置；NaN/Infinity、负零、surrogate、重复键末值及首次键位置按原行为保留。字典覆盖后的废弃节点在最终后序整理中去掉；原解析资源仍计入这些已处理值。

解析、容器关闭、最终节点整理与释放均使用扁平结构和迭代工作栈。原1200层成功值已对照，不把旧普通JSON桥128层保护强加于原文件解析。宿主显式提供原字节/码点以及解析工作区节点、引用、标量和深度限额；数字暂存也检查剩余标量额度。限额为逻辑计数，不冒RSS。NativeValue的wire限额在编码阶段检查。直接解析函数不创建服务截止时间、Project、图会话或来源授权。

`semantica-runtime::native_values::parse_import_json` 将原字节解析结果送入既有真实ggen的 SemanticNativeValueEnvelope，复用原encode和严格有类型wire，而非经过普通JSON再猜回丢失类型。共享合同、来源治理和工程采用各自负责；没有另建知识图。

原解释器两批共3853份样例：660成功，3193失败。比较完整typed值、IEEE位、码点、字典顺序及原错误类别/文字/字符位置/行列。首版在Unicode转义刚好被截断时给出不同诊断，失败及源码保留，按原观测修复后通过，未改预期。属于开发暴露回归，不能声明穷举原行为、未见迁移或RRSI收益。

下一步接原导入节点/边构造、同一DatasetSession图/索引的有类型状态及实际HTTP生命周期；CSV另行迁移。解析成功不证明本体语义、工程适用性或实际项目制造验收。完整原库/平台、独立接收与正式skill合并继续单独验收。

后续新增[同会话有类型图记录存储](NATIVE-CONTEXT-RECORDS.md)，复用原context图与DatasetSession；原值索引/变更、字段投影及HTTP入口仍待迁移，存储成功不改签为整条导入链完成。
