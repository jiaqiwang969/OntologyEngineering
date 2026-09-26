# Mechanism 模块:机构学先行的设计方法论(雨点雨刮沉淀)

本模块是 142-雨点雨刮项目固化下来的两件套方法资产,已被 153-天花板小车
(FA04 凸轮摆杆—链条整机)验证为可迁移。核心启发:**机构学基本建模、
轨迹优化、数学建模作为"先行"**,再据此指导实体 CAD 设计、实物运动映射
与渲染 —— 而不是先画实体再补运动学。

## 先行链(方法主线)

```
二维机构拓扑判读(视频/拍屏,只冻结拓扑构型,数值待矫正)
  → 机构学建模(刚体、副、闭环约束;config 里机器可读)
  → 轨迹求解与优化(唯一求解器;Octave/Python)
  → 机器可读轨迹契约(schema;下游一致性靠它)
  → 指导实体设计(CAD 几何跟随机构参数,不是反过来)
  → 实物运动映射与渲染(RenderAppearance 与物理证据严格分离)
```

## 两件资产

### 1. 雨刮领域本体历史范本(Cetus + D31H,v001→v004)

CQ 驱动范围,OWL/RDFS 开放世界语义 + SHACL 交付期封闭闸门 + SPARQL
回归(能力问题 + 期望为空的不安全查询)。原目录已移至私有迁移归档，不在此运行旧校验器；未来正式语义须进入 Semantica 受控包。历史目录包含:`ontology/`(TBox)、
`registry/`(seed + 机器生成的全 occurrence 零件注册表)、`data/`(ABox)、
`shapes/`(编写 / 渲染分区 / FEA 释放 / cinematic 多重闸门)、`sparql/`、
`validation/`(实际闸门报告)、`scripts/`(构建与校验,开发侧运行)。

三条最有迁移价值的纪律:

- **三层严格分离**:`MaterialFamilySlot`(物理材料族候选)≠
  `RenderAppearance`(渲染显色)≠ `FeaPropertySet`(力学数值)。
  颜色永远不是材料证据;无可追溯 TDS/检测证据时数值保持 null,
  RDF 不写数值三元组。
- **FEA 释放闸门有意为 false**:未核定的材料与力学属性不能靠角色或
  颜色推断代替 —— 闸门报 false 是安全设计,不是缺陷。
- **`PROVISIONAL_IMPORT_BOUND`**:稳定 IRI 绑定到 STEP 哈希 + 导入
  manifest + 序号,但未经原生 CAD persistent ID 证明可跨导入器重建;
  它不等于 OEM 零件号,也不等于材料释放。

### 2. `wiper-kinematics/` —— 机构建模分层范本(config→求解器→测试→契约)

分层合同:`config/`(机构拓扑、定性视频状态、图片合同、G0 前参数)→
唯一运动学求解器包 → 刚体/接触/闭环/时序回归测试 → 机器可读轨迹
schema → 生成的轨迹/图/视频产物。`validation_results.json` 是随包的
验证快照。

## 已验证的迁移案例

153-天花板小车 `ceiling_trolley_project/mechanism/` 自述"采用与
`142-雨点雨刮/opensource/python/wiper_kinematics` 相同的分层方式",
把同一套 config/求解器/tests/schema 结构迁移到凸轮摆杆—链条整机;其
`artifacts/screening_topology_v1.md` 同样遵循"只冻结拓扑构型、数值待
G0 矫正"的判读纪律。迁移新机构时按同样步骤:先拓扑判读,再 config
化,再唯一求解器,再契约与回归。

## 在本 skill 内怎么用

- 本模块的默认环境只提供工程辅助库，不安装或运行本地 rdflib/pyshacl 语义后端。
  正式 CQ、SHACL、规则和项目 review 走上级工程本体论锁定的 Semantica。
  历史构建器、TTL/SHACL 与验证快照保留在迁移归档或其原始资料中，
  不能当作本机重新验证、当前项目通过或 standalone 门禁。
- 与 `assembly/` 模块的关系:assembly 管"装成什么样、能不能装",
  mechanism 管"怎么动、轨迹对不对";实体建模(NXOpen/Journal 执行)之前先过
  mechanism,装配交付之前过 assembly。
- 与旧语义侧车的关系：旧范本仅供历史方法对照。
  `scripts/semantic_query.py capabilities` 只列当前操作工具，不支持本地 CQ/shape
  或 `--historical-comparison`。唯一正式语义执行面是父级锁定的 Semantica。
