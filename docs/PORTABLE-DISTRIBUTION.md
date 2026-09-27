# 完整本地 skill：下载资料放在哪里

0.7.0 提供完整的软件与方法分发。代码经 Git／ZIP 交付，大资料经
Drive 传输；**下载后归回当前 skill 的 `sources/`，形成完整本地目录**。`.gitignore`
和核心打包器排除 `sources/` 和 `var/`，避免原件、项目、索引和缓存随代码上传 GitHub。
所有本 skill 自有的文件都位于这个根目录；不再使用 `~/.local` 下的外置数据目录。

## 默认目录

安装到 `~/.codex/skills/ontology-engineering/` 时，布局如下；程序从自身位置确定根目录，
不写死安装者的主目录。

```text
ontology-engineering/
├── SKILL.md
├── ontology_engineering/         共用实现
├── skills/                       CAD、制造成本、证据等模块
├── runtime/source-delivery.json  链接、大小、SHA、六书导航
├── var/                          本机私有文件，Git 和核心 ZIP 忽略
│   ├── projects/                 项目、对象版本、上下文、CAD 与工程证据
│   ├── state/misumi/             查询历史与判断记录
│   ├── cache/misumi/             Jev 缓存和按需生成的原页图片
│   ├── maintenance/              检查、迁移及维护证据
│   ├── builds/                   构建暂存和候选 ZIP
│   └── legacy/                   有保留价值的旧记录，不作为当前实现
└── sources/                      本地资料，Git 和核心 ZIP 忽略
    ├── books/
    │   ├── vol1.pdf              工程本体论
    │   ├── vol2.pdf              产品可信工程
    │   ├── roloff-matek.pdf
    │   ├── norton.pdf
    │   ├── interchangeability.pdf
    │   └── dfma.pdf
    ├── misumi/
    │   ├── source-pack.json
    │   ├── catalog-and-metadata.tar.xz
    │   ├── full-pdfs.tar.xz
    │   └── segments/             37 个原页 tar.xz 分片
    └── .indexes/                 接入时生成，随本地目录搬迁
        ├── local-pdf/            六书文字索引和相对路径登记
        └── misumi/               目录索引、压缩分片及原页缓存
```

[Drive 资料库](https://drive.google.com/drive/folders/1QoNTu0gy9aPwQ-Sk6y5IWSBcVUCDVdXK)
共 47 个上传文件、约 3.16 GB：46 个来源文件及一份上传清单。
[交付清单](../runtime/source-delivery.json)是程序的定位和校验依据；Drive 上传清单可保留
对账，不登记为教材。访问受账号权限限制，接收方权限尚未验证。

## 下载、归位与校验

以下命令在 skill 根目录运行。列出文件、链接与默认目标位置，不联网：

```bash
python3 scripts/source_library.py list
```

浏览器下载后只解开 Drive 生成的外层 ZIP，保留内部 `tar.xz`。下面命令识别清单目录
或扁平文件名，逐件核 SHA 和大小，再放入相应子目录；保留原下载目录，不覆盖不同字节：

```bash
python3 scripts/source_library.py import --from /path/to/unzipped-drive-folder
python3 scripts/source_library.py verify
```

也支持已登录的 gws 按清单直接下载，必须显式选择范围：

```bash
python3 scripts/source_library.py fetch --select books
python3 scripts/source_library.py fetch --select misumi
```

单书可选 `roloff-matek`、`norton`、`interchangeability`、`dfma`、`vol1`、`vol2`。
gws 使用接收者的登录状态；程序不导出凭据、不改共享权限。远端下载仍需在接收方环境
验收，本地模拟和既有上传校验不能代替它；权限或网络不通时可用浏览器下载后导入。

## 登记与查询

教材登记需要 Python 3.11+、SQLite FTS5 和 Poppler 的 `pdfinfo`／`pdftotext`；图片查看
另需 `pdftoppm`。六书导航随清单交付，用户无需手填章节 JSON。

```bash
python3 scripts/source_library.py register
python3 runtime/misumi/setup.py install --from sources/misumi --data-root sources/.indexes/misumi
python3 runtime/misumi/setup.py verify --data-root sources/.indexes/misumi
python3 scripts/jev_knowledge.py --status --sources all --json
python3 scripts/jev_knowledge.py "轴承配合与装配条件" --sources all --local --render 1
```

`register` 默认登记六书；部分安装时显式 `--select`，缺件会报告。已有索引需显式
`--replace-index` 才能重建；它替换整个选定书目集合，因此应选定希望保留的全部书。
米思米安装目标须为新目录，已有目录先 `verify`。安装只恢复索引和元数据，同盘优先
硬链接压缩分片；查询按需恢复原页。显式恢复两本完整目录：

```bash
python3 runtime/misumi/setup.py restore-books --data-root sources/.indexes/misumi
```

教材新登记使用相对路径，整体移动 skill 时带上 `sources/` 和 `var/`。自定义
`--root`、`--pdf-root`、`--data-root` 和 `--state-root` 也必须位于当前 skill 内；
不再自动使用外部旧索引。默认查询历史在 `var/state/misumi/`，Jev 缓存和原页图片在
`var/cache/misumi/`。项目上下文、证据与报告放入 `var/projects/<项目>/`。
历史查询保留当时记录的绝对位置；迁移清单记录新旧目录对应关系，新查询生成当前位置引用。
`--view-record` 可按当前已登记的来源和页身份重新定位；只有原件与正文哈希等身份核对
一致才返回原页，不改写旧记录。未登记对应来源或字节不符时明确拒绝。
原件、历史与工程事实不因移动而改写或获得新的批准。

维护时把有价值的旧目录实际迁回 `var/maintenance/` 或 `var/legacy/`，核对文件身份后
移走外部旧目录；不要留下指向外部的符号链接。历史快照中的 `SKILL.md` 无损存为
`SKILL.md.gz` 并记录原文 SHA，防止被发现为重复技能；读取历史时直接解压读取字节，
不在安装目录展开为另一套活动入口。构建暂存完成后清理，只保留候选 ZIP。
浏览器下载文件和用户提供的原件可以作为
临时导入来源，但登记后的运行不得依赖它们。系统 Python、Poppler、NX 与账号认证仍由
运行环境提供；本规则约束 skill 自有数据，不要求复制其他应用或全局登录配置。

`--local` 不调用 Jev、不声称适用性已通过。在线模式用接收者自己的凭据。
[知识入口](../references/supplier-knowledge.md)保留原页 SHA、物理页与图像身份；PDF 登记
不替代作者源码校验、全文本体化或工程采用。页面缓存按阅读量增长，全部展开会增加占用。

## 核心构建与运行环境

发布版本按已核查的逐文件清单构建；在源码仓库运行：

```bash
python3 scripts/build_shareable_core.py --output var/builds/ontology-engineering-core-v0.7.0.zip
```

继续研发时可另建私有候选；候选状态和已发布版本分别保留：

```bash
python3 scripts/build_engineering_candidate.py --refresh-ledger
python3 scripts/build_engineering_candidate.py --output var/builds/new-core-candidate.zip
```

`sources/` 与 `var/` 排除在核心之外；旧 `--source-pack` 内附布局保留为显式兼容选项。每个 ZIP
含字节清单，解包后可用 `python3 scripts/package_skill.py` 核对。容量分别报告核心 ZIP、
下载原件、安装依赖和页面缓存。语义传输包与锁定 wheel 仍随核心交付。

按任务准备环境，不复制维护者虚拟环境：

```bash
bash runtime/setup_runtime.sh --preflight
bash runtime/setup_runtime.sh
bash runtime/setup_runtime.sh --doctor
bash skills/cad-agent/setup.sh
bash skills/cad-agent/doctor.sh --json
bash runtime/jev-ultrafast/setup.sh
python3 scripts/jev_browser.py doctor
```

首次依赖安装可能联网。NX 软件、许可、NXOpen 和执行主机由接收者提供，见
[NX 指南](../skills/cad-agent/references/nx-execution.md)；浏览器使用已授权的连接与账号。
Semantica 是唯一正式语义执行器，仍需项目绑定和本轮证据。工程结果、语义结果与发布状态
分别报告；下一版范围见[升级草案](releases/mechanical-design-next.md)。
