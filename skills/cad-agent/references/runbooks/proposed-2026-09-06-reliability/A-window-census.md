# RUNBOOK_DELTA —— 文档改动建议（仅提案，未写入 skill 目录）

以下三处改动只有在补丁**实机烟测通过后**才应写入
`~/.codex/skills/cad-agent/references/`。每条给出「插入位置 + 精确文字」。

---

## 1. `references/fusion-execution.md`

### 1.1 「其他铁律」第 1 条 —— 替换

**原文（第 1 条）**

```
1. **单次调用 8–20 秒是正常的**(含窗口验真与 cell 启动),不是卡死。
```

**改为**

```
1. **单次调用 8–20 秒是正常的**(含窗口验真与 cell 启动),不是卡死。
   守卫层的窗口验真会连续做 2–3 次浅层普查直到「窗口签名视图」稳定;
   同一个 Fusion PID 只在第一次调用时抓一次辅助窗口截图,之后复用
   `~/.local/state/cad-agent-fusion-mcp/local/fusion-mcp-auxiliary-<uid>-<pid>-*.json`
   里的证明。看到 `known_nonblocking_auxiliary.proof_source` 从 `screenshot`
   变成 `cached-per-pid-signature` 是正常的。
```

### 1.2 「其他铁律」—— 新增第 7 条

**在第 6 条之后追加**

```
7. **窗口普查期间不要求你停止使用电脑,但要知道它在看什么。** 守卫按
   「标题 + 属主 PID + 尺寸类 + 是否主窗口」判定窗口身份,**不**用 CGWindowID
   和窗口原点 —— macOS 切换 Space / Mission Control 时会把 Fusion 的全部窗口
   一起平移,窗口原点在动画期间是不可信的。因此其他 App 的窗口遮挡 Fusion、
   或你在别的 Space 里工作,都不再是守卫层拒绝的理由。
```

### 1.3 「故障排查」—— 新增两条

**在「深度健康验收」那条之后追加**

```
- 守卫层瞬态拒绝(退出码 2、**上游未收到任何请求**)的判别与处置:
  - `Fusion module-loading auxiliary is not uniquely stable`
  - `Fusion module-loading auxiliary has no unique primary canvas`
  - `Fusion top-level window set did not settle: …`
  - `Peekaboo Fusion window census failed: …`
  这四条都发生在 `fusion-mcp.in-flight.v2` 写入之前,不构成 same-PID
  no-retry 边界,可以直接重试(建议退避 45 s)。判据是 Fusion AppLog 里
  没有对应的 MCP 请求。**不要**因为这类拒绝去重启 Fusion 或退役 PID。
- Peekaboo 传输模式:`FUSION_CAD_PEEKABOO_MODE` 默认 `local-ondemand`。
  当 bridge / on-demand 传输本身不可用(超时、握手不全、socket 不在)且
  local 模式权限齐全时,守卫会自动降级到 `local` 采集窗口证据,并在证据里
  写下 `peekaboo_mode_requested` / `peekaboo_mode_used` /
  `peekaboo_mode_fallback`。降级只影响证据采集方式,不放宽任何判定;
  实质性拒绝(阻塞窗口、辅助窗口不唯一、应用身份不符)绝不降级重试。
  如果证据里出现 `peekaboo_mode_fallback`,说明 daemon 需要修:
  `peekaboo daemon stop && peekaboo daemon start`(manual 模式)。
```

---

## 2. `references/runbooks/fusion-runtime-safety-incidents.md`

### 2.1 「非原生崩溃安全事件」表 —— 新增一行

**在表尾追加**

```
| 20260906,PID 56355 / 12487 | 守卫层窗口普查误拒 + 一次由此触发的 PID 退役 | `FusionWindowBoundsUnstableDuringSpaceSwitchAnimation`(新签名);上游边界 `PeekabooTransportFaultRetiredHealthyPid` | 17 次 `Fusion module-loading auxiliary is not uniquely stable`,全部发生在上游请求之前(AppLog 无对应请求),每次令调用方退避 45 s,6 组连拒 2 次。实测 600 次普查:辅助窗口 windowID(226933)、尺寸(860×536)、窗口数(20)恒定,而原点 x 在 16.4% 的采样里被平移(最远 −7273),相邻普查不一致率 23.4%,每次漂移都伴随前台应用切换 —— 即 macOS Space 切换动画。守卫用 `(windowID, x, y, w, h)` 全等判稳,故误拒。另 2026-09-05 23:00,bridge 模式一次 `Peekaboo Fusion window census failed (bridge TIMEOUT)` 把健康的 PID 12487 推进 terminal retirement,当时 local 模式可用但无降级路径。见 `reports/fusion_reliability_20260906/A_window_census/ANALYSIS.md`。 |
```

### 2.2 「强制检索与拒绝映射」表 —— 新增两行

**在 `FusionWindowOnDifferentDisplayOrSpace` 那一行之后追加**

```
| 守卫层报「辅助窗口/主画布不稳定」,或窗口 bounds 在两次普查间变化 | `FusionWindowBoundsUnstableDuringSpaceSwitchAnimation` | 这是 macOS 桌面切换动画,不是 Fusion 状态异常。窗口身份必须用「标题 + 属主 PID + 尺寸类 + 主窗口角色」,不得用 CGWindowID 或原点;稳定性用有界 settle-and-retry(≤6 次普查 / ≤10 s)判定。**不得**因此重启 Fusion 或退役 PID;该拒绝发生在 in-flight 标记之前,可直接重试。 |
| Peekaboo bridge / on-demand 传输故障(超时、握手不全、socket 失效)导致窗口证据采集失败 | `PeekabooTransportFaultRetiredHealthyPid` | 这是**本地证据采集**故障,不是上游歧义,绝不能升级为 terminal retirement。先在 local 模式(`--no-remote`)复采一次证据并把选择写进证据;仍失败才拒绝该次请求。修复动作是 `peekaboo daemon stop && peekaboo daemon start`(manual 模式),不是重启 Fusion。 |
```

### 2.3 「计数与证据边界」—— 一句补注

**在「另有 9 起 timeout、queue-hang 或 transport 安全事件」那段之后追加**

```
守卫层在上游请求之前做出的拒绝(窗口普查、辅助窗口分类、Peekaboo 传输)
不计入上述任何一类事故:它们没有向 Fusion 发出请求,不产生歧义结果,
也不触发 same-PID no-retry 边界。它们只应被计为可用性缺陷。
```

---

## 3. `references/runbooks/fusion-mcp-ambiguity-triage.md`

### 3.1 「歧义边界」—— 新增反向清单

**在「以下情况一律按结果未知处理」那份清单之后追加**

```
以下情况**不属于**歧义边界,不得写 retirement:

- 守卫层在写入 `fusion-mcp.in-flight.v2` **之前**拒绝的请求 —— 包括窗口普查
  失败、辅助窗口分类失败、窗口集合未 settle、Peekaboo 权限/桥接/传输故障、
  owner lease 争用。这些请求从未到达 Fusion。
- 判据是可验证的:Fusion AppLog 中没有对应的 MCP 请求,且不存在匹配当前
  PID/启动令牌的 in-flight 标记。
- 处置:退避后用**同一个** Fusion PID 重试。把这类拒绝升级为 retirement 会
  白白丢掉一个健康的 Fusion 进程(2026-09-05 23:00 的 PID 12487 即为此例)。
```

### 3.2 「回归门禁」—— 新增三条

**在现有清单末尾追加**

```
- Peekaboo 传输故障(CLI 超时、桥接握手不全)在有可用的 local 模式时降级采集,
  并把 `peekaboo_mode_requested` / `peekaboo_mode_used` / `peekaboo_mode_fallback`
  写进窗口证据;实质性拒绝与 `FusionRuntimeRetiredError` 绝不降级;
- 窗口普查的抖动(同一窗口 id、同一尺寸、原点被动画平移)不产生拒绝,
  而阻塞标题或 modified 文档只要在任意一次 settle 普查中出现即拒绝;
- 辅助窗口的空白面证明按 (PID, 启动令牌, 窗口签名, 证明时窗口 id) 缓存,
  跨 PID、跨启动令牌、窗口重建或 TTL 过期后必须重新截图证明。
```

**并在文末「聚焦测试位于」清单中追加一行**

```
- `tests/test_fusion_window_census_stability.py`
```

---

## 4. `lessons-inbox/20260906-fusion-import-chunk-sizing-and-startup-deadline.md`

该文件里这一句需要更正(它把误拒描述成了正常现象):

**原文**

```
- Peekaboo guard-layer rejections such as `Fusion module-loading auxiliary is not uniquely stable` happen before any upstream request and are retryable after ~45 s; treat them separately from upstream timeouts, which are terminal for the PID.
```

**改为**

```
- Peekaboo guard-layer rejections such as `Fusion module-loading auxiliary is not uniquely stable` happen before any upstream request and are retryable with the same PID; treat them separately from upstream timeouts, which are terminal for the PID. Root-caused 2026-09-06: the released guard required byte-equal CGWindow ids *and* bounds across three censuses, while macOS translates every Fusion window during a Space-switch animation (measured: window id and size constant across 600 censuses, origin drifting in 16.4% of samples, consecutive censuses disagreeing 23.4% of the time). 17 rejections cost ~13 min of 45 s back-offs today. Fix and evidence: `reports/fusion_reliability_20260906/A_window_census/`.
```
