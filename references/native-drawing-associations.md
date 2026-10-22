# 尺寸关联的原生观测与重开检查

`cad nx` 的 inspect_detailed 与 maintain_drawings 继续通过当前 Rust worker 和锁定 NX SDK 边界执行。后者只维护任务自有副本：原生回读、视图刷新、保存，再由另一个进程 inspect_detailed 重开。原项目交付与旧通过保持历史身份。

当前观测增加 UF_DRF_ask_associativity_data 的关联对象、对象视图、点/线方式原码和绝对关联位置。每个父对象保留原生类型、所属零件与查询返回码。只有 SDK 明确支持的实体、面、边执行近似包围盒查询；其他类型保持未查询。单位从实际零件回读，异常、无关联、未支持信息不推成通过。新增独立 association ABI 检查大小和每个字段偏移，保留已有 ABI2 与 extension1 的布局。

`cad drawing compare-associations --saved-job <v2任务目录> --reopened-job <另一任务目录> --output <var内新回执>` 核对任务、源码/构建文件、工具、记录的完成状态、保存文件与重开输入字节。重开必须使用同一 worker/runtime 和独立进程。比较保留全部维度与关联的数量，排除进程临时 tag，分别报告 matches、changed 或 unknown。此入口核验记录及字节，不重新观察 OS；原生执行证据来自此前真实 NX 任务及控制器回执。

相同位置、类型及近似包围盒不能确立唯一对象身份，也不能证明标注指向设计意图要求的边或孔。retained=false 同样不足。需要将当前 Project 的对象版本、孔列/配合意图与原生对象独立对应，才可进一步判断正确性。自动尺寸重关联、布局修改、新设计出图、图面可读性和制造验收仍各需专项依据。本入口不自动执行这些修改。

NX SDK 明确说明 UF_MODL_ask_bounding_box 偏重速度，裁剪/布尔几何可能不准确；因此本观测只用于诊断，不能把盒尺寸当作公差或计量证据。选取 SDK 头文件/库摘要与真实编译记录随私有 worker 归档，全 SDK 传递依赖身份范围另行说明。

## 将独立的尺寸要求接入当前项目

`cad drawing check-references --input <内部请求> --project <同一Project> --project-context <当前内容快照> --output <var内新回执>` 使用现有NX任务回执检查和Rust内核，不另建项目或语义状态。内部请求包含 `intent`、`job_directory`、`part_input`（例如 `inputs/part.prt`）和 `native_result_ref`。输入需来自已完成的 `inspect_detailed` 任务；原CAD输入字节必须和意图的 object.source_sha256 一致。保存结果应先由独立进程重开再检查。

agent 从原需求、孔列/配合设计和受控坐标推导 `DrawingReferenceIntent`，不要从待评回读复制预期。当前仅支持该CAD文件的 `part_absolute` 绝对坐标。每个 `DrawingReferencePoint` 给出独立身份、三个有限坐标、点/线方式及第一父对象类型原码；明确 mm/inch 和坐标匹配容差。容差用于比较原生浮点观测，不能代替制造允差、计量不确定度或工程师确认。进程tag、注释名称、近似包围盒及显示文字均不能充当稳定设计身份。

意图和观测存进同一 `EngineeringBasis.facts`：`drawing_reference:{intent_id}` 的 value 为完整意图，`native_drawing:{intent_id}` 的 value 为 `NativeDrawingObservation`；各自保留 status/source_refs。`EngineeringAssertion.kind="cad_drawing_reference"`，subject=intent_id，requirements=完整意图，affected_decisions 沿既有 Project 依赖重审。Project.evidence 登记CAD、独立设计依据和原始native_result_ref的摘要。内核重新计算观测匹配，不接受自报pass；原始NX回执仍保留其实际worker版本。正式入口还核对 Project 中的观测与已核验任务原件一致。

`proof_scope="reference_points"` 只判断一个完整尺寸的关联点/原码/单位/第一父对象所属零件是否满足要求；重复候选、重叠匹配、失败查询和未支持坐标保留 unknown，完整观测缺少必要尺寸或retained=true给出具体诊断。意图 `/1` 的 `unique_model_parent` 保留 unknown；`/2` 按下述独立完整圆边要求进一步检查。两种版本均不授予工程采用。制造公差、计量、图面可读性和自动重关联各另验；不得把正式要求改成较小证明范围消除未知。

公开观测外壳为 `part.annotations`：schema=`oev2.nx-annotation-observation/2`，units_query 含 return_code/unit，dimensions 含 retained、retained_query_code 和 associativity_parents。后者含 wrapper_code、uf_return_code、capture_complete、entries；entry 保存 association_position（三分量）、association_position_valid、unit、coordinate_basis、point_type_raw、line_type_raw 和 object_1/object_2/view。父对象 metadata 的 type_query 与 owning_part_query 记录查询结果及是否同零件。具体点的含义须核对适用NX SDK，不按原码猜物理作用。

尚无独立设计意图时，保留同一 assertion，requirements=null；说明缺项的来源仍须在 Project.evidence 登记。Rust 返回 `drawing_reference_intent_missing` 与所需输入，并传播 affected_decisions；不要构造零坐标或降低正式证明范围来取得通过。

本轮受控实例核对安装的 NX 2412 `uf_drf_types.h`：association 的 ARC_CENTER_POINT 原码为2；另一个用于圆心线方法的 `UF_DRF_center_point=1` 不是同一枚举。只有明确核对过的 SDK 原码才能用于该版本；公共类型不设置案例默认值。新原生执行可复用较早验证的 v2 reader，但必须保留其编译源码、SDK、worker 与 controller 身份，不改签成当前整套源码。

## `/2` 完整圆边与原生标注值

每个点的 `parent_edge: CircularEdgeIntent` 由独立设计依据给出 feature_id、center、normal、radius、linear_tolerance、angular_tolerance_radians、unit、local_frame 和 full_circle。当前支持单零件 ABS 坐标中的完整圆形 solid edge；±法向描述同一无向平面。旧 `/2` 的中心和半径共用 linear_tolerance；需要独立容差时使用下述 `/3`。进程 tag 只用于这份精确观测里的关联，跨版本稳定特征身份仍需另证。

原生 `part.model_edges` 包含全部 solid body 的 edge 清单、原始类型/所属零件、UF_EVAL 查询返回码及有效圆弧参数。清单与每条查询须完整；缺查询保持 unknown，独立要求没有匹配边给出 violated，重复几何保持歧义 unknown。选到唯一边后，再核对 annotation 关联父 tag、点/线模式、位置和单位。一个看似正确的尺寸不能掩盖未完整查询的其他范围。

可选 `dimension_value: DrawingDimensionValue` 明确 computed_linear、value、numeric_tolerance、unit 和 dimension_subtype_raw。NX 回读位于 `part.annotations.dimensions[].computed_size`，含 wrapper_code、value_valid、value、unit、basis 和完整 metadata；当前 basis 是 `NXOpen::Annotations::Dimension::ComputedSize`。缺值、单位/类型不符和超出诊断容差分别处理，显示文字不能代替原生计算值。此容差不是制造批准或测量不确定度。

`cad nx` 的 `create_reference_dimension` 接收已冻结 `/2` 或 `/3` intent、CAD输入、正交 plane_origin/x/y_axis 和 text_origin_wcs。Rust先验输入、共面性、数值及CAD摘要，复制到任务自有 out，再读完整几何、选择父边、通过薄SDK ABI创建 parallel dimension、回读和保存。保存文件须另派一个 inspect_detailed 任务独立重开。该 action 创建模型空间关联标注；几何参考页走下述独立 action。制造公差、排版和可加工图仍各有验收义务。

## 当前审查重新核验原任务

`NativeDrawingObservation /2` 在完整观测之外登记 job_directory 与 part_input。目录限定当前根 `var/`，成员须精确属于任务输入。当前 `engineering::checks_current` 由 Project work、Harness、kernel review 和 experience 共用：保留原声明/几何检查，再追加独立 `engineering_native_source:*` 检查。它重新读取任务manifest、参数、完成状态、原worker/process/runtime、输入/产物与回执原字节，再核对声明观测。合法相对路径便于显式接收重定位，接收端仍须取得并核验对应私有任务。

`/1` 缺定位时保留声明诊断的历史范围，当前原生来源结论为 unknown。声明中加入原回执不存在的标注，即使摘要未变，也会给出 `drawing_native_declared_observation_differs_from_job` 并沿Project重开决定。声明几何的旧诊断仍执行，不被来源缺项覆盖。该检查证明记录及字节相符，不重新观察OS，不把旧worker回执改签成当前CLI，也不授予工程采用。

## `/3` 独立圆心与半径诊断容差

`DrawingReferenceIntent /3` 的点使用 `parent_edge_v2: CircularEdgeIntentV2`；不混入旧 `parent_edge`。该类型保留独立 feature_id、center、normal、radius、angular_tolerance_radians、unit、local_frame、full_circle，分别明确 `center_tolerance`（三轴圆心欧氏距离）与 `radius_tolerance`（半径绝对差）。两者不会相互补偿。旧 `/1`、`/2` 的规范化 wire 和共用 linear_tolerance 含义继续保留；新字段进入旧协议会被拒绝。新协议仍只做指定文件内的几何诊断，制造允差、计量不确定度与工程采用另取证。

供明确接口的调用方使用：`oe_nx_worker::geometry::model_edges::select_v2(&CircularEdgeIntentV2, &serde_json::Value) -> anyhow::Result<Value>`。结果 schema 为 `oev2.model-circle-edge-check/2`，保留 conclusion、reason、candidate_edge_tags、details、identity_scope、engineering_adoption，并显式记录 tolerance_policy。合同解析错误和未知观测分开；没有模型或当前项目输入时不能自报通过。

完整原生边 JSON 的公开传输外壳：part 有 units、solid_count 和 solid_tags；part.model_edges 有 schema=`oev2.nx-model-edge-observation/1`、complete、unit、coordinate_basis=`UF_EVAL ABS coordinates; single-part solid edges`、solid_type_raw=70、edge_subtype_raw=3、bodies、entries。每个 body 有 body_tag、metadata、edge_list_return_code、capture_complete、edge_tags；每个 entry 有 body_tag、edge_tag、metadata、query、arc。metadata 有 type_query（return_code、type_raw、subtype_raw）及 owning_part_query（value_valid、matches_expected_owner）。query 有 capture_complete、wrapper_code、initialize_code、is_arc_code、arc_code、free_code、is_arc。arc 有 value_valid、is_periodic、limits（两个弧度值）、radius、center（三分量）、x_axis、y_axis（各三分量；叉乘给出圆面法向）。此 DTO 不另声明 reported_edge_count；边清单由 bodies.edge_tags 与 entries 对应，不能在调用方另填计数后当作已执行检查。具体原码仍须对应已核对SDK。

## 局部几何参考页与当前审查

`cad nx` 新 action=`create_drafting_sheet` 的参数含 part 与生成的 `NativeDraftingPlan`。agent 明确当前 object/CAD、sheet_name、unit、width/height、scale_numerator/denominator、projection_angle、model_view_name、view_center、source_refs、purpose=`prototype_geometry_only`；不设案例默认值。新增页只写任务副本；已有同名页拒绝。模型视图来自当前同一零件的实际命名视图，而不是任意相机或动画帧。SDK创建后读实际页信息、视图比例/位置/边界/方向/所属模型/可见对象及原码，再保存；最后另进程 inspect_detailed 重开完全相同的文件字节。

内部请求 `{plan,job_directory,part_input,native_result_ref}` 进入 `cad drawing check-drafting --input <request> --project <Project> --project-context <current context> --output <new receipt>`。当前 EngineeringBasis 中登记 `drafting_plan:<sheet_name>` 与 `native_drafting:<sheet_name>` 两个事实，后者继续用 NativeDrawingObservation /2。声明 kind=`cad_drafting_sheet`、subject=sheet_name，沿同一Project依赖关联图纸、检验、工艺和成本。kernel/work/Harness/experience 使用同一个 checks_current；声明计划与完整原任务核验分别报告。来源读取失败仍保留计划不符、旧条件或数值等具体诊断，不改为统一提前退出。

页面/视图诊断浮点精度固定为检查协议/1的1e-7×max(1,预期量值)，它不代表加工公差或计量不确定度。原始必需query return_code须为0；true有效标志不能覆盖实际错误码。非空可见对象、未越页边界和命名视图一致只证明几何参考范围，独立视觉可读性仍未审。模型空间标注不会自动转成图纸尺寸；尺寸关联与公差、基准链、材料/工艺、同版BOM、交换/PDF输出与审图义务继续单验。

安装SDK的标准纸型在 sheet-info `/2` 中保留原始枚举、单位族和版本化尺寸解释；旧 `/1` 未支持标准纸型的 unknown 保留。A3不能满足A4，未知码或单位族不符不通过。几何参考观测 `/2` 的视图边界必须实际绑定当前零件全部实体并回读对应对象集合，仍检查原页面范围和裁切。定位使用SDK专用 `UF_DRAW_move_view`，不以另一setter静默回退；创建不完整时读取部分状态用于诊断，保持失败且不保存。

创建观测 `/4` 新增明确smart model anchor：Rust计算当前全部实体盒的联合中点，仅作布局提示，SDK创建有长度量纲的辅助smart point并使其对应独立图页位置。只隐藏新增辅助点，不隐藏原实体或标注。part观测 `/3` 在保存和独立重开后还核对实际锚点的原码、smart身份、所属part及有限ABS坐标；旧part `/1`、`/2`继续原范围。近似盒不替代精确几何、公差、质心或制造基准。模型原点、模型锚点、图页参考点与实体中心分别记录，不能因输入合法或摘要相同便认为位置正确。SDK签名、编译及受控检查不代替这一版本的原生保存重开证据；原生执行结果以维护记录为准。

创建观测 `/5` 分开保存导入位置与原计划的最终图页参考点。Rust依据实际命名视图的正交旋转、零中心和比例，计算有界导入坐标；非零中心、非正交或反射矩阵保持未支持，不猜平移含义。实际 `UF_VIEW_map_model_to_drawing` 必须将smart锚点映射到原计划位置，才能继续完整实体边界、移动、更新及保存。part观测 `/4` 在原锚点检查之外重新核对映射原码/有限坐标、实际display part和打开图页的返回码。自有headless worker设定显示前置并记录实际结果；不代表已通过人工视觉检查。原创建 `/3`、`/4` 及part `/1`–`/3` 的范围与失败身份保持原样，新协议不能降低原位置、页界、可见对象和独立重开要求。

上述 `/5` 导入点补偿候选已在页外输入时被NX原码 `UF_DRAW_point_not_on_drawing` 拒绝；算术正确不证明满足接口输入域。创建 `/6` 改为只在输出副本设置实际命名模型视图的中心，再以原计划内位置导入。Rust仍根据完整当前实体盒决定布局中点，SDK `UF_VIEW_set_center` 和实际 `ask_center` 分别记录原码及前后值，不平移实体绝对坐标。part观测 `/5` 另核命名视图中心与该中点一致；这是此版本的新页构建试验条件，不是任意现有图纸的通用有效条件。推广至其他布局需明确适用机制并冻结新基线，不能删掉原约束后宣称原试验通过。显示flag、当前drawing、实际workview/type各自回读；flag不替代真正显示的证据，原可见性query仍必须成功。每一步失败保持未保存并保留部分诊断。原页面、比例、映射、实体集合、裁切、关联尺寸及独立重开要求继续执行；原生结果以该版本实际维护回执为准。

`/6` 原生反例确认显示前置生效，视图中心虽已改变，导入映射仍偏离最终要求。创建 `/7` 把初始映射保留为未完成定位的诊断，先建立完整实体边界，再调用专用移动接口，最后严格核最终锚点映射才更新。初始查询失败仍拒绝；任何原生失败的副作用不能当作成功继续。最终part `/5` 检查保持原要求，不能将中间诊断冒充保存或重开结果。未执行的另一顺序方案与冻结候选也保留为未执行身份，不改签成实际失败或通过。


创建 `/8` 单独版本化保存参考点导入桥，显式传入 `use_ref_pt=true`；旧v1导入ABI不变。实际第八候选import=0，但锚点仍偏离冻结图页目标，完整实体边界返回731009；后续move/update/save未执行，输出与输入同字节。八次试验各守原source/worker/process/SDK/input身份，原visible查询1170056不能解释为成功零对象，未执行7A方案不算实际试验。这只否定本例“启用该模式足以正确定位”的假设，SDK保存参考点的具体含义和唯一根因仍未知。

当前模型空间32mm关联参考CAD已有受控创建/保存记录，独立新进程重开及当前Project检查按各自原任务核验；它不能证明新图页已保存、尺寸已上页或制造合格。最新实际结果与各层范围记录在私有维护报告中。下一候选应先取得可核查SDK例子或已有工作布局的真实机制依据，再保持原页、中心、比例、全实体、可见性、保存与独立重开要求验证；不得扩大页面、隐藏几何或用失败API副作用作fallback。

## 旋转与可见库存的回读一致性

命名视图与导入视图的矩阵都须为有限、正交、右手旋转；两份相等不能使非法矩阵通过。数值诊断使用既有导入坐标算法的1e-8基准，不是制造公差或测量不确定度。合法矩阵但方向不相符继续给原方向违反；非法原生表示保持unknown。

原生报告提供visible_tags或clipped_tags时，两清单都须有完整数组，数量与原query计数相符，tag为非零u32。矛盾或不完整保持unknown；合法非空裁切仍violated。历史count-only观测保留原限定意义，并显式报告未核个别tag；清单一致不能证明实体无遮挡、图面可读或制造图完整。

安装SDK允许导入视图不设置smart anchor；这不修改旧观测/3–5的构造基线。若采用不依赖关联tag的直接绝对点映射，先取得真实工作对照并新建明确观测合同，原页面、方向、比例、完整实体、位置与保存独立重开验收必须保持。


## 独立ABS布局定位的新观测 /6

安装SDK允许import_view的关联anchor为空，且一个已闭环项目的实际成功图纸只读对照中六个关联tag均显示CONDEMNED；这些不能当成NULL导入已完成终态的证明。当前真实项目的隔离诊断把关系变化收窄到member update窗口，并通过直接模型XYZ映射定位到独立计划中心。为此另建observation/6，旧/3–5及原反例不改签。

/6使用spec正本生成的NativeLayoutReference和NativeModelToDrawingMapping：同part完整实体bbox并集ABS中点、单位、当次part/view关系、实际map输入点、原码、完整标志和图页XY须一致。原display、完整boundary、有效旋转、显式可见/裁切库存、幅面、比例、投影、reference和未过期状态继续检查。布局点不是质心、制造基准或测量值；可选smart anchor只作诊断，只有ALIVE才允许深读。

几何终态仍按独立计划及32mm参考审查，需实际保存和独立进程重开；软件合成DTO通过不构成NX或制造验收。新机制的私有候选不自动切换工作实现；工程采用、标注、公差、图面视觉、同版BOM、制造检验及最终项目专属精度计量义务保持。


当前有界native实现已取得独立保存重开证据，详见canonical status和本轮REPORT。`NativeLayoutReference`由生成DTO直接序列化，禁止序列化后添加未声明字段；原生失败/未知保留具体deserialize或validate阶段。构造中间幅面不保存，最终原页检查通过才进入保存；重新开同一saved摘要时使用无refresh/update/save的独立进程，另核原完整圆边与ComputedSize意图。具体32mm及两圆几何是私有实例，不成为通用默认值或制造公差。


## 图页尺寸创建前检查与独立动作（候选）

`oe_nx_worker::geometry::sheet_dimensions::prepare` 复用生成的 `NativeDraftingPlan` 与 `DrawingReferenceIntent /2–3`，要求相同对象、版本、条件、CAD原字节和单位。调用方给原输入字节、同次详细回读及图页文本原点。先执行原 `/6` 图页检查，再由完整圆边 owner 选出两个唯一父边，并验证模型圆心距与视图投影距均符合独立长度。这样支持不同圆面之间的轴向长度；旧 `create_reference_dimension` 的模型空间含义和原约束保持。

返回中的 sheet/view/edge tag 只属于该次观测，不能跨进程当作稳定身份或直接重放。页内文本原点不证明文字范围、遮挡或可读性；投影长度检查不替代实际 SDK 映射和原生 ComputedSize。候选新增 `create_sheet_reference_dimension`，使用独立 `oev2.nx-sheet-dimension-params/1`、原生PRT成员名、sheet/reference两个生成合同及二维text_origin_drawing。Rust先核原页与父边，再打开指定图页并核成员视图；薄SDK接口显式填写 object_view_tag。创建后及保存后重新核原生尺寸值、非retained状态、父边和视图关联，原实体几何及原标注tag库存须保留。失败保持失败；保存后失败不能把已产生输出作为通过。该候选的Windows编译、原生派发、独立重开和正式安装按本轮实际证据登记，不能由源码存在推断已完成。


图页尺寸协议上报漏项的显式v6恢复使用同一writer审查owner，见[有限恢复](../docs/NX-SHEET-DIMENSION-RECOVERY.md)。旧任务状态保留；修复版必须重新取得自己的Windows源/程序身份及原生保存重开证据。


## 新建图页标注的保存重开比较

显式 `cad drawing compare-sheet-reference --saved-job ... --reopened-job ... --output ...` 使用 `oev2.sheet-reference-reopen/1`，接收 create_sheet_reference_dimension 与独立 inspect_detailed 的完整任务原件。它沿原回执 owner 核来源、程序、SDK声明、原输入及保存重开字节，再复用原生成的 NativeDraftingPlan/DrawingReferenceIntent 与纯几何检查；新进程重新选择圆边、视图和唯一合格标注，不复用创建tag。错误尺寸、retained、错误成员视图、重复候选、刷新或缺原件拒绝。全部已有标注的持久性仍由原关联投影报告，不用新标注通过掩盖其它unknown或变化。

原 `compare-associations` 的维护图纸范围和返回格式保持。新检查的原CAD→保存CAD摘要接续只用于核实际产物，不修改Project版本，不生成新的采用决定；当前对象、条件、原意图、原件及新观测仍须经原Project入口与内核绑定。图面可读性、公差/加工/检验及实物通过分别验收。实际原件测试默认忽略，只有显式提供原件并运行才计入验证。


同一Project的 `cad_sheet_reference_persistence` 义务使用生成的 `SheetReferencePersistenceIntent`：当前保存对象、原创建图页/尺寸意图、两个完整任务目录及Project来源身份。对应fact为 `sheet_reference_persistence:{intent_id}`，不得以缓存结论代替。声明检查只核绑定；正式 `checks_current` 再重读原结果、原参数、保存及独立重开字节，并调用原CAD持久性owner。它将原创建对象与当前保存对象分别保留，发生变化时沿现有affected闭包连接图纸、检验及制造决定。无原件保持unknown，意图或来源不符拒绝；图面、制造、实物和采用仍各自验收。

每个持久性义务来源还须在实际Project依赖图中触达其声明受影响决定；仅在检查报告列affected而没有实际依赖边不合格。该检查复用原project::impact，不另建依赖图。


普通工程对象编号与RDF节点不等同。完整 `kernel review` 的 Binding 可显式增加 `rdf_focus`，值为生成的 `RdfObjectFocus`（schema=`oev2.rdf-object-focus/1`、原object_id、绝对focus_iri）。RDF内该focus须恰有一个 `urn:oev2:project-context:objectId` 字符串声明，且原ID只能对应这一focus；版本、工况、来源、shape覆盖和条件摘要检查继续执行。映射进入Binding摘要，不修改Project/CAD原对象，不猜测前缀。未提供时原有效IRI路径及非IRI拒绝保持，历史None序列化不增加字段。此映射当前作用于显式Binding的审查路径，其他版本化typed入口保持原合同。
