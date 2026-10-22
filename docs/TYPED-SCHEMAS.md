# 原106模型和typed执行

工程本体论规定含义→ggen生成合同→Rust Semantica执行→RRSI根据实际反馈检验改进。中性模型能力是这一体系的代码化能力，不获得独立的工程含义、工程采用、Project或版本治理权威。两本方法书、四本机械教材与 MISUMI 仍沿本地知识、工程与专业执行入口组织。

`specs/engineering.toml` 的106个 `semantic_models`、529个字段是本片的编辑正本。保留原字段注解、默认及工厂结果、别名、约束、原字段行、源和类摘要；模型JSON Schema含原递归引用。Rust normalizer只校验/规范化、建立RDF关联与变化影响，锁定真实ggen产生catalog、合同及106个强类型snapshot。原 `runtime/semantica-scope(.lock).json` 的13494范围不变。

`RustSchemaRuntime` 从编译的生成catalog读取规则，提供原构造、`model_validate`、unchecked `model_construct`、dump/by_alias/property、exact JSON Schema及显式生成port。构造不是校验，校验不等于工程采用。`property_on_output` 读取已经存在的字段；缺失必需字段的unchecked构造仍可成功，读取原属性会失败。默认列表/对象每次独立复制。

生成snapshot保留具体标量、递归/嵌套类型与原无界整数；整数使用 `serde_json::Number`，不得直接截断到i64。原字典使用单一JSON owner的 `OrderedObject<String,T>` 保留成员顺序及真实字面对象。snapshot公开数据本身不是校验证书；`validate_typed<T>` 先核生成类型的模型/源/类身份并调用同一验证器，随后转换。完整生成enum `SemanticRegisteredModelSnapshot` 为CLI/schema port与动态typed消费者提供同一106强类型边界，generic `generate_snapshot<T>` 保留实际receipt和同一调用预算。

`RustTypedRuntime` 的manual流程只通过 `SemanticTypedDispatchPort` 调用。工程适配 `RustTypedDispatchHarness` 复用唯一 `SharedModelCallContext`、既有 `generate_base_structured` / `generate_provider` 及凭据结果guard：structured-first、plain fallback、原totalN轮、校验反馈及原sleep均真实执行。budget/guard/configure拒绝立即终止。成功观察再次核实际request SHA、raw响应解析与给定输出，DTO自报completed不能越过这条绑定。默认参数的显式Rust后继 `generate_default` 保留三轮；wire DTO的max_retries是显式整数，原float/None反射输入不转成成功。

CLI `oev2 schemas list/show/evaluate` 受同一编译源和生成合同绑定；纯模型候选没有模型调用与工程采用。evaluate输入是生成 `SemanticSchemaValidationRequest`，含原始UTF8 JSON及SHA。`model_construct` 返回执行完成但 `validation_executed=false,valid=false`；操作退出码不等于工程判断。

实际验收原106个纯模型的JSON域、原21语义抽取案例、原17manual案例及共享预算/凭据反例；私有receipt逐命令记录当前字节和终止状态。静态catalog/API/consumer库存不是执行证明。原native NaN/Inf、datetime实例、任意Python对象/反射以及Instructor SDK和业务消费者须保留各自unknown；不能用有限JSON通过或snapshot读写替代Explorer、各route、CrewAI/server业务的实际迁移。原消费者绑定调查只给接续边界，未执行状态逐项保留。

模型记录进入 `changes` 与 `impact`，触发语义合同、schema/typed owner及其验证。RRSI候选仍经过同一ggen→构建→固定原oracle/基线比较→既有资格/版本owner；本片不另建selection/use/feedback/recovery账本，也不把模型校验状态升级成工程采用。

文本转换复用单一 `semantica-json` owner：原Python Unicode15.1 printable/whitespace由独立历史builtins冻结，有限浮点repr共用既有formatter，不在schema复制第二算法。对原837标量边界的74个非有限native成功观察仍明确unknown；有原约束拒绝的NaN/Inf以原错误观察核对。

原置信度validator显式调用Python float与普通Pydantic float的Unicode数字语义不同。单一JSON owner实现完整Unicode15.1 decimal映射；超出浮点可表示范围的原整数触发明确overflow失败，manual typed按原普通异常每轮等待一秒重试并保留原prompt。整数约束按原任意精度比较，不能因as_f64失败跳过大小限制。原1120+360新增数值预期不修改，93个非有限native成功观察仍unknown。
