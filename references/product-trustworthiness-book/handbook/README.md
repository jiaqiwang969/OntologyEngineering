# 《产品可信工程》TeX/PDF 构建工厂

本目录保存第二卷自己的可继续维护的出版源，不再把仓外 PDF 当作唯一成书证据。

## 内容与执行边界

- `../front-matter/preface.md`、20 个正式章节各自的 `chapter.md`、`../appendices/appendix-*.md` 是内容正本；正式章节缺少 `chapter.md` 时构建立即失败，绝不回退到 README。
- `build_handbook.py` 只做确定性的 Markdown→TeX 转换；`fragments/*.tex` 与 `fragments/INDEX.md` 是生成的、可审计的出版快照，禁止直接手改成第二套内容真相。
- `main.tex`、`preamble.tex`、`book-metadata.tex` 是排版与装配正本。
- 本书是供人阅读、评审和追责的规范；Semantica 是全书唯一可执行语义。章节包是否已可执行或发布，必须以 Semantica 注册表、验收收据和发布记录为准，不能由本书版次反推。

## 当前 clone 的三道哈希锁

干净 clone 已包含完整 TeX、Markdown 和约 95 MB 的实际使用图像，不需要另找母库：

- `current-source.sha256` 锁定 25 个成书 Markdown 正本、当前重写后的 5 个 TeX/转换/测试工厂文本，以及正式隔离复现门禁 `build_isolated.py`；
- `authoring-assets.sha256` 锁定 42 幅仓内 PNG、6 幅仓内附录 PDF 和 1 份渲染清单。
- `formal-search-guides.sha256` 锁定卷根 README、命题索引、本说明和 20 章 usage guide，
  供正式检索在读取每一批字节时核对内容身份；它不替代正文锁。

默认隔离构建先要求源码与资产两个清单“无缺项、无越界、无哈希漂移”，再把仓内输入
复制到 `mktemp`。正式问答检索另外要求 guide 锁 23 项精确闭合且逐字节哈希一致。
因此构建和检索各自验证并消费当前仓库内容，而不是某个机器上碰巧存在的旧母版或锁外
导读 allowlist。

二进制出版资产位于以下两类相对路径；完整逐文件清单见资产锁：

- `handbook/figures-imagegen/*.png`
- `handbook/figures-rendered/*.pdf` 与 `handbook/figures-rendered/.render-manifest.json`

## 生成 TeX 快照

当锁定图资产已装入本目录时，从书目录运行：

```bash
python3 handbook/build_handbook.py
```

该命令重建 `handbook/fragments/`。内容变更后必须重跑，并用测试阻止陈旧快照进入发布。

## 自包含隔离 PDF 构建

只校验当前 clone：

```bash
python3 handbook/build_isolated.py --verify-only
```

构建但不持久写回（临时目录在成功后自动清理）：

```bash
python3 handbook/build_isolated.py
```

只有显式给出 `--output` 才会把 PDF 写到指定位置：

```bash
python3 handbook/build_isolated.py \
  --output handbook/产品可信工程-全书.pdf
```

脚本在临时目录中复制 OE 内通过哈希核验的内容正本、TeX 工厂、图和 manifest，然后运行片段生成器与 `latexmk -xelatex`。需要保留现场诊断时才使用 `--keep-workdir`。

`authoring-provenance.sha256` 只保存最初取材母版的历史对照语义。只有明确需要做来源审计时才显式提供只读母版路径：

```bash
python3 handbook/build_isolated.py --verify-only \
  --audit-provenance /path/to/historical-authoring-root
```

该可选审计不向母版写入，也不是默认构建的前置条件。

## 工厂单测

```bash
PYTHONPATH=. pytest -q handbook/test_build_handbook.py
```

测试覆盖仓内源码/资产锁、无环境变量的默认隔离路径、哈希漂移拒绝和 TeX 快照再现。发布前必须让图像全集成测试不跳过，并重建 TeX 快照与 PDF。
