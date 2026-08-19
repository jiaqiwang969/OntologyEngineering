# 《工程本体论》TeX/PDF 构建工厂

本目录保存第一卷可继续维护的出版源；工程本体论-全书.pdf 是构建产物，不是唯一成书证据。

## 内容、排版与执行边界

- main.tex、preamble.tex 与 chapters/*.tex 是装配、样式和正文的 TeX 正本。
- build_handbook.py 先验证正式 source lock、vendored wheel、已安装版本与 PEP 610
  wheel 身份，再从内容哈希已验证的 Semantica chapter package 生成
  fragments/*.tex 与 fragments/INDEX.md。这些 fragment 是可审计的出版快照，
  不应直接手改成第二套内容真相。INDEX 同时记录本次生成所用的 commit、版本、
  wheel 文件名与 SHA-256；staging 模式还记录 descriptor SHA-256 与非权威警告。
- figures/ 保存编译所需的本地图资产；gen_figures.py 与
  make_deck_plan.py 保存图资产和演示计划的作者工具。
- 本书负责人读规格、教学叙事与追责；Semantica package/runtime/runner/receipt
  是唯一可执行语义正本。Protégé、Jena、Fuseki、RDFLib、三元组库与属性图库只可作为
  历史互操作工具或只读 projection adapter，其结果必须回到 Semantica 重绑定、重验证。

## 作者源码锁

卷根 `../authoring-sources.sha256` 逐文件锁定卷根总览、ch01–ch09
读者指南、resources 指南，以及当前可编译出版树中的 TeX 正本、生成 fragment、图资产、
作者工具、本说明与锁测试。条目形如 `README.md`、`ch03-ontology-methodology/README.md`
和 `handbook/chapters/ch03.tex`；绝对路径、`..` 与非规范路径一律拒绝。正式 PDF、缓存和
锁文件自身不进入清单。它证明“本次出版源是什么”，不替代仓库级 Semantica
wheel/source lock。

验证当前树：

    python3 -m unittest -q test_authoring_sources.py

## 重建出版快照

正式模式是默认且唯一可进入 release 候选的模式。先在 ontology-engineering 的受控
runtime 中安装 source-locked Semantica wheel，再从本目录运行：

    ../../../runtime/.venv/bin/python build_handbook.py
    ../../../runtime/.venv/bin/python -m unittest -q test_authoring_sources.py
    latexmk -xelatex -interaction=nonstopmode -halt-on-error main.tex

只有为打破旧 wheel/新实现循环而构建 W0/F0 时，才可显式选择 staging descriptor：

    ../../../runtime/.venv/bin/python build_handbook.py \
      --staging-runtime-descriptor /controlled/staging/semantica-staging-runtime.json

descriptor 必须使用严格 schema `ontology-engineering.semantica-staging-runtime/v1`，并与
同目录 wheel bytes 及当前 venv 中已安装的精确 wheel 一致。该模式不读写正式 source
lock；生成的 INDEX 标为 `staging-non-authoritative`，不得冒充 release fragment。
正式收敛仍须用最终 S1/W1 更新并重装正式 lock 后，以无参数命令重新生成。

修改卷根/章节/resources README、正文或生成 fragment 后，必须从 skill 根用
`scripts/update_book_authoring_locks.py --write` 重新生成卷根 authoring-sources.sha256，
再运行测试。
正式发布还须单独完成 PDF 视觉检查、字体嵌入检查、引用检查与两卷书的 Semantica 门禁；
“能编译”不等于章节 package 已可发布。
