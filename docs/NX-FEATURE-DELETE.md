# 有源目标孔特征删除：显式候选合同

`delete_hole_feature` 接续 P05：在来源闭包的任务副本中，查清当前目标孔对应的真实特征，再调用 `UF_MODL_delete_feature`。不会重新创建少一个孔的板，不会按名字、生成顺序或旧进程 tag 猜目标。工程稳定特征 ID 保留在 `target.feature_id`；当前 NX tag 只作为本次独立辨认后的调用参数。

共享结构 `NxFeatureDeleteIntent1`、`NxFeatureObservation1`、`NxFeatureInventory1`、`NxFeatureDeleteOutcome1` 来自 `specs/modules/cad-engineering-binding.toml`，由锁定 ggen 生成。参数为 `oev2.nx-feature-delete-params/1`，包含 `intent: ArtifactReference` 与原 `NxAssemblyBudget`。意图分别声明旧对象版本与来源 SHA、完整 PRT 来源闭包、目标原型路径、目标和保留孔的圆边意图、板外边界、目标原生特征类型与位置及来源引用。旧对应关系 `/1`、组件复制 `/1` 和已有构造语义不变。

首个可执行 profile 是毫米制、轴向矩形板、两个互不相交的竖直圆形通孔。它对应现有受控 `CreateHolePlate` 的减圆柱构造。`feature_type` 冻结为 `CYLINDER`；只有真实 NX 查询返回该值才可继续，实际出现其他类型必须记录反例并调查，不能替换字符串或按成功需求改预期。此有限 profile 不表示一般孔、布尔组合、阵列、混合单位或任意特征删除已完成。

选择前先完整读取原型及装配、两种参考集、详细几何和该 body 的全部特征。每个特征保留实际 owner、body、是否 occurrence、原生类型、位置、当前关联面和五项原始查询码。目标必须同时满足当前原型与实体、类型、独立位置意图以及目标柱面关联，且完整枚举后恰有一项。零项、两项、未知查询、错误 owner、不同 body、旧进程编号、遗漏关联面或不完整来源均拒绝；不把其余未知候选略过。目标关联面通过当前几何辨认，特征名和创建顺序不能提供独立同一性。

几何检查覆盖完整圆边域、完整面域、六个外平面、板的精确 work-CSYS 外界和另一通孔的上下完整圆边及柱面。目标上下圆边必须从各一项变为在完整域中零项；全部圆边从四个变为两个，面数从八个变为七个。`UF_MODL_ask_face_data` 的 `face_bounds` 仅承诺包围，不保证最小，因此检查其有限、有序且包围预期范围，不能当作精确最小尺寸。面法向 `norm_dir` 仅按 SDK 接受有效的 ±1；它本身不能证明孔的内外语义。原生装配的体积另按固定板与圆柱尺寸独立计算核对，误差仍用已有装配持久性检查的 `1e-12 m³`；详细 `PartInfo` 没有体积字段，不虚构该测量。

真正调用前，原生边界再次核当前 work part、prototype solid 与 feature 的 owner/body。创建 UF 输入列表、放入目标、回读恰好一个且等于所选 tag 后，才进入 `UF_MODL_delete_feature`。保存进入与返回状态、删除码、列表各步返回码和更新码。释放的是列表，不是另一次对象删除；不做删除或保存重试。正常失败返回后仍尽可能保留真实删除后观察，随后拒绝保存。若调用后无法取得完整观察，同样停在保存之前并保留已取得的事件。保存失败保持失败及输出隔离，不把部分保存包装为成功。

检查原型和装配时，仅允许目标孔删除对应的体积变化；其余原型几何、当前 part/body 身份、原实例、父子关系、位姿及两种参考集必须保留。该动作对原型的修改会影响所有引用它的 occurrence；不会暗中复制原型或改变实例绑定。源文件持续校验，加载范围与保存仅限任务拥有的完整来源副本。此前组件复制的失败及原复制严格判据保持。

固定事件先后如下（原生诊断 `native_load_status` 可在其间出现）。有绑定事件可从 readback 的相应引用重核；两个 pending 事件只在原事件流中：

1. `feature_delete_before`、`feature_delete_before_reference_sets`、`feature_delete_before_detail`。
2. `feature_delete_feature_inventory`、`feature_delete_selection`、`feature_delete_pending`。
3. `feature_delete_returned`、`feature_delete_after_detail`、`feature_delete_after`、`feature_delete_after_reference_sets`。
4. `feature_delete_save_pending`、`feature_delete_save_returned`、`feature_delete_saved_detail`、`feature_delete_saved_inventory`、`feature_delete_saved_reference_sets`。

`selection` 绑定当前 part/body/feature、稳定 `target_feature_id` 及原 inventory/detail 文件引用。readback 绑定 `object`、`intent`、`target_part` 与完整观察引用；明确 `independent_reopen_checked=false` 和 `engineering_adoption=not_assessed`。独立新进程的详细重开和装配重开继续使用原执行入口与源闭包，不能用本次保存后的同进程读取替代。

接收端使用单独版本的 `CadFeatureDeleteRequest1`，复用原 `CadCorrespondenceRequest1` 的 Project、旧新对象观察、bindings 与 changes，另外核真实删除任务和旧新完整装配来源。原对应关系仍可报告几何缺失及 `declared_only`，新删除接收负责核原生谱系，不能因几何缺失就假设真实删除。原 Project 的变化传播、旧依赖决定重审、制造检验及物理采用继续分别完成。

SDK 来源是 NX 2412 官方 `uf_modl.h` 中 `UF_MODL_delete_feature`、`UF_MODL_ask_body_feats`、`UF_MODL_ask_feat_body/type/location/faces`，以及官方 `uf_modl_utilities.h` 中输入列表创建、插入和释放声明。当前私有来源登记保留实际文件 SHA、版本和原始字节；分发不携带 SDK 或设备资料。Rust 中只绑定这些经核声明，不提供任意符号执行入口。

软件测试冻结了独立数值与负例：选择零/多/未知/错误 owner/旧身份、另一孔或外形变化、完整域遗漏、原型与装配改变、调用及保存失败，以及成功事件顺序。合成测试属于 `development_replay`，不能证明真实 NX 类型、布尔历史删除结果、独立重开或 Project 工程结论已通过。平台编译、真实执行、独立接收、正式部署和工程采用须各自登记原件，不能从代码存在推定完成。
