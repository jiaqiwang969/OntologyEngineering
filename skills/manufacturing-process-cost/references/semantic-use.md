> 独立核心版说明：原书稿不随包分发；文中的章节名称只作概念来源标识。

# 语义执行与证据范围

方法模块、语义候选、企业采用和产品放行各有状态。使用 [候选身份与覆盖](case-execution.json) 定位精确包；该文件是证据索引，不执行推理，也不能替代原生回执。

## 在本工作环境使用

1. 按根 skill 做 source-lock preflight、doctor 和 discover。
2. 从项目入口取得该项目自己的 binding、任务、授权范围及 Semantica workspace 路径。通用候选的 workspace ID 为 `manufacturing-process-cost-native`；路径由环境配置提供，不把作者电脑的用户名写进模板。
3. 通过根入口 `history --binding … --workspace … --candidate …` 查原生候选状态。`open` 只登记接入，缺执行证据时可以返回 blocked；不能据此声称验证通过，也不能把“没有跑验证”误写成产品失败。
4. 候选测试只能证明所列输入与 oracle 的结果。分析真实项目，要投影本轮有来源的对象与关系，说明未投影部分，再执行与本次决定有关的查询；另保存本轮输入、回执及输出验证。
5. 按根入口和 `domain-ontology-loop` 治理新语义增量。已提交的资产不覆盖，修正使用后继候选。实际运行须检查 execution、oracle、regression、receipt 和 release，不能只读退出码。

当前包是本地技术候选，未晋升到行业 registry；不能用内置包的 `run --package` 假装它已注册。根 skill 已随附冻结的 Semantica 案例传输包，可用 [案例运行入口](../../../scripts/run_manufacturing_cases.py) 独立回放；入口校验容器、每项资产和运行时锁，再交给 source-locked `create_package_runner()`。它不安装 registry、不沿用原项目权限，也不创建第二套语义实现。工作区状态转换仍由统一入口和 Semantica 负责。

本工作环境另有工艺替代[因果与反证关口](causal-decision-gate.md)的 Semantica 后继候选 `0.1.3`，精确 delta/candidate SHA-256 为 `73bdf2e23f73d64b98aa5fc0223ee1d842fccfdbf0d2ae8ccc2c031e6fbd4dd8`。它增加必要功能非空与逐项覆盖、适用挑战、参考模型支持边失效及输出过度承诺的原生查询／SHACL 场景；75 个脱敏场景的原生草案回放均通过，证明仅限这些输入与 oracle。当前原生状态为 **`proposed`**；`commit`、提交后的 regression、`release_complete`、promotion 和真实项目执行均未发生。因此它还不是已发布或已采用的项目门禁，冻结传输包 `0.1.1` 也未替换。较早的 `0.1.2` proposal 保留为历史草案，缺少“声明清单完整却没有任何功能条目”的反例检查，不应选作后续提交目标。换机器时先用 `history` 核对该候选是否存在；不能从本段文字推断候选已随传输包分发。

## 换到另一企业或机器

复制独立核心包即可取得方法、合成案例、记录模板、锁定运行时 wheel 和冻结的制造案例资产；两卷书源不在此包。无需访问原客户项目、作者目录或旧工作区。分发与新目录检查见 [独立分发说明](../../../docs/PORTABLE-DISTRIBUTION.md)。从根目录执行：

```bash
python3 scripts/run_manufacturing_cases.py --list
bash runtime/setup_runtime.sh --preflight
bash runtime/setup_runtime.sh
runtime/.venv/bin/python scripts/run_manufacturing_cases.py --run \
  --output ../work/manufacturing-replay-001
```

首次安装仍需要 Python 与相应依赖；本包未携带所有平台的离线 wheelhouse。`--list` 只检查传输文件；`--run` 才会在新环境执行并验证案例。输出目录必须为新路径，不覆盖旧回执。冻结包的源工作区状态是历史信息，不能代替本次运行结果。

真实企业使用自身事实、角色权限、任务信封及绑定，工作状态在企业自己的 Semantica 工作区产生；这些是运行输入，不是依赖原作者电脑的缺件。冻结教学案例不能自动变成企业正式采用的本体或制造规则。按根 skill 在新组织范围内治理变更，不复制原项目绑定冒充新企业授权。

企业采用具体工艺、供应商、检验或报价仍使用该企业的证据及决定。方法版本采用不会将一个项目的参数变为默认工艺。

## 覆盖怎么理解

- 原 16 类设计各有正常、单故障、歧义、历史模式四种输入，共 64 个变体；历史模式是回归构造，不声称每种错误都实际发生过。
- 另有 4 类组织关系：交接关闭、决定权限、采用及再使用、效果可比性，各 4 变体。
- 原生 runner 每场景有两个固定槽名，因此 80 变体对应 40 个配对场景；槽名不代表实物合格/不合格。
- 另检查空输入、费用重复、版本接受范围、配置要求下的复核独立性、重复消息、关闭记录、有限规则和词表声明。
- 原生通过确认方法中有限关系的表达和检查。它不证明所有工程问题已覆盖，也不证明客户已接受、能力已测或降本已兑现。

一次词表审阅发现作者工具把关系名识别成了类型。原候选保留，后继版本修正并新增原生反例。这说明原有案例通过的范围有限；检查自身模型与新输出仍然必要。

冻结传输包只在 Semantica 中形成并验证后导出，按新版本和精确摘要更新；不在 ZIP 或临时解包目录中直接修改语义资产。其 manifest、141 项资产及原生候选身份由 [传输锁](../../../runtime/semantic-bundles.json) 对应，解包只用于本次原生执行，不作为新的作者正本。

工艺替代的项目级审查走根入口的 `review`，用当前 task 中有摘要的 RDF ABox 和
已晋升 package 的命名 CQ/shape，检查旧工艺承担的功能是否由新路线覆盖、开放挑战
是否仍支持整批交付主张。`0.1.3` 的因果关口目前是候选，可用于作者测试和案例回归；
它未晋升前，不能把候选测试的 `clear` 当作真实项目的正式 Semantica 审查结论。
