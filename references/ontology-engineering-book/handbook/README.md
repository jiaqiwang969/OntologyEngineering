# 《工程本体论》TeX/PDF 构建工厂

本目录保存第一卷可继续维护的出版源；工程本体论-全书.pdf 是构建产物，不是唯一成书证据。

## 内容、排版与执行边界

- main.tex、preamble.tex 与 chapters/*.tex 是装配、样式和正文的 TeX 正本。
- build_handbook.py 从内容哈希已验证的 Semantica chapter package 生成
  fragments/*.tex 与 fragments/INDEX.md。这些 fragment 是可审计的出版快照，
  不应直接手改成第二套内容真相。
- figures/ 保存编译所需的本地图资产；gen_figures.py 与
  make_deck_plan.py 保存图资产和演示计划的作者工具。
- 本书负责人读规格、教学叙事与追责；Semantica package/runtime/runner/receipt
  是唯一可执行语义正本。Protégé、Jena、Fuseki、RDFLib、三元组库与属性图库只可作为
  历史互操作工具或只读 projection adapter，其结果必须回到 Semantica 重绑定、重验证。

## 作者源码锁

authoring-sources.sha256 逐文件锁定当前可编译出版树：TeX 正本、生成 fragment、
图资产、作者工具、本说明与锁测试。路径都相对于本目录；正式 PDF、缓存和锁文件自身
不进入清单。它证明“本次出版源是什么”，不替代仓库级 Semantica wheel/source lock。

验证当前树：

    python3 -m unittest -q test_authoring_sources.py

## 重建出版快照

先在 ontology-engineering 的受控 runtime 中安装 source-locked Semantica wheel，
再从本目录运行：

    python3 build_handbook.py
    python3 -m unittest -q test_authoring_sources.py
    latexmk -xelatex -interaction=nonstopmode -halt-on-error main.tex

修改正文或重新生成 fragment 后，必须重新生成 authoring-sources.sha256，再运行测试。
正式发布还须单独完成 PDF 视觉检查、字体嵌入检查、引用检查与两卷书的 Semantica 门禁；
“能编译”不等于章节 package 已可发布。
