# 第 2 章来源回读记录

状态：`source reread complete / problem contract candidate frozen / figure pending / full-chapter human review pending`

回读日期：2026-08-15

本记录只约束当前新架构稿
`functional-safety-book/ch02-concepts-terminology/chapter.md`。旧稿中的完整 HARA 路线、
Safety Goal→FSR/TSR 展开、ASIL 教程、五幅旧图和门禁实现已经退出本章；它们不能作为本轮正文已完成的来源或验收证据。

## 1. 受控来源身份

### ISO 26262 Part 1

- 原始受控 PDF：`ISO 26262-2018/ISO 26262-1-2018.pdf`
- PDF SHA-256：`3dee084e104aebe3e503b4a15e951ffeb89bd1bb800e1ac43ae6b2a9021f1411`
- MinerU 输出目录：`structured/mineru/ISO-26262-2018/part-01-vocabulary/`
- 结构化回读入口：
  `structured/mineru/ISO-26262-2018/part-01-vocabulary/native-full/ISO 26262-1-2018/auto/ISO 26262-1-2018_content_list_v2.json`
- JSON SHA-256：`7978027faf7330e083d6e11a0bd5e854ab78fbbc1ab08cc08e673140db29ca1c`
- MinerU manifest SHA-256：`fc1a6f36af509ac47b1fb05c4c524bf4d80fd0fd0289168958e87b400147e7fe`
- 来源角色：Part 1 的术语定义是本章 ISO 专用概念边界的规范来源；MinerU 产物只负责定位与回读，
  不替代受控 PDF，也不作为公开再分发文本。

### ISO 26262 Part 10

- 原始受控 PDF：`ISO 26262-2018/ISO 26262-10-2018.pdf`
- PDF SHA-256：`60a013c32bc22154af46deb033d870155862dbbec298ada91a862ad479603055`
- MinerU 输出目录：`structured/mineru/ISO-26262-2018/part-10-guidelines/`
- 结构化回读入口：
  `structured/mineru/ISO-26262-2018/part-10-guidelines/native-full/ISO 26262-10-2018/auto/ISO 26262-10-2018_content_list_v2.json`
- JSON SHA-256：`326152272cc212467b620d97aa33af99f0c85720f2daf746dff3a476302fae4e`
- MinerU manifest SHA-256：`ffbee8cca7bfd319c8751e4bcdde2008218311d436c8b0ea3b56e88674011d42`
- 来源角色：Part 10 是资料性指南，只用于理解 Part 1 概念关系与示例；本章不把它升级为新的普遍规范义务。

### 页码与 OCR 规则

- 下表 `pdf_page` 从 PDF 封面起按物理页计数；`block` 是 `content_list_v2.json` 页内零基块号。
- 本轮最小回读范围为 Part 1 物理页 16、18—20、24、29、33，以及 Part 10 物理页 14—16。
- 关键术语标题、编号、定义块和相邻 Note 已与 MinerU JSON、Markdown及原始 PDF 的文本层交叉核对。
- Part 1 第 20 页少量 MinerU 标点含噪，正文没有复制该噪声；相关结论只使用可明确辨认的术语、起终点和主体关系。

## 2. ISO 主张—来源账

| 主张 ID | 正文位置与允许的转述 | 精确物理定位 | 来源身份/力度 | 禁止扩大 |
|---|---|---|---|---|
| `C02-ISO-01` | §2.2：`element` 是 system、component、hardware part 或 software unit 的统称，不是强制中间层 | Part 1 3.41：标题 `pdf_page=16, block=23-24, bbox=[57,687,134,715]`；定义 `block=25, bbox=[57,714,882,732]`；Note `block=26-27` | 规范术语定义 | 不画成 `item→system→element→component` 的普遍固定树；不把 element 当对象永久本质身份 |
| `C02-ISO-02` | §2.2：`item` 是 ISO 26262 应用的 system 或 system 组合，在车辆层实现功能或部分功能 | Part 1 3.84：标题 `pdf_page=24, block=4-5, bbox=[58,178,102,206]`；定义 `block=6, bbox=[57,205,884,237]` | 规范术语定义 | 不把采购框、ECU 外壳、文件夹或物料顶层自动当 item |
| `C02-ISO-03` | §2.2：`system` 至少关联 sensor、controller、actuator；相关 sensor/actuator 可在系统内或外 | Part 1 3.163：标题 `pdf_page=33, block=21-22, bbox=[114,565,181,595]`；定义 `block=23, bbox=[112,594,941,625]`；Note `block=24` | 规范术语定义 | 不从物理共箱或名称相同推出 system 与 item 恒等 |
| `C02-ISO-04` | §2.2：Part 10 说明 item/system/element/component/part/unit 关系，并强调 element 的语境性 | Part 10 §4.2：`pdf_page=14, block=1, bbox=[55,203,739,221]`；说明 `block=2, bbox=[55,230,885,362]`；Note `block=4-5` | 资料性解释 | 不把 Figure 3/4 或说明提升为跨行业固定产品分类法 |
| `C02-ISO-05` | §2.4：error 是值或条件相对正确参照的差异 | Part 1 3.46：标题 `pdf_page=18, block=0-1, bbox=[57,98,110,128]`；定义 `block=2, bbox=[55,127,884,158]`；Note `block=3` | 规范术语定义 | 不把 error 限定为软件或“内部不可见”；没有参照时不冒充精确 error 判断 |
| `C02-ISO-06` | §2.4：failure 是 fault 显现后 element/item 预期行为终止 | Part 1 3.50：标题 `pdf_page=18, block=19-20, bbox=[58,587,121,615]`；定义 `block=21, bbox=[57,615,882,645]`；Note `block=22` | 规范术语定义 | 不以人是否观察到划线；不在主语和 intended behaviour 未声明时下 failure 结论 |
| `C02-ISO-07` | §2.4：fault 是能够导致 element/item failure 的异常条件；跨层关系是条件化的 | Part 1 3.54：标题 `pdf_page=19, block=0-1, bbox=[114,98,161,128]`；定义 `block=2, bbox=[114,127,722,143]`；Notes `block=3-5` | 规范术语定义 | `can cause` 不改写为必然传播；fault/error/failure 不建成普遍子类链 |
| `C02-ISO-08` | §2.4：Part 10 的 fault→error→failure 与 component failure→item fault 是资料性示例 | Part 10 §4.3.1：标题 `pdf_page=15, block=2, bbox=[114,469,531,485]`；说明 `block=3, bbox=[112,495,942,627]`；Figure 5 Notes `pdf_page=16, block=1-2` | 资料性示例 | 不推导“每个下层 failure 恒等于上层 fault”；环境因子与观察边界不得省略 |
| `C02-ISO-09` | §2.5：FDTI、FHTI、FRTI、FTTI 分别有不同起终点与主体 | FDTI 3.55：`page=19, block=6-14`；FHTI 3.56：`page=19, block=15-20`；FRTI 3.59：`page=20, block=4-8`；FTTI 3.61：`page=20, block=13-24` | 规范术语定义及 Notes | 单位或数值相同不产生 measure identity；诊断周期不自动等于 FDTI；element 参数不冒充 item FTTI |
| `C02-ISO-10` | §2.5：Part 10 明确给定 mechanism 的 FHTI 与 item 的 FTTI 主体不同 | Part 10 §4.4：`pdf_page=16, block=3-8`，其中主体 Note `block=6, bbox=[55,702,884,732]` | 资料性解释 | 不在本章据此完成项目时间预算、hazard 判断或 mechanism 充分性证明 |
| `C02-ISO-11` | §2.5：safe state 是 failure 语境中 item 的 operating mode，不由 `OFF` 自动决定 | Part 1 3.131：标题 `pdf_page=29, block=23-24, bbox=[114,690,203,720]`；定义 `block=25, bbox=[114,719,941,750]`；Note/例 `block=26-28` | 规范术语定义 | 不把 state 建成 item 的永久类型；不从关断或“正常”字样推出任一场景下安全 |

回读裁决：当前正文使用中文概括，没有复制长段标准原文；Part 10 的资料性示例没有升级为普遍公理，
Part 1 的术语也没有被剥离命名空间后冒充跨行业通用类。

## 3. 工程本体论方法来源

| 文件 | SHA-256 | 本章使用角色 |
|---|---|---|
| `/Users/jqwang/.codex/skills/ontology-engineering/references/ontology-engineering-book/ch02-ontology-foundations/README.md` | `865482aecbb7c7f634aaeba210fece3c3f51c33dfc5905f1584b379834d735f3` | 类、个体、对象属性/数据属性与类层次的基础边界 |
| `/Users/jqwang/.codex/skills/ontology-engineering/references/ontology-engineering-book/ch02-ontology-foundations/examples/core-concepts.txt` | `b4758062d5c21d76234a3b29b4d5533b386ea9ccfa068e5d24d217db06f22703` | “对象—关系—属性”不能由同名自动合并；映射必须显式 |
| `/Users/jqwang/.codex/skills/ontology-engineering/references/ontology-engineering-book/ch03-ontology-methodology/examples/ontoclean-evaluation.txt` | `5084fd309be51c79543765dc9fcbfdd6dfca7dfccba29ecd842decb90ce553ed` | 同一性判据兼容性；刚性类型与反刚性 state/role 分离；关系不冒充继承 |
| `/Users/jqwang/.codex/skills/ontology-engineering/references/ontology-engineering-book/ch07-knowledge-graph/README.md` | `6a184352a8a12f5027ec0b4f1db9ef9b55dda5e019657984ef9d0e326277fbc1` | TBox/ABox 与实体消歧在知识融合中的位置 |
| `/Users/jqwang/.codex/skills/ontology-engineering/references/ontology-engineering-book/ch07-knowledge-graph/examples/entity-resolution.txt` | `547a45e489e33274be1c32f581dde1541a41a26eed2076ad23113d531f1dc522` | 同物异名、异物同名、候选匹配、冲突裁决与强同一传播风险 |

### 方法材料的使用边界

- 本章采用“类型相容的身份判据”“state/role 不冒充本质类型”和“强同一关系需谨慎”的方法判断，
  但没有把方法书中的设备示例复制成 EPS 或 ENV-01 的事实。
- 方法材料中的相似度阈值是教学示例，本章没有将任何固定阈值写成身份决策规则。
- 本章只冻结 Same / Different / Unknown 的问题边界，没有交付 `owl:sameAs`、`skos:closeMatch`、
  SHACL、SPARQL 或实体解析算法；这些属于 ch12 的独立实现责任。
- 工程本体论仍是语义与共识根；受控记录/原生证据是事实来源，有权人和组织是身份裁决及工程决定的权限来源。

## 4. 非 ISO concern 工作区分

正文 §2.6 对准确度、漂移稳定性、可靠性、可用性、可维护性和数据完整性的说明，
是为拆开 `ENV-01 已验证可靠` 这一含混句建立的章内工作问法，不冒充 ISO/IEC/行业标准的完整定义。

允许的教学作用：

- 证明不同 concern 需要不同对象、条件、时间、measure、模型和证据；
- 证明校准系数变化与网络重启对各 concern 的影响不能使用全有或全无默认值；
- 证明准确、稳定、在线、易修复和数据绑定完整不能互相推出。

禁止扩大：

- 不用该表计算跨 concern “可信总分”；
- 不据合成数值作真实产品可靠性或可用性结论；
- 不把数据完整性缩成单一 checksum，也不把网络在线率冒充有效服务；
- 不把 EPS 的 ASIL、SafetyGoal、HazardousEvent、个体或身份链迁移给 ENV-01。

## 5. 合成案例与外部事实边界

- `EPS-RC17`、H3.2、`SW1.8.3.bin`、`DUT-P07`、`SN-EPS-000417`、`EPS_ASSY_043` 与 RR-17 均为合成标识。
- R17 裂纹、开路、ADC 差异、错误转矩请求与非预期助力是条件化教学路径，不是事故事实或真实因果证明。
- `ENV-01-A17`、`SN-0038`、`EQ-1172`、`dev-8af2`、固定偏置和每日两分钟网络重启均为合成教学事实。
- 任何 Same/Different/Unknown 结论只属于问题合同中的候选场景；真实身份必须回到原始标识、履历、冲突处置和有权裁决。

## 6. 当前开放项

- [x] Part 1 的 item/system/element、fault/error/failure、FDTI/FHTI/FRTI/FTTI 和 safe state 已逐项回读。
- [x] Part 10 §4.2、§4.3.1、§4.4 的资料性力度和禁止外推边界已记录。
- [x] 类/个体、同一性判据、role/state 与实体解析方法已从 ontology-engineering 随附资料回读。
- [x] `PTW-PC-02` 已冻结同名异物、异名同物、Unknown、三条 CQ 草案和镜像验收边界。
- [ ] 图 2-1 尚待 ImageGen 生成、语义/视觉/成书/权利复核和正文消费回写。
- [ ] 同树 handbook PDF 尚待在图文落位后重建并做第 2 章逐页视觉检查。
- [ ] 第 2 章全文的独立人工冷读、用户章级接受和出版放行尚未授权。

因此，当前来源证据足以支撑正文与问题合同进入图像设计阶段；它不表示图文闭环、章级人工接受或全书发布已经完成。
