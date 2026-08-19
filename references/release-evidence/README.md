# 两卷 artifact v1：技术候选证据

本目录保存 `references/book-release-artifacts.json` 引用的机器可读证据。v1 **只能形成
technical candidate**：它证明被绑定字节、技术检查与人工视觉审阅的状态，但无签名 JSON
永远不能授予版权、风险、合规或公开发布权限。`--claim-release` 是保留的 fail-closed
参数，调用必定失败；v1 manifest 的 `artifact_status` 始终是 `candidate`。

## 收敛命令

从 ontology-engineering 根目录运行；`/controlled/semantica/checkout` 必须是
`runtime/semantica-source-lock.json` 所锁 commit 的**干净、精确** Semantica checkout：

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

`--oe-source-commit` 必须是当前 HEAD 的祖先，并代表候选的 OE 源码边界。从该 commit 到
当前 worktree，除了脚本定义的固定生成物（两卷 PDF、证据/日志、manifest 与 sidecar）外，
不得有 tracked 或 untracked 漂移。

## 治理边界

首次运行 `governance` 时会同时初始化两个 `pending` 记录。若两个现有文件都是合法的
`pending`/`blocked` 记录，命令原样保留；只存在一个、格式不合法或尝试写成 `approved`
都会 fail closed。只有明确运行下列命令才会把两个现有记录一起重置为 `pending`：

```bash
runtime/.venv/bin/python scripts/collect_book_release_evidence.py governance \
  --reset-existing
```

重置不是批准。rights 和 publication 两个 blocker 在 v1 中始终保留，必须由未来具备可信
身份/签名机制的外部授权流程处理，不能靠编辑本目录 JSON 消除。

## 验证含义

章节 package 清单只从 source lock 指向的**精确 wheel 字节**读取 `status` 和
`release_status`，并逐字节核对 wheel `RECORD`、29 个固定章节坐标、manifest、声明资产及
目录闭包。book artifact v1 不携带、推断或接受 package receipt 和 gate verdict；这些属于
Semantica 自身的 package 生命周期，不是这个静态候选 manifest 的授权证据。

JSON 必须使用 UTF-8、键排序的紧凑 canonical encoding 并以单个 LF 结尾，manifest 另有
`.sha256` sidecar。`create` 会写入后立即验证，`verify` 会重新核对作者锁、book binding、
runtime identity、隐私、PDF/字体/视觉记录、wheel 与全部引用哈希，并重跑固定的 OE 和
Semantica pytest 命令，比较测试数量。两条命令都要求 Semantica checkout HEAD 等于 source
lock 且 worktree 干净；OE 则必须满足上述 source closure。
