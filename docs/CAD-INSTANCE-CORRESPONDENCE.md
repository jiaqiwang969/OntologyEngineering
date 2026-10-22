# 显式跨版本实例对应 /2

`cad evidence correspond-instances --input <源请求> --output <新私有目录>`接显式`oev2.cad-correspondence/2`。用户说工程目的与取舍，agent维护原件；不要求用户填写合同。

同一旧/新工程状态分别使用原CadOccurrenceBindingRequest1。既有correspondence/1仍检查完整变化来源、绑定覆盖、基数和复制新身份，新profile再组合同一个当前实例owner的不可反序列化资格。两端均有有效几何并不足以交换两个工程特征：retained要求定义及根对象工程ID、特征ID和明确完整路径保持；路径变化无谱系时保留未解析。空选定实例端仅为本显式跨版本检查内部使用，原bind-occurrence仍不接受空绑定。

分别输出设计声明变化、观察到的根位姿/几何变化、原生/模型/意图证据是否需要重绑定。不能比较的观察为null，不能冒充未变。两端观察即使各自有效，仍须先支持retained的同一实例身份，才输出同实例位姿/几何变化的布尔值；路径交换、根对象身份不明、复制或拓扑声明未成立时保留null，完整原观察继续保留。整个装配文件变更会使未改几何的其它实例也需要新证据，但不把它们报告为设计移动。

新建、复制、删除保留声明及完整旧义务，不继承工程合格。split/merge与原生创建谱系仍需其真实证据。根圆继续来自原型加原生装配变换，native_occurrence_edge_observed=false；不得将原型edge tag用于根装配命令。原correspondence/1和图纸入口保持原合同，不隐式读取新版。

生产入口重用两个端点的同一Project来源/条件检查、Project::reconcile和旧新依赖影响。当前Project须登记新变化来源；旧Project保持原件，不要求倒填后来声明。删依赖边不能消掉旧义务。完成前重读来源、Project/请求和编译执行身份，报告不写Project、不自动采用。

先按冻结预期核软件及生产CLI，再取当前Windows/SDK/原生移动和独立重开，以及同一原Project/work/Harness和施工图证据。已曝光历史native数据仍仅development_replay，完整16/63、19模块/19协议/22owner/13494范围、制造检验及最终项目专属精度实测不关闭。当前实现、实际验证、安装选用和方法收益分别看本轮维护回执。
