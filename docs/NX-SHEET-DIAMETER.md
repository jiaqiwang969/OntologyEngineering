# 有源单圆名义直径

`SheetDiameterIntent` 与 `create_sheet_diameter_dimension` 使用显式 `/1` 合同，连接当前对象、版本、条件、CAD 原字节、图页与一个完整圆边。独立名义直径同时核对两倍半径与原生 `ComputedSize`。数值检查容差不定义制造公差、配合或实物合格。

规范在 `specs/modules/nx-sheet-dimensions.toml`；共享类型由锁定真实 ggen 生成。手写 Rust owner 为 `oe-nx-worker::geometry::sheet_diameter`，复用已有圆边清单、图页和几何检查。原两圆心平行尺寸合同保持。单圆须在成员视图中无缩短投影，选取不明确、原件变动、缺单位或来源均拒绝。

Windows NX2412 的窄 C++ ABI 只桥接 `UF_DRF_create_diameter_dim`，用当前安装的 `uf_drf.h`、`uf_drf_types.h`、`uf_object_types.h` 与编译依赖清单核验。Rust 明确检查成员视图、单圆关联、类型26/10和实际尺寸；创建只写任务副本，保存前后均回读。原几何和原标注内容必须保持，失败产物不取得成功资格。

`oev2 cad drawing compare-diameter --saved-job … --reopened-job … --output …` 接收完整作业、工具、源/产物哈希和独立进程。重开不刷新视图，按保存字节重新选父圆和唯一尺寸，不沿用进程 tag。旧尺寸保持复用完整线性标注签名；已有其他标注类型需各自接收支持，不能默认为通过。该接收支持有限的名义尺寸和关联持久性，完整几何另由原 CAD 任务检查；图面可读性、制造检验及采用分别取证。

Mac 执行规范生成、编排与回执检查；Windows 执行同源 Rust worker、SDK 桥和 NX。软件反例、Windows 构建、真实创建、保存、独立重开、新根安装和正式选用分别登记在维护记录；代码存在不等于当前原生能力已经接收。六本来源、Project/work/Harness、完整 CAD16、Semantica 与63项目标继续保留。

单圆径向标注的实际关联语义增量见[整圆关联/2](NX-SHEET-DIAMETER-ASSOCIATION.md)：0表示整个圆对象，追加独立绝对圆心检查；原/1及失败保留，当前资格核新的保存与独立重开回执。
