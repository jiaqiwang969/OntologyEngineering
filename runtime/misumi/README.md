# 内置目录资料与可移植索引

这是统一 Jev 知识入口的内部来源组件。资料保留原页身份和来源；不把目录陈述
自动提升为工程规则。核心保存方法、工具与凝练示例；下一版将原始教材和大型
来源库独立经 Drive 交付，下载、安装后按查询需要恢复原页。
目录的第三方来源和权利不因集成而改变。

## 独立来源组件与本地安装

米思米来源包包含 4,652 个原始页面、两本完整 PDF
及全文索引的无损压缩归档。没有删除工艺说明、原理图、公式、尺寸标注或图表，
没有图片降采样或 PDF 重编码。每个原文件的字节数和 SHA-256 均保留在来源清单。
正常查询使用同一个 `scripts/jev_knowledge.py`，下载并登记后可离线读取原件。
来源组件已通过 gws 上传并完成逐文件大小、MD5 与 SHA-256 校验；下载定位、来源版本、
字节数和哈希统一见[来源交付清单](../source-delivery.json)。访问受限，接收方权限尚未验证；
共享权限未变，尚未发布新版。教材按[本地 PDF 登记](../../references/supplier-knowledge.md#本地教材与原页图片)
接入同一入口，索引可在接收方重建；本地登记不随核心代码包交付。

此前完整候选在 `runtime/misumi/source-pack/` 中内附该来源包，旧 ZIP 保持历史身份。
Drive 资料下载后归回本 skill 的 `sources/misumi/`，教材在 `sources/books/`。
目录索引安装在 `sources/.indexes/misumi/`；Git 和核心 ZIP 排除整个 `sources/`。
默认布局与下载归位见[完整安装说明](../../docs/PORTABLE-DISTRIBUTION.md)。无需独立 MISUMI 应用：

在解压后的 skill 根目录运行：

```bash
python3 runtime/misumi/setup.py install --from sources/misumi \
  --data-root sources/.indexes/misumi
python3 runtime/misumi/setup.py verify --data-root sources/.indexes/misumi
python3 scripts/jev_knowledge.py "直线轴承 旋转限制" --local --json \
  --data-root sources/.indexes/misumi
python3 runtime/misumi/setup.py restore-books --data-root sources/.indexes/misumi
```

安装目标必须是新目录；安装恢复全文索引和来源元数据，PDF 分片与整本 PDF 继续
保持压缩。查询只恢复命中的单页到该资料目录的 `cache/archive/`；`restore-books`
是需要阅读整本时的显式操作。恢复后的原页与整本 PDF 均再次核对原始哈希。
同一文件系统优先硬链接压缩分片，跨文件系统复制。查询历史位于 `var/state/misumi/`，Jev 与原页图片缓存位于 `var/cache/misumi/`；原件与恢复的 PDF 原页位于 `sources/`。这些本地目录均不随核心代码分发。

4,652 页原文件共 9,154,665,613 字节，37 个分片合计 1,273,015,956 字节，
节省 86.09%；该数字不含整本 PDF、索引及代码。两本完整 PDF 的外层无损压缩为
1,009,765,156 字节；包含它们、索引与清单的资料包合计约 2.30 GB。
42 MB 是早期 166 页样本结果，
不能作为全集大小。下载包大小、安装后索引大小、查询缓存大小分别计算。
缓存按实际阅读页增长，不自动删除；读取全部页面仍会占用原页对应空间。

原件无损不表示全文检索能理解全部细节。当前索引来自原生 PDF 文字，27 页文字
稀少；复杂表格、跨栏文本、曲线、公式及图中条件仍需回读原页。图像完整保留，
未声称已完成全量 OCR、公式验证或全部知识凝练。

## 轻量核心与来源库

仅核心包不附原件和全文索引。凝练方法与少量来源绑定示例随核心提供；原目录
按需登记为查证档案。没有登记时全文查询返回 `not_ready`，不能冒称全部目录已凝练。

在 skill 根目录运行，需要 Python 3.10+（含 SQLite FTS5）：

```bash
python3 scripts/jev_knowledge.py --status --json
python3 runtime/misumi/setup.py register \
  --archive sources/ebook-archive --index sources/catalog.sqlite3
python3 scripts/jev_knowledge.py "直线轴承 旋转限制" --local --json
python3 runtime/misumi/setup.py verify
```

默认登记在本 skill 的 `sources/.indexes/misumi/`，不再回退到用户目录的旧登记。
它只记录 archive/index 的位置与来源身份，不复制 PDF，不需要独立 MISUMI 应用。
`--data-root` 可选择当前 skill 内的另一登记目录或自包含资料包；CLI 不接受安装目录外的持久路径。
新登记以相对路径绑定 archive/index，可随整个本地目录搬迁；旧绝对登记移位后需在新目录重新登记，保留旧登记记录供追溯。

`--local` 使用标准库且不联网；状态检查核目录和索引结构，`verify` 流式核查
来源字节及来源/索引关系。在线 Jev 用用户自己的凭据。历史默认写入
`var/state/misumi/`，缓存默认写入 `var/cache/misumi/`；`--state-root` 只能指向当前 skill 内的自定义位置，其缓存位于该位置的 `cache/`。

## 可选的自包含归档导入

需要搬运完整档案时可以导入匹配的原归档与 SQLite；通常只需上面的 register。
导入不是凝练，不用于默认轻量分发。目标必须尚不存在，原文件
保持不变；任何来源哈希或页映射不符均终止，不保留半成品：

```bash
python3 runtime/misumi/setup.py import \
  --archive sources/ebook-archive --index sources/catalog.sqlite3 \
  --data-root sources/.indexes/misumi-imported
```

归档合同为两个 catalogCode `fabiaozhunpin202210`、`fajingjixing202306`：

- `tmp/pdfs/<catalogCode>/metadata.json`：原电子书元数据；
- `tmp/pdfs/<catalogCode>/pages/<pdfUrl>`：每页原 PDF；
- `下载记录/<catalogCode>-manifest.json`：页 ID、序号、文件、URL、bytes、SHA；
- `下载记录/<catalogCode>-assembly.json`：完整 PDF 相对路径、页数、SHA；
- assembly `file` 指向的完整 PDF，及存在时的历史 `qc_report`。

SQLite 的 catalog/page 记录必须匹配上述源身份。导入只选取这些文件，不复制下载
临时环境、缓存或个人配置。`data-manifest.json` 逐文件记录相对路径、bytes、SHA、
媒体类型和角色；没有开发者主目录路径。它是传输完整性清单，不是独立签名或工程验收。

## 重建索引

只有从原 PDF 重建时才需要 [PyMuPDF 固定依赖](requirements.txt)。先把同样的归档
布局置于资料目录 `archive/`，再运行以下命令；重建需可写的资料目录，并先保存
已有 index/manifest 快照。`--reindex` 只重建 SQLite，不会自动更新冻结分发清单。

```bash
python3 -m venv runtime/misumi/.venv
runtime/misumi/.venv/bin/python -m pip install -r runtime/misumi/requirements.txt
runtime/misumi/.venv/bin/python scripts/jev_knowledge.py --data-root sources/.indexes/misumi --reindex
```

固定依赖安装可能联网。新索引应使用 `import` 在新的资料根重新形成来源校验后的
清单，再重新打候选包。普通查询不要求安装 PyMuPDF，也不依赖作者已有虚拟环境。
当前文本索引来自原生 PDF 字符，没有完成全量 OCR、公式求解或结构化知识图谱；
图表、版式、算式和适用条件仍按 [知识采用合同](../../references/supplier-knowledge.md)回读。
