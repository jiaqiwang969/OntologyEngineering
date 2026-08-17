# 第11章：综合收口 —— 三个相关项，一份不能借绿灯的报告

本章只读装配 EPS、BMS 与 AEB 的已登记教学对象，回答九个连续问题：

```text
子门禁绿、总报告红
  → 精确冻结对象
  → 三案例成熟度差异
  → 追溯链终点
  → 开放 Claim
  → 单因反例
  → 可复现报告
  → 缺口责任
  → 历史/当前/未来的可签结论
```

> 具名事实：`reports/acceptance/20260814-ch11-rewrite-diagnostic.json` 锁定了重写期工作树的相关输入与检查器身份；旧 manifest 有 10 处登记输入漂移，`capstone.py` 在装载候选图前失败关闭。这不是“当前 capstone 绿色”，也不是未来工作树的永久预期。不要为了跑出绿色演示重冻或覆盖旧 manifest。

## 主要资产

| 资产 | 当前用途 |
|---|---|
| `chapter.md` | 九问正文；区分历史报告、当前漂移工作树和未来候选 |
| `bundle-manifest.yaml` | 保留的历史冻结身份；逐输入角色、相对路径、SHA-256 与包级哈希 |
| `src/capstone.py` | 只读核验旧 manifest、初始字节快照和末次状态；身份不符即失败关闭 |
| `src/freeze_bundle.py` | 未来经上游接纳和集成后显式生成新 manifest；不是当前快速开始命令 |
| `../../eval/eval-cases.yaml` | `CQ-CH11-01/02/03` 与 `GATE-CH11-01` 的查询和 oracle 正本 |
| `../../eval/test_capstone_bundle.py` | 哈希、路径、快照、查询一致性、Clause 8 状态和单因变式的契约测试 |
| `../../ontology/abox-bms-system.ttl`、`../../ontology/abox-aeb-system.ttl` | ch05 上游的最小 FSR→TSR 教学链；ch11 不修改 |
| `../../ontology/abox-eps-validation.ttl`、`../../ontology/abox-eps-safetycase.ttl` | ch05 Clause 8 对象与 ch03 CAE 角色/状态的上游正本；ch11 只读查询 |

## 三个 CQ 与一条窄 GATE

- `CQ-CH11-01`：按 Item 统计已经分配到元素的 FSR/TSR；注册 oracle 为 AEB=2、BMS=2、EPS=6。
- `CQ-CH11-02`：沿危害事件归属找具有 ASIL D 安全目标的 Item；注册 oracle 为 AEB 与 EPS。
- `CQ-CH11-03`：精确绑定 Clause 8 规范、活动、结果、评价、报告与 Claim 的状态，不把 Draft/Planned/NotRun/NotPerformed/EvidenceCandidate 提升为接受。
- `GATE-CH11-01`：只检查已标记进入系统层的非 QM 安全目标，是否至少有一个经 `derivedFrom+` 可达、已 `allocatedTo` 且类型属于 `SafetyRequirement` 子类闭包的后代。BMS/AEB 的主目标在范围内并由现有 FSR/TSR 满足；它不证明每条路径完整，也不覆盖未标记的次要目标。

上述是开发态查询合同和注册 oracle；旧 manifest 已漂移，所以不能把它们转述成当前冻结 capstone 的通过结果。

## 只读诊断入口

```bash
.venv/bin/python functional-safety-book/ch11-capstone-three-items/src/capstone.py
```

在 2026-08-14 诊断快照上，该命令因旧 manifest 与所记录工作树不匹配返回 2。未来运行必须按当时实际输出报告，不能把快照结果当永久预期；这类失败只证明身份核验关闭，不是候选包通过或工程内容失败。

契约测试可以单独复核检查器的局部行为：

```bash
.venv/bin/python -m unittest eval.test_capstone_bundle
```

契约测试绿色只说明已命名的检查器合同通过，不能抵销 capstone 的 manifest 漂移，也不能替代整书门禁、专家复核或产品放行。单因实验必须在完整临时副本中进行，不修改旧 manifest 或共享 ABox。

## 只读责任与下一次冻结

- Clause 8 安全确认规范、活动、结果、评价和报告由 ch05 对象责任边界维护。
- 它们能否进入 Claim/Evidence、状态能否跃迁，由 ch03 CAE 治理边界审查。
- ch11 拥有收口查询、oracle、fixture 与报告合同，只能定位缺口、登记并退回责任章；不得为关 Claim 临场修改上游事实。
- 只有上游事实被接纳、规则影响复核、受控集成和冻结授权均完成后，才可由冻结者生成下一版 manifest，并以新身份重跑。历史旧 manifest 继续保留，不为演示重冻。
