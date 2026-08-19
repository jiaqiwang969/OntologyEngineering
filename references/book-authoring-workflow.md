# 两卷书作者与 Semantica 收敛工作流

本工作流只在用户明确要求修改、重写、校正或重新出版两卷书时使用。普通行业本体
delta 不自动进入书稿；书只吸收通用方法、稳定教学命题、ISO 推演修正或真实能力边界。

## 正本角色

| 卷 | 人写内容正本 | 生成内容 | 正式构建产物 |
|---|---|---|---|
| 第一卷《工程本体论》 | 卷根/九章/resources README，`handbook/main.tex`、`preamble.tex`、`chapters/*.tex`、图与作者工具 | `fragments/*.tex`，必须从 source-locked Semantica 生成 | `工程本体论-全书.pdf` |
| 第二卷《产品可信工程》 | `front-matter/preface.md`、20 个 `chapter.md`、4 个 appendix Markdown；`handbook/main.tex`、preamble、metadata | `handbook/fragments/*.tex`，必须由 `build_handbook.py` 生成 | `产品可信工程-全书.pdf` |

不得直接手改生成 fragment 来建立第二套内容真相。PDF 是出版产物，不替代 TeX、
Markdown、图源、作者工具和哈希锁。

第一卷卷根 `authoring-sources.sha256` 的条目以第一卷卷根为基准，因此同一作者锁
同时约束读者指南与全部 handbook 构建输入。第二卷 `handbook/current-source.sha256`
约束正文/装配源，`handbook/formal-search-guides.sha256` 另行约束卷根 README、命题索引、
handbook README 与 20 章 usage guide；任何路径都必须是无 `..` 的规范 POSIX 相对路径。

## 修改前

1. 从 skill 文件自身位置确定仓库根；不要写死某个用户目录。
2. 运行 `bash runtime/setup_runtime.sh`，验证 vendored wheel 与 source lock。
3. 识别修改属于 `none`、`vol1-method`、`vol2-iso-exemplar` 或同时影响两卷的 `both`；
   该四值枚举必须与 PackageDelta 顶层 `book_impact` 完全一致。
4. 找到受影响的稳定命题、章节 package、CQ/scenario、书源锚点和权利状态。
5. 若变更改变可执行语义，先在 Semantica 中形成 package candidate；不得在 OE
   旁边新增 ontology/query/shape/case/rule 副本。

## 第一卷

在 `references/ontology-engineering-book/handbook/` 修改人写 TeX 后：

```bash
../../../runtime/.venv/bin/python build_handbook.py
cd ../../..
runtime/.venv/bin/python scripts/update_book_authoring_locks.py --write
runtime/.venv/bin/python scripts/update_book_authoring_locks.py
cd references/ontology-engineering-book/handbook
../../../runtime/.venv/bin/python -m unittest -q test_authoring_sources.py
latexmk -xelatex -interaction=nonstopmode -halt-on-error -file-line-error main.tex
```

`build_handbook.py` 默认生成的 fragment 必须来自已安装、source-locked 的 Semantica wheel；
它会在清理旧 fragment 前验证正式 lock、vendored wheel、installed version 与 PEP 610
wheel 身份。只有 W0/F0 staging 收敛可显式传入
`--staging-runtime-descriptor /controlled/path/semantica-staging-runtime.json`；该模式不会更新
正式 lock，INDEX 必须记录 `staging-non-authoritative`、commit、version、wheel SHA-256
和 descriptor SHA-256。
正式 PDF 写回前先保留 `main.pdf` 做视觉、字体、链接、引用和文本抽取检查；只有检查
通过后才替换正式中文文件名的 PDF。

## 第二卷

在人写 Markdown 或 TeX 装配正本修改后，从第二卷目录运行：

```bash
../../runtime/.venv/bin/python handbook/build_handbook.py
cd ../..
runtime/.venv/bin/python scripts/update_book_authoring_locks.py --write
runtime/.venv/bin/python scripts/update_book_authoring_locks.py
cd references/product-trustworthiness-book
../../runtime/.venv/bin/python handbook/build_isolated.py --verify-only
PYTHONPATH=. ../../runtime/.venv/bin/python -m pytest -q handbook/test_build_handbook.py
../../runtime/.venv/bin/python handbook/build_isolated.py \
  --output handbook/产品可信工程-全书.pdf
```

默认隔离构建只消费当前 clone 中锁定的 31 项源码和 49 项出版资产。历史母版只能通过
显式 `--audit-provenance PATH` 做只读来源核对，不是默认构建依赖。
第二卷正式检索指南锁也必须与本次 guide 文本一起更新；检索会在每次读取同一批字节时
逐项核对 SHA-256，指南漂移不会退回未锁定 allowlist。

## 双仓收敛顺序

把一次影响书与执行语义的修改作为同一候选事务。该事务必须明确分成
staging 收敛和 final reproducibility 两段，不能用旧 locked wheel 的生成结果要求
新实现“无 diff”：

下面 W0/W1→F0/F1/F2 的 wheel-fragment 循环只适用于第一卷，因为只有第一卷的
生成 fragment 直接读取 Semantica wheel。第二卷 fragment 只由作者锁中的 Markdown
与 TeX 工厂确定；第二卷仍要完成 source/book binding、作者锁、PDF 与发布元组收敛，
但不得伪造一条“由 wheel 生成第二卷 fragment”的依赖。

```text
冻结命题/CQ/来源/权利
  → Semantica package candidate、完整回归与审阅后的 candidate commit S0
  → 从 S0 构建受控 staging wheel W0（不更新 OE 正式 source lock）
  → 用 W0 生成第一卷新实现的 authoritative fragments F0
  → 修改人写书源、更新作者锁并完成 guide/primary/TeX/book asset rebind
  → Semantica package verify-books，审阅并冻结最终 Semantica commit S1
  → 在两个干净构建环境中从同一 S1 构建 final wheel W1a / W1b
  → 分别用 W1a / W1b 对同一第一卷书源生成 fragments F1 / F2
  → 要求同源 F1 = F2，并核验 wheel 的可复现身份
  → 选定经验证的 final wheel，更新 OE source lock 并强制重装
  → 构建和检查 PDF
  → 运行技术、隐私、权利与发布门禁
```

`F0` 是打破旧 wheel/新语义循环的受控 staging 产物；它与旧 locked wheel
产物不同是预期的，也不得要求 `F0` 与最终 rebind 之后的 `F1` 相同。
“无 diff”只适用于同一 `S1`、同一书源、同一生成合同下的 `F1/F2`。
任何 staging wheel 都不得被记为正式 OE source lock，也不得对外冒充 release wheel。

Builder 在第一次读取 Semantica asset 之前必须显式调用
`verify_runtime_source_identity()`。正式模式不带参数；staging 模式只接受
`staging_descriptor=/controlled/path/semantica-staging-runtime.json`。描述符只允许：

```json
{
  "$schema": "ontology-engineering.semantica-staging-runtime/v1",
  "commit": "0123456789abcdef0123456789abcdef01234567",
  "version": "0.0.0+controlled-staging",
  "wheel_filename": "semantica-0.0.0+controlled_staging-py3-none-any.whl",
  "wheel_sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
}
```

Wheel 必须是描述符同目录的普通文件；描述符 schema/字段、wheel 字节哈希、
installed version 和 pip PEP 610 archive SHA 任一不一致都 fail closed。
Staging 验证不读写正式 source lock。

两仓与两卷完成技术收敛后，book artifact v1 至少绑定以下元组：

```text
OE source commit/tree + 两卷作者锁/PDF hash
+ Semantica source lock/commit/version + exact wheel hash
+ 29 个 package IDs/versions/status/release_status/manifests/assets
+ 作者锁、book binding、runtime、隐私、PDF QA 与两仓固定回归证据
+ rights/publication 的 pending 或 blocked 治理记录
```

package 的 `status`/`release_status` 只能从 exact locked wheel 的 manifest 读取并复验。
book artifact v1 不携带 receipt 或 gate verdict，也不得用外部 JSON 补写它们；receipt、
regression gate 与 release gate 属于 Semantica package 自身的治理生命周期。缺少上述任一项、
技术验证失败，或任何 package 未达到 `status=complete` 且 `release_status=complete` 时，都
必须如实保留 blocker。

v1 始终是**技术候选**，不是公开发布凭证。无签名 JSON 不能批准 rights/publication；两项
治理状态只接受 `pending` 或 `blocked`，且确定性 blocker 永远存在。`governance` 在文件不存在
时初始化两个 `pending` 记录，已有合法记录默认原样保留；只有显式
`governance --reset-existing` 才会把两项一起重置为 `pending`，重置本身仍不是批准。

## Book artifact v1 收敛

先冻结所有非生成源文件并取得其 OE commit，再只生成脚本允许的固定 PDF、证据、日志、
manifest 与 sidecar。`--oe-source-commit` 必须是当前 HEAD 的祖先；其后的任何其他 tracked
或 untracked 漂移都会破坏 source closure。Semantica checkout 必须精确位于 source lock
指定 commit 且 worktree 干净。

从 OE 根运行：

```bash
runtime/.venv/bin/python scripts/collect_book_release_evidence.py governance
runtime/.venv/bin/python scripts/collect_book_release_evidence.py static
runtime/.venv/bin/python scripts/collect_book_release_evidence.py book-bindings
runtime/.venv/bin/python scripts/collect_book_release_evidence.py regressions \
  --semantica-root /controlled/semantica/checkout
runtime/.venv/bin/python scripts/collect_book_release_evidence.py pdf-qa \
  --visual-review references/release-evidence/pdf-visual-review.json
runtime/.venv/bin/python scripts/book_release_artifacts.py create \
  --oe-source-commit 0123456789abcdef0123456789abcdef01234567 \
  --semantica-root /controlled/semantica/checkout
runtime/.venv/bin/python scripts/book_release_artifacts.py verify \
  --semantica-root /controlled/semantica/checkout
```

`create` 写入 candidate manifest/sidecar 后立即验证；`verify` 重放固定 OE 与 Semantica
pytest 命令并比较记录的测试数量，同时复验 source closure、干净精确的 Semantica checkout、
锁、PDF、wheel 和引用字节。完整证据合同见
[`release-evidence/README.md`](release-evidence/README.md)。

## 最终检查

从 OE 根运行：

```bash
runtime/.venv/bin/python scripts/rebind_semantica_books.py \
  --book-root . --semantica-root /explicit/complete/semantica/checkout
runtime/.venv/bin/semantica package verify-books --book-root . --json
runtime/.venv/bin/python scripts/check_semantica_backend_policy.py \
  --root . --policy runtime/semantica-backend-policy.json --mode strict --json
runtime/.venv/bin/python -m pytest -q tests
```

OE 书包和 Semantica package 是两个独立发布面；向获授权的 Semantica fork 推送代码，
不自动授权公开两卷书、图片、PDF 或标准派生表达。Semantica package 达到
`release_complete` 也不会把 book artifact v1 从 `candidate` 变成公开发布物。
