# 可执行佐证：OE 薄入口 → Semantica built-in packages

本目录不再实现本体、查询、形状、规则或案例。每个 demo 只是教学薄入口：

```text
书中论断 + 书源锚点
  → stable Semantica package/scenario ID
  → SemanticPackageRunner
  → exact oracle + operation report + PROV/receipt + release verdict
```

RDF/OWL、CQ、SPARQL、SHACL、facts/rules、正例、单因反例、合同和生命周期资产
全部由 Semantica 内建包拥有。OE 不保存 fixture 副本，不直接调用 RDFLib、pySHACL、
PyOxigraph、owlready2、Jena 或其他后端，也不存在 fallback。

## 运行

```bash
bash runtime/setup_runtime.sh
runtime/.venv/bin/python demos/<demo>.py
```

`runtime/setup_runtime.sh` 安装 `runtime/semantica-source-lock.json` 指向的本地构建
wheel。锁中绑定具体源码 commit、wheel 文件与 SHA-256；不要用一个未核验的全局
Semantica 安装替代它。

从 ontology-engineering skill 根发现 source-locked registry：

```bash
runtime/.venv/bin/python scripts/semantic_engagement.py discover
```

该入口自动核验并注入 source identity。原生 `semantica package list/show` 只供维护者
排查底层 registry/manifest，不是 demo 或读者的主发现路径。

## Demo 与唯一执行正本

| OE 薄入口 | Semantica 执行正本 | 书中论断 |
|---|---|---|
| `vol1_ch02_reasoning_modes.py` | `semantica.chapter_packages.vol1.ch02` | 单调推论与 OWA/CWA 的边界 |
| `vol1_ch03_cq_acceptance.py` | `semantica.chapter_packages.vol1.ch03` | CQ 是范围与验收合同 |
| `ch04_shacl_open_vs_closed.py` | `semantica.chapter_packages.vol1.ch04` | OWL 开放世界与闭合交付约束不可混同 |
| `ch05_forward_chaining.py` | `semantica.chapter_packages.vol1.ch05` | 有界正向链可复算且可追踪 |
| `vol2_iso_normative_query.py` | `semantica.chapter_packages.vol2.normative` | 受控转述可形成带模态、来源锚点的规范 domain package |
| `vol2_hara_asil_corroborate.py` | `semantica.chapter_packages.vol2.ch14` | 情境危害与 HARA 判定链必须保留证据和查表纪律 |
| `vol2_claim_gate_corroborate.py` | `semantica.chapter_packages.vol2.ch11` | 六绿不等于接受；主张缺件即拒 |
| `vol2_ch12_identity_bridge.py` | `semantica.chapter_packages.vol2.ch12` | 型号、批次、单件身份不可偷换 |
| `vol2_ch13_governance_distance.py` | `semantica.chapter_packages.vol2.ch13` | 签名、角色、距离与授权有效期要还原成关系 |
| `vol2_ch15_reopen_list.py` | `semantica.chapter_packages.vol2.ch15` | 需求追溯边要带理由，变化后可查询重开 |
| `vol2_metrics_recompute.py` | `semantica.chapter_packages.vol2.ch16` | 数字必须绑定分子、分母、来源与复算结果 |
| `vol2_ch17_change_verdict_gate.py` | `semantica.chapter_packages.vol2.ch17` | 旧结论必须交代其版本世界，保留/作废都要理由 |
| `vol2_ch18_independence_meet.py` | `semantica.chapter_packages.vol2.ch18` | 依赖闭包与排除事实共同决定独立性结论 |
| `vol2_ch19_substitution_gate.py` | `semantica.chapter_packages.vol2.ch19` | 现场代换不能用对照表冒充设计评估 |
| `vol2_release_binding_gate.py` | `semantica.chapter_packages.vol2.ch20` | 发布必须绑定正确快照、证据、处置和收据 |
| `internalization_loop.py` | Semantica governed ontology lifecycle | 冲突判决、版本谱系、PROV 与旧 CQ 防遗忘回归 |

以上 15 个 package launchers 只是 29 章 registry 的教学抽样。未单列 demo 的章节
仍有独立 Semantica package、manifest、CQ/场景合同和状态，不能用相邻章节的一次
运行替代其 receipt。

## 如何读输出

输出应分开报告：

- `书源锚点`：论断来自两卷书的哪里；
- `Semantica 包/场景`：实际执行的稳定身份；
- `oracle checks`：SELECT 绑定多重集、ASK 布尔值、图同构、规范化 SHACL 违例或
  规则结论是否与合同一致；
- `执行状态`：声明的场景是否通过；
- `发布状态`：源码/wheel/输入/输出/PROV 和全部能力门禁是否闭合。

场景 oracle 通过并不保证 release complete。若包声明了尚不支持的 DL、一般 SWRL
内建、非单调、时序、概率能力，或缺少必要证据，runner 必须返回 blocked；demo 和
文档都不得把它改写成绿色。

## Python / CLI / MCP 是同一个核心

底层 Python 使用 `semantica.chapter_packages.SemanticPackageRunner`；原生 CLI 暴露
`semantica package list/show/run/verify`；MCP 暴露
`list_chapter_packages`、`get_chapter_package`、`run_chapter_package` 和
`verify_chapter_package`。三者都是同一 runner 的薄适配，不各自解释 package 合同。

读者与 Agent 不直接调用原生 `run`/`verify`，也不手抄 runtime commit 或 wheel SHA-256；
统一 `scripts/semantic_engagement.py` 从 lock 读取并绑定这些值。OE demo bootstrap 复用同一
校验器；缺失或错误的身份必须 fail closed。原生接口仅保留为底层适配器诊断。

## 教学与权利边界

- 第二卷的 EPS-RC17、ENV-01、人物、事故和数值是合成教学材料。
- normative package 保存的是有来源锚点的受控转述与派生语义，不是 ISO 原文。
- 精确条款、表格或引文必须回用户合法持有的受控来源核对。
- 自动门禁只说明当前输入满足已编码合同，不授予合规、认证、风险接受或发布权限。
