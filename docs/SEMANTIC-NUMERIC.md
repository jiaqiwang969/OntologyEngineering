# 共享数值原语与验收边界

`semantica-numeric` 是 Rust Semantica 的共享数值模块，负责任意精度整数、带显式上下文的 Decimal，以及有界 binary64 转换和标量运算原语。它让数值表示、舍入、失败和资源限制有一个共同实现，并为既有 duration 消费者提供算术。模块状态为 `source_only`；本次源码是否通过构建、冻结测试、旧消费者回归或正式安装，分别看绑定当前源码的执行回执。没有新增公开 `/5` 查询版本。

工程本体先说明需要保留的含义：[数值规范](../specs/modules/semantic-numeric.toml)将需求、模块、来源、真实函数和验证连接起来。该规范经现有 Rust 规范化和锁定 ggen 生成合同及检查映射；手写 Rust 核心执行复杂算术。生成成功证明合同能够生成，实际计算仍须用提前冻结的输入和预期检查。RRSI 改进流程据此保留原目标、原失败、候选源码和独立预期，再判断有限改动是否可接收；候选输出不能反过来改写验收答案。

## 当前实现

| 能力 | 精确边界 |
|---|---|
| 整数 | `BigInt` 加、减、乘及一元正负保持精确，受显式资源预算约束。文本转换的位数限制只在解析和输出边界应用。RDFLib 乘除链使用 Decimal 上下文，不能据整数乘法原语推断其表达式提升规则。 |
| Decimal 表示 | 有限值保存独立符号、`BigUint` 系数和 `BigInt` 指数；尾零、scale 和负零属于表示的一部分。Infinity 与 NaN 保留符号，NaN 另保留 signaling 和 payload。 |
| Decimal 运算 | 加、减、乘、除和一元正负逐次按显式 HALF_EVEN 上下文处理。默认原型为 precision28、Emin=-999999、Emax=999999、clamp0；默认捕获 InvalidOperation、DivisionByZero、Overflow。它是固定源的默认原型，不证明历史应用未修改上下文。 |
| 舍入与信号 | precision 与 Etiny 在一次舍入中处理；保留精确除法的期望指数、非终止商、进位、次正规数、上下溢出和零符号。成功结果带本次运算的完整信号集；trap 返回主信号和故障，不返回数值。完整 trapped flags 属于核心显式合同，未声称已观察到 C backend 等价。 |
| 词法和输出 | Decimal 精确构造不舍入，按固定纯 Python 源先 strip 再删除全部下划线；整数有独立的空白和下划线规则。Unicode 数字复用 `semantica-json` 的唯一表。fixed 输出保留 scale 和负零，正指数零输出 `0` 或 `-0`。 |
| binary64 精确构造 | `decimal_from_binary64` 从原始 IEEE 754 位直接构造 Decimal，不走浮点显示或字符串重读，也不按算术上下文舍入。`0.1` 保留对应二进制值的全部精确小数位；负零保留符号，整数浮点值的指数为0。Infinity 保留符号；所选 quiet NaN 按固定源的 repr 规则变为正 quiet、空 payload。 |
| 浮点构造信号 | `DecimalContext::from_binary64` 先核上下文及其预算，再在值转换前产生 FloatOperation；捕获时没有数值，未捕获时返回精确数值和本次信号。不提供持久 Context flags，也不实现 `create_decimal_from_float` 的后续舍入。 |
| binary64 反向转换与运算 | Integer/Decimal 到 binary64 使用精确整数比值作一次最近偶数舍入；Integer 溢出报错，Decimal 溢出得带符号无穷。四则及一元运算保持源步骤；除以正负零单独报错。目标运行环境和 query 能力另行接收。 |
| 预算 | 输入字符数、系数位数、输出字符数及实际生成的十次幂分别有界。大指数可紧凑保留，无法输出的展开报资源失败；极小数和相距很远的加数有无需完整展开的路径。资源失败与数值 trap 分开。 |

默认预算是各项4096，消费者可显式选择其已论证的预算。算术容许由两个有界输入形成的临时值；核心 `check_digits` 先按每位四个二进制位预检，再转换文本核精确位数，因此可能先形成约1.205倍位数上限的临时文本才拒绝。它仍有界，但不是“任何分配之前均已核准精确十进制位数”。词法输出采用逐次受限的数字缓冲，并在展开指数零之前检查完整输出长度。

binary64 → Decimal 精确转换的临时量由输入格式固定限制：分母最多为 `2^1074`，十进制系数最多767位；构造后再核系数和指数输入预算。该路径使用有限的5的幂，不请求10的幂或显示输出，因此不会把10的幂的预算误用为5的幂的限制。对于已满足上下文准入的构造调用，FloatOperation trap 先于值构造的系数预算失败；无效上下文及上下文自身预算仍先拒绝。

## 原始来源与验证

来源、改写范围、锁定依赖及保留许可证见 [PROVENANCE.md](../crates/semantica-numeric/PROVENANCE.md) 和 [LICENSE-PYTHON.txt](../crates/semantica-numeric/LICENSE-PYTHON.txt)。Decimal 算法依据固定 Python3.13.11 `_pydecimal.py`，保留其 SHA256；Rust 将字符串系数改为 `BigUint`，上下文、信号和预算改为显式参数。词法的 Unicode 正则接受域与 C `_decimal` 的全部构造上限并未合并成一个已验证声明。本轮没有执行旧 Python 来生成新的原始结果。

[公开冻结用例](../crates/semantica-numeric/tests/fixtures/source-derived-arithmetic.json)是实施前私有冻结记录的路径脱敏副本：70组中69组来自源码和数学推导，1组是主动输出预算合同。私有原件 SHA256 为 `17537b2c280da7ba85fe2939a05081653661564e340bb65c7ce5bd73f257a21a`。脱敏不改变 cases、context、值、信号或分类；保留摘要标签本身不能替代实际内容比较和当前执行记录。

[Rust 适配器](../crates/semantica-numeric/tests/frozen_cases.rs)逐步检查数值种类、符号、系数、指数、NaN payload、信号和失败族，并把保存的数值直接传给下一步。它不把中间结果转字符串重读，也不只检查最终数学值。例如 precision3 下 `1/3` 再乘3得到 `0.999`；把整串算式精确求完后只舍入一次会错误地得到1。另有12个核心单元测试覆盖词法、字长、负零、格式化、预算、次正规舍入和紧凑运算路径。测试定义存在与这些测试在当前源码实际通过分别报告。

[binary64 冻结数据](../crates/semantica-numeric/tests/fixtures/source-derived-binary64.json)另有16组实施前固定的来源和数学预期，私有原件摘要为 `f06930161fcbe451e0fcdfa6e118c46d44af6f5aeb059b10dcc97266efa92892`。14组精确转换覆盖正负零、0.1、1.0/1.5、2^53边界、最小正规/次正规、最大有限值、正负无穷和所选负 quiet NaN；2组核构造信号与 trap。[对应 Rust 适配器](../crates/semantica-numeric/tests/binary64_frozen.rs)按原始位输入并比较完整表示和每调用信号；初始 flags 为空，不据此推定持久上下文正确。另1项主动资源测试独立检查低系数预算下的 trap 顺序和十次幂/输出预算用途，不混入16组来源预期。

历史首次 crate 冒烟记录包含15项 Rust 测试：原12个单元测试、70组算术适配器、16组 binary64 适配器和1项主动资源测试。案例数量与 Rust 测试数量分开计数；原回执仅约束当时源码，不计入当前增量验证。当前执行资格由绑定实际源码的独立回执确定。

源码、Cargo manifest、许可证与来源记录、适配器和公开 fixture 一并进入同一编译来源绑定。`semantic_numeric` 依赖 `semantic_json`，`semantic_runtime` 消费数值模块；本体约束映射实际函数和测试。源码、预期或依赖变化后重新建立执行身份，旧独立审查和旧通过回执不改签。

## 既有 duration 消费者

`rdflib_literal_normalizers.rs` 的 duration 算术迁入共享核心，同时保留旧 `Dec` 适配器的显示、`split12`、负零到整数零的投影和失败传播。月份除法仍先舍入到28位，再精确向下取整；直接对原值整除会改变临近整点的结果。旧 duration 的科学计数显示与新 Decimal fixed 输出用途不同，不能相互替换。

原 duration 入口限制10000个 UTF-8 字节。适配器为这一路径显式保留输入和舍入中间量所需预算，不默默改成核心默认4096。新增4个公共测试核对表示与显示、舍入后 floor、构造边界及旧输入预算。既有282和310组完整构造器输入仍以原摘要、mutation、snapshot 和失败结果约束迁移；它们需要显式提供私有原件并实际执行。公共合成测试、原私有消费者回归和70组源码推导原语测试分别取证，不能互相顶替。

## 私有查询数值接线

核心仍不拥有 RDF、Dataset 或 AST。实际查询消费代码在同一个 `spareval` 中：`numeric.rs` 接收已求出的操作数并调用共享数值核心；`expression.rs` 的原有 AST 分派器负责求值顺序和捕获边界，没有增加第二套表达式解释器。`rdflib760-nullable-internal` 默认关闭，私有准备入口仅接收真实查询文本；同一 session 连到实际 parser、关闭整体优化的共享 planner、现有 evaluator 和同一 Dataset。生产执行方法 `execute_presence(self)` 只消费已绑定的 plan、初始上下文和 Dataset，最终 decoder 也在首次错误后停止；plan/evaluator/initial getter 仅供测试检查缓存或选择已有物理分支，不向生产调用方开放替换计划。本增量从已接收的有限 RC.4 源码继续开发；该次 RC.4 不包含 SourceLiteralV1。私有增量不自动改动正式入口，没有新增公开 `/5`。

每个实际 cell 同时携带 RDF 项和自己的数值状态。RDF 相等与 Hash 字节仍只看真实 RDF 身份；数值缓存的系数、指数、零符号和来源不参与去重。相同 RDF 可以保留不同缓存，不能用一个全局“RDF → 数值”表替代。例如 DirectString `1E+2` 归一成 RDF `100`，缓存仍是系数1、指数2；`100` 除以它得到 `1.00`。另一个由 `100` 构造的同 RDF 项，缓存指数是0，不能相互覆盖。

| 输入来源 | 当前处理 |
|---|---|
| 查询 typed string，如 `"1E+2"^^xsd:decimal` | 按既有 Literal 的 datatype override 路径解析缓存，保留查询中的 RDF 词法 `1E+2`。 |
| 实际数字 token，如裸 `01` | parser 的真实构造回调按 kind、sign、magnitude 执行，在显式归一策略为 true 时得到 RDF `1`；不会与 typed `"01"` 混为一谈。负 Decimal 的 VALUES token 保留原解析阶段 TypeError，表达式一元负号另走运算路径。 |
| DirectString | 先解析数值缓存，再按调用方明确的归一策略构造 RDF；不从归一结果倒推原缓存。 |
| KnownCached | 调用方提供实际 RDF 与缓存；RDF datatype 须属于支持的数值类型且与缓存声明一致，实际 NumericValue 家族可以不同，不按 datatype 重构它。工厂本身不证明调用方的外部来源声明。 |
| 已归一导入或 Dataset 扫描 | 默认 Unknown；允许普通运输、COUNT 和 SAMPLE。算术或数值 EBV 真正需要缓存时明确失败，不借用相同查询常量的缓存。 |

`NumericDataset` 包装并转发同一个底层 D，未复制图或新建存储。Known 值内部化后由底层 D 回读，若词法或 datatype 被改变则报 `LossyNumericBackend`，不把旧缓存挂到另一个 RDF 身份上。初始值只构造和内部化一次，根 tuple 编码复制完整 cell；BIND、Project、SAMPLE、单一分组键及聚合 literal 路径使用相应缓存。COUNT 的结果由同一 session 构造为 Known integer。

默认 `IntegerDecimalV1` 私有算术覆盖受预算约束的整数和 Decimal：整数加减不缩窄到 i64，乘除链先进入 Decimal，并在每步使用显式上下文。缓存 scale、负零和特殊值随实际结果保存；同一 session 累积本次查询产生的信号。原始尾操作数的读取顺序、语法变量与子表达式的区别仍由同一个编译器控制。乘除内部的 InvalidOperation/DivisionByZero 转为原局部 SPARQL 错误；一元和加减的 Decimal trap、Dataset、未知缓存、资源和不支持错误保留 fatal 边界。FILTER 对裸变量传播 fatal 属于主动安全合同，不能声称与原 Python broad-except 完全相同。

当前新增的 `And` 只接收两个操作数，且直接子项不能再是 `And`：先按顺序读取全部原始值，再按原 `all(EBV)` 规则短路。变量重新抛出的错误和 fatal 会停止原始值读取；EBV 新产生的 SPARQL 错误保留单次捕获及新身份。三项及嵌套源码的平坦/括号边界尚未由现有 AST 完整运输，明确拒绝，不从二叉形状猜测原意；旧 And 短路方式单独保留。

当前私有 `Slice` 把 OFFSET 与 LIMIT 放在同一消费边界：先消费 OFFSET，再检查 LIMIT。因此 `OFFSET 1 LIMIT 0` 也必须保留跳过阶段实际遇到的 fatal，随后停止读取。这个先后顺序依据固定 CPython 3.13.11 `islice_next` 源（摘要 `9ca79a4c9ad30b3fa0c42767f93220fd4bb4fdf16cee59b6fcf23426519aa63f`）；普通两行 VALUES 的 OFFSET 对照仍检查只留下第二行。已观察到 fatal 后停止是这里的合同，不表示完整 Join/Union 原始调度或任意计划已经对齐。

递归属性路径 `*`、`+` 当前在私有编译入口明确拒绝，递归编码同样拒绝序列等嵌套位置中的递归路径，例如 `(p/q*)`。原因是旧闭包算法会缓冲错误并继续扫描，尚未完成这一消费方式的独立接收。真实 prepare → bound compile 测试检查三种形状在扫描前拒绝；默认 evaluator 与非闭包路径的原实现保留，普通 BGP 继续由 QN27 独立检查。这里没有宣称递归算法已经迁移完成。

默认 `QueryNumericSession::new` 始终选择 `IntegerDecimalV1`（`integer-decimal-v1`）：float/double 叶子、真值及 COUNT/SAMPLE 运输继续可用，混合和一元/二元 float 算术仍在原来的到达分支拒绝。首个乘除操作数若是 float，仍先经过 Decimal 构造边界，产生 FloatOperation 或 trap 后才可能报未支持；不会为拒绝而预扫描整链。旧 QN24 和对应单元测试继续实际运行，不只保留历史原件。

### 显式 mixed binary64 能力

新增能力只由 `new_with_capabilities(..., MixedBinary64V1)`（`mixed-binary64-v1`）明确选择。能力绑定 session 后不可变，没有提升已准备查询的 setter；prepare 签名、真实 parser、同一 AST compiler、Dataset、消费者和最终 decoder 继续使用同一个 Rc session。它扩展现有一元及四则，不开启公开 `/5`，也不改变默认查询能力。

加减与乘除的转换顺序不同。加减在每步按实际缓存值转换 Decimal/float，再按原 RDF datatype 规则提升；大 Integer 转 float 的 OverflowError、sNaN 转 float 的 ValueError 仍是 fatal。乘除的第一个 float 先成为精确 Decimal 并产生 FloatOperation；之后遇到 float rhs，才把 Decimal 累积值转为 binary64。因此 `0.1e0 * 1` 和 `1 * 0.1e0` 的结果种类、舍入和信号可以不同。已经成为 float 的累积值再乘除 Decimal rhs，按源规则报 MixedNumericTypeError，不能擅自做对称提升。

只有乘除自身的捕获边界把 float 除正负零转为原 returned SPARQL 错误；它保留原 Decimal InvalidOperation/DivisionByZero 的局部捕获。FloatOperation trap、IntegerOverflow、SignalingNaNConversion、MixedNumericTypeError、资源、Dataset 和未知缓存错误继续是 fatal。反向转换不读取 Decimal context，也不发 Decimal signals；只有真正的 Decimal 构造或算术产生其信号。同一编译器仍先求完整原始 tail，再进入对应数学步骤，括号子表达式保留独立边界。

binary64 数值缓存与 RDF 文字和 datatype 分开运输：xsd:float 与 xsd:double 都使用 Python 语义的 f64，不强转成 f32；一元和乘除默认结果 datatype 来自实际值家族，加减另保留原提升规则。BIND、SAMPLE、后续算术不能用较短 RDF 文字重建缓存。唯一浮点显示函数在 `semantica-json`，JSON/native 和 query 共用；保留负零、选定格式切换、`nan`/`inf`/`-inf`，不冒充全部原词法与最短显示行为已接收。

`binary64-conversion-budget-v1` 是主动资源合同。Integer 先核系数预算，Decimal 先核完整输入表示，随后才处理特殊值、零或确定上下溢出；不把会变零的输入当作绕过预算的理由。非零 Decimal 的 adjusted exponent ≥309 可直接给 Infinity，≤−325 可直接给带符号零，不物化巨大幂。中间范围在展开前检查**原表示的绝对指数**与十次幂预算，不能先消掉尾零或改称5的幂来绕过。core 转换不输出文字，不继承整数文本位数或输出字符限制；query 真正输出 RDF 时仍核输出预算。现默认各项4096不变，用例扩大预算必须显式记录。这是新反向转换的成本合同，不改写原 binary64 → Decimal 的16组预算规则。

本轮独立冻结 `QUERY-BINARY64-FROZEN-01.json` 摘要为 `69974deeacb163228a361b5b3d03de93ee2fe437c1d5a95361b7d7de62ed34dd`。它有42项核心预期（37项来源/数学、4项主动资源、1项来源溢出兼主动禁止文字中转的控制）、21组实际查询族/54次运行和2项旧能力对照；公开副本分别是 [核心 fixture](../crates/semantica-numeric/tests/fixtures/binary64_mixed_cases.json) 与 [query fixture](../vendor/spareval/tests/fixtures/query_binary64_cases.json)。独立整数/有理数中点见证检查已给定的有限预期位，不运行原 Python 或拿候选输出定义答案。硬件算术 NaN 的 payload/sign 不补猜为精确位预期，非默认 fenv、FTZ/DAZ、跨平台、历史可变 Decimal context、完整比较/ORDER 等仍开放；有限 MIN/MAX 由后述来源增量单独登记。原70、16、旧 query 与 QN24 继续是独立活跃检查；此处记录本轮源码合同与接收范围，最终通过须有新的源码绑定回执。

另列[构造信号与资源补充](../vendor/spareval/tests/fixtures/query_binary64_signal_resource.json)：私有原件摘要 `98ee2a3aed96c57f8aa44f7e67c6b1ca48d049ebfc993d84e257e3e66c864da4`，4次运行分别检查两种能力下的 FloatOperation trap 开/关。first-float 构造已经到达的信号，即使随后精确转换因系数预算失败也须留在 session；trap 开启时则先返回 FloatOperation。它由实现作者阅读候选路径发现缺口后按固定源补充，属于来源回归与主动资源合同，不是独立盲测，也不并入原42/54计数。trap 优先级由结构化错误区分，不冒称有未实现的内存分配计数；实际先失败再修复的记录另行保存。

[查询冻结数据](../vendor/spareval/tests/fixtures/query_numeric_cases.json)当前采用记录02（摘要 `2a63d9a7043935df5c925b5ed5285d4447a24c9f0d09a532432fd8c2442cf5fd`），保留28组、56个查询变体和显式环境。原01（摘要 `d42be1722ef47d25f72c1bb563eda15cd55c8c085e5151d09516800c3a1badbf`）及失败回执未改签：QN03前三个 DirectString 用例曾误填了与自身归一前提冲突的三个结果，按固定 `term.py` 源和 RDF 身份规则更正为词法 `100`、两个 DISTINCT 计数 `1`；后面的 typed-query 对照仍为词法 `1E+2`、DISTINCT `2`。02只改这三处预期，输入、预算、来源、缓存断言和其他预期全部保留。这三处已经曝光，属于来源复核后的开发回归，不再称为盲测或用候选输出来定义规则。[真实接收适配器](../vendor/spareval/src/eval/nullable_numeric_receiving.rs)逐组执行实际准备、编译、消费者及 presence decoder，比较完整结果、投影前缓存、指定信号和失败阶段。来源与主动安全分类分别保留；额外安全回归不混入冻结案例。这里记录源码接线与验收定义，当前最终通过须另有冻结源码绑定的执行回执；一次资源 Err 本身也不能证明分配前拒绝。

### 显式 SUM/AVG 与来源分组合同

`QueryGroupSemantics::{LegacyPrivateV1,SourceAggregateV1}` 是独立于数值能力的私有选择轴。六参 `new_with_group_semantics` 在准备前固定它，旧 `new` 和 `new_with_capabilities` 始终选择 LegacyPrivateV1；选择 MixedBinary64V1 不会自动改变 Group。SourceAggregateV1 接通同一个 parser、planner、raw compiler、Dataset 与结果解码器的来源聚合流程；本节规定其实现和接收边界，当前通过与否以实际源码绑定回执为准。默认 feature 关闭，公开 `/5` 仍拒绝。

| 步骤 | 来源合同 |
|---|---|
| 两元素 sum | 每行对“旧累积值、新值”重新从整数0开始，保留每一步 Decimal 舍入与信号。Decimal/float 同现时按先旧值、后新值惰性转 float。唯一 `python_sum_binary64_pair` 只处理两项 float 的 CPython 普通分支：`0.0+first`、一次 Neumaier 补偿及有限非零 correction；不跨行保存补偿，也不代表混合数值都可直接传给它。 |
| SUM update | 先取 raw 值，再读取/提升并写 datatype，之后 numeric/sum，最后写 DISTINCT seen。只跳过源指定的 raised NotBound；None、返回错误或非 Literal 的属性访问失败不冒充缺失。 |
| AVG update | 先 numeric/sum，再 datatype，最后 seen 和 BigInt counter。跳过 raised NotBound 与 SPARQLTypeError；普通 datatype promotion TypeError、资源和 Dataset 错误不被吞掉。counter 位于真实累加器，增长受明确预算约束，不改变旧 COUNT 的既有有界合同。 |
| DISTINCT | SUM/AVG 的前置求值在 update 的捕获边界外，通过后才再次求值，并在成功时记第二个对象。以 RDF 身份或返回错误对象身份判断 seen，不能按数值缓存去重；COUNT 自己的 missing 处理和 SAMPLE 的一次选择仍分别保留。 |
| finish | 空 AVG 返回 integer0。非空 AVG 按最终 RDF datatype 选择除法分支，再按实际缓存家族执行；dtype 与 cache family 可以不同。SUM 保留指定 datatype，AVG 按实际结果家族构造默认 datatype。需要 Decimal(float) 时发 FloatOperation，后续资源失败不能抹掉已发信号。 |
| 精确整数商 | `binary64_divide_integers` 对完整整数比值只舍入一次，避免先把两个大整数各自转 float 的溢出或二次舍入。先依次核两输入系数预算，再判零分母；商溢出报 IntegerOverflow，零/下溢保符号。没有 Decimal context、文字中转或额外 pow10 预算。 |

真实私有 parser 保留 `SourceGroup` 的 Implicit 与 Explicit，以及 `Raw(expr)` 与 `As { expression, variable }` 的区别。AS 通过同一个 Extend helper 先按源序执行，之后仅读 alias 作 key；Raw 直接走现有 raw compiler。裸缺失 key 可成为 None，返回错误对象仍是不同 key，不能先用额外 Extend 把它们都吞成缺失。key 的缓存随原 cell 保留而不参与 RDF Eq/Hash；最终输出由 aggregate bindings 构成，不能自动把 keys 复制成 SELECT 列。

源分支为真实重复 aggregate occurrence 保留独立消费者和次序，不因表达式相同而合并。裸投影变量通过隐式 SAMPLE 和 alias Extend 输出；HAVING 的采样转换总执行。隐式空组的 pending SAMPLE 经 alias 后为 ExplicitNone，显式空 Group 没有累加器，按源产生一行空 bindings。这是 presence 边界；原 Result 迭代与外层 DTO 删除 None 的规则仍另验。

创建 SourceGroup 输出迭代器时不读取子输入，不执行 update，也不发出这些步骤的数值信号；第一次 next 才初始化。请求第一组时先完成全部输入、key 和 update，再按 key 首次出现的顺序惰性 finish。输入、key 或 update 失败时，尚未产出任何组；后组 finish 失败时，前面已经返回的组保留，未完成的当前组不返回半行，然后只报一次错误并停止。没有上游 poll 的 LIMIT 0（OFFSET 0）不启动分组；LIMIT 还可避开后组 finish 及其信号。若 OFFSET 大于 0，仍按前述 Slice 合同先消费跳过的行，再检查 LIMIT，即使 LIMIT 为 0。旧 Group 的 hash 调度、键字节与原默认行为继续独立检查；不能用新分支的顺序倒签旧回执。

独立补充 `QUERY-SUM-AVG-DEMAND-SUPPLEMENT-01.json`（SHA `a66e658c20a014f949a45ca301f2d460871d636f28aca21e87f8464502b0cfce`）从固定 generator/islice 来源推导4项时序控制，并复用既有 BQ01 数学预期。[公开补充 fixture](../vendor/spareval/src/eval/fixtures/query_sum_avg_demand_cases.json)由真实 bound query 测试检查准备后、创建迭代器后和首次请求后的信号与结果。该补充来自检查候选时发现的缺口，先冻结再做专门失败复现与修复；不混作最初盲测，不计入主16/57项，也不据此宣称全部查询管线惰性已证明。

上述旧 RdfIdentityV1 路径仍不保存普通 Literal 的 datatype 来源是否缺省；既有 SUM/AVG 回执不扩签。后述独立 SourceLiteralV1 增量专门补此传输及 plain/显式 xsd:string 的失败次序，接收状态仍以新源码绑定回执为准。

未捕获的 SPARQL ErrorValue 与数值/基础设施 fatal 采用不同的 typed 通道。原错误对象身份、同一 session 的已发 signals、底层转换失败和之后不再求值分别核验，不能只按诊断文字匹配。第一组已返回后遇到 finish 错误与 update 阶段零行失败是不同结果。

原始冻结 `QUERY-SUM-AVG-FROZEN-01.json`（SHA `e58a335cd7dc736112164c520f11f3fbd399aeffeb9581476818636a8d204c98`）保留不改。当前接收绑定 `QUERY-SUM-AVG-FROZEN-03.json`（SHA `f86864839e50f1902e18ee89a9315377ead0577ab3065fdc6407d73f25246566`），仍为16项 core、12族/57次查询预期。02 原件（SHA `7d0b63709f2e0d8d8694fd550b4669277bbb9d6b5a14a52f96ffbfff115b2b47`）同样保留。02 只按实际转换链纠正 SA12-06 的主动故障注入：从第2次改为第4次，对应组结果回读、别名读取、别名回读、最终解码；期望调用次数和说明同步，数值、行、缓存、信号和失败类型均未改。该纠正经过源码核对、发生在候选执行前，但不是未见实现的盲测预期。03 再仅纠正 SA12-03 的资源 operation 标签：由 `output_characters` 改为既有整数格式化器实际先触发的 `integer coefficient or output digits`。该项在失败暴露后逐源核对，按开发回放记录；保留失败和旧预期，其他56次查询与16项核心预期不变。[core fixture](../crates/semantica-numeric/tests/fixtures/sum_avg_cases.json) 与 [query fixture](../vendor/spareval/src/eval/fixtures/query_sum_avg_cases.json)保留其公开投影。ID08 与 SA12 是单列主动资源/来源/后端政策，不能称为原 Python 运行结果。原70/16/mixed与 QN24、COUNT/SAMPLE、旧 Group 和公开 `/1–4` 检查继续保留。该 fixture 选择显式上下文和固定 CPython 普通 sum 分支，不证明历史进程的 build/fenv/context；NaN payload/sign、图存储遍历顺序、任意函数/对象等未证明内容不补猜为精确预期。

[私有算式链](SEMANTIC-NULLABLE-CHAINS.md)中原有界整数、整除、算术子节点认证和负 Decimal 零拒绝仍作为同一编译器的旧诊断路径及其回归保留。新的 prepared numeric 路径单独启用缓存和共享核心，不改签旧记录。LeftJoin、旧 OrderBy 变体、Lateral、Service 及递归属性路径 `*`、`+` 仍拒绝；有限 MIN/MAX 另见后述增量，完整比较/函数、SELECT *、任意计划及连接代表值选择、原 context thaw/forget、图导入缓存、完整浮点词法与跨平台行为、wrapper 和公开 profile 仍需后续闭合。SourceAggregateV1 的有限来源接收不表示完整 Group 上下文或任意查询计划已完备。

六书的来源适用性、工程模型与计量职责，原63项目标、完整语义迁移及实际工程验收继续保留。数值代码的来源正确、合同生成成功、有限行为通过和真实工程采用是不同结论；任何一项都不能代替缺失的材料、载荷、几何、制造、测量或批准依据。


## 私有 SourceLiteralV1 增量：保留原对象身份

`QueryLiteralSemantics::{RdfIdentityV1,SourceLiteralV1}` 是第三个明确选择轴。七参 `new_with_literal_semantics` 在真实准备前固定它；旧构造入口仍选择 RdfIdentityV1。选择 MixedBinary64V1 或 SourceAggregateV1 不会自动启用它，默认 feature 关闭，公开 profile 继续为 `/1–4`，不提供 `/5`。本段登记实现合同和独立预期；当前字节由共同编译输入清单绑定，完整源码冻结和执行资格另行取证，不能从函数存在或本段计数推定通过。

来源事实在真正的 parser occurrence 或明确的原对象构造处保存：datatype 是缺省、明确指定或未知；语言保留原始大小写；ill_typed 区分 None/false/true；String、None 与 Numeric 缓存事实分别保留。`SourceLiteral`/`SourceValues` 经过 lowering、逆转换及 SourceGroup 采样不会先按 RDF 相等合并，Display 保留显式 xsd:string、原语言以及数值 token 的类型、符号和拼写。完整优化器跳过这种来源计划；任意手写 AST 不会获得 sealed prepared 来源资格。

底层 RDF 的 Eq/Hash 字节沿用旧规则。实际来源身份单独使用词法、可缺省的源 datatype 和小写后的原语言；缓存值与 ill_typed 不参与。普通 clone、BIND、SAMPLE 保留对象状态，显式 `Literal(existing)` 构造则按原代码重置 ill_typed=None。Unknown 不从 xsd:string 反推普通文本：裸 transport、非 DISTINCT COUNT 可以携带它，需要 DISTINCT、Group 或其他真实来源身份时主动失败。

`CachedInternalTerm` 每个传输实例持自己的原 raw 值，不建立按 RDF 值索引的全局表。来源已知的文本和数值都经过真实 Dataset 内化回读，已承诺的 RDF 被改变就失败；最终 `execute_source_presence` 再真实外化并返回完整来源结果，Dataset 错误和取消会停止后续读取。旧 Term-only 入口在新模式下拒绝丢 metadata。tuple 的新增 fallible key/兼容 helper 不改变旧 Eq/Hash，也不等于完整 Join/context thaw；同一初始上下文再注入可识别同一个传输实例，但不能用此绕过真正的 DISTINCT 身份检查。

SUM/AVG 在新模式读取源 datatype 的 Option。原 `type_promotion(t1,None)` 在 subtype 归一化之前直接保留 t1：因此“先数值再普通文本/语言文本”的 SUM 进入 numeric 的 SPARQLTypeError，“先数值再明确 xsd:string”则在类型提升阶段失败。AVG 仍先 numeric/sum，再 datatype，并维持自己的跳过与计数规则。底层数值算法不因这个接线更换。

来源语言规则是固定原代码的 ASCII 正则，没有八字符子标签上限。direct 输入空语言先变 None；一个末尾 LF 可被 Python `$` 的结束规则接受，并保留在原对象里；CR、两个 LF 等不成立。真实 query 的 LF/CR/空格位于 token 外时属于空白，不是标签原文。source 模式在原规则验证后使用安全 Rust unchecked 字面量载体；它表达私有原对象行为，不宣称该较宽域全部符合 BCP47/RDF 标准。旧/default 语言验证不放宽，非法 direct 与 query 分别保留源 ValueError 和准备阶段 Syntax。

主预期 `LITERAL-IDENTITY-FROZEN-01.json` SHA `06c8b66119ae3a6138d08d3418bab7684096a789e65a170e8b6448bd31e94f98` 含12 core、17族/31 query，[公开投影](../vendor/spareval/src/eval/fixtures/literal_identity_cases.json)保持其内容。语言补充 `LITERAL-LANGUAGE-SUPPLEMENT-FROZEN-02.json` SHA `7096daa56ce66da5fe842ecc9f0a970f74021b37ac430744f6e1d7824bd66523` 另含17 core、17 query，[公开补充](../vendor/spareval/src/eval/fixtures/literal_language_cases.json)独立保存。02 只纠正 parseQuery 来源行号，01 与所有预期不改。补充源于静态发现原规则与 BCP47 域不同后冻结，不伪称未见实现的盲测；两批均来自固定来源和主动合同，未执行原 Python，也未用候选输出生成期待。

上述 Literal 身份增量本身不接收 GROUP_CONCAT；后述独立增量声明其有限范围。该 Literal 增量不继承后续 MIN/MAX 资格，也不接收完整比较/排序、任意图扫描/连接的来源 metadata、全 context thaw 或任意 Python 对象缓存。旧整数/Decimal、mixed Binary64、SUM/AVG、语言校验和公开 profiles 的回归继续单独保留。完成这些来源事实运输只是后续文本聚合及比较的前提，完整 Semantica 和原工程目标继续推进。

Source-literal transport also checks nonliteral backend identity. Only the explicit `SourceLiteralV1` Dataset constructor verifies NamedNode and BlankNode at both real backend boundaries; retained keys require no extra conversion. The old `NumericDataset::new` schedule is preserved. Generic Dataset import grants no unchecked source snapshot under the new policy. Eight independent active controls and two separately authored bound-query controls cover this policy; these declarations are not execution or original Python parity claims. This does not enable source graph scans or joins.

VALUES 的来源补充固定了两族四次执行预期：与初始值身份相等时，仍由 VALUES 的新对象覆盖局部绑定，保留它的原语言和数值缓存；额外的隐藏初始变量不能改变这个选择。`LITERAL-VALUES-REPRESENTATIVE-SUPPLEMENT-01.json` SHA `5d1da98bc5825595638e8d0e6e13815431c33e33548fcfc0435df52ef6e8d965` 来自静态发现差异后的独立源推导，在专门候选执行前冻结，不算未见实现的盲测或原 Python 执行。两项实际测试符号已映射，执行结果由绑定当前源码的外部回执提供；主冻结预期和旧/default EqHash 合同保持。


## Source GROUP_CONCAT：在原对象文本上聚合

本增量沿同一个 parser、SourceGroup、raw 表达式编译器和 bound source decoder，要求现有 SourceAggregateV1 与 SourceLiteralV1 同时选择；数值能力继续独立选择，不增加按函数切换的开关。普通 RDF-only GROUP_CONCAT 与旧默认行为保持，私有 feature 默认关闭，没有公开 `/5`。实现、独立预期和实际执行资格分别登记。

source_text 读取当前 Literal 词法文本、URIRef/BNode 文本、显式 None 或已明确来源的错误构造参数。它不把数值缓存格式化回字符串。UnknownNormalized 的当前词法可用于非 DISTINCT 拼接，但不能因此恢复 datatype/语言来源；DISTINCT 和 source_repr 实际需要这些事实时仍拒绝未知。错误的 diagnostic 不自动成为原 str(error)：同一 ErrorValue 中另存固定异常类和单个 None/Text 参数，clone 与去重仍看原对象身份。None 参数的异常文本是 "None"。

动态 numeric 错误在实际失败发生处用 source_repr 构造，Literal/URIRef/BNode 的外形遵循固定源，字符引号与转义复用 semantica-json。该 GROUP_CONCAT 增量最初只为两类 numeric 类型错误、乘除捕获的固定 "divide by 0" 及显式可信源错误提供来源；后述 MIN/MAX 增量另接收真实 missing-variable 构造。EBV/And 等未接收产生点仍保持 opaque，真正要求其文本时主动报 UnknownSourceErrorText，不按诊断反猜。任意 Python 子类、多参数/可变 args、lone-surrogate 与其它未承载对象不在这次范围。

实际 DISTINCT gate 在 update 的 NotBound 捕获之外先求值；通过后 update 再求值，保留第二个对象，再把第二个对象加入 seen。update 跳过抛出的或返回的 NotBound，保留显式 None 与其它返回错误；其它抛错和 Dataset/fatal 按身份传播。裸 ?x 读到 empty SAMPLE 的 None 会输出 "None"；算术 ?x+1 的参数取值先把同一个 None 变成 NotBound，GROUP_CONCAT 因而跳过，不能把两条路径混同。

finish 才按顺序转换并拼接，默认分隔符为一个空格，也允许显式空串；零个或一个值不使用 separator。输出是 datatype/language 缺省、ill_typed=None、String cache 的 plain Literal。先完成全部输入更新，再逐组惰性 finish；后组失败保留已返回前组，LIMIT1 不必 finalize 后组。原有 max_output_characters 按 Unicode scalar 限制最终结果，检查长度溢出并对 join/保留向量作 fallible reserve；它是主动预算合同，不能称原 Python 资源语义、总查询内存上限或新的每聚合行数限额。输出不重过 max_input_characters。

固定 SOURCE-GROUP-CONCAT-FROZEN-01.json SHA `cd764129bcd981247cd7fc31d99f7d5cb6397a960a3e507358fd69e331fa0655` 含10 text、5 repr、6协议、19 query 与9项原始 active 声明；其中7项保留可进入查询的前提，GA03/GA04 另登记为无效 session 配置。另原样引用旧 GC01–07。协议闭包只测试实际 accumulator，不充作另一查询解释器。公开 fixture 保留冻结内容，测试与案例数量分开。GQ18 在正式冻结前由原 parserutils 源核纠正，原草稿经过保留；没有执行原 Python 或用候选输出生成预期。当前源码、公开 fixture、来源记录与真实测试符号共同绑定，实际通过数与新资格只写外部源码回执，不回填本编译输入。有限 MIN/MAX 由后述独立增量登记；完整排序/比较、其它函数、完整上下文及 wrapper、完整 Semantica 和原工程目标继续推进。

资源前提补充 `SOURCE-GROUP-CONCAT-LIMITS-SUPPLEMENT-01.json` SHA `f78e004f278779fcddf8661f8873998ebaf8310919ff2f3128c6075a114a5d3a` 及[公开补充](../vendor/spareval/src/eval/fixtures/source_group_concat_limits_supplement.json)保留 GA03/GA04 原件与失败含义：max_input_characters=1 无法容纳默认 Decimal Emin/Emax 的六位幅值，session 构造即报 coefficient_digits；两项专门测试只检查这一前提拒绝，不执行 query，也不算原聚合行预期通过。新 GCS01/GCS02 显式使用输入预算6；分别以6字输出核未使用的7字 separator，以13字输出核结果超过输入预算却仍受输出预算约束。这两项是在原失败曝光后依固定源与既有主动合同推导的开发回放，未运行原 Python 或用候选结果定义预期。原主 fixture 字节不变，核心预算不放宽；新增测试实际结果仍仅登记在外部当前源码回执。


## Source MIN/MAX：比较原对象，再构造选中结果

本增量使用同一 SourceGroup、raw 表达式编译器和 bound source decoder，明确选择 SourceAggregateV1 与 SourceLiteralV1；数值能力仍由现有轴选择。默认 feature、旧构造器和公开 `/1–4` 保持原范围，不开放 `/5`。以下登记实现合同和来源，实际通过需要当前源码的独立执行记录。

核心比较返回 Equal/Less/Greater 的布尔关系与已发信号，Integer/Decimal/Binary64 按真实缓存精确比较。不把大整数先转 float，不用有舍入的减法判断大小，也不把表示相等当数值相等；scale、符号零和巨大指数分别处理。比较不按 precision/Emin/Emax 作算术舍入，但既有 context/输入预算仍检查。Decimal/float 的 EQ 记录 FloatOperation 而不因该 trap 停止；LT/GT 可以先 trap，再进入精确转换及 NaN 分支。session 在成功、trap 或资源失败时都先合并已经发出的 signals。

来源 `_val` 类别与 tuple 的来源身份相等先于值比较；同身份的不同 cache 可以保首个代表而完全不发生数值比较。Literal GT 的快路径/fallback、LT 的 `not GT and not .eq`、NotImplemented 及反射保持各自阶段；普通源 TypeError（含相关 FloatOperation）与 SPARQLTypeError、主动 Unknown/Resource、Dataset fatal 不混同。NaN 下的这些关系不能成为 Rust 总序或严格弱序。

MIN/MAX DISTINCT 仍只求值一次，不建 seen gate。None 是未选中哨兵；裸 missing 的 raised NotBound 跳过，表达式返回的错误对象仍可被选中或在第二值比较时失败。真实编译器按 scope 分派：parserutils 的变量参数由 source_raw_variable/source_variable_operand 在发生处构造 NotBoundError(None)；裸 _eval 由 source_evaluated_variable 构造 Text("Variable x is not bound")。终端不根据 kind 或普通诊断反猜来源文本。

finish 对既有 Literal 保持缓存和来源，仅重置 ill_typed=None；IRI/BNode 新建 plain String Literal；源错误新建 plain ErrorObject Literal，保留真正 ErrorValue Arc，不能改成 String cache 或 ReturnedError。该 payload 随真实 Dataset 内化/外化、BIND、SAMPLE、COUNT、GROUP_CONCAT 和最终 source decoder 运输；旧 RDF Eq/Hash 与来源 identity 都不纳缓存，不建立 RDF 值到缓存的全局表。

既有 Literal copy 不解析也不重用输入/输出预算；新建 node/error Literal 只核输出预算。单值 unknown Literal 可以复制并仅将 ill_typed 置为已知 None，其余事实保持未知；实际分支第一次需要身份或缓存时再主动拒绝。SourceGroup 沿用首次 next 才更新、全输入 update 先于首组、逐组惰性 finish；后组失败保留已返回的完整前缀，LIMIT1 可避免后组 finish。预算、未知事实、后端和取消单列主动合同，不称原 Python 资源语义。

冻结 SOURCE-EXTREMA-FROZEN-02.json SHA `6df0965cedb36486e13c20876eab75315fb77e42c5af66a0b4278e3dbe5e55fa` 含34个新 source/math 对象、原样引用 MM01–04 和11个主动合同对象。每对象可有多个 run/operation，不能当 Rust 测试数或通过数。[核心投影](../crates/semantica-numeric/tests/fixtures/source_comparison_cases.json)与[查询投影](../vendor/spareval/src/eval/fixtures/source_extrema_cases.json)保留原对象及明确 context；核心、协议与真实查询分别映射。预期由固定 RDFLib/CPython 源和显式整数数学推导，没有执行原 Python 或用候选输出生成预期。

固定范围为内建对象与 DAWG=false。该 MIN/MAX 增量自身不开放 ORDER；后述 SourceOrderBy 增量单独接收排序与 key/比较副作用时序。日期/XML/custom cache、任意函数/图来源/修改过的 datatype globals、完整 context/wrapper 和完整 Semantica 仍未完成；正式 RC.4 不自动包含此私有增量，六书、工程本体和原63项目标继续保留。

本轮另有三项作者边界控制：真实编译器检查 MIN/MAX 的双轴缺省组合，普通 public 查询检查旧 MIN/MAX，旧 OrderBy 节点在 source 编译器继续返回明确拒绝。原 source_extrema_does_not_enable_source_order_by 属于9a85dae及更早版本；新 source_extrema_does_not_enable_legacy_order_node 明确核旧节点，SourceOrderBy 的有限资格另见下节。它们与冻结34/4/11个对象分列，存在测试不等于当前执行通过。

来源更正02保留原 SOURCE-EXTREMA-FROZEN-01.json（SHA `9cb28bb8ab35653f2490037f627925fbe8c4b92b0fffa51563f8c1c9367fb497`）、原公开投影和实际失败记录。仅 EA01 两种 LIMIT 情形中投影变量 `g` 的 ill_typed 由 null 改为 false：原数字 token 经隐式 SAMPLE/alias 直接运输，不经过 MIN 的 `Literal(existing)` 构造。固定 aggregates/algebra/term 源支持这一点；MIN 的选中结果规则、34项 source/math、4项旧 MM、另外10项 active 及所有输入和资源判据不变。此项是失败曝光后的来源纠正和开发回放，不称盲测或原 Python 观测，后续通过仍需新的当前源码执行回执。


## 私有 SourceOrderBy 与执行启动边界

工程规范先区分“启动查询失败”“迭代下一行失败”和“返回的错误值”，再由 ggen 把规格、Rust检查、来源与真实测试对应起来。执行仍经过原 parser → 共享 planner → 同一个图/表达式编译器；SourceOrderBy 是末尾追加的私有 AST 节点，没有另造查询解释器。后续回归与实际任务反馈应沿这些对应关系发现偏差、提出并检验改法，测试通过本身不构成工程采用证据。

只有已有 SourceLiteralV1 sealed 入口生成 SourceOrderBy，没有新增 session 轴。普通/非来源入口拒绝该节点，source 入口仍拒绝旧 OrderBy。解析和双向 lowering 保 raw keys、ASC/DESC、重复项、来源对象和链括号，不插隐藏 Extend；原默认 OrderBy 仍按原规则处理。聚合 ORDER 按固定 `complete=False`：整个条件列表中任一项有 aggregate 才重写全部条件，投影/HAVING/ORDER/裸投影 SAMPLE 的先后不变。整个来源计划跳过优化器，旧枚举 Hash 字节和原输出变量范围保留。

启动 Result 在原执行器内贯通。来源 ORDER、Project、Slice 和 Union 立即启动；Filter、Extend、Distinct 及 SourceGroup 等原 generator 边界直到首次 poll 才启动 child。因此直接 LIMIT0 也可能触发排序错误或 flags，而 DISTINCT LIMIT0 可不启动 ORDER；OFFSET1 LIMIT0 仍可要求输入。默认 EXISTS 继续其历史首项存在语义，私有 numeric/source EXISTS 在编译时明确拒绝，不能吞掉启动失败。

ORDER 先完整收集 child，任何后期 Group finish/Union child 错误都先于 key。之后按最后条件到第一个条件逐轮排序，每轮先按上一轮行序求完所有 key 再比较。bare variable 的 missing/ExplicitNone 是 Variable 占位，已绑定 ReturnedError 原对象抛出；一般表达式复用原 numeric compiler，返回的 error/None 变 None key。None key 只有实际比较才报错，零/单行不凭空比较。排序直接搬原行，保缓存、原语言拼写及 ill_typed，不重新构造 Literal。

索引排序是固定 CPython3.13.11 `listobject.c` 的 Rust **算法移植**，保自然 run、minrun 插入、Powersort、merge/gallop、DESC 前后反转和不一致比较时序。不是仅参考行为后换用 Rust 排序。完整 [Python 许可](../vendor/spareval/LICENSE-CPYTHON)、[移植说明](../vendor/spareval/SOURCE-SORT-PROVENANCE.md) 与已有 MIT/Apache 声明共同保留，该排序移植的许可组合为 `(MIT OR Apache-2.0) AND Python-2.0`；后述 SipHash 移植另保留独立 MIT 声明。这两文件已显式纳入编译来源清单，也由现有公开源码打包清单递归收录。

固定 `SOURCE-ORDER-FROZEN-01.json`（127656bytes，SHA `738d39778338087562cf4edbac0fb651a4d1af66cd59002ce7a47b3a4fcc2b2d`）区分38项 source/math、11项主动合同、4项作者控制和25组 native sort。查询预期来自固定原源码/数学推导；只有25组排序完整比较序列实际观察过原 Nix CPython3.13.11，共16007比较对。原执行器身份、固定 tag 分支证明和候选运行分开；它们不是原 RDFLib 查询执行、全平台或穷尽输入证明。公开 [查询 fixture](../vendor/spareval/src/eval/fixtures/source_order_cases.json) 与 [排序 fixture](../vendor/spareval/src/eval/fixtures/source_sort_schedule_cases.json) 保机械投影，OA12/OA13 只列未来图扫描前提，未进入本轮接收。

可失败的内存预留、索引溢出、取消、来源 Unknown 和 Dataset 原因身份是主动安全合同。numeric 输入/输出字符上限不充当查询行数上限；容量溢出控制也不冒称真实操作系统 OOM。现阶段只有代码/来源/接收范围的声明，当前测试结果和 ggen 回执在编译输入之外绑定实际源码。默认 feature 关闭、公开 profiles1–4 及正式 RC.4 不因此改变；完整 Semantica、图扫描/连接来源、context thaw/wrapper、其余函数和跨平台边界继续按原目标推进。


## SourceContext 运输与绑定基础

本增量把“变量实际属于哪一层、覆盖时保留哪个对象、失败发生在哪一步”作为工程本体中的执行条件，登记到现有 nullable-tuples 合同，再由 ggen 连接真实 Rust 符号和接收测试。实现继续使用同一 parser、共享 planner 和 InternalTuple；没有第二套查询解释器。新增 SourceContext 是原 AST 末尾的注释节点，仅显式上下文解析入口生成，旧枚举 Hash 和默认入口保持原样。注释保存 Join 的递归 lazy 选择、OPTIONAL 左侧作用域、Filter/Extend 的来源变量范围、VALUES 的 Records/EmptyTranslatedList，以及 GROUP BY 的裸变量、裸函数、括号无 AS、括号 AS 四种来源形态。Lower/reverse、Display、输出变量和整计划优化器屏障共同运输这些事实。

基础阶段交付运输、绑定域、哈希与集合。当时 Prepared 查询明确拒绝尚未接收的 SourceContextV1 集成，普通编译器也拒绝直接提交的新节点；该阶段的源码和拒绝回执保留原身份。后续有限整查询增量见下节，不能把这些基础拒绝测试改称整查询通过。显式 QueryContextSemantics::SourceContextV1 要求 SourceLiteralV1 与同一不可变 hash runtime，原构造保留 LegacyPrivateV1；图扫描、EXISTS、时间/空白节点上下文、wrapper 与完整 Semantica 仍未完成。

绑定基础保存局部插入顺序、当前域与不可变 initial 的区别。有方向覆盖保留键首次插入位置，但采用新值对象；缺失才回退 initial，None/error 是实际局部值。solution(empty) 保全部绑定，而 project/remember(empty) 为空。thaw 的空 frozen 回退、push 展平后重施 initial、forget 的短路、右侧 initial 兼容与局部-only Mapping equality 分别保留。源单元相等按对象同一性、类型、datatype、语言、词法逐步判断，不从 numeric cache 或 ill_typed 补猜未知来源。这些 helper 的基础接收不单独证明整查询 context owner；整查询须另核下面的实际消费者映射。

FrozenBindings 的原 hash 失败会留下逐步 XOR 的**部分缓存**；不能为了“原子性”回滚为未计算。运输 clone 共享原对象及缓存，新的 solution/project/forget/merge 对象另建缓存。接收绑定 SOURCE-CONTEXT-PRIMITIVES-FROZEN-02.json（SHA `4cc85999a7f5926fc9f89f74ecad3ce424255d6bd2dc614d17600a7488fa5773`）的23项来源推导和5项主动未知事实合同；02 在候选执行前纠正 CP03 的新增顺序预期，旧六项无序 map 预期与01原件均保留。它们不是本次执行过的原 Python 上下文原语。

[公开 set/hash 预期](../vendor/spareval/src/eval/fixtures/source_context_set_cases.json)来自固定 Nix CPython3.13.11 的17组 set(generator) 和3个显式 seed 的真实观察；Literal/Frozen XOR 仍单列为源推导。实现移植固定64位、小端、SipHash13 的字符存储宽度、探测、增长、代表对象和最终槽位顺序，不用 Rust HashSet 或插入顺序替代。新 Rust error 使用自身对象地址；没有读取原进程隐藏 secret 或复活历史地址。原 [Python 许可](../vendor/spareval/LICENSE-CPYTHON)、独立 [SipHash MIT 许可](../vendor/spareval/LICENSE-SIPHASH) 和[来源说明](../vendor/spareval/SOURCE-HASH-SET-PROVENANCE.md)均纳入编译与发布来源。容量、取消、Unknown 拒绝属于单列主动合同；删除、callback 内可变集合、set/dict fast path 和任意 Python 字符串不在此有限 API 范围。

历史更正必须与新能力轴分开。固定原 RDFLib7.6.0 的15次独立诊断确认：空 VALUES/无变量 VALUES 在此次诊断查询启动时抛出 `AttributeError: 'list' object has no attribute 'name'`；括号内 GROUP BY 表达式没有 AS 时形成 GroupAs(None)，首次 poll 抛出 `Cannot eval thing: None (<class 'NoneType'>)`。旧冻结中把这些输入当普通空流或普通 raw group key 的预测不成立；旧输入、预期、成功/失败回执不改签，也不再作为这些形态的来源等价证据。真正的空流仍可由合法非空 VALUES 经 FILTER 产生，但替换输入必须另立预期，不能改旧 fixture 隐去差异。VALUES-left OPTIONAL 与 BIND-left 的 _vars 不同；旧 O02 的 VALUES 写法实际一行，BIND 版本才是零行，initial 又会影响结果。新运输区分这些来源形态，后续整查询冻结另外绑定其真实错误阶段和作用域。原诊断观察摘要为 `aaa5f066bab637b75d92e2f90c564c051a4f7a5b8063ae806d8ba03615cfa43d`。

本轮复用既有 nullable-tuples 片段，foundation 保持4194086字节及218字节余量，不加片段、不提高4MiB上限、不删除旧合同、原63项目标或全范围19/19/22/13494。源文件和公开 fixtures 绑定编译清单；实际运行、ggen 与提交回执留在编译输入之外。正式 RC.4、公开 profiles1–4、默认关闭的 private feature 不因这些基础实现自动取得新版发行资格。

13个历史输入形态是 SA03-01/02/03、SA10-01/02、EX11-01/02、GQ04/05/18、OQ01-a/OQ02-a/OQ25。上述15次结构诊断不等于这13个完整查询；后续已独立重跑这13条精确查询字符串，另立 CH-H01–13 纠正对象，保存外层 generator/ORDER/Group 导致的准确错误阶段。旧能力版本的候选回归和旧预期不改签，不再作为这些形态的来源等价证据。

## SourceContext 的有限整查询执行

本次把变量域、代表对象、错误阶段和来源条件落实到同一 parser、planner 和 Rust evaluator 中。既有工程本体合同规定这些含义，ggen 将它们连接到真实检查与接收函数；没有另一套字符串识别或查询解释器。实际执行资格须由外部回执绑定当前源码。

显式 SourceContextV1 与 SourceLiteralV1 共用不可变 initial/hash runtime；旧构造继续选择 LegacyPrivateV1，聚合仍须 SourceAggregateV1。全计划变量使用同一变量 slot 表，但每行保自己的本地域和插入顺序，表达式查找仅在局部缺失时回退 initial。Project 过滤本地域，最终解码前才映射声明的输出列，不把隐藏初始变量泄露到结果域。

VALUES 每行先 push 当前绑定并重施 initial，再依原源赋值；相等时采用实际 incoming 对象。Filter/Extend 用来源注释的 scope 和 forget(before)，不把输出变量清单当原 `_vars`。Lazy Join 按每个左行 thaw 并重启右侧，返回 right.merge(left)；materialized Join 先启动左侧、收全右侧原集合，再遍历左行并返回 left.merge(right)。两者可能保留不同语言拼写或数值缓存代表对象。OPTIONAL 保留两次探测、左侧作用域和条件判断；MINUS 先兼容再判断局部域是否相交。DISTINCT 在 yield 前查成员，在恢复 generator 后才执行 pending set add，LIMIT 不会凭空触发后续插入。

GROUP 仍复用同一聚合器。所有 GroupAs 的 Extend 先依原顺序执行，再读取分组键；无 AS 的括号表达式先产生真实求值效果，随后在其键位置抛 NullGroupKey。H05 因此先保留 DivisionByZero，再报空键；H13 外层 eager ORDER 使这个错误发生在启动阶段。空 VALUES/无变量 VALUES 的原 list AttributeError 也由实际节点启动及外层 generator 决定时机，不能统一说成空流或首次 poll。

本轮冻结 SOURCE-CONTEXT-JOIN-FROZEN-01.json（SHA `ea2cd7e1619a8b9fa92734acb8ae3d85608b00be97777e05008e0b0610f5fd58`）包含55来源查询对象、13条精确历史输入纠正、10个共享消费者 protocol 和10个主动安全对象。前68项的接收入口限定为 sealed prepare 与真正 bound execution；protocol 用同一生产消费者接受受控原始行，不能称为完整 parser 构造资格。各对象保留真实原程序观察或固定源码推导的分类，不能把88对象称为88次原 Python 实测或Rust通过数。公开机械投影是 `vendor/spareval/src/eval/fixtures/source_context_query_cases.json`；当前实际执行结果另行绑定最终源码。

来源对象与 RDF 值相等是两回事。RawExpressionValue 的对象 token 随 clone 和真实 codec 往返保留，BIND/SAMPLE 选择的对象不因再内化变成新对象；Literal(existing)、新的 metadata 声明或新的 Literal 结果则产生新 token，同时按原源保缓存或 Error Arc 并重置 ill_typed。token 不参与原物理 RDF Eq/Hash，也不授予 Unknown 的来源身份或哈希事实。

Unknown 的运输完整性是单列主动策略：仍先执行真实后端外化，成功且 RDF 与保留对象一致才能还回原对象；不能用 snapshot 隐藏后端错误或改写。独立导入的未知对象保持不同身份，旧轴继续原重构政策。8 个作者控制单列接收，不作为新增独立原查询观察。

实际 true/false 解析 token 现保留 Boolean(bool) 缓存、指定 xsd:boolean、无 language 和 ill_typed None。已校验的规范词法与现 EBV 一致；4 个直接检查分别核来源构造/EBV，以及主动非法来源、缓存一致性和比较边界。需要读取布尔缓存的排序仍拒绝，既有 RDF rank/identity 短路保留；不宣称完整 bool 字面量构造、运算或排序族已接通。

此前13条历史形态的完整原查询字符串已独立重跑并记录准确阶段；旧15次结构诊断不代替这些13次。SA03/SA10、EX11、GQ和OQ中的旧输入、预期及成功/失败回执保留，旧候选回归继续说明其旧能力行为，不再充当这些形态的来源等价证明。新的 context 轴不能隐藏这项历史更正。

源码/公开预期进入既有 owner 和编译输入闭包；当前测试、ggen、提交与安装回执留在编译输入之外。正式仍RC.4，公共profiles1–4和默认关闭的private feature不变；graph/GRAPH/path/SERVICE/EXISTS/Lateral、图/时间/空白节点上下文、完整wrapper、跨平台和完整Semantica仍按原目标推进。foundation保持原4194086字节，不加第33输入、不扩4MiB上限，也不删原63工程目标或19/19/22/13494完整范围。

## Frozen 行所属的绑定上下文

SourceContextV1 的行现在分别保存局部值快照和真正所属的绑定上下文。上下文持有独立的可变绑定与共享不可变 initial；后续上下文赋值不会改写已经返回的行快照。行 lookup 只按局部值、所属上下文的 initial 查找，不把 owner 的当前全部绑定当作隐藏局部值；None/error 仍遮住 initial。owner 自身储存无 owner 的绑定，避免通过行形成自持引用环，也不进入旧物理 Eq/Hash 或来源局部映射相等。

Project/remember/forget 保留行的 owner，merge 保留左侧 owner。显式 context.thaw(row) 使用调用方上下文；row 的 owned thaw 使用行所属上下文，两者不能混同。空局部行回退该上下文的当前绑定，非空行采用自身快照，再重施同一 initial；新的上下文与旧 owner 不同，但共享 initial。push/clean 同样保留原 clone([]) 的空值回退行为。真正的 VALUES 赋值更新其新 owner；Extend 的冻结行 merge 只增加局部值，不替换 owner 绑定。Group 输出使用聚合调用方的上下文。

SOURCE-CONTEXT-OWNER-FROZEN-01.json（SHA `b9aeb9841ea106723b9d9249305f4b6533efac01b7165a61838d89b86a24b75c`）固定13项源码推导原语，由[公开 owner fixture](../vendor/spareval/src/tuple/fixtures/source_context_owner_cases.json)机械投影；这不是13次原 Python 实测。原语中 Group/VALUES 的构造协议也不单独证明实际查询生产者接通，完整查询需另接 sealed prepare 与 bound execute_source_presence。冻结里的两个 active 边界与原语分列，尤其 EXISTS child-start 取消仍待 EXISTS 实现，不能由普通查询的取消控制替代。

实际生产者另由 SOURCE-CONTEXT-OWNER-QUERY-FROZEN-01.json（SHA `cd748af00421d75621c9972262397fc1439fea644c6bfec99ebaf537ca5f7670`）的7条源码推导查询和2条主动输出边界接收，[公开投影](../vendor/spareval/src/eval/fixtures/source_context_owner_query_cases.json)保持原输入与预期。它们走真正 prepare→execute_source_presence：检查 VALUES 的独立 owner、Extend 局部值与 root owner 的区别、Group 调用方 owner，以及相同局部结果下 lazy/OPTIONAL 保右 owner、materialized Join 保左 owner。真实生产点的 observer 只读取实际对象关系，不补造上下文或额外查询结果；这7条并未执行原 Python。

两个主动控制在真实首个 OutputRow 生效：取消须在最终解码前停止；另一个先确认 project/forget/owned thaw 不访问后端，再让真实 Dataset 外化返回保留的原错误对象，后续轮询熔断且第二行不再执行。它们不能代替尚未开放的 EXISTS child-start 取消。存在这些函数与来源映射不等于当前版本已经运行通过，实际接收回执另存。

这项共同基础仍复用工程本体合同、ggen 和既有 Rust 执行器。50条 EXISTS 原查询的独立观察为后续实现约束，尚不是 Rust EXISTS 验收；其 mapping 语法与运行期 graph 属性、未 simplify/analyse 的子图及不同错误阶段继续待接。当前不开放图、时间、空白节点上下文或公共 /5，不继承正式 RC.4 的执行资格。当前运行回执留在编译输入外，完整 Semantica 与原工程目标继续保留。

## Source Literal 真值与解析事实桥前置

同一 `QueryNumericSession` 现在提供来源 Literal 的 Python 真值读取、`Literal(bool)` 结果构造与保留实际数值 token 构造结果的入口。Python 真值读取已有 cache：cache 非 None 时按该对象取真值，已知 None 才看原 lexical 是否非空；它不调用 SPARQL EBV，不重新构造值，也不追加 Decimal flags。因此 NaN/sNaN 为真，空失败数值 lexical 为假，非空失败 lexical 为真；运行时 `Literal(SPARQLError(""))` 保有实际 exception cache，即使 lexical 为空仍为真。未知来源到该次真实消费时明确拒绝。

[truth 公开输入](../vendor/spareval/src/numeric/fixtures/source_literal_truth_cases.json)机械保留23个来源真值、2个 bool result 与5个单列主动边界。[token 输入](../vendor/spareval/src/numeric/fixtures/source_literal_numeric_token_cases.json)固定9个原构造动作，[exception 输入](../vendor/spareval/src/numeric/fixtures/source_literal_error_truth_cases.json)只覆盖运行时已有 ErrorObject。它们是独立审阅的固定源码/数学推导，不是本轮原 Python 实测。新结果用现输出字符预算；bool token 旧入口与 typed-string/比较边界保持。

这组直接接收调用实际共享 factory/helper，检查 cache、metadata、source object 与 flags；它不证明 parser 的每 occurrence callback 次数或绑定到最终 compiler leaf 的传递。数值 factory 内部仍保原动作，例如正整数的再次字符串构造，不能把一次 grammar callback 混称原 Python 只创建了一个 Literal。解析桥 PB 的实际 prepare/registry/decoder、七项完整 EXISTS 前端 phases 与50条 EXISTS 查询运行各自需要接收；公开 profiles1–4、私有默认关闭及无 profile5 的界限不因这组 helper 改变。实际测试、ggen 和提交回执均在编译来源之外。

解析桥另有[机械公开输入](../vendor/spareval/src/eval/fixtures/source_literal_parser_bridge_cases.json)：一个来源推导的 VALUES/BIND 查询和六个单列桥接控制。每次真实 prepare 持有独立 registry，grammar occurrence 保存同一次实际 factory 的 Raw 对象；compiler/VALUES 只按原 scope、index 与完整描述取回。两个相同 RDF 的语法位置仍是不同对象，同一 BIND 常量跨行仍是原对象。Decimal 坏词法的原 cache 与 sticky flags 同样穿过最终解码，不能由输出相等代替对象身份检查。

接收直接观察真实 occurrence ID 集合、双向 planner 运输及 registry 原对象；foreign scope、被改描述或缺 registry 明确拒绝。旧 `occurrence=None` 的四字段 Hash/Debug 与 Query Send+Sync 另有作者兼容控制。PB 每 token callback 一次是桥的主动合同，不是原 Python 内部构造次数的观察。完整 EXISTS 的 mapping/overlay phases 与执行仍单独接收；当前公共入口继续拒绝私有 SourceExists，正式 RC.4 与公共 profiles1–4不变。

前端结构的准入另有[主动边界输入](../vendor/spareval/src/eval/fixtures/source_exists_frontend_gate_controls.json)。真实 parser 产生的 SELECT 无 overlay 与 BIND/NOT EXISTS 有 overlay 对象，在既有 legacy/raw/numeric 编译器均于 child 编译前拒绝；同一绑定执行入口不能先返回迭代器再绕过此门槛。公共 initial-binding/group-null visitor 明确要求私有准备路径。错误沿用现 `Unexpected` 与稳定消息，未追加虚构的 typed cause chain。这三项是主动准入合同，不是原 Python 行为预期；EA-AST07 与完整50条 EXISTS 运行仍留在执行阶段。

PB04 保留原 `SELECT (-1.50 AS ?x) WHERE {}` 输入，但纠正首次接收暴露的来源推导错误：原 parser 先把负号识别成 UnaryMinus，实际结果为 Decimal `-1.50`、`ill_typed=None`，flags 不变。另增 PB06 的 `VALUES ?x { -1.50 }` 才走负数 token 的 `Literal.__neg__`，此固定输入在原 parseQuery 抛 TypeError。两条均经过固定原程序各 plain/trace 一次，共四进程；02 是曝光后纠正开发回放，01 预期、公输入、旧审查与失败回执全部保留。不能将此具体异常文字泛化为所有 Decimal。公共 visitor 拒绝控制依赖私有 parser，实际在私有 feature 测试构建中运行；默认回归仍独立验证。

这条实际桥接还接通来源 `ToMultiSet` 的透明委托：按固定 `evalMultiset`，由同一 child compiler 与原迭代器继续执行，沿用当前 context、owner 和错误对象，不再构造字面量。BQ01/BP05 通过真实 VALUES/BIND 的最终对象与 flags 接收此路径。该窄改动不表示 `Unanalyzed` overlay 或 SourceExists 已可执行。


## EXISTS 前端的语法与翻译运输

新的私有前端复用同一 grammar、来源 Literal registry 和共享 planner。临时 `SourceSyntax` 只在解析阶段承载真实语法，`finish` 后必须全部消解；最终 `SourceExists` 分别保存 mapping 语法与实例上的 graph/no_isolated_scope 属性。原 traversal 只遍历 mapping，因此不能顺手给 overlay 补 simplify、analyse、`_vars` 或 lazy。直接 SELECT 中没有 overlay 的形态仍没有可执行子图，NOT EXISTS 与外层普通 NOT 也保留区别。

[前端公开输入](../vendor/spareval/src/eval/fixtures/source_exists_ast_cases.json)接收六项选定 AST 不变量：12条代表输入核实际 snapshot、真正 overlay 图、父层/子层注释、隐藏 Extend、外层聚合抽取顺序与 Literal 原对象；lower→reverse→再次 lower 都检查相同事实。另50条精确原输入仅核原程序已经完成的 parse/translate 阶段及 flags，并检查临时 capture 未逃逸。它们不执行 EXISTS，不接收这50条的查询行、异常、别名或运行时阶段；EA-AST07 仍留在执行增量。

这不是完整 RDFLib parser 等价声明。完整 graph/path 重排、SERVICE/扩展语法、原进程 set 敏感的 SELECT-star 列序与 CompValue 错误 repr 仍需后续来源接收；AST 保留隐藏变量也不单独证明运行时 slot 注册正确。当前 SourceExists 仍由原编译器和公共准入 gate 拒绝。真实测试与 ggen 回执继续放在编译来源之外。

显式 GROUP_CONCAT separator 也来自真正的 String grammar occurrence：[逗号、空串、缺席三项](../vendor/spareval/src/eval/fixtures/source_exists_separator_cases.json)分别检查原 factory 对象、与相同文本的数据 Literal 的不同身份，以及缺席时不凭空创建默认空格。它们通过实际 overlay 链接和 planner 往返接收，只证明前端捕获；不在此执行 GROUP_CONCAT 或 EXISTS，也不把一次 Rust callback 当原 Python 构造次数观察。

[字段缺席补充](../vendor/spareval/src/eval/fixtures/source_exists_mapping_cases.json)保留原 EX01/05/06 输入，区分空 group 无 `part`、空单变量 VALUES 有 `var` 无 `value`、无变量单空行 VALUES 有 `value=[[]]` 无 `var`。实际原记录证明字段成员，字段插入顺序则由固定 ParamList→CompValue OrderedDict→`_addVars` 源码推导，不能从排序后的 JSON 声称观察到原顺序。

独立静态审阅还补齐了[projection 实例赋值反例](../vendor/spareval/src/eval/fixtures/source_exists_projection_instance_cases.json)。原 `translateAggregates` 的 `v.expr = ...` 写实例属性，不替换 mapping 的 aggregate 根；后续 Extend 读取实例别名，mapping-only `_addVars` 仍看到原 aggregate 和缺失变量。原 BX07 记录证明 mapping、实际 Extend 和 scope，实例槽与共享节点关系由固定源码推导，不能冒称原 observer 直接读取了该槽。相反，EX16/17 的外层 `_aggs` 确实会沿 mapping 重写嵌套字段，两条路径必须同时保留。先前窄范围接收未检查这个区别；新反例补接而不重写50条原输入或旧预期，错误与修复回执保留在外部。

本轮将静态目录关联从首个 `rust_path` 匹配扩展为全部匹配。原新增测试把 `semantic_runtime` 直接当作既有通用映射的首个 owner，这个假设错误；实际失败保留，不将旧首匹配规则改称原合同违约。现在多个模块共享 `semantica-runtime` 时，文件可具有多个声明的目录关系，来源映射按路径和模块稳定排序。以下断言接收的是显式增强后的规则：保留运行时 gate 的 `semantic_runtime` 关系，核对完整匹配集合、排除不匹配模块，并在倒置模块声明顺序后检查 RDF 与来源摘要不变。静态目录关系不证明每个共享文件实现了每个模块的功能，也不替代真实检查、行为接收或发行资格。


## 来源 EXISTS 的实际执行增量

新增私有 `prepare_source_exists_numeric_query` 与原前端入口共用 parser、planner 和 Dataset，绑定同一 source session、occurrence registry 与 context。原 `prepare_source_frontend_numeric_query` 保持前端资格，仍拒绝 EXISTS 执行。普通 numeric hooks、跨准备 registry、SourceSyntax 与公共入口不能借用新资格。

SourceExists 在真实求值时从现有 tuple owner thaw；真实子计划仅编译一次，每次调用只 start 一次、最多 poll 一次并释放。成功行表示存在，错误保持错误；NOT EXISTS 仅改变成功布尔的极性。来源 SPARQL 异常在这个 Expr 边界保留原 ErrorValue 身份，fatal、取消和资源错误继续传播。未分析 Join 使用物化右侧 set，Filter/Extend 在读取时处理缺席 scope，不重新分析或修改来源属性。

[完整50条原输入的接收投影](../vendor/spareval/src/eval/fixtures/source_exists_runtime_cases.json)保留固定原观测；本增量声明18条实际查询的行、列、字面量 metadata/cache、错误阶段和 flags 检查，另检查首 poll/drop、同准备资格、owner、取消、输出预算和 fuse。实际通过仍以外部执行回执为准，源码不含自身通过回执。

未翻译 CompValue 的原始错误文本、BX16 初始 None/error 绑定及完整 graph/time/bnode context 继续补齐；未分析 OPTIONAL/GroupAs 也未获得这18条的资格。BX05/06 与 BX09 的新接收范围见下节。当前 `UntranslatedSourceGraph` 仅在实际求值时保留语法并报错，其显示不等于原 CompValue repr，不能据此关闭其余原例。完整 Semantica、公共 `/5` 和正式 release 均不由此局部接收替代。

## 私有关系比较与普通逻辑取反增量

`prepare_source_exists_numeric_query` 的实际 prepared session/registry 新增有限 `>` 与普通 `!` 执行。复用原变量/Expr 读取、已有 Literal GT 和 Boolean 结果构造；比较按左后右顺序读取，并独立承担 nonliteral guard。普通逻辑取反使用来源 EBV，区分 plain/string 的 lexical 长度、真实 Boolean cache、numeric cache 与新来源 SPARQLTypeError，不把 Literal Python truth 当作通用 EBV。变量错误在读取时保持原 Arc，Expr 返回的错误才进入下一算子；未知、取消与资源失败继续 fatal。

固定的 12 个来源协议与原 BX05/BX06/BX09 查询分别接收，预期不由候选生成；EB02 仅冻结类别和流程，精确参数不计通过。实际执行资格以本轮外部回执为准。原 50 查询、全文 CompValue/set 表示、初始 None/error、完整 graph/time/bnode context、全部关系/EBV 及完整 Semantica 目标继续保留。完整 XSD_DTs、关系 TypeError/FloatOperation 不透明参数和 uncached Boolean EBV fallback 当前显式未支持，不以猜测错误对象替代。旧 frontend、默认、raw、公共 `/1–4` 资格沿原合同，未开放 `/5`、未正式安装。

## 同一来源初始上下文中的 None 与错误对象

私有 `prepare_source_exists_numeric_query` 的实际执行准备可接收三种初始 cell：RDF、明确的 `None`、带固定来源构造事实的错误对象。`PreparedNumericInitial` 直接保留完整 `SolutionValue` 值域、输入顺序和错误对象身份；RDF 构造和 Dataset internalize 仍只执行一次，None/error 不走 RDF 转换。旧 RDF-only `initial_bindings` 从同一份值派生，不能替代来源上下文。

变量读取沿用当前 runtime 的初始 tuple。局部 None/error 遮蔽初始 RDF，只有缺失才回退；已有 frozen owner 继续使用自己的不可变初始值。投影移除未选择的局部键，project/forget/thaw 和子查询保留初始 owner。空 frozen row 的 thaw 仍读取原 owner 的 bindings，而非从空局部行重建事实。

来源 `BOUND` 接入同一编译器并核对实际 context/session/registry：RDF 返回 true，None/absent 返回 false；绑定的来源 SPARQL error 在 builtin 前抛出，由既有 Expr catch 返回同一对象。成功结果使用现有来源 Boolean 构造器与输出预算；原错误、取消、Dataset 失败和资源拒绝保持各自边界。

冻结依据为九项来源/当前输入协议和原 BX16，公开合成 fixture 为 `source_initial_cells_protocols.json`。原 BX16 的查询、初始 x=None 与未使用 err=NotBoundError(None)、列顺序、实际 None presence、false/true 来源 Boolean、空 flags 及错误 alias 关系保持。新增组件和子查询控制单独登记；实际执行资格及完整回归从外部源快照与回执核对。

旧准备入口、只捕获前端的入口、原 RDF 构造器、无来源诊断错误和 kind/class 不一致的错误继续拒绝这些新输入。公共 `/1–4` 与默认关闭的私有 feature 保持其原边界，未开放 `/5`。任意错误类/参数、未翻译 CompValue 的原 repr/set 顺序、完整 graph/time/bnode、其他关系与完整 EBV、全部 Semantica 和工程发布目标继续开放。


## Private source CompValue and syntax variable sets

The same prepared source compiler now reads the actual ordered `graph` child through the source Expr getter boundary and constructs the generic `builtins.Exception` outside the SPARQL catch. Its full argument formats the captured mapping: outer CompValue str has the repeated name, nested repr has one, instance attributes stay separate, and literals resolve their actual occurrence in the existing registry and reuse source_repr/Python quoting. The implementation never reads frozen answers at runtime.

Parser variable analysis retains native new/copy/reduce-or history, including skipped None children, Extend zip behavior and SubSelect projection. A private registry hook executes this temporary program with the existing SourceHashRuntime/SourceSet owners, preserving physical table iteration for `_vars` repr; the annotation membership view remains derived and sorted. Eight independently frozen original Python set protocols receive hash, insertion/copy/union and iteration behavior. These protocols are separate from the prior row-set qualification.

All remaining28 unchanged original queries receive full class/module/args, stage and decimal flags: EX22/EX24/EX30 fail at evaluate_start, other25 on first_poll with a terminally fused iterator. These are expected-error compatibility results, not successful engineering cases. Together with prior receiving, this reaches the fixed50 query set. Forty-four new test functions also cover generic fatal/catch, actual initial None/error, nested source mappings, owner refusal, cancellation and resource controls; final source and generated-contract qualification is recorded separately.

Unreceived Triples/path/function/native Values representation, arbitrary source exceptions/arguments, full graph/time/bnode, all relational/EBV/XSD domains and the complete original Semantica/platform scope remain open. Cycles and missing nodes refuse provenance; the private256-depth representation guard is not Python recursion equivalence. Allocation errors remain fatal; unrelated Boolean/Literal output budgets do not replace the generic exception. The public /1–4 profiles, /5 refusal and installed RC4 remain unchanged; this increment is private and not installed.

## 来源图常量与变量运输

私有来源前端的三元组常量现在直接携带实际 `ParsedSourceLiteral` occurrence，经过 `TermPattern`、`GroundTermPattern` 和当前 planner 的往返仍引用同一构造结果与 registry。普通 RDF 转换明确拒绝这些来源项；不从 RDF 相等反推原构造、缓存或语言。相同物理 RDF、不同 plain/typed 构造和重复 occurrence 分开保留，重复读取与克隆复用已有 source object，外来、缺失或改写的 occurrence 描述拒绝。

来源 SELECT 的变量发现、三元组变量分析及 syntax lookup 展开 subject/predicate/object，并保留 Bind、SubSelect 与 mapping/instance 的原有边界。变量 set 仍走同一 source hash/set program；排好序的成员视图只用于派生作用域。来源原库的19项 BGP/投影运输和 GT11 mapping 变量域独立冻结接收，含 literal metadata/cache/repr/hash；七项控制核对 owner、转换、planner、旧解析和当前 graph gate。正式执行资格以外部的实际同源回执为准。

这一步尚未授予真实 graph 读取资格。已冻结24项原库非空图查询的完整原结果和 trace，供下一轮 Dataset/BGP 使用；本轮仅接收上述运输部分。完整源三元组重排、parsed row/path 的 CompValue 表示、实际同一 Dataset 来源输入、None/error 赋值冲突、命名图/图切换/now/bnode 继续需要接入和接收。QuadPattern/source selector 仍拒绝未资格的来源图项；不会因为新增 term 运输而向公共 /1–4 或 /5 放开能力。本增量保持私有、未安装，完整 Semantica 与全部63项工程发布目标继续保留。

## 私有来源值数据集增量

`SourceMemoryGraph` 将带构造信息的真实值保存在同一个默认图及其 SPO/POS/OSP 派生索引中；源键区分普通字符串和 xsd:string，数值缓存不作事实去重键。八种 SPO 绑定模式沿原 Memory 的字典第一对象与查询绑定对象规则返回；全通配读取使用原生三元组哈希及 `SourceSet.copy` 的物理遍历。

只有 `NumericDataset::new_with_source_dataset` 的封闭通道可将同一实际 Dataset 和相同 Rc 查询 session 的来源值接回既有查询数据通道。原生回读仍先核所有者与实际内容，来源缓存不会掩盖错误；普通 SourceLiteralV1 不授予图扫描的来源信息。未知来源、外来图对象、取消和未核命名图分别返回明确错误。

原件先固定12项 Graph/Memory 协议，本增量接收11项默认图存取及十项 Rust 边界控制；DP08 初始错误模式只作后续基线。当前不是完整查询接入：原编译 BGP 排序与初始 None/error、三元组/path 容器、删除重插、命名图切换和 now/bnode 仍须核实。实际 ggen 和回归回执在私有维护目录登记后才作为本轮证据；本段不表示正式 RC4 已更新、公共 /5 已开放或完整 release 已完成。全63项原目标保留。


## 同一来源图上的编译 BGP 查询

本增量由封闭的 `prepare_source_graph_numeric_query` 明确接入：同一实际 `SourceMemoryGraph`、同一 Rc session、同一 registry 及已有 parser/planner/evaluator/presence decoder。译码与 native scan 共享实际原件的构造信息、缓存、对象和取消标志；普通 prepared frontend 或仅选择 SourceLiteralV1 仍不能获得这种执行资格。底层只有一个默认图事实集及其派生索引。

翻译时，每个 TriplesBlock 先按原算法重排，再合并；最后只沿 mapping 域 postorder 简化 BGP。执行进入每个 BGP 后，仅按当前上下文缺值数量稳定重排，编译好的 selector 使用原生 SPO/POS/OSP 惰性读取。Missing/ExplicitNone 在查图时都是通配，但已有 None 的赋值不允许覆盖；subject AlreadyBound 向外抛出，predicate/object 冲突跳过。返回错误以实际异常对象身份参与字典查找，不能冒充缺值。源 BNode 与同名 Variable 保持不同键，内部别名只作为既有 slot 的存储句柄。

SELECT * 在这个封闭阶段使用原生 Variable set 的实际插入和原地合并顺序形成 PV。`FILTER(?o = 1)` 复用现有 Literal.eq 值比较；数值相等不改变默认图的来源身份键。GT09 保留原负 Decimal token 的准备阶段 TypeError 参数；GT13 保留 SingleGraph 上 GRAPH 查询的 first-poll dataset-required Exception。未接收路径在普通解析器展开成 BGP 前就拒绝，防止跨过能力边界。

[固定公开投影](../vendor/spareval/src/eval/fixtures/source_bgp_query_cases.json)包含34个实际原库完整查询输入、12个代数排序结果、8个只替换排序子调用的原库协议及 DP08 的11次原生错误键扫描。接收比较原列序、行序、datatype/language/cast/cache、repr/hash、源对象共享关系、异常构造/阶段、flags 和终止。BO12 只接收重排结果；BR 的替代子调用不提供真实行执行资格。此前 Memory/BGP 门禁回执仍表示其当时有限范围，不被后续接入改签。

本轮实施与资格仍分开，最终执行回执绑定实际源文件和 ggen 合同。GT10 的 parsed EXISTS 容器异常、GT14 的真实 path、BO13 长度/flatten、任意 VALUES 的 SELECT * 与 native repr、命名 Dataset/删除重插/now/bnode、完整关系/EBV/XSD/异常及全部 Semantica 平台功能继续推进。私有 feature 默认关闭，正式 RC4 与公共 /1–4、/5 拒绝不因本段改变；全63项工程发布目标保留。


## 实际解析行与具来源的路径 mapping

私有图 frontend 在现有 PEG 归约时保留 `TriplesSameSubjectPath` 的原 flat row：逗号与分号共享一行，点号分行。named Path 的 `PathElt/PathSequence/PathAlternative` 和括号层次同样来自实际解析。Literal occurrence、原语言大小写、BlankNode 与同名 Variable 均沿既有构造 owner 保存，不能按 RDF 值相等倒推来源。

每个捕获的 TriplesBlock 节点只关联该次归约产生的物理 TriplePatterns；临时映射随解析 owner 释放。翻译沿同一算法排序/合并，现有 compiler、唯一 SourceMemoryGraph 及 presence decoder 执行。未翻译的 projection EXISTS mapping 和 instance 属性保持不同遍历域；错误 renderer 读实际 rows/path 与原生变量 set。没有第二图存储或运行时 AST 解释器。

[固定公开输入](../vendor/spareval/src/eval/fixtures/source_triples_query_cases.json)接收21个完整原查询输入：13个原期望 first-poll generic Exception（含此前未接收的 GT10），8个正常终态。5项额外 Rust 控制检查旧 frontend 边界、真实路径拒绝、实际取消与同值 Literal 的不同构造身份；这些控制不冒充额外原库成功案例。

此处只接收 resolved-IRI/literal 的 post-PName 表示；原始 ParseResults/pname/literal CompValue、真正路径运算、嵌套集合/reifier、native Values/function、非法 row length、图生命周期及完整 context 仍需另验。旧 BGP fixture 的 GT10 pending 是该历史增量的范围，不改签；本新增量通过后的资格由当前源码绑定回执说明。普通 frontend、公共 `/1–4` 与 `/5` 拒绝保持；正式 RC4、独立接收、全库和63项总目标未因此完成。

## Private native path compiler increment

40 fixed original native Path/Graph.triples inputs (38 normal terminals,2 first-poll generic Exceptions) and8 well-formed boolean-string constructor observations. Actual metadata/hash, object aliases, row order, real scan selectors and scan starts before each poll are compared. Seven Rust controls are separate policies. Query path grammar/translation/mixed-BGP ordering/projection and GT14 remain pending, as do noncanonical boolean casts, BO12 complete flatten,BO13,native Dataset lifecycle,public profile5,formal installation,independent acceptance and full63 release.

SourcePathDefinition is compiled once to closures over the sealed NumericDataset,source codec and graph scans. Native operands retain source identity and Python truth; paths remain outside RDF predicate terms. The full query entry still gates true paths before ordinary expansion. The frozen42 full path-query inputs are pending integration oracles,not passing Rust queries.


## 显式 source 函数注册增量

[私有 source 函数注册与执行](SEMANTIC-SOURCE-FUNCTIONS.md)补充执行时的注册表查找、原始语法和上下文读取、回调异常及宿主失败边界。它与既有数值/日期 owner 共用实现，原固定入口与公共版本保留；75个已曝光原案例和六项边界控制按本轮源码回执接收。原完整范围、LC04失败、独立接收和采用义务继续保留。


## 原验证数据的无损存储

七份 CD001–CD834 日历 Duration 验证数据复用已有的
`private.source-query-fixture/1` 压缩读取器。七个原始 SHA256 常量保留，
同时约束压缩包中的 `decoded_sha256` 和 `original_fixture_sha256`；
解压后恢复原 JSON 的全部字节。读取器原有的 1 MiB 输入、4 MiB 输出、
CRC、长度、尾随数据和摘要检查不变。另一份有效压缩包也不能替代本份原件。
原 834 个执行案例、输入、预期和失败历史全部保留；新检查只约束存储身份及完整成员。
源码总容量上限仍为 256 MiB。

这一改动为继续接入保留来源语义的 RDF 解析器释放容量，没有扩大运行时能力。
另行冻结的 173 步原加载和更新观察尚待 Rust 接收。其中，原后端的一次顺序更新
在写入后报错，版本号未增加，但数据改变且缓存失效；空更新在进入后端前拒绝，
缓存保留。后续须分别接收实际数据、版本和缓存，不能由版本相等推断回滚。
这些结论仅适用于绑定的原版本和输入；现有公开原子更新合同保持原含义。
完整解析格式、命名数据集、更新语法、公共入口和平台验收继续保留。

## Source UPDATE 的可变图基础

私有 `SourceSet` 和同一 `SourceMemoryGraph` 接续删除、重插、命名图与空图状态，依据原始 CPython/RDFLib 行为核对完整记录。相同最终四元组集合不保证相同读回：原索引可能保留较早对象，因此会话集成必须保留历史，不能从排序后的最终数据重建并声称等价。

本次覆盖79次集合操作、59次图操作与2360次读取；初次采集脚本的回调序列别名错误及失败保留，修正采集后重新运行原实现。外国 owner 和预执行取消由单独 Rust 控制验证。公共1–4和既有单图准入保持原义。当前仍须接续真实 DatasetSession 的持久状态、UPDATE WHERE 图上下文、Variable 数据、资源授权、部分失败与缓存/修订号，完整884个 UPDATE 原始动作尚未接收。局部基础能力通过不表示完整发行或方法收益。

## Source 会话的连续状态

私有 source 会话现在保留实际集合槽位、删除标记、索引父对象、图上下文和原始对象别名。加载先接收实际解析事件，再按原临时图遍历写入当前会话；ConstructDTO 保留直接顺序。查询使用同一状态及原解析/执行器，原生物化投影与来源成员在缓存使用前分别核对。不可变数据可跨线程传递，会话克隆的修改与父会话隔离；没有后台 worker、unsafe Send 或从最终集合重建历史。

独立原始137个动作在旧实现中有10次仅顺序不同的查询；本次沿相同原件接收，不改预期。另将原59个图状态/2360次读取跨线程传递后重新核对象身份、哈希、上下文及顺序。线程/克隆、取消、数据漂移和资源控制为独立声明的 Rust 检查。

已登记但尚未接收的字面量转换器只保留实际来源键和明确 Unknown 的值缓存；普通图准入与查询的拒绝不放宽。持久状态另检查保留的实际来源拼写字节不超过当前 dataset 字节上限，表/索引槽位不超过64倍 quad 上限加64；这不是实际分配器总内存或工程成本估计。失败不发布临时加载状态。

本增量仍未完成884个完整 UPDATE 原始动作：Variable 数据、真实 WHERE/GRAPH/WITH/USING、授权资源、部分修改及缓存/修订号继续推进。原公共1–4与独立旧合同不改，不开放5、不据此宣称完整 Semantica、平台或发布完成。


### Private source Variable and prepared DATA receiving

The source value carrier now keeps a real RDFLib-style Variable alongside RDF
terms. It retains stored-name identity, source hash and immutable object aliases;
conversion to a native RDF term is fallible. The sealed dataset transport reads
back that same source owner. It checks native bindings whenever a native term
exists; it never invents a URIRef for a Variable-valued fact. The prior RDF-only
single-graph role contract remains. Explicit source datasets can carry these
values through actual query parsing/evaluation, including source BOUND behavior.
Default query scans select the actual default context, while the explicit union
scan retains its established meaning.

The independent Variable observation predates these changes. Receiving covers
10 constructors, 64 pairs/384 relations, 80 direct operators, seven mutable graph
actions/56 scans, and actual source parsing of ten INSERT/DELETE DATA mutations
followed by 80 query projections. Query-error text is compared at the result-owner
boundary. The 105 full original runtime actions remain required: wrapper error
module/type, revision/cache and 15 snapshots are not received by the standalone
query test. The full preceding 884-action UPDATE contract also remains pending.

The prepared DATA executor uses its parser-owned registry and original source
session; it is not installed as the runtime UPDATE API. Native atomic UPDATE,
public profiles 1–4 and all existing requirements remain independent. Source
UCASE/LCASE positive execution is currently received for ASCII only; non-ASCII
case mapping remains an explicit unreceived obligation, without silently using
the host Unicode version. These are exposed development replays, not evidence
of generic method benefit or completion of the full Semantica/release scope.


The explicit DatasetDataV2 contract retains the previous DefaultGraphV1
constructor, native-coverage quad count and frozen refusal behavior. Only the
explicit DatasetDataV2 reports full source quad count. The source runtime DATA lifecycle
uses the same existing persistent source graph,
retained partial native projection, original all-source counts and actual
DatasetSession caches/revision. Complete VS/VP/VO/VA/VG receiving compares all105
frozen original actions, including result/error module/type/message, snapshots,
cache flags and ordered actual source term/context observations. Graph-list
observation can register the default context, as in the pinned original.
Typed result/snapshot failures keep their original class at the reached owner.
Failure publication retains reached data and clears caches without advancing
revision, under unchanged source/native resource and binding checks. Five Rust
policy controls cover partial failure, input-phase cache retention/native
admission, clone/thread isolation, explicit contract admission and bounded-state refusal; they are distinct
from original compatibility evidence. Full884 original UPDATE actions, complete
bindings/WHERE/WITH/USING, graph management/loading and all previously pending
errors remain necessary. Public profiles1-4, native atomic updates, all5674 prior
definitions,353 fixture byte sequences and63 engineering requirements remain.
No generic method benefit or full release is inferred from development replay.
