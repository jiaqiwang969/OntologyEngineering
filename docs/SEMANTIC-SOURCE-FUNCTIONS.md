# 私有 source 函数注册与执行

发布接续：下文历史“默认关闭”属于独立库默认配置或原轮次，不代表 RC.5 CLI 的实际依赖图。RC.5 显式编入 source-session 所需两个 feature；运行准入仍按请求版本，公共查询 `/5` 继续拒绝。编译范围检查与行为/平台接收见[发布范围](RELEASE-SCOPE.md)，旧 RC.4 原门槛保留。

本增量在现有 SourceLiteralV1、SourceContextV1 和 source graph 会话上显式附加共享的 Rust 函数注册表。它接续工程本体约束 → 锁定 ggen 合同 → Rust 执行 → 冻结反馈比较的原链路。实际接收与安装看绑定本轮源码的回执；公共查询仍为 /1–4，默认关闭私有 feature，正式 RC.4 不因源码变化自动升级。

注册表是可显式共享的句柄。相互独立的句柄隔离；共享句柄下，在实际调用时读取函数条目，取出条目后释放借用。因此解析后、执行后、读下一行前或回调内部的注销和替换都能影响后续调用，当前已取出的函数继续完成。重复注册、override、装饰器身份、注销兼容参数及缺项警告分别保留。警告返回结构化类别和文字，由宿主接收；Python 全局 warnings 过滤器及自定义钩子不因此视为已迁移。

同一源解析保留 Function 的有序字段、IRI、DISTINCT、参数结构和变量范围，计划器继续携带这些元数据。raw 回调按需读取参数；没有 expr 字段是 None，每次读取都重新执行嵌套参数。普通回调展开参数列表。上下文查找读取原冻结绑定的局部值和 initial 值，保留缺项、None 与错误对象。返回 SPARQLError 对象与抛出异常分开；只有普通调用按原构造器转换 TypeError，参数个数错误也保留。宿主取消、资源失败和实际触及的未支持操作持续失败，回调不能将它们改签为成功。

七类 default_cast 共用原数值和日期 owner，实际调用 IRI 决定转换含义；别名注册不会自动改成注册时的某个类型。原固定入口继续按原合同执行。任意 Python 对象、动态修改表达式/上下文属性、一般异常对象参数及 Python 插件运行不是当前 Rust ABI 的能力。本轮可捕获的宿主表示只覆盖新回调异常 owner 中的显式类型；既有数值、日期或其他表达式所产生的源异常，尚须逐项核对它们与 fatal 宿主失败的桥接，不由这75个案例推定全部兼容。原始输入中函数名和错误文本的兼容字段不授予脚本执行能力。

接收材料为75个提前观测的原输入/输出、回调程序与生命周期事件；额外六项边界检查包含旧87个 stock 查询。核最终行、列、标志、异常、注册状态、回调顺序和已支持的 RDF 对象别名关系。原 Variable 异常对象的任意身份及全量 AST 内部对象协议仍另验。Rust 的 after_parse 接点在 source parser/frontend 返回之后、既有查询绑定/计划之前；after_translate 接点在准备查询返回之后。比较并不声称两个实现的内部阶段划分相同。

这些材料已经曝光，属于 development_replay；不能证明工程方法收益。现有 Project/work/Harness 的提出假设、冻结可区分动作、保留失败/部分/无法区分结果、有证据地更新决定继续适用，本增量记录 no_delta。参数优化沿旧合同；机制探索保持共同父目标、载荷和完整必要验收，必要时显式扩展合同。采用审查、独立验证及后续任务反馈仍保留。

268份旧 fixture 不变，LC04 缺原随机构造输入的历史失败继续显式回归。完整63项目标、19模块/19协议/22 owner/13494范围、平台、原生 CAD 和工程交付不因局部通过而缩减。实际项目最终的专属精度要求仍需实物、工况、覆盖及不确定度证据。


## 运算异常与回调的接续

显式 SourceLiteralV1 的乘除链在已提升的 float 遇到 Decimal 右操作数时，由数值 owner 保存原 TypeError 类别、参数和实际 *= 或 /= 运算。共享 source_exception 类型供运算及回调使用；没有注册表时也保留该来源构造。既有非 source profile 保留原 MixedNumericTypeError，不能从笼统诊断文字猜出源异常。

36个提前冻结的原查询覆盖直接运算、普通和 raw 回调、捕获与忽略参数、SELECT/BIND 部分行、除零返回错误、成功算术以及所列日期比较。普通调用按原规则转换 TypeError，raw 调用可以继续向外传递或捕获它。资源超限、取消和未支持操作仍由宿主保留，回调返回空值或另抛 TypeError 不能覆盖真实失败。额外三项控制分别核旧 profile、无注册表构造及实际资源失败；此前取消和未支持操作的控制继续回归。

本增量保持269份旧 fixture 字节、LC04失败、完整原目标和采用条件。它补齐上述实际观察的运算与回调连接，不表示所有源语言异常、任意宿主对象或全部日期运算均已迁移。既有探索合同足够，方法记为 no_delta；曝光回放和局部软件修复不能证明跨工程任务收益。


## Decimal 条件、原异常及下一步处理

本候选继续同一数值会话和 source_exception owner。已观察的 SourceLiteralV1 一元/加减/乘除和 float→Decimal 路径把 DecimalTrap 保存为原 Decimal 异常类别及“已启用 trap 的信号类列表”。会话同时保留全部实际触发标志；异常参数和 flags 的含义不同。旧 profile 仍保留数值核心故障类型，乘除对 InvalidOperation/DivisionByZero 的原 SPARQLError 转换继续执行。

FloatOperation 保留其 TypeError 子类行为：普通函数按原规则转换为带类列表参数的 SPARQLError，raw 函数可分别捕获 DecimalException 或 TypeError。类列表不是普通文字参数；str、repr、Literal(error) 缓存和再次读取保留来源结构。未读取参数不触发计算。资源、取消和未支持执行仍属于宿主故障，回调返回空值或改抛 Decimal 异常不能覆盖它们。

108个原案例分别在对应实现变更前冻结：93个查询/回调案例、3个实际 str/repr/Literal(error) 观察，以及发现 STR 编译入口未接通后新增的12个正常值/空值/错误值原案例。原96个案例的诊断保留3项 STR 未接通失败，未改旧预期。Builtin_STR 仅在已有来源图执行准入中接入，同一表达式 owner 保存返回错误并传播真实异常；文字构造仍走原资源预算。旧93个与全部270份旧fixture不改。另有旧profile、无注册表构造/flags、资源失败和封闭前端不获执行权四项控制。固定上下文包含精度、指数边界和 trap 设置；本增量不授予查询中任意改变会话上下文的能力，也不概括所有异常继承或任意宿主类对象。

这些区别供调用者保留失败含义、选择有依据的处理动作，不自动采用工程结论。现有 Project/work/Harness 探索方法足够，方法记 no_delta；验证资格以当前源码快照及实际回执为准。公共合同、LC04历史失败、完整原63项与原工程交付继续保留，完整平台/独立接收/正式采用及后续任务收益仍需另验。

旧上下文、排序和 EXISTS 接收器按原记录的 Decimal 类别核验。内部从 Core trap 扩展为显式来源异常后，接收器读取该结构，并继续检查原类别、标志、行、阶段和终止行为；来源异常还须保留类型化类列表。未改动的比较或聚合路径仍核原 Core 信号。旧原件不变，其中未观察到的参数不能因本次适配而声称补齐；新案例单独严格检查原构造，不接受笼统 Core 故障代替。


## 原始关系比较接续

当前候选把限定来源图中的六种比较接到既有 Literal/数值与表达式 owner：等值和非等值沿 value equality，大小关系保留原 gt/lt/ge/le 的短路与 NotImplemented 处理，不用普通总排序或反射比较代替。CPython 3.13 的默认警告策略与原观察绑定；完整宿主警告过滤器仍未接收。旧 ORDER/extrema 的原方法和证据不被关系运算的成功替代。

关系边界将实际 Decimal trap 接到共享来源异常。比较已经触发的全部 flags 保留，但异常参数只含本次抛出类别；例如等值比较的 FloatOperation 标志不加入后续 InvalidOperation 的参数。FloatOperation 按其 TypeError 子类身份经过原 SPARQLError 构造，普通/raw/捕获以及错误 str/repr/Literal 消费继续同一 owner。宿主预算、取消、未知输入保留原失败，不能被回调吞掉。新 LT/GE/LE 仅经原来源图准入执行；旧默认/公开 profile 和封闭前端权限保持。

360 个独立先观测的原查询分四份冻结 fixture，并列明四项无注册表/封闭前端/旧 profile/宿主资源控制；实际资格须核本轮源码、ggen 与回归回执。273 份旧 fixture 不改，LC04 随机身份失败与完整原63项/全 Semantica/原平台和工程验收继续保留。此为 development_replay，未由这些测试取得一般方法收益、独立接收或正式发布资格。


## 原始值域与不等比较接续

来源图构造复用当前 Literal 元数据、日期解析和共享 duration 词法/微秒累加；保留 Date、固定 timedelta、布尔值及原转换表以外 datatype 的真实 None 缓存。旧快照 normalizer 仍使用同一时间原语，其日历 Decimal、求值顺序和 10000-byte 原预算保持。输入未成为事实、元数据伪造或已登记但未接收的转换器继续拒绝；日历 Duration、类型动态绑定和这些新类型的直接初始构造按未接收处理。

比较使用原 XSD_DTs 的实际20成员，dayTimeDuration 不是该清单成员。无效或未知 datatype 的同类型不同词法值保持原 TypeError→SPARQLError 参数，不能伪造 false 或普通总排序。原 Decimal flags/异常和宿主资源/取消路径继续原合同。

同一 Literal 比较算法显式接收调用范围：新增缓存的关系比较资格不扩展到既有 ORDER/extrema。旧布尔缓存排序拒绝与 RDF 身份短路继续由原测试核对；日期、时长和未登记 datatype 的排序资格仍需另行接收。

原 != 经追加的 SourceNotEqual 保留到同一表达式编译器。它在原操作数和异常之后作真假取反，不能改写为逻辑 !(a=b)：缺值时两者的回调错误确实不同。真正的 NOT/EBV、旧解析模式、枚举原判别值与默认关闭权限不变。

396条原查询（360值域加36不等/逻辑取反对照）、原类型清单与4边界控制共同接收；原277份fixture逐字节保留。当前资格须查看源码绑定的ggen、回归及后续接收回执。LC04旧失败、完整原63目标/完整Semantica/平台/工程交付保持开放；此开发回放不证明独立泛化或方法收益，正式安装另行验收。


## 直接初始值与实际表达式消费

接续上述原值域合同，封闭 source graph 初始入口增加 date、固定 dayTimeDuration 和真正未登记 datatype 的直接字符串构造。normalize 开关保留原词法或使用同一 temporal owner 的规范化，已知有效/已知无效/未登记转换分别保留 false/true/None；query Literal(existing) 原 None 状态不改。时长格式算法从现有 snapshot normalizer 移到共享 temporal owner，原日历/Decimal处理与资源预算继续原合同。

同一表达式编译器接入来源 DATATYPE 和 IF。前者按原元数据返回类型，后者先检查条件再只执行选中分支，沿用 source EBV；变量错误、回调返回错误、None与普通异常不混同。原384项初始构造案例保留，另62项冻结对照核对语言/空类型、错误与分支调用顺序，4项独立控制检查元数据、注册/日历拒绝、输入输出预算和旧入口关闭。未接收操作数域与旧ORDER/extrema不因本轮IF或构造成功而开放。

本增量共有446原查询与4控制，使用原Project/work/Harness记录实际缺口、判据、回读和有范围的决定；通用探索方法no_delta。283旧fixture和完整原63项、原失败与平台/工程验收继续保留。私有feature默认关闭，公共查询仍/1–4，独立接收及正式安装以各自回执为准。开发回放不能证明后续工程有效性或方法收益。


## 类型对象身份与构造证据

DATATYPE 读取沿既有 source value owner 保存类型 URI 的构造身份。同一 literal 的重复读取和传递副本共享该身份；两个独立构造的 typed literal 即使类型文字相同也不合并。无类型和语言类型的默认结果分别复用当前 prepared session 的命名空间对象；不声称跨 session 的原进程缓存寿命一致。Literal(existing) 新建 literal，但无类型覆盖时保留原 datatype 对象；拆成 RDF/cache/metadata 的记录不能重造原对象身份。原 IF 选择和回调副作用由既有表达式 owner 继续承担。

49 个原始查询与3项控制核对完整结果、重复读取、独立构造、对象复制、未知来源及既有构造器输入。4次原 LC04 输入各自保留真实随机标识和异常，复用已有 constructor hook 进行带原输入的独立比较；旧 fixture、旧失败及未固定随机数的测试不改签。39项主观察与6项实际嵌套回调补充观察均按原输出保留，包括空参数函数产生的原错误。

通用探索方法保持 no_delta；Project/work/Harness 用冻结判据实际区分随机构造差异与类型对象身份缺陷，再执行读取、回读和有范围的决定更新。该 development_replay 只支持声明的行为，不证明一般工程收益、总成本或正式采用。完整原63项、288旧fixture及完整Semantica/平台/工程验收继续保留。


## 显式时间值构造与消费

在同一私有 SourceContextV1 上显式使用 `with_source_time_values_v1` 接入 xsd:time。既有 graph 会话仍拒绝该类型，旧测试继续执行；公共 /1–4、默认配置与早期构造入口不升级。共享 temporal owner 的既有解析器和格式化器承载不可变 Time 缓存，不新建查询引擎或重复解析规则。

312 个提前冻结的原始查询覆盖26种字符串、直接构造的 normalize=false/true、查询内构造，以及 identity、DATATYPE、STR、IF/EBV 消费。直接构造区分有效与无效缓存；查询内已有 literal 的 ill_typed 保留 None。无效转换、微秒截断、时区偏移、回调顺序和实际对象别名按原观察接收。3项独立控制检查元数据篡改、未知来源、输入输出预算、语言冲突、旧入口拒绝和未接收的排序边界。

Time 比较、ORDER/extrema、运算和其他未接收转换器继续列为缺口；构造通过不等于这些能力通过。旧291份fixture、旧LC04随机构造失败及完整原63目标保持原含义。既有 Project/work/Harness 对本轮做冻结、执行与回读；通用方法 no_delta。development_replay 不证明通用方法收益、完整工程达成或正式采用。


## 显式时间比较、排序与极值

`with_source_time_comparisons_v1` 在同一私有会话中额外接收 time 缓存的值比较、ORDER 和 MIN/MAX。原 TimeValuesV1 仍只接收构造及既有消费；旧拒绝测试不改。保留原 RelationalExpression 类型集合，因此两个 time literal 的大小比较仍按原程序给出错误，不由排序可用性推定放开。

复用 shared Time、literal rich comparison、原库对time的naive/aware分组、反射 LT、词法回退、稳定排序和极值 owner；不另建查询引擎或借用 datetime 算法。锁定 CPython 3.13.11 的 time_richcompare 对不同的 aware offset 使用 timedelta 整秒分解，再比较 time 的微秒字段，负偏移采用向下取整；这属于源兼容行为，不是物理经过时间的通用计算规则。来源见官方标签 Modules/_datetimemodule.c 的 time_richcompare，原始观察与来源摘要各自保留。

312个主观察保留直接原文本/规范化/查询构造、原相等/不等/有序比较、8组序列的双向稳定排序及极值。另24个补充观察区分正负小数偏移与完整微秒 UTC 运算。3项控制核旧入口、会话隔离和未知来源。旧295份fixture及历史LC04失败保留。原所有源/输出预算、公开/默认profile和完整原63项范围不变。

通用Project/work/Harness方法no_delta；按提前冻结的312项主观察判据实际执行来源判断，24项补充证据保持独立身份。development_replay只支持声明行为，方法收益、物理原因、完整工程达成及独立/正式接收仍未知。时间运算、任意tzinfo/fold对象及剩余完整API继续按原主线推进。


## datetime 值比较与 Literal 排序的分工

原 RDFLib Literal.__gt__ 对 datetime 和 time 使用 _TOTAL_ORDER_CASTERS：先按 naive/aware 分组，再比较组内值。现有 datetime 路径此前遗漏此步骤；修复同一 Literal 比较 owner，同时保持原 DateTime::value_cmp 在 mixed-awareness 下返回 None、value_eq 为 false。datetime 仍使用完整微秒 offset，不借用 time 的整秒分解规则。原有 time 显式准入、类型检查、无效值词法回退、稳定排序、MIN/MAX、未知来源拒绝及公开/default边界继续保留。

324项冻结原观察覆盖16组数值对的三种来源与六种比较，以及12组序列的ASC/DESC和极值。28个完整场景测试逐项执行所有成员，包括基线失败后的剩余成员；每个原case只产生一次有身份的终态，接收程序额外核全324个ID。2项控制核原始值语义及来源限制。分组用于去掉重复合同文字，原预期和验收分母不减少。新增规范保留旧前缀及固定源/输出预算，经真实锁定ggen生成。

该修复接续同一Project/work/Harness的来源调查与限定决定；通用探索方法no_delta。开发回放、完整Semantica资格、后续任务收益及工程采用各按自己的证据判断。

完整验证映射仍受原 `generation_identity` 合同约束。日期时间接收后，旧模板的 evaluator 单组实际达到 4,117 行，触发锁定 ggen 的 4,096 行上限；失败原件保留。模板现按名称 SHA256 首位分成四个互斥且穷尽的查询组，再连接并按名称排序。既有独立检查从全部规范验证项直接投影完整有序结果，核对实际 ggen 输出；不使用 LIMIT、不截断映射，也不提高上限。此处只调整生成执行分组，原规范含义与工程验收继续保留。


## 日历时长：共享值、构造阶段和实际查询使用

私有 SourceCalendarDurationV1 复用现有 source session/graph factory，默认关闭。semantica-temporal 的 DurationContext 同时服务原快照规范化与查询构造；年份、月份和固定 timedelta 分别保留，Decimal 运算沿共享 numeric owner。缓存绑定原词法、normalize 策略与构造时的 Decimal context，不能从归一化后的 RDF 猜回原缓存。

实际查询接入构造、EBV、比较、稳定排序、极值、有限日期加减、STR/DATATYPE/BOUND 和嵌套 isLiteral。原程序中年月转位、月末截断、Duration 不支持原生大小比较后回到 Literal 词法次序、部分 Duration-first 减法返回 None 等行为分别保留。初始值只构造一次，并先于查询解析；失败的原类型、参数、Decimal flags 以及未执行的 registry 动作均接收。原入口、公有 /1–4 和默认关闭规则继续保留。

834 项冻结原观察由50个完整场景逐项执行，包含13项原构造错误；另有3项来源、旧入口与年月/固定时长边界控制。旧305份预期原件保留。当前范围不覆盖任意 Decimal context、calendar 溢出错误全貌、时间/日期差全API或独立接收；局部匹配不代表完整 Semantica 完成。

这轮沿现有 Project/work/Harness 使用已冻结的来源调查方法，记录从基线构造拒绝，到分辨后续运算/错误阶段/表达式缺口，再到有范围的结果回读。通用方法 no_delta，方法收益及工程采用仍须后续任务证据。


## 日历时长的上下文与错误接续

在原私有能力内补接9组 HALF_EVEN 精度、指数范围和trap设置：711项查询、21项原divmod及2项来源/失败恢复控制。非有限年份/月分量、格式化商余数及sticky flags由原temporal/numeric owner承担；构造时吞下的转换错误、Literal回退后运算错误、年份ValueError、C-int与最终日期OverflowError分别保留。缓存复核绑定原构造上下文，不能污染另一个查询的flags。

原834项默认上下文及312份旧fixture继续回归；旧/public/default入口不变。其它舍入/clamp、完整任意上下文、日期差/时间运算与全部API仍待验证。Project/work/Harness继续承担先冻判据、分辨缺口、回读结果、更新限定决定；曝光回放不证明一般方法收益。


## 日期时间差与原错误接续

同一私有会话补接480项冻结输入：日期/日期时间差、微秒与offset、混合awareness、无效值、时间运算、连续算式及结果消费者。差值使用既有缓存与精确整数，构造原timedelta结果并保留ill_typed=None；原TypeError/IndexError/AttributeError/RecursionError与资源限制分别处理。

24项原无效左值递归先使全跟踪观察器失败，后经无跟踪原运行及限定Function事件监测取得有效错误证据；原失败与两份观察均保留，不据此声明任意调用栈或递归内部轨迹等价。原318份fixture、原公共/default入口、完整63验收继续保留。其它未接收时间/上下文/API、LC04与全库/平台/工程交付继续开放。本轮仍是development_replay，方法收益待后续任务验证。


## 时间操作数、无效缓存和嵌套接续

同一私有owner接续762项冻结原输入，覆盖无效time/date/dateTime、三种时长、操作数顺序、嵌套与非Literal操作数。先做原兼容性检查，再按实际缓存与Literal回退保留combine TypeError、显式yearMonthDuration错误、原递归终止错误及datatype AttributeError。既有source类型owner共用类名，不把缺少属性、datatype=None和未知来源合并。

102项全帧跟踪失败分别用无跟踪运行与限定Function事件监测恢复原观察；原失败不改写。本轮保留原325份fixture和原公共/default入口，完整63验收继续。内部递归深度、任意上下文/跨类型API、全部平台及工程交付仍另验；本轮development_replay不证明通用方法收益。

新增8份案例的完整JSON以显式私有gzip/base64封装保存，Rust测试按原字节摘要、解码预算和终止位置核验后读取全部字段；压缩前原件及首次源码容量失败继续保留。已有325份案例不变，原256MiB源码和规范输入上限不变。解码依赖只由默认关闭的私有feature启用，调用在测试接收代码内；没有新增运行时语义分支。两项附加检查验证完整762输入/102恢复观察及损坏、超限、尾随数据拒绝与恢复。

后续混合操作数接收另冻结792个原程序输入（735正常结束、57原生异常、无观察器失败）：覆盖日期/时间/duration与无datatype、字符串、数值、布尔、anyURI/未知datatype、语言字面量和IRI的双向加减，以及显式None、初始/回调错误、嵌套长链和后续回调的实际求值。首次696项匹配、96项在字面量构造阶段失败；按原源码补齐anyURI和查询typed-boolean入口，并区分既有Literal与直接字符串的构造身份及ill_typed。另冻结19个实际返回值案例（布尔四种合法词形、空白/Unicode/任意anyURI文字），连同来源身份拒绝检查，共13项新增回归、4项约束。全部811输入按原预期接收；算术owner保持no_delta，复用既有有界测试资料解码器。原333份案例、原失败、公共1–4和默认关闭的私有feature保持；本轮是development_replay，不证明方法收益或全库完成。
