# 私有集成候选与验证范围

本页说明研发用候选构建器；0.7.0 软件交付与安装见[分发说明](PORTABLE-DISTRIBUTION.md)。
候选包状态仍为 `candidate_not_published`，不因公开软件版本存在而取得工程验收。
`CANDIDATE-STATUS.json` 绑定候选清单，
`PORTABLE-MANIFEST.json` 绑定实际 ZIP 字节；哈希不构成可信签名或来源权利放行。

## 共用方法、凝练知识与原件档案

| 层次 | 内容与边界 |
| --- | --- |
| 共用核心 | 一份工程方法/ODP 对齐、Jev 接入、情景与证据合同；CAD、制造和证据模块共用，正式语义仅由 Semantica 执行 |
| 领域知识 | 少量带对象、机理、条件、反例和来源的凝练示例；保留完整原页定位，不宣称全目录已凝练 |
| 查证档案 | 下一版通过 Drive 独立交付原始教材和压缩来源包；保留原图、来源身份与校验清单，下载后接入同一知识入口 |

0.7.0 分为完整代码／方法 ZIP 与独立来源组件。来源已通过 gws 上传并完成逐文件大小、
MD5 与 SHA-256 校验，下载定位、来源版本、字节数和哈希统一见[来源交付清单](../runtime/source-delivery.json)。
访问受限，接收方需具有资料库权限；本次软件发布未更改共享权限。此前内附
米思米压缩资料的冻结候选 ZIP 保持不变；其体积和验证不代表新版核心交付。

领域包引用共用模式，不能复制一套互相漂移的 ODP/Jev 判断规则。相同陈述可合并并
保留多处出处；条件或对象不同必须保留分支。示例及模型输出仍需主 agent 复核，
不能自动晋升 TBox。[直线运动凝练示例](../examples/knowledge-distillation/README.md)
仅覆盖两页八个原文片段，说明怎样保留条件、反例、未知和共用模式引用。

压缩归档与知识凝练分别验收。完整资料保留 4,652 个独立原 PDF 页以及两本合并 PDF，
没有降采样、PDF 重编码、裁剪或重绘。关键图、表、公式、图例和单位仍需回读原页；
全文索引不等于理解全部图示或完成全量 OCR。书源方法记录章节和来源身份，
两卷工程书正文未附，不能称为已读取包内原文。版本范围见[下一版草案](releases/mechanical-design-next.md)。

## 构建

在维护者完整源码工作区使用同一构建器，默认仍是方法、工具与少量凝练示例：

```bash
python3 scripts/build_engineering_candidate.py --refresh-ledger
python3 scripts/build_engineering_candidate.py --output var/builds/new-core-candidate.zip
```

需要复验既有内附资料布局时，构建器仍支持显式选择同一来源包；下一版默认核心
构建不加 `--source-pack`，资料独立交付：

```bash
python3 scripts/build_engineering_candidate.py --refresh-ledger --source-pack /path/to/source-pack
python3 scripts/build_engineering_candidate.py --source-pack /path/to/source-pack \
  --output var/builds/new-integrated-candidate.zip
```

候选仅纳入来源清单声明的 `source-pack.json`、metadata、各页分片与整本 PDF 归档，
目标为 `runtime/misumi/source-pack/`；不复制生产临时文件或展开的 9.15 GB 页面。
冻结清单记录相对路径、角色、字节数和 SHA，不带生产者的绝对路径。
每次构建重新核对字节、隐私、链接与语义包锁，流式 ZIP64 写入已压缩归档，
漂移会失败并移除未完成 ZIP。原件的第三方来源和权利不因集成而改变。

默认模式不会继承旧大数据清单；原 `runtime/misumi/data/` 不存在也能构建。
仅维护者兼容旧展开布局时可在两步都显式加 `--include-source-data`，与 `--source-pack`
互斥，缺旧数据即失败。旧 0.6.0 审批字节保持不变，不追认当前候选。

## 接收方验收

解压核心 ZIP 后，将 Drive 原件归回本 skill 的 `sources/` 并核对交付清单；
Git 与核心打包排除该目录，本地代码和资料形成一个整体。完整位置与六书登记命令
见[分发说明](PORTABLE-DISTRIBUTION.md)；不展开所有 PDF。在 skill 根目录执行：

```bash
python3 scripts/package_skill.py
python3 runtime/misumi/setup.py install --from sources/misumi \
  --data-root sources/.indexes/misumi
python3 runtime/misumi/setup.py verify --data-root sources/.indexes/misumi
python3 scripts/jev_knowledge.py "直线轴承 旋转限制" --local --json \
  --data-root sources/.indexes/misumi
```

安装到新资料目录，只展开索引与元数据；普通查询仅恢复命中原页，逐页核对原 SHA。
需要阅读整本时显式运行 `python3 runtime/misumi/setup.py restore-books --data-root sources/.indexes/misumi`。
全部来源位置、缓存增长和原文索引局限见[资料说明](../runtime/misumi/README.md)。
仅核心包也可登记既有原件；缺资料时返回 `not_ready`。无需外置 MISUMI 应用；
资料下载并登记后，本地查询和原页查看不要求持续连接 Drive。

本地查询需要 Python 3.10+、SQLite FTS5，`--local` 不调用 Jev 或声称已完成适用性判断。
在线 Jev 要求接收方自己的凭据和网络；索引重建才需固定 PyMuPDF。Jev 浏览器要求
Python 3.12+，NX 软件/许可/NXOpen 和执行主机外部提供。正式 Semantica 执行仍需
锁定运行时、项目绑定及实际证据。依赖安装可能联网。

[整体迭代记录](ENGINEERING-ITERATIONS.md)逐轮记录问题、修复和复验。
文件完整、模型选择、工程适用、原生回读、实物验收及发布分别报告；软件候选通过
不能证明整条机械设计链已完成原生 NX 或实物验收。
