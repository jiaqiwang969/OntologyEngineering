# cad-agent Fusion 作业工具族(提案)

把 2026-09-05/06 手搓出来的一次性驱动脚本产品化,使一次「大 STEP → Fusion 装配」
作业能够**无人值守**跑完:一次调用一个受保护 cell、每次调用留回执、可续跑、
传输安全按构造保证、失败分类后只重试真正可重试的那一类。

> 状态:**提案**。本目录不是正本。合入前请与 A(census 逻辑)、B(预算/休眠)、
> C(caller/传输)的结论对齐;`scripts/_common.py` 里的结果分类法是**本地占位**,
> C 的 caller taxonomy 落地后应当替换掉 `classify_call()` 与 `RETRY_KINDS`。

安装位置(建议):整个 `scripts/` 拷进 `~/.codex/skills/cad-agent/scripts/`,
`doctor.sh` 覆盖同名文件,`references/fusion-execution.md` 追加 `RUNBOOK_DELTA.md`
的内容。

---

## 工具一览

| 工具 | 作用 | 是否会调用 Fusion |
|---|---|---|
| `fusion_preflight.py` | 作业前只读体检:端口归属、守卫 marker、Peekaboo 模式探测与推荐、窗口普查、电源断言、磁盘 | 否(`--health-probe http` 仅打普通 `GET /health`,不是 MCP 请求) |
| `fusion_step_split.py` | 大 STEP 按族正则切块 + 绝对路径 manifest + 唯一性/体积闭合校验 | 否(纯 CadQuery/OCP) |
| `fusion_step_import.py` | 按 manifest 逐块导入并在同一脚本内保存;present-skip、可续跑、看门狗重试 | 是(每块一次受保护调用) |
| `fusion_document_f1.py` | F1 门:精确重名检查 → 建空文档 → save-as → 新会话回读,产出 lineage | 是(4 次) |
| `fusion_post_import.py` | 回读 → 用户参数 → as-built 关节 → 全组件关节普查 → close/reopen | 是(每阶段 1 次) |
| `fusion_supplier_occurrences.py` | 供应商 STEP 原件按 4×4 位姿放置为命名 occurrence | 是(每件 1 次) |

所有工具共享 `scripts/_common.py` 与 `scripts/templates/*.py.tmpl`。
模板里**没有任何项目专有身份**(测试会强制检查不含 `FA04_` / `urn:adsk`)。

---

## 三条按构造保证的传输规则

这三条对应今天三次「结果看着正常、其实没执行」的失败:

1. **ASCII-only 字面量。** 任何离开本进程的脚本都过 `assert_transport_safe()`:
   出现 >127 的字符直接抛错。路径用 `py_str()` 渲染成 `'天花...'`。
   原因:原始 UTF-8 路径让 `createSTEPImportOptions` 报
   `3 : The selected file does not exist.`(传输层 mojibake)。
   注意不要**二次转义**——`py_str()` 内部已经做了 `backslashreplace`,
   再包一层 `ascii_escape()` 会得到字面量反斜杠(`test_py_str_is_not_double_escaped` 就是钉这个)。
2. **多行短字面量。** 生成的数组/字典一律 `py_json(..., indent=1)`,并且任何一行
   超过 `MAX_SCRIPT_LINE = 400` 直接拒绝。原因:一个 ~7 KB 的单行 `PARAMS`
   让 Fusion 2705 返回 delivered / success=true / message "" / 29 ms —— 脚本
   根本没跑。
3. **「空 message + 极短耗时」= 没跑,不是成功。** `classify_call()` 把
   `message == ""` 且 `duration_ms < 1500` 归类为 `script-no-output`,
   绝不当作 ok。

---

## 结果分类法与重试策略

`_common.classify_call()` 把一次 `fusion_call.py` 调用映射为:

| kind | 含义 | 处置 | 退出码 |
|---|---|---|---|
| `ok` | delivered 且脚本 `success: true` | 继续 | 0 |
| `inner-error` | 到达 Fusion,业务失败(lineage 不符、脏文档…) | fail-fast | 3 |
| `script-no-output` | delivered 但脚本没输出且耗时极短 | fail-fast(**不要**当成功) | 4 |
| `guard-transient` | 上游请求发出**之前**的守卫拒绝(窗口普查、辅助窗不稳定、socket 巡检失败…) | 等 45 s 重试,最多 `--max-transient-retries` 次 | 6 |
| `guard-terminal` | PID 身份变了 / in-flight 可能已提交 / 被隔离 | 停,该 PID 不得再收任何 MCP 请求 | 8 |
| `transport-failure` | 传输/会话层失败 | 停 | 2 |
| `timeout` | 上游超时或 caller 墙钟超时 | 停(对该 PID 是终局) | 7 |
| `refused-busy` | 另一个 cell 在跑 | 停 | 9 |

**只有 `guard-transient` 会重试**,因为只有它保证没有任何上游副作用。

---

## 典型无人值守链路

```bash
SK=~/.codex/skills/cad-agent/scripts
CQ=/Users/<user>/106-链条传动/motion_analysis/cadquery_export/.venv/bin/python

# 0. 体检(阻断项 -> 退出码 3,不要开始作业)
python3 $SK/fusion_preflight.py --expect-doc "$DOC" || exit 3

# 1. 切块(不碰 Fusion)
"$CQ" $SK/fusion_step_split.py IN.step OUT_DIR \
      --max-bodies 20 --families families.json

# 2. 建文档(拿到 lineage)
python3 $SK/fusion_document_f1.py --name "$DOC" --out-dir ./job \
      --description "..."
LIN=$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["document"]["lineage"])' \
      "./job/$DOC.F1-document-identity.json")

# 3. 导入(自带 caffeinate、preflight、present-skip、续跑)
python3 $SK/fusion_step_import.py --manifest OUT_DIR/chunks.manifest.json \
      --doc-name "$DOC" --lineage "$LIN" --out-dir ./job --activate --caffeinate

# 4. 参数 / 关节 / 普查 / 重开
python3 $SK/fusion_post_import.py --doc-name "$DOC" --lineage "$LIN" \
      --out-dir ./job --parameters params.spec.json --joints joints.spec.json

# 5. 供应商件
python3 $SK/fusion_supplier_occurrences.py --poses poses.json --layout gauge_460 \
      --doc-name "$DOC" --lineage "$LIN" --out-dir ./job/supplier
```

失败后续跑:先跑一次只读回读,再把结果喂给导入器。

```bash
python3 $SK/fusion_post_import.py --doc-name "$DOC" --lineage "$LIN" \
      --out-dir ./job --stages readback
python3 $SK/fusion_step_import.py ... --present-from-readback ./job/A7_readback_geometry.result.json
```

`--present-from-readback` 的三态:整块都在 → 跳过;一个都不在 → 导入;
**只在一部分 → 立刻停(退出码 7)**,因为 manifest 与文档对「切法」的理解已经不一致,
再导会产生重复 occurrence。这一条永远不自动修复。

---

## Peekaboo 模式

`--peekaboo-mode` 取 `local` / `local-ondemand` / `bridge` / `auto`。
默认 `local`;`auto` 走 `fusion_preflight.select_peekaboo_mode()`,
在 ≤3 s 预算内按 `local > local-ondemand > bridge` 选第一个健康的
(本机实测三个模式探完 0.6 s)。

**硬规则:探测不得改变被探测的状态。** 永远不要在没有 `--no-remote` 也没有
`--bridge-socket` 的情况下调用 peekaboo 子命令——那种形状会按需拉起
`daemon run --mode auto ... --idle-timeout-seconds 300`,它既过不了守卫钉死的
7 参数 `manual` 校验,又会在 5 分钟空闲后退出、留下一个没有监听者的陈旧 socket。
因此 `bridge` 模式默认**不探测**,要探得显式 `--probe-bridge`。

---

## 回执与证据

每次调用在 `--out-dir` 下留:

```
<stage>.py            送进 Fusion 的确切脚本(execute 调用)
<stage>.args.json     确切的工具参数
<stage>.result.json   caller 的 stdout
<stage>.stderr        caller 的 stderr
<stage>.exit          退出码 / 墙钟 / 完成时刻 / 第几次尝试
<stage>.receipt.json  分类结果、耗时、mode(schema cad-agent.fusion-call-receipt.v1)
```

作业级:`preflight.json`、`step_import.state.json`(可续跑)、
`step_import.receipt.json` / `post_import.receipt.json` /
`supplier-import.<layout>.receipt.json`、`*.log`。

---

## 测试

```bash
tests/run_tests.sh            # 任意带 pytest 的 python3;或 PYTEST=... 指定
```

103 个用例,全部离线:切块/manifest 逻辑(含用 CadQuery 造的合成 STEP 端到端
往返)、present-skip 与续跑、preflight 解析器(用本机真实命令输出做夹具)、
模板渲染与结果分类。合成 STEP 那两个用例通过子进程驱动 CadQuery 解释器,
所以主测试进程不需要 OCP;CadQuery 不在时自动 skip。
