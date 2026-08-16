# ISO 26262 本体化刻录层（形式 B）

把标准逐条款刻成机器事实：`isoN:NormativeUnit` 个体承载 **条款坐标、模态
（shall/should/may/NOTE/…）、中文转述、关键词、书章映射、提取件锚点**。
公开层不含标准原文一个字——转述为本书作者综合；需要原文时沿锚点回到
本地受控提取件（`ISO_SOURCE_ROOT`，不随本仓库分发）。

## 当前刻录进度

| 部分 | 骨架 | 已刻转述 | 来源 |
|---|---|---|---|
| Part 1 术语 | 153 | 153 | 自动挖掘自第二卷附录 C（151/153 已钉提取件锚点） |
| Part 3 概念阶段 | 87 | 10 | 骨架自动生成；转述种子来自第二卷 ch04 |
| Part 2/4–12 | 待刻 | — | 按第二卷章序推进（伴读包节奏） |

## 文件

- `normative-tbox.ttl` — 刻录 TBox（类/模态/属性 + 三条刻录纪律）
- `partN-*.ttl` — 各部分刻录正本（机器可查）
- `partN-cards.md` — 自动生成的卡片视图（喂全文检索）
- `glosses/partN-glosses.yaml` — 转述种子（慢慢刻的工作面）

## 重刻

```bash
python3 scripts/engrave_iso.py          # 需本地提取件（ISO_SOURCE_ROOT）
```

修改 glosses 后重跑即可；TTL 与卡片视图同步再生。每新刻一包，
向 `references/eval-cases.json` 加检索用例，评测门禁守住刻录质量。
