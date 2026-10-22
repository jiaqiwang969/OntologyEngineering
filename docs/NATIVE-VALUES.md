# 有类型原值

状态：隔离候选，原值保存层；完整文件解析、图写入和HTTP导入尚未接通。

工程本体要求信息不能在迁移中丢掉含义。specs定义SemanticNativeValueEnvelope，经真实ggen生成；Rust runtime适配委托既有semantica-json的native_value模块。Project、来源权限、模型预算和工程采用均没有新增入口。

协议semantica.native-value/1采用扁平后序节点：整数以规范十进制字符串保留，float64以16位十六进制IEEE位保留，字符串以Unicode码点保留（包括孤立surrogate），list/tuple/set/frozenset及字典键类型与顺序独立保留。节点引用只是数据表示，不承诺Python指针身份、对象别名或Python相等/哈希运算；容器成员不会被自动去重或字符串化。

调用宿主给出节点数、引用数、标量存储字节、深度与实际wire字节限额；无默认扩权。循环/前向/悬空/不可达引用、非法码点、非规范整数及不可哈希的字典键拒绝。扁平wire支持深值，普通JSON桥仍保留128层保护，并在复制共享子值前检查展开节点和wire长度。逻辑限额不冒称进程RSS上限。

无损普通JSON桥拒绝NaN/inf、surrogate、非字符串键、tuple/set等类型丢失；看起来像协议标签的普通JSON对象继续是字典。原始wire先经严格JSON和闭合shape检查；旧严格JSON方法不因增加数据表示而放宽。该桥不是Python json.dumps的有意类型转换实现，也不代替原文件JSON/CSV解析器。

六份真实原HTTP导入后的typed状态用作暴露开发回归，逐个类型、字典键、码点、IEEE位和容器顺序核对。源文件原字节、原解析行为、工程适用、RRSI独立迁移收益仍另验。下一步接原解析与同一DatasetSession的有类型状态，不能把仅能往返的数据载体宣布成完整导入器。

原文件字节入口的解码阶段见 [原JSON字节](ORIGINAL-JSON-BYTES.md)。码点保存已单独实现；语法解析、图状态和HTTP导入尚未接通。

现有生成合同新增[原JSON解析入口](ORIGINAL-JSON-PARSER.md)，从原文件字节直接接续同一值层；不赋予图写入或工程采用权限。
