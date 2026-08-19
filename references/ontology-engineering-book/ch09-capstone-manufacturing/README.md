# 第9章：综合实战案例——智能制造知识管理系统

## 项目定位

本章把前八章的概念、方法、语言、查询、规则与质量意识汇入一个制造业综合案例。
旧版将本章描述为 Java/Jena 查询服务与 Python/owlready2/Pellet 推理器的双栈工程；
在当前架构中，那些片段只保留为书史和实现比较，**不再是权威执行入口**。
本章的唯一可执行正本是 Semantica 内建包及其 runner。

## 唯一系统边界

```text
书中规格（章节、CQ、解释、练习）
                 │ source anchor
                 ▼
semantica.chapter_packages.vol1.ch09
  ├── manifest / contract / CQ registry / scenario registry
  ├── manufacturing.owl          # Manchester 来源资产；当前不解析
  ├── engineering-rules.swrl     # 结构化迁入；当前不执行一般 SWRL
  ├── cq01.rq … cq07.rq          # 查询正本
  ├── CQ1 positive / single-fault-negative fixtures
  └── SemanticPackageRunner → SemanticRuntime → receipt / release verdict
```

书目录不再保留迁移前的源码目录、Maven 工程、Jena service 或 owlready2 runner 副本。

## 当前包状态

- package：`semantica.chapter_packages.vol1.ch09`
- package status：`partial`
- release status：`blocked`
- payload：ontology/rules `adapter`；CQ/SPARQL/case `native`；shapes `absent`
- 原生场景：`OE-V1-CH09-SCN-CQ01-001`
- replacement：`execute_chapter_query` 替代旧 `QueryService.getEquipmentForMaterial`
- exact oracle：CQ1 正例返回两台具名设备；单因反例返回 0 行

## CQ 覆盖不能假绿

| CQ | 书中问题 | 当前执行状态 |
|---|---|---|
| CQ1 | 哪些设备可以加工指定材料？ | 原生 SPARQL + 正/单因反例 + exact multiset oracle |
| CQ2–CQ7 | 状态、冲突、工艺、工序及扩展查询 | 查询资产已迁入，但缺少书内 ABox exact oracle，不得宣称通过 |
| SWRL 冲突/推荐 | 规则推导 | 规则资产保留；当前 runtime 不解析或执行一般 SWRL/built-ins |
| DL 一致性 | Manchester/OWL 约束 | 来源资产保留；当前 runtime 不执行完整 DL/tableau |
| SHACL | 数据质量门禁 | 本章尚无 shape；不得借用别章后称为章内完备 |

## 复算正确入口

先由受控发布流程把实际 runtime commit 与精确 wheel/工件 SHA-256 分别写入
`SEMANTICA_RUNTIME_COMMIT`、`SEMANTICA_RUNTIME_SHA256`；缺失或错配必须失败关闭。

```bash
semantica package show semantica.chapter_packages.vol1.ch09 --json
semantica package run semantica.chapter_packages.vol1.ch09 \
  --runtime-commit "$SEMANTICA_RUNTIME_COMMIT" \
  --runtime-artifact-sha256 "$SEMANTICA_RUNTIME_SHA256" \
  --scenario-id OE-V1-CH09-SCN-CQ01-001 --json
semantica package verify semantica.chapter_packages.vol1.ch09 \
  --runtime-commit "$SEMANTICA_RUNTIME_COMMIT" \
  --runtime-artifact-sha256 "$SEMANTICA_RUNTIME_SHA256" \
  --scenario-id OE-V1-CH09-SCN-CQ01-001 --json
```

`run` 可以证明 CQ1 场景是否符合声明 oracle；`verify` 还要检查包完整性、能力边界、
来源/资产/运行时哈希与发布收据。当前发布状态必须保持 blocked。

## 实战扩展顺序

1. 在 Semantica ch09 包内为 CQ2–CQ7 分别补充最小正例、单因反例和精确 oracle。
2. 若要支持 Manchester 或 SWRL，先声明可验证的 profile，再实现解析与语义，不以静默降级替代。
3. 增加本章自有 SHACL，并区分开放世界推理与封闭式完整性校验。
4. 对每次变化执行 snapshot/diff、旧 CQ 回归、PROV 记录和显式 release verdict。
5. 将真实设备动作留给具名、有权限、可回滚的执行器；语义通过不等于操作授权。
