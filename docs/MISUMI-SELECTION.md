# 米思米三层选型审查

米思米资料在工程中承担三种不同作用。`sources/misumi/incad/` 的 inCAD 案例用于理解机构、功能、运动和可能的结构组合；标准品目录与技术手册用于确认系列、尺寸、精度、安装方式、限制和页码依据；供应商官网当前页面用于在实际工况下观察具体型号、选项、配置和取得日期。来源管理保存它们的身份、版本、字节摘要和项目绑定，但来源管理本身不是第四种选型知识。

三层资料必须沿同一个 Project/work 和对象身份回到项目文件体系。每层都保存状态 `used`、`not_used`、`unavailable` 或 `stale`，以及可回读的来源路径、SHA256、角色和定位。官网观察还要保存观察时间、具体型号和完整配置。案例 BOM、案例 CAD、历史下载或目录中的相似型号都不能单独变成当前采购型号。

选型理由沿工程本体的链条保存：必要功能、物理机制、工况、接口/空间/精度约束、为何选择、替代候选和未知。`checks` 逐项记录依据，至少能区分当前官网配置、目录/手册依据、工况匹配和身份一致性。通过只表示这份回执内部完整；工程强度、寿命、制造和实物验收仍要由相应 owner 独立完成。

## 审查回执

当前 v2 提供离线的结构检查入口。它不打开浏览器、不替用户选型号、不下载、不下单、不运行 NX，也不把回执写成工程采用：

```sh
oev2 --root "$ONTOLOGY_ENGINEERING_ROOT" supplier selection-review \
  --input var/selection/misumi-review.json \
  --output var/selection/misumi-review-result
```

输入是 `oev2.misumi-selection-review/1` JSON，顶层字段为 `schema`、`project`、`subject`、`decision`、`layers`、`rationale` 和 `checks`。`project` 要显式含 `project_id`、`work_id`、`context_sha256`；`decision.status` 可为 `candidate`、`selected` 或 `blocked`。每个 `layers` 成员必须保留 `status` 与 `references`；`official_web` 的引用还需有 `observed_at`、`part_number` 和 `configuration`。具体项目的来源引用和条件由 Agent 根据实际原件填写，用户不填写内部合同。

当 `decision.status` 为 `selected` 时，审查只有在以下条件均满足时才产生 `selected_pending_engineering_verification`：官网层为 `used` 且存在同一具体型号和配置，目录/手册层为 `used` 且有定位，`official_configuration`、`catalog_basis`、`condition_match`、`identity` 四项检查均为 `pass`。否则产生 `selection_blocked`。`candidate` 只产生 `reviewable_candidate`，不会因为案例命中而自动升级。

输出目录中的 `selection-review.json` 原样保留项目、三层来源、理由和检查，并明确标出 `source_bytes: not_read`、`browser_execution: not_run`、`native_cad: not_run`、`purchasing: not_performed`、`engineering_adoption: not_established`。这些字段避免把结构回执误读为原件复核、网页选型或工程通过。将回执接入真实项目时，用同一项目的 workspace commit 保存它的 ArtifactReference、阶段和决定；不要在项目外另建一张选型总账。

## 使用顺序与变化处理

先从根对象的功能和条件出发，检索 inCAD 形成结构候选；再按候选的功能、接口、载荷、精度和空间限制查目录/手册；最后在官网当前页面重新配置并取得具体型号。网页当前状态、价格和供货时间带观察日期，不能由历史来源继承。原件、版本、条件或接口变化时，回到当前 Project 的 `reconcile`，把受影响的选型和下游 CAD/BOM 决定置为待复核；旧回执仍保留为历史证据。

官网当前配置仍不替代 NX 原生导入、保存重开、装配干涉、计算、制造和实物检验。NX 几何成功也不自动证明额定寿命、载荷或工程适用；这些结论要沿具体验收项分别取得依据。
