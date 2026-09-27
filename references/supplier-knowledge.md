# 工程资料：教材、供应商与同一项目决定

当当前决定需要零件的功能机理、结构比较、计算边界、接口装配、失效维护、应用
经验或规格时使用。根入口和 CAD、制造、证据模块共用这条资料路径；知识解释无需
启动 CAD。Jev 选择本轮有用的主题和来源，工程本体负责解释、核验与回到决定。

## 接续同一项目

先读现有项目记录的目标、对象 ID/修订、必要功能、工况、已有依据、未决问题和
变化，再生成本轮小快照。保留需求、观察、假说、来源陈述和已采用决定的区别。
不能用上一条查询或工具日志代替项目状态；也不另建一套自动覆盖项目的记忆。
用户说“这部分呢”时由 agent 续接上下文，用户不需要填写 JSON。

使用 `ontology-engineering.supplier-knowledge-context/v1`：

| 字段 | 内容及边界 |
| --- | --- |
| `task_id` | 当前已有任务 ID；临时知识讨论可使用明确的临时 ID |
| `decision_id`（可选） | 原项目的当前决定 ID；正式交接前须明确，不能用查询任务 ID 代替 |
| `project`、`object` | 项目/对象 `id`、`revision`，对象另有 `name`；未知留空，不编造 |
| `goal`、`stage`、`background` | 本轮要支持的决定、已有状态和必要背景；阶段是说明，不是强制状态机 |
| `functions` | 当前必要功能；与供应商商品分类分别记录 |
| `statements` | 每项 `id/text/status/source_refs`；状态限 `requirement`、`observation`、`hypothesis`、`source_report`、`accepted_decision`、`unknown` |
| `unknowns`、`changes` | 当前缺口及已发生的变化；变化不自动宣告旧方案失效 |
| `alternatives`、`prior_decisions` | 待比较路线和原决定的有范围摘要，不继承历史 PASS |
| `record_refs` | 原台账及记录引用，留在本地；语义正文仍只发送已授权的必要信息 |
| `source_bindings`（可选） | 显式投影产生的来源相对路径、完整文件 SHA 和字段映射；用于重新核对声明的项目记录，不证明工程事实 |
| `decision_criteria`（可选） | 当前有关因素：`id/dimension/kind/statement/source_refs`；kind 为 `hard_constraint`、`objective`、`preference` 或 `unknown`，不自动建立权重或评分 |
| `tradeoffs`（可选） | 当前矛盾和方案取舍；条件、可比性和证据不足时明确保留未知 |
| `iteration`（可选） | `id/focus/stop_condition`，续接原迭代的重点和有依据的结束条件；不另建流程状态机 |

除 schema 和非空 task_id 外允许字段缺失。输入校验是操作接口检查，不是工程
语义判定。快照上限 8 KB；从实际项目选择相关事实，不能为了缩短而删除关键反例或
适用条件。本地来源绑定另限 24 KB。`source_refs`、`record_refs`、`source_bindings` 不发送给 Jev；正文里的私密信息仍须由 agent
按任务授权选择。没有 source_refs 的陈述不会被程序自动补成有证据事实。

已有 `cad-agent.supplier-acquisition-context/v1` 时，从相同的目标、对象、约束、
需求、已取得资料与接受记录生成这个检索快照，沿用原 ID 和 record_refs。两个接口
分别处理知识定位与网页真实资源选择，不把 acquisition 的 `fulfilled_needs` 当成
本轮适用性已通过。

已有 JSON 项目记录时，优先用 `--project-context` 按显式字段映射生成本轮快照。
映射由 agent 根据原记录准备；用户仍用日常描述。以下仅示意映射格式，路径和字段
须替换成项目实际存在的内容，不能为满足接口补造身份或修订：

```json
{
  "schema": "ontology-engineering.project-context-projection/v1",
  "base": {"schema": "ontology-engineering.supplier-knowledge-context/v1", "task_id": "current-inquiry"},
  "fields": [
    {"source": "var/projects/example/loop-run.json", "pointer": "/project_binding/project_id", "target": "/project/id"},
    {"source": "var/projects/example/discovery.json", "pointer": "/decision/id", "target": "/decision_id"},
    {"source": "var/projects/example/discovery.json", "pointer": "/decision/plain_language_goal", "target": "/goal"},
    {"source": "var/projects/example/discovery.json", "pointer": "/trigger/object_id", "target": "/object/id"},
    {"source": "var/projects/example/discovery.json", "pointer": "/trigger/product_revision", "target": "/object/revision"}
  ]
}
```

只读取 skill 内 `var/projects/` 的原记录，按 JSON Pointer 复制字段，不解释或转换
事实。项目修订缺失保持未知，不能用方案修订冒充。数组须符合上述快照结构；源值
因默认补字段而变化时拒绝投影，由 agent 核对映射。最多 16 个来源、64 项映射。
原记录不会被修改。新增事实也须有映射；`task_id` 是本轮元数据。

每次查询重新读取原记录；知识查询前、后审前后及交接时核对完整源文件哈希和
映射值。变化或缺源标为 `stale_context`，旧结果保留为历史候选；重新投影并查询
才能取得新状态下的判断。检查范围仅为显式声明的记录，不是后台同步或整个项目
依赖完整性的保证。来源文件任意字节变化都会保守失效，可读回核对后重新查询。

## 可执行入口

统一入口为 `scripts/jev_knowledge.py`。Jev 从当前问题选择主题、章节、来源片段和
下一核查；米思米与本地 PDF 来源共用同一判断链。无需另装 MISUMI
应用，也不依赖 PATH 中的旧命令。检索、哈希与文字定位由内部确定性工具完成，
正式语义判断继续由 Semantica 执行。

核心包提供代码、方法与凝练示例；原始教材、压缩目录与全文索引已独立上传 Drive
并完成逐文件大小、MD5 与 SHA-256 校验，仍由同一来源组件接入。下载定位、来源版本、
字节数及哈希统一见[来源交付清单](../runtime/source-delivery.json)。访问受限，接收方权限
尚未验证，下载者需具备资料库访问权限；软件发布不更改该权限。下载后回填当前 skill 的 `sources/books/`、`sources/misumi/`；
索引位于 `sources/.indexes/`。Git 与核心 ZIP 排除 `sources/` 和 `var/`，本地仍是一个完整 skill。
按[默认位置与接入命令](../docs/PORTABLE-DISTRIBUTION.md)自动归位、校验、登记；本地查询不要求持续连接 Drive。
独立原页按需恢复并核对原始哈希；工艺图、曲线和完整页面保持原文件字节。
未安装或登记来源时，全文检索明确返回 `not_ready`；方法示例不代表全目录知识已就绪。
账号、项目事实、缓存和查询历史不随 skill 分发。

```bash
python3 scripts/jev_knowledge.py "这部分现在该重点查哪些资料？" \
  --project-context var/projects/<project>/knowledge-projection.json \
  --output var/projects/<project>/supplier-evidence-001.json
```

入口按自身位置解析 skill root；从其他工作目录也可用绝对脚本路径调用。
`--data-root` 指定米思米资料目录，`--pdf-root` 指定本地 PDF 索引；`--state-root` 指定 skill 内的私有状态目录。
默认历史在 `var/state/misumi/`，Jev 缓存与原页图片在 `var/cache/misumi/`。
这些路径及项目上下文、输出须位于当前 skill 内；不回退到外部旧目录。
`--output` 必须是新文件，保留先前查询。`--top` 控制返回页数，`--candidates` 控制有限候选池。

终端也可直接查：

```bash
python3 scripts/jev_knowledge.py --status --json
python3 scripts/jev_knowledge.py "直线轴承与微型滚珠衬套的旋转使用限制"
python3 scripts/jev_knowledge.py "这部分下一步查什么？" --context var/projects/<project>/current-knowledge-context.json --json
python3 scripts/jev_knowledge.py "联轴器 选型 偏心" --local --json
```

默认进入工程知识模式；没有项目文件时只按当前提问建立临时上下文，不声称知道项目历史。
`--project-context` 从声明记录重新投影；`--context` 接收已有快照，两者互斥。
未携带来源绑定的旧快照报告 `project_source_freshness.status=not_bound`，不能据此
声称原项目仍是当前状态。加 `--local` 只做本地召回，
主题、相关性及条件识别均保持未判断；Jev 失败也不会静默冒充本地筛选成功。
Jev 在线模式需要用户自己的凭据，按[共用 Jev 接入](../docs/judgment-intake.md)配置。
旧 `scripts/search_misumi_knowledge.py` 仅为既有调用提供包内转发，不是独立后端。

### 本地教材与原页图片

交付的六本书优先用 `python3 scripts/source_library.py register`，清单已含整书导航；
原件在 `sources/books/`，默认索引在 `sources/.indexes/local-pdf/`，登记为相对路径。
以下 JSON 接口用于新增自有资料；先把原件归入 `sources/`，不长期引用外部原件。

用 `--register-pdfs <manifest.json>` 登记原件并建立可重建的私有索引；登记使用
Poppler 的 `pdfinfo`／`pdftotext`，原 PDF 保持原位和原字节。清单结构如下，路径相对
清单或为本机绝对路径；换机器时重新指向相同 SHA 的原件即可，不把作者路径写入代码。

```json
{
  "schema": "ontology-engineering.local-pdf-manifest/v1",
  "sources": [{
    "id": "book-id", "title": "实际书名", "edition": "实际版次",
    "path": "sources/book.pdf", "sha256": "原件的64位SHA256",
    "chapters": [{"id": "chapter-1", "title": "原章名",
      "start_page": 1, "end_page": 20, "aliases": ["经审阅的中文导航词"]}]
  }]
}
```

章节范围使用 **1-based 物理 PDF 页**。导航别名只帮助召回，不进入正文或主张；
扫描页明确返回无原生文字。每本书分别分配候选，独立索引的 BM25 分数不直接比较。
教材候选预算允许时，先为 Jev 选中的每章保留一页，再补充跨书词法候选，避免
相关章节被其他书的词法匹配挤出；章内优先有较充分原生文字的页。原生文字缺失
仍只返回导航候选供看图，不补造正文。`routed_chapter_candidate_counts` 显示各选中章
实际进入候选池的页数；覆盖了章节标题不等于其正文已进入判断。
Jev 共用主题判断与后审；章节按请求字节上限分批筛选，独立批次最多两路并行，
按原批序记录结果与失败，再对候选集合统一归并。正文筛选沿用最多四路并行；
同一状态下的独立问题合在一次请求，依赖已返回原文的后审等待前一步完成。
并行不减少调用数或 token 用量，不授权并行修改项目记录或 NX 模型。
不直接比较不同 Choice 候选集的概率。每批保留 considered、omitted 和失败范围；
批数上限或局部失败返回 partial，标题进入判断不代表正文已读或适用性已核。

`--sources auto` 默认查询已登记来源；显式 `--data-root`／`--book` 保持米思米范围。
`--sources all` 明确检查所有来源，`misumi`／`local-pdf` 可限来源。登记已有目录须显式
`--replace-sources`；失败重建保留旧索引。米思米的冻结压缩索引不被教材修改。

```bash
python3 scripts/jev_knowledge.py --register-pdfs /private/sources/manifest.json
python3 scripts/jev_knowledge.py "轴承配合与装配条件" --sources all --local --render 1
python3 scripts/jev_knowledge.py --view-record var/projects/<project>/evidence.json --render 2
python3 scripts/jev_knowledge.py --view-page local_pdf:book-id:page:116 --render-dpi 180
```

`--view-page PAGE_ID` 直接查看当前已登记来源的原页，无需该页先进入搜索结果，
不调用 Jev。教材 ID 为 `local_pdf:<登记书籍id>:page:<物理PDF页>`；先从登记清单或
章节导航核对书籍身份和页码。米思米使用索引中的原页 ID（如 `fabiaozhunpin202210:page:13238`），
其中末段是原目录页标识，不能用印刷页或整书 PDF 页码猜测替换。两类来源均核对当前
原件哈希，保留整页图与未解释状态；无效 ID、原件变更或缺失明确报错。
它与 `--view-record`、查询及 `--render N` 互斥，可用于查看扫描章的后续页、公式或
跨页条件，再由 agent 读图或按需提取；原页可见不代表已完成 OCR 或知识采用。

`--render N` 使用 `pdftoppm` 按需生成完整原页 PNG，缓存保留原件 SHA、实际文件页码、
分辨率、渲染器和图像 SHA。教材从整书的指定物理页渲染；米思米独立单页从文件第 1 页
渲染，并另保留目录页号。未核印刷页保持未知；图片能打开不等于公式或参数已核验。
图像缓存损坏可重建，原件变化须重新登记。书本原理与本体化的整书覆盖仍需逐模块核查。

## 从资料到凝练知识

先以既有能力问题和设计模式确定要提取什么，再保留有来源的对象、机制、条件、
反例、取舍和验证义务。重复的通用含义引用既有模式；领域条目只增加领域差异。
正文相同但适用型式或上下文不同的片段不自动合并。完整规格表、导航和广告不进入
共用方法，具体型号参数按需回查原页。示例见
[运动适用性凝练](../examples/knowledge-distillation/README.md)，当前仅两页，非全量转换。

Jev 的候选模式相关性与来源解释不构成事实采用或本体晋升。继续使用已有
[批量判断及证据审查](../docs/judgment-intake.md)、[采用与模式演化](../docs/judgment-governance.md)
及原项目记录。缺少支持时保留未知；新资料能用已有模式表达时，不增加 TBox。

## Jev 实际接收和返回什么

判断组合参考 TypeSafe 的 [Choice](https://docs.typesafe.ai/primitives/choice)、
[分层候选筛选](https://docs.typesafe.ai/cookbooks/hierarchical_classification)和
[引用核查](https://docs.typesafe.ai/cookbooks/citation_check)（2026-09-27 核读）。
同一状态下独立的问题可以一起问；需要前一步取得的新证据时才进入下一请求。
代码保留完整候选身份与返回分布，生成候选和实际工程采用分别记录。
当前知识适配器使用 Choice；增加 Score／Noul 要有对应合同与行为测试。
Choice 概率受候选集合影响，不拿不同分批中的最大概率直接排序；最终在同一候选集归并。
置信度表示回答分布的集中程度，不代表引用正确、工程合格或执行权限。

[维护的主题与方法](supplier-knowledge-policy.json)与根情景 instruct 的共用方法段落
装配为 `resolved_method`，实际进入每次主题、章节、正文和结果后审请求；同一请求
只传一次方法，保留文本、共同 instruct、原书锚点及策略的哈希。书源解读、机械应用
推论和完整设计链中的用途见[从书的方法到工程用途](supplier-knowledge-interpretation.md)。
按本轮 CQ → 对象与功能 → 机理和条件 → 可改变决定的资料组织查询。
检索前的模式问项明确处于“选择本轮相关核查义务”阶段：来源正文尚未取得，
不把检查尚未完成解释成义务无关；原模式定义、选项及未知边界保持不变。
这仍是候选判断，主 agent 须核对漏选；阶段说明尚不能保证模式选择完整。

1. 从当前快照和问题独立判断七类主题是否需要，允许多选和未知；再选择已观察到的
   主章节与确有用途的配套章节。
2. 按对象、主题、正文及章节召回有限候选；明确并列的对象各有召回池，技术说明
   使用独立正文池及页首内容线索，避免只按产品型号排序。召回线索不判工程真假。
   Jev 判断正文相关性及可重叠的知识角色，规格页没有固定优先权。
3. 返回实际评估的原文片段、完整页文本、书/印刷页/PDF 页、来源 URL、原页文件、
   文本和 PDF 哈希。提示条件、禁忌及本轮缺项，保留完整原页供核对。
4. 基于实际返回的相关片段、当前未知与变化，再由 Jev 独立判断十三类下一核查是否
   必要；输出 `review_plan`，包含完整答案、选中及未知动作、待答问题、交接对象与
   目标产出。原文输入有预算及明确省略范围，不将省略当作缺失证据。

后审片段保留开头身份，并按当前问题与条件语言选取有源窗口，避免只取页首而漏掉
中部禁忌。窗口有精确父片段和字符定位；这是有界取样，所有条件是否覆盖仍未证实，
实际采用前继续核原页及省略部分。

`review_plan` 是当前问题的核查候选，不是已执行计划或完整工程检查证明。
`knowledge_gaps` 是检索前生成的通用审查提示；两者分开。后审失败使结果为部分完成，
保留已取得资料及错误；`--local` 后审不运行、零网络。方法变化使缓存身份变化。
模型可能多选或漏选，尤其会把未来设计步骤选成本轮工作。按实际输入与决定复核后
再交接。实际采用始终保留来源和理由，不因 Jev 未选 `record_adoption` 而省略记录。

`knowledge_units` 是**来源段落候选**，不是已拆解和验证的机械规则。字符范围对应
索引中归一化的原生文本，不对应 PDF 几何坐标。长页分片有重叠；版式、图示、公式、
跨页续文和表格行列仍需回到原页。单段的条件标记为 absent 不证明整章没有条件。
目录型号热点不是完整配置产品，模型概率也不是工程合格概率。

## 回到工程本体的采用动作

入口返回 `ontology-engineering.supplier-evidence/v1`，核过源文件哈希、文本哈希、
精确片段及上下文绑定。`completed` 仅表示检索完成；所有资料仍是
`pending_agent_review`，`applicability=not_assessed`。主 agent 必须实际读入内容：

交接前可运行 `--verify-packet var/projects/<project>/evidence.json --project-context var/projects/<project>/knowledge-projection.json`，也兼容 `--context`。
入口复验来源与片段，再核项目、对象修订及 `decision_id`：同一完整快照为
`ready_for_agent_review`，条件或身份变化为 `changed`，未登记工程身份为 `unbound`。
绑定了来源但仍使用旧快照、原记录变化或缺失时为 `stale_context`；校验分别报告
当前快照和包内快照的来源状态。无来源绑定时 `not_bound` 与身份校验分开显示，
`ready_for_agent_review` 本身不证明原项目状态最新。
同时保留原查询状态、错误、是否有返回候选及后审状态；退出码 0 只表示这次身份与
完整性校验完成，不能把原查询的 `partial`／失败或零候选改说成检索成功。
概念讨论仍可使用未绑定的来源；上述结果均不表示适用性已通过。跨任务复用须在新情景
重新判断，不能把项目修订直接等同 CAD 模型修订；CAD／制造的明确对象映射仍需完成。

- 把与当前问题相关的原文拆成有范围的候选主张，逐项带上对象型式、成立条件、
  禁忌、例子与来源。厂商推荐、算例参数、一般机理和本项目观察分别表述。
- 比较项目与来源的配置/载荷/运动/温度/润滑/安装等条件；仅比较与当前主张有关
  的字段。未知保留未知；反例有自己的对象范围，不强行合并成冲突。
- 从原文和项目差异提出会改变决定的最小下一动作：查原图、核表、计算、对照、
  试验或取得当前供应商资料。实际读入 `review_plan`，对选中和未知动作说明采用、
  暂缓或修正的依据；高分不替代工程判断。`knowledge_gaps` 不是自动判缺件。
- 在**原项目台账**记录 `query/run/context hash → source/page/span → candidate claim
  → adopted/deferred/rejected + reason → affected decision + next check`。采用记录引用
  实际证据及对象版本；一个候选可只采用其中有支持的一部分。
- 新观察或工况变化后，生成新快照并查询。沿已记录依赖重审原决定；无依赖记录的
  范围仍未知。保留旧查询，不能把旧高分缓存或历史采用自动继承为当前证据。

| 检索材料 | 工程本体中的位置 |
| --- | --- |
| 项目对象、版本、功能、条件 | 复用项目实例与当前问题，不由商品名称创建正式类别 |
| PDF、页、文字片段及哈希 | 可追溯的来源与提取活动记录 |
| 目录中的建议、限制、算例 | 待审查的 source_report，尚非项目事实或通用 TBox 规则 |
| 适用性比较、计算或实测 | 支持/反驳/未知的有范围证据；依各自方法验收 |
| 已复核的采用与下一动作 | 回写原决定及工作依赖，继续 CAD、制造或证据模块 |
| 真正可复用的表达缺口 | 才进入行业本体候选与治理；资料增加通常为 `no_delta` |

正式 CQ/SHACL/rule 和项目语义审查仍走已有 ProjectOntologyBinding、task 与
source-locked Semantica。这个检索入口没有第二个本体后端，不生成通过回执。
原生 CAD、物理适用性和采购实时信息分别取证。当前版本没有自动全量知识图谱、
公式求解、全书 OCR 或后台持续更新项目的功能。

## 验证调用是否起作用

除结构检查外，用不含参考答案的工程快照实际运行入口，核对原页、主题、缺项和
回到当前决定的采用记录。同一句“现在重点查什么”，在结构比较、计算准备、装配
异常及条件变化情景下应有可解释的不同；反例和未知对象也要保留。合成检查是开发
诊断，不宣称独立准确率。模型漏选或选错时由 agent 按实际证据修正并保留原因。
