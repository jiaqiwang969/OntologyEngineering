# 工程资料库：从原件到工程使用

本目录与 `modules/` 同级，统一保存合法取得的工程资料。书籍放入 `books/`，米思米目录、案例与模型放入 `misumi/`；论文、标准、器件手册、视频等按需要增加分类。然后告诉 Agent：

> 整理 sources 中新增的资料，接入现有读取与检索能力；告诉我哪些内容可供当前项目查证、哪些还没处理。

Agent 负责清点、登记、处理和使用记录，用户不必填写 JSON、TOML 或 RDF。投放不会自动触发后台处理；索引、本体化、适用性核验和工程采用分别记录。

资料路径沿用 v2 的 `sources/` 布局；已有登记支持先预览差异，再按稳定来源 ID 合并新增或修订资料。已登记的六本书保持来源身份；第七本液压书先保留原件，尚不登记。逐书按 v2 建立本体知识图谱是下一阶段待讨论的工作，本轮不做全书分解、OCR 或 Drive 内容更新。

## 放在哪里

按资料类型、书或资料集建子目录。不同书可各自保存 `original.pdf`，导入时按交付清单的完整相对路径辨认；扁平目录中的同名文件无法唯一匹配时拒绝导入，不猜所属资料。保留原件及旧版；处理成果另存，并保留回到原件的对应关系。资料目录不另建专业 Skill。

```text
ontology-engineering-unified/
├── SKILL.md
├── modules/
├── sources/
│   ├── README.md                 # 本目录唯一随软件公开的文件
│   ├── books/                    # 两卷方法书、领域教材等原件
│   ├── misumi/                   # 供应商目录、案例与模型
│   └── .indexes/local-pdf/       # PDF 登记及页级索引，保持既有位置
└── var/                           # 处理回执、Project 与实际使用反馈
```

`sources/` 中只有精确路径 `sources/README.md` 随 Git 和软件包公开。其他原件、提取内容、私人清单、导航笔记与索引留在本机或单独授权的资料交付包中。软件包的路径检查也拒绝这些私人内容；不要用强制添加绕过仓库排除规则。

现有 [Google Drive 资料包](https://drive.google.com/drive/folders/1QoNTu0gy9aPwQ-Sk6y5IWSBcVUCDVdXK)继续使用。交付清单中的 `books/...` 对应本安装根 `sources/books/...`，`misumi/...` 对应 `sources/misumi/...`，路径含义与现有 v2 保持一致。本地已有原件时核对摘要后直接迁入，不重复下载。安装根或原件路径改变后，显式重建完整登记与索引；历史回执保留原身份，新位置另记核验结果。

## 两卷方法书的身份

《工程本体论》（`vol1`）与《产品可信工程》（`vol2`）是本系统的方法来源。当前工程流程分别以 `ontology_method`、`trust_method` 绑定它们；机械设计等其他教材承担相应领域依据。方法来源支持如何组织对象、条件、主张与证据，其陈述仍须核对适用性，不能直接当作当前项目事实。

两卷的 PDF 阅读原件放在 `sources/books/`，ID、版次及摘要在迁移时保持。已有著作维护树中的 TeX、Markdown、素材与锁文件另有维护和校验身份，位于维护工作区的 `sources/book-authoring/`，不随普通 PDF 路径变更移动，也不自动接入新安装。部分方法已有[来源映射与有限检查](../references/judgment-pattern-source-map.json)，另有[章节包与执行接入](../docs/SEMANTIC-SERVICE-BOOKS.md)；这些不代表整书已经图谱化、所有章节已实现或工程已通过。后续逐书本体化在这些既有内容上核对缺口，并用独立案例、反例和实际证据检验，不以本系统遵循两卷方法书证明两卷本身正确。

## 资料如何贯穿四个环节

| 环节 | 资料的作用与边界 |
|---|---|
| 工程本体 | 组织原文中的对象、概念、关系、主张、条件和证据。两卷提供方法来源，领域书提供专业依据，米思米提供具体型号、规格和安装条件线索；来源陈述与当前项目事实分别保存。 |
| ggen | 将需要软件承接且已审查的含义写入 `specs/`，由锁定 ggen 生成合同与约束。原 PDF、目录或模型不会因放入资料库就自动成为规范或代码。 |
| Rust 工程模块 | 执行已经实现的登记、检索、原件回读、来源核验、规则与计算。记录实际覆盖及未知，程序成功不替代来源适用性或工程验证。 |
| RRSI | 从实际使用的失败、纠正、反例与反馈判断要改资料、解释、规范还是实现，在固定目标和判据下独立比较后决定采用。 |

同一份原件通过来源 ID、版次、摘要和原页位置贯穿这些环节，无须复制成四套。原件、解释、正式表达及实现验证各保留身份；知识图谱是后续有范围的表达与核验成果。

## Agent 如何整理新增资料

### 1. 清点原件和版本

比较本次文件与已有完整书目：记录稳定来源 ID、题名、版次、来源和取得方式、文件大小、SHA256、实际路径。相同摘要先标记重复，不擅自删除；同名不同摘要需辨认新版、修订、扫描差异或损坏。新版保存与旧版关系，不能沿用旧版的页码和采用结果。路径变化保留原来源身份及新回执，不只改显示名称。

登记清单是私有资料。当前 PDF 登记合同为 `ontology-engineering.local-pdf-manifest/v1`，由 Agent 按实际 `knowledge register` 合同生成，包含 `id`、`title`、`edition`、`path`、`sha256`、`chapters`；来源与许可等补充记录另存私有目录，不向严格合同随意加字段。相对原件路径相对于清单所在目录解析，登记前核对实际解析结果。

### 2. 按资料类型处理

PDF 文献先核可读性和总页数，再对照实际目录记录章节范围、标题及检索别名，区分 PDF 页序与书上印刷页码。当前原生登记使用 Poppler 提取有界文本；保留原件摘要、页级位置、提取工具和覆盖结果。非 PDF 资料先记录清点状态，按实际读取能力接入；转换时保留原件、格式与转换记录，不声称已被 PDF 索引接收。

**新增或修订 PDF 使用 `--merge`。** Agent 只需为本次新增、修订的来源准备私有清单：新 ID 加入书目，同 ID 更新该来源，未列出的旧书保留。更新前先用 `--dry-run` 核原件、旧索引摘要和差异；预览不提取页文本、不发布索引，也不是锁定输入或后续执行的凭证。实际登记会重新核验输入，预览后文件变化需重新判断。

```sh
# 清单由 Agent 根据实际新增或修订资料准备，以下路径是示例。
"$ONTOLOGY_ENGINEERING_ROOT/var/install/bin/oev2" --root "$ONTOLOGY_ENGINEERING_ROOT" knowledge register \
  --manifest var/source-intake/new-books.json --index sources/.indexes/local-pdf --merge --dry-run
# 核对差异符合本次任务后执行。
"$ONTOLOGY_ENGINEERING_ROOT/var/install/bin/oev2" --root "$ONTOLOGY_ENGINEERING_ROOT" knowledge register \
  --manifest var/source-intake/new-books.json --index sources/.indexes/local-pdf --merge
```

| 登记模式 | 对目标书目的作用 |
|---|---|
| 不加 `--merge` 或 `--replace` | 创建新索引；已有目标不会被隐式覆盖。 |
| `--merge` | 按稳定 ID 加入或更新，保留本次未列出的旧来源。 |
| `--replace` | 按本次完整清单重建；未列出的旧来源退出该索引。 |
| `--dry-run` | 预览所选模式的输入与差异，可配合 `--merge` 或 `--replace`；不发布变更。 |

`--merge` 与 `--replace` 互斥。合并时，摘要、路径与导航等身份未变的来源复用已有页记录及原提取工具身份，只对新增或需要重提取的来源执行提取；全部相同返回 `unchanged`，索引字节保持。`preview`、`unchanged`、`registered` 分别表示预览、无需修改和已登记，均不等于工程采用。核结果 `update` 中的 `mode`、`added`、`updated`、`unchanged`、`retained`、`removed`、`final_source_count`，以及预览的 `extraction_required_ids`、实际执行的 `reused_source_ids` / `extracted_source_ids`，确认旧来源没有意外退出。

已有交付清单时，`knowledge library source-delivery register` 同样支持 `--merge`、`--replace`、`--dry-run`。例如只接入清单中的一项，用 `--select` 配合合并：

```sh
"$ONTOLOGY_ENGINEERING_ROOT/var/install/bin/oev2" --root "$ONTOLOGY_ENGINEERING_ROOT" knowledge library source-delivery register \
  --manifest var/source-intake/delivery.json --select new-book --merge --dry-run
"$ONTOLOGY_ENGINEERING_ROOT/var/install/bin/oev2" --root "$ONTOLOGY_ENGINEERING_ROOT" knowledge library source-delivery register \
  --manifest var/source-intake/delivery.json --select new-book --merge
```

**完整重建仍使用 `--replace`。** 安装根或原件路径迁移时保全旧清单、索引与回执，准备包含全部保留来源的清单，先用 `--replace --dry-run` 核移除项，再显式重建。仅传新书或以 `--select` 选一项后执行 `--replace`，会使其他书退出目标索引。完成后核全部来源、页数和代表性命中；失败和未知继续保留。

米思米目录使用既有供应商读取流程，保留目录版次、原页、完整型号、选项与安装条件；案例图、目录参数、已下载 CAD 和原生模型核验分别记录。历史资料不替代当前型号规格、价格或交期核查，CAD 文件完整也不等于导入、保存重开或工程适用通过。具体边界见[工程工作中的供应商来源](../references/engineering-work.md)。供应商资料不因同在 `sources/` 就自动成为六书 PDF 索引的一部分。

已有完整米思米 source-pack 资料包时，在当前安装根显式重建到查询使用的索引位置：

```sh
"$ONTOLOGY_ENGINEERING_ROOT/var/install/bin/oev2" --root "$ONTOLOGY_ENGINEERING_ROOT" knowledge build-index \
  --source-pack sources/misumi/source-pack.json --index sources/.indexes/misumi
```

`build-index` 与默认查询均使用 `sources/.indexes/misumi`。已有 `misumi-native` 索引若需继续使用，应显式选择其路径；程序不会自动搬迁或猜测旧索引。需先确认 Poppler 可用；已有目标索引时先核来源与保全记录，再按合同使用 `--replace`。零散网页、模型或手册不直接套用完整 source-pack 的构建命令，供应商重建也不套用 PDF 登记的 `--merge`。

扫描页、稀疏文本、公式、表格和图形分别记覆盖与未知。必要时按实际任务读原页、做 OCR 或图表审查；OCR 需要明确可用工具、授权和预算，不在普通登记中自动启动模型。提取成功不证明公式、单位或图表关系正确。大书可逐章推进，逐次记录已处理和待处理范围。

### 3. 按问题导航，图谱另行设计

资料可先作为既有工程问题和模块的来源候选，保留版次、原件摘要、章节/原页位置、适用前提与待核范围；按任务检索后回读。私人导航留在 `sources/` 或 `var/`，不把整本书塞进 `SKILL.md`，也不为每本书新建 Skill。

通用 `knowledge query` 可检索实际登记的新增 PDF；当前 `project work` 的六书角色仍有固定的规范与实现。登记新书不会自动扩展这些角色，不能套用不相符的已有来源 ID。逐书分解与本体图谱的实体、关系、溯源、验证和增量更新方式后续单独设计；当前页级索引不宣称已具备这些能力。

### 4. 回到当前 Project 查证与采用

先明确当前 Project、对象修订、目标和工况，再检索候选。通过原页回读核实版次、公式、单位、假设、适用范围及反例，说明资料支持哪个主张、还缺哪些独立依据。保存实际查询、候选与原页核验包，条件或来源改变时重核。

使用 `knowledge verify-packet` 核包与来源，再按当前 Project 的证据合同登记。新查询为每个本地 PDF 候选保存单来源身份、章节导航摘要和实际索引目录；复核时另以 `source_registration_freshness` 说明它是否仍对应当前登记。比较针对该来源，无关新书导致全库摘要变化，不会使未变的旧书候选失效。

| 登记状态 | 含义与接续 |
|---|---|
| `current` | 与所绑定索引中的当前单来源及章节导航一致。仍需核原件、当前 Project 条件与适用性。 |
| `changed` | 同一来源 ID 的登记身份或章节导航已变化；重新查询并回读受影响原页。 |
| `removed` | 在已核当前索引中找不到该来源 ID；保留旧包，查清移除原因。 |
| `unavailable` | 所绑定的当前登记或索引无法取得或核验；先补齐或纠正来源，不能推定当前。 |
| `not_bound` | 包中缺少当前登记绑定，例如旧版查询包；用当前查询取得新包，不手填绑定。 |
| `not_applicable` | 候选不属于本地 PDF 登记，例如米思米；按该来源原有合同核验。 |
| `mixed` | 包内候选的登记状态不同；逐项处理，不能把整包视为当前。 |

原件与历史包的完整性检查仍独立保留：默认复核的 `status` 可以是 `ready_for_agent_review`，同时登记状态为 `changed` 或 `removed`。这表示旧原件仍可回读，不表示它仍是当前书目依据。`current` 也只说明与本地当前登记一致，不证明是出版方最新版、内容正确或适用于当前工程。

当前任务需要以现有本地 PDF 登记为依据时，Agent 在复核和后续反馈中使用 `--require-current-registration`：该选项要求本包**全部候选都是本地 PDF，且都有 `current` 绑定**。仅选一个当前候选不能跳过包内其他候选的检查。以下路径及上下文由 Agent 按实际 Project 准备：

```sh
"$ONTOLOGY_ENGINEERING_ROOT/var/install/bin/oev2" --root "$ONTOLOGY_ENGINEERING_ROOT" knowledge verify-packet \
  --packet var/projects/example/retrieval/packet.json --context var/projects/example/context.json \
  --require-current-registration
```

旧包无绑定、米思米候选或含非适用候选的混合包均不能满足该严格选项。严格复核未满足时报告 `source_registration_not_current`，另以 `historical_verification_status` 保留历史完整性状态；严格反馈拒绝写入。需要当前本地 PDF 证据时，用 `knowledge query --provider local-pdf` 生成新包；供应商资料继续走各自来源合同。默认不加该选项的历史回读保持原语义，不能将默认复核成功写成“当前登记通过”。

登记变化不改写旧 packet 或 feedback，也不自动扫描全部 Project 或更新决定。Agent 根据本次任务找出受影响的引用，保存新查询与复核，再按原 Project 合同处理。检索命中、摘要一致或登记状态 `current` 都不直接更新工程决定；决定与 `project adopt` 仍遵守各自的来源、检查和独立批准要求。专业工具结果使用现有 `project tool-result` 合同，不能把文献摘要伪装成工具实测。

### 5. 保存实际使用反馈，再考虑改进方法

经 `knowledge feedback` 记录哪些候选实际采用、延后或拒绝及其依据。需要当前本地 PDF 登记约束的反馈，使用同一 `--require-current-registration`；命令会重新核本包全部候选，登记不当前时拒绝写入，不把先前的复核结果当作本次通行凭证。默认历史反馈语义保留，新反馈另存，旧记录不改写。参数和结构化记录由 Agent 准备，用户不必填写绑定字段。有真实 Project 使用经验时按工程记忆合同保存时间线、失败、用户纠正、因果未知与适用条件。召回仍须在新任务中复核，不把整本书复制成经验结论。

资料提出新方法时，先区分项目实例补充与软件/方法规则改动。需要扩展正式工程路由、共享合同或行为时，沿 `specs → 锁定 ggen → Rust → 回归与独立验证` 推进；RRSI 仍按固定根目标、基线与判据比较，不因读到新书就晋升规则。

## 实际命令与检查范围

先使用本安装已核入口查看命令；以下 help 不会下载或重建资料：

```sh
"$ONTOLOGY_ENGINEERING_ROOT/var/install/bin/oev2" --root "$ONTOLOGY_ENGINEERING_ROOT" knowledge --help
"$ONTOLOGY_ENGINEERING_ROOT/var/install/bin/oev2" --root "$ONTOLOGY_ENGINEERING_ROOT" knowledge register --help
"$ONTOLOGY_ENGINEERING_ROOT/var/install/bin/oev2" --root "$ONTOLOGY_ENGINEERING_ROOT" knowledge library source-delivery --help
```

| 目的 | 当前入口与边界 |
|---|---|
| 清点交付与本地摘要 | `knowledge library source-delivery list/verify`，显式提供实际可用的交付清单 |
| 取得缺失原件 | 同组 `import/fetch`；本地已取得的资料优先核验后迁入，`fetch` 仅用于实际缺失且授权的下载 |
| 建立或增量更新 PDF 索引 | `knowledge register`，或同组 source-delivery `register`；新增/修订先 `--merge --dry-run`，核差异后 `--merge` |
| 完整重建 PDF 索引 | 同一登记入口的 `--replace`；传完整保留书目，先预览移除项 |
| 检索和原页回读 | `knowledge query/view`；查询候选与原页核验分别记录 |
| 核查询包及实际反馈 | `knowledge verify-packet/feedback`；原件完整性与 `source_registration_freshness` 分别读取，不授予工程采用 |
| 要求当前本地 PDF 登记 | 两命令加 `--require-current-registration`；整包全部候选须为已绑定且当前的本地 PDF |

Agent 根据本次真实输入准备参数与私有记录。先核 `knowledge status`，再核旧书仍可检索、新书来源和页码可回读、失配摘要会拒绝、扫描/图表覆盖如实显示。每轮向用户交代新增数量、完成范围、未处理项及可供当前项目查证的内容，不以命令退出成功代替这些检查。

## 何时更新 Google Drive

仅本地路径调整或软件升级，不需要重新上传相同原件。未来新增资料、新版或可复用图谱等处理成果时，再单独更新资料交付包：核内容许可、原件与派生关系、版本、摘要及覆盖范围，保留旧版，并验证远端回读与新用户导入。私人 Project、session、记忆库、设备配置和凭据不混入资料包。本轮继续使用现有 Drive 内容。
