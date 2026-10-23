# 米思米资料在统一 Skill 中的使用

米思米资料统一放在安装根 `sources/misumi/`。标准品目录用于查型号、参数和原页；inCAD 用于查机构案例、官网声明的功能与条件、BOM 和 CAD 来源。资料图谱把这些对象及出处关联起来，选中资料后再接入同一工程 Project。

```text
sources/
├── books/                         书籍原件
├── misumi/
│   ├── source-pack.json           两套标准品目录的原资料包合同
│   ├── segments/                  目录单页压缩分片
│   ├── incad/                     机构案例、原页面、BOM、CAD与原投影
│   └── incad-bundle/              可移植交付的分片及摘要清单
└── .indexes/
    ├── local-pdf/                 已登记书籍索引
    ├── misumi/                    标准品目录索引
    └── incad/                     从固定原件构建的案例图谱
```

本地可以直接迁入已有真实归档，不重新下载。原件、原清单及历史回执保留原字节；绝对路径改变后另记迁移关系，并用新路径重新验证。旧下载捕获或历史 Project 请求中的绝对路径不会因目录移动自动获得新身份，不以软链接绕过来源核验。

## 图谱表达的内容

| 对象与关系 | 来源和含义 |
|---|---|
| 案例及其固定版本 | 案例编号、固定 projection/case manifest 及其 SHA256；不能仅靠可变 current 指针建立版本身份 |
| 功能、工件、环境、运动、尺寸、精度和载荷段落 | 保留官网原文、段落位置、原文摘要和声明角色；单位解释与跨段一致性仍须审查 |
| BOM 行及型号 | 保留原行、列、型号文本和来源；零件链接仍在原来源记录中，不凭同名把不同条件的型号合并 |
| CAD 格式包及成员 | 关联原 ZIP、成员路径、长度与摘要；实际字节核验和 NX 原生检查分别记录 |
| 资料、投影及派生图 | 保存来源路径、摘要与派生关系，图中声明可重新从原资料构建并对比 |

BOM 行与 CAD 文件没有明确对应依据时，不猜测配对。官网选型说明和参考答案保留其来源角色，不能改成当前项目的独立设计结果。已阅读的案例按 `development_replay` 使用；图谱生成不证明陌生案例迁移收益。

这个阶段完成资料层的结构化本体关联。机构为何可行、条件是否足够、因果解释是否正确，以及制造与实物是否合格，仍需要围绕具体工程问题继续查证。来源图谱不会自动创建 612 个 Project，也不会授予工程采用。

## 建立、查找和回读

以下目录均为安装根内的真实路径；索引输出须是新的目录。先核已安装程序的 `knowledge library incad --help`。

```sh
oev2 --root "$ONTOLOGY_ENGINEERING_ROOT" knowledge library incad build \
  --archive sources/misumi/incad --index sources/.indexes/incad

oev2 --root "$ONTOLOGY_ENGINEERING_ROOT" knowledge library incad search \
  --index sources/.indexes/incad --query '夹具' --limit 5

oev2 --root "$ONTOLOGY_ENGINEERING_ROOT" knowledge library incad view \
  --index sources/.indexes/incad --case 000210

oev2 --root "$ONTOLOGY_ENGINEERING_ROOT" knowledge library incad verify \
  --index sources/.indexes/incad
```

读取不能仅信任图文件自己的摘要：程序重新核原来源并重建相应声明，对照派生图；相关原件变化或来源不完整时拒绝。检索会重建全部案例的来源声明，并对返回案例的 BOM/CAD 文件逐项复核摘要；未返回案例的 CAD 字节复核明确为 `not_run`。完整 `verify` 则核全部所选来源闭包，执行范围和读取预算以实际报告为准。索引可用不代表全部 CAD 已通过原生导入、保存重开或工程适用检查。

新增或修订归档后，保留旧固定版本和索引，向新目录构建并复核，再由 Agent 在当前配置中显式切换。已有 Project 引用仍保持旧身份；需要当前资料时重新检索并审查受影响的决定。不手改已生成的图文件来消除来源不一致。

## 与四个环节的关系

- **工程本体**：区分机构、零件、表示、功能、条件、主张与来源；建立有依据的关系，保留缺项。
- **ggen**：从规范生成资料图谱、交付和检查所需的共享合同；不是把书或 CAD 本身编译成程序。
- **Rust**：核原件与清单，生成和检索图谱，验证派生关系，安全恢复分片资料。
- **RRSI**：利用实际选用与验证反馈改进检索或分析方法；同源案例不冒充独立验证，来源命中不等于方法收益。

选中案例后，Agent 根据当前目的、工况和验收要求准备已有 `project incad prepare/verify` 所需的来源绑定。该入口仍要求真实、完整的 Project/work；来源、CAD 暂存和当前设计分别沿[现行接入合同](CAD-INCAD-INTEGRATION.md)与[CAD 暂存合同](INCAD-CAD-STAGING.md)验证。书籍和标准品目录可以补充同一任务的证据缺口，各自保留页码、版本及适用范围。

## 新版资料包与空目录恢复

软件在 GitHub，资料在独立交付包。资料包携带 `source-delivery.json`，列明相对路径、长度和 SHA256；已有目录 source-pack 的身份保持，新 inCAD 使用单独的 `ontology-engineering.incad-source-bundle/v1` 合同。没有实际 Drive 文件 ID 时采用下载后的本地导入，不填虚构 ID，也不假定默认 `runtime/source-delivery.json` 已存在。

把完整下载目录放入安装根的 `var/downloads/`，按该版本包内 README 的实际文件名执行。例如：

```sh
oev2 --root "$ONTOLOGY_ENGINEERING_ROOT" knowledge library source-delivery import \
  --manifest var/downloads/OntologyEngineering-sources-20261023/source-delivery.json \
  --from var/downloads/OntologyEngineering-sources-20261023

oev2 --root "$ONTOLOGY_ENGINEERING_ROOT" knowledge library incad verify-bundle \
  --manifest sources/misumi/incad-bundle/bundle.json

oev2 --root "$ONTOLOGY_ENGINEERING_ROOT" knowledge library incad import-bundle \
  --manifest sources/misumi/incad-bundle/bundle.json --directory sources/misumi/incad
```

恢复目标必须不存在，全部分片和成员验证后才发布目录。拒绝路径越界、链接、特殊文件、重复成员、缺件、摘要失配和已有目标；失败不能留下被当作完整库的半成品。恢复成功后再建立、检索和复核图谱。书籍及标准品目录仍按[资料接入说明](../sources/README.md)分别登记和建索引。

交付范围是**选定来源文件的闭包**：原 manifest 保留，但浏览器捕获、下载进程、个人配置、日志和私人 Project 不随包分发。清单明确未交付路径，不能把恢复成功说成上游原 manifest 全部成员都已复现。本地完整历史归档继续保留；新版交付不会改写它。

源码能力、所获资料范围、字节核验、原生 CAD、工程适用和采用分别报告。当前版本是否已上传、使用哪个资料版本以及空目录验收范围，以交付包和维护回执为准。
