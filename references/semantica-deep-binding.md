# Semantica 深绑定契约

状态：**唯一运行时边界已确定**。具体源码 commit、构建产物和 SHA-256 只以
`runtime/semantica-source-lock.json` 为准；具体 package/scenario 完成度只以 registry、
runner report 和 release receipt 为准。

用户的比喻是“瓶中装满石头，再把水倒满”，随后进一步明确：

> **石头只有两卷书；其余语义与生命周期制品都应成为 Semantica。**

因此，深绑定不是 OE 调用几次 Semantica，也不是保留旧实现后换一层 facade。它是
语义所有权的转移：OntologyEngineering 保存两卷可读来源；Semantica fork
（<https://github.com/jiaqiwang969/semantica>）是唯一可执行语义系统。

## 1. 关系定义

```text
两卷书（9 + 20 章，唯一外部书源）
  └─ Semantica built-in packages
       ├─ 29 chapter packages
       ├─ Vol.2 normative domain package
       ├─ ontology / CQ / SHACL / SPARQL / case / rule / contract
       ├─ dataset / query / validation / reasoning
       ├─ snapshot / diff / version / PROV / receipt / release gate
       └─ Python / CLI / MCP（同一 runner 的薄适配）

ontology-engineering
  └─ 两卷书 + 来源地图 + 检索/教学薄入口 + Semantica 源码构建锁
```

RDFLib、pySHACL、PyOxigraph 或其他标准引擎可以是 Semantica 内部实现细节；OE 与
调用方不得直接依赖或感知这些对象。“唯一替代”指唯一公共 API、唯一 package
registry、唯一运行时责任边界与唯一 receipt，不表示重复发明全部 W3C 算法。

## 2. 资产所有权

| 制品 | 唯一正本 | OE 可保留什么 |
|---|---|---|
| 两卷正文、图、PDF、术语与命题索引 | OntologyEngineering | 完整书源 |
| 章节来源锚点与检索地图 | OntologyEngineering | 相对路径与受控逻辑 ID |
| 29 章 manifest/contract/CQ/scenario | Semantica | package ID 的薄引用 |
| TBox/ABox、RDF/OWL、SHACL、SPARQL | Semantica | 书中用于阅读的短片段，不得成为可执行副本 |
| 正例、单因反例、ambiguity、prior-release fixtures | Semantica | 不保留 fixture 文件 |
| 工程规则与 exact oracle | Semantica | 不重写 runner 逻辑 |
| normative 转述/卡片/派生语义 | Semantica domain package | 书中自然语言解释与合法来源指引 |
| snapshot/diff/version/PROV/receipt/release | Semantica | 验证并展示返回 DTO |
| 源码与 wheel 身份 | source lock | lock 与本地 vendored artifact |

旧路径只可出现在 Semantica migration map 的 provenance 字段中；不能因为要保存历史
就继续维护一个可运行副本。

## 3. 统一 SemanticRuntime

Semantica 的统一运行时必须提供稳定、后端中立的 DTO，并至少覆盖：

- RDF term、quad 与 Dataset，包括 blank node、datatype、language、named graph；
- load/serialize、query SELECT/ASK/CONSTRUCT/UPDATE；
- SHACL validation 与规范化 violation tuples；
- 显式声明的有界正向规则推理与解释；
- package load、scenario run、exact oracle comparison；
- snapshot、semantic diff、PROV、execution report、receipt 与 release verification。

返回值不能泄漏底层 backend 对象。不支持的 profile、未知 package/scenario、缺失资产、
损坏 manifest、hash mismatch 或无从验证的 oracle 必须 fail closed；禁止空结果冒充
查询成功、恒真校验、静默换后端或启发式 CQ 冒充 exact execution。

当前契约不把完整 DL/tableau、一般 SWRL built-ins、非单调/默认逻辑、时序或概率
推理说成已有能力。包可以保存书中的相关知识，但执行合同必须把未支持能力标为
blocked/partial，而不是伪装成 green。

## 4. 29 章与 normative package

稳定 package IDs 为：

```text
semantica.chapter_packages.vol1.ch01 ... ch09
semantica.chapter_packages.vol2.ch01 ... ch20
semantica.chapter_packages.vol2.normative
```

每个 chapter package 独立声明：

- 书卷/章节、稳定命题与书源锚点；
- CQ、场景和所需 runtime capabilities；
- ontology/data、query/shape/rule assets 及其 SHA-256；
- positive、single-fault negative、ambiguity 与 prior-release cases；
- exact oracle（binding multiset、boolean、graph isomorphism、violation set 或 rule facts）；
- operation reports、dataset/revision identity、PROV 与 release receipt；
- `native / partial / placeholder / absent / blocked` 等诚实状态和阻断原因。

章节可显式复用另一个 package 的资产，但不能复用一个笼统“全书通过”结果。29 章
各自拥有 manifest、scenario 状态和 receipt；normative domain package 是额外的受控
规范转述层，不把 29 章计数变成 30 章。

## 5. Python / CLI / MCP 同核

- Python：`semantica.chapter_packages.SemanticPackageRunner`
- CLI：`semantica package list/show/run/verify`
- MCP：`list_chapter_packages`、`get_chapter_package`、
  `run_chapter_package`、`verify_chapter_package`

三种入口只能适配同一 registry、runner 和 DTO。不能在 CLI/MCP 中复制 package 解析、
业务 oracle 或 release verdict 逻辑。执行身份必须绑定 runtime commit、wheel SHA-256、
package/assets、输入/输出和 PROV；OE 薄入口从 source lock 提供该身份。

## 6. 源码构建与可复现身份

Semantica 使用完整、非浅、非稀疏的本地 Git checkout 开发和构建。最终锁文件负责记录：

- fork/canonical repository 与实际 commit；
- 构建输入和工具链；
- wheel 文件名、SHA-256 与包含内容；
- 可复现构建结果；
- package/runtime/CLI/MCP 与 OE gate 的验证结果；
- 已知能力边界。

文档不复制尚未冻结或容易过期的 commit/hash。更新 Semantica 后必须重建 wheel、重跑
门禁并原子更新 lock；仅修改 branch 名或版本字符串不构成可复现升级。

## 7. Zero-exception gate

切换完成的判据是严格扫描零发现、零 allowlist exception：

1. OE 活跃代码不直接 import/invoke RDFLib、pySHACL、PyOxigraph、owlready2、Jena；
2. 不通过动态 import、私有 backend、subprocess 或内嵌语义 payload 绕过 Semantica；
3. OE 不保留 executable `.ttl/.owl/.rq/.sparql`、SHACL、rules 或 fixtures；
4. runtime factory 只有 Semantica，不存在 fallback；
5. 29 个 chapter packages 与 normative domain package 可从统一 registry 发现；
6. query/SHACL/rule oracles 按其语义类型精确比较；
7. 每次执行生成绑定源码、wheel、package/assets、输入和输出的 receipt；
8. unsupported、partial、placeholder、absent、missing 或 mismatch 一律阻断；
9. 本地 wheel 安装、平台/版本回归、CLI/MCP parity 与隐私/权利门禁全部可审计。

门禁失败说明瓶中仍有平行实现、空隙或假绿色；不得靠新增例外清零。

## 8. 规范与权利边界

Semantica normative package 只承载依法制作、可审计的转述和派生语义，不收纳 ISO
原始 PDF 或未获许可的逐字抽取件。精确条款、表格与原文必须回用户合法持有的受控
来源核对。package 通过只证明当前输入符合已编码合同，不授予标准版权、产品合规、
认证、风险接受或对外发布权。

## 9. 默认语义介入与行业本体炼化

`ontology-engineering` 的长期复用入口不是“查书后可选运行一次 demo”，而是一次
source-locked semantic engagement。每次被另一个工程 skill 或项目调用时，都必须：

1. 读取严格的 project binding 与 task envelope；
2. 用两卷书选择方法镜头和解释边界；
3. 发现并核对 Semantica package、baseline、capability 与 runtime identity；
4. 在适用时运行现有 CQ/query/shape/rule/oracle；
5. 分开返回工程结果、Semantica execution/receipt/release 和本体学习判定；
6. 返回 `no_delta`，或把有证据的可复用缺口送入 Semantica 原生炼化状态机。

行业本体候选必须由 Semantica 管理内容寻址、不可变版本、差异、CQ 回归、PROV、
receipt 和 promotion。项目绑定只能引用受控 workspace/registry identity 与 digest；
不得把 package ID 拼成任意文件路径，也不得让 OE 自己加载一个外部 RDF 目录。

受治理 workspace 是候选事务区，不是第二发布正本。只有完成
`candidate → proposed → committed → regression_passed → release_complete → promoted`
并由有权人批准后，内容才成为可复用行业 package；`published` 始终是独立外部决定。

完整候选 delta 必须同时覆盖 ontology、CQ、SHACL、named query、受支持规则、
positive/single-fault-negative/ambiguity/prior-release cases、contract、provenance 和
book impact。只积累类名、属性名和注释不能称为行业本体已经炼化完成。

详细跨-skill 输入、三联输出、授权矩阵和失败状态见
`semantic-engagement-contract.md`。两卷书的作者/TeX/PDF 收敛流程见
`book-authoring-workflow.md`。
