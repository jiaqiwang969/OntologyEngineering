# 第 2 章：同一句“可靠”，为什么可能在说不同的事

本目录是第 2 章的新架构候选工件。章节承接第 1 章末尾的 `EPS-RC17` 同名争论，
用一条连续问题链拆开名称、对象身份、功能、行为、concern、measure、state 与 role，
并把 `fault/error/failure`、safe state 和安全时间概念作为 ISO 26262 的严格纵向样板。

当前状态：`正文与 PTW-PC-02 候选冻结 / 来源回读完成 / ImageGen 与同树 PDF 尚待闭环 / 全章人工接受未发生`。

## 目录内容

| 文件 | 作用 |
|---|---|
| `chapter.md` | 当前新架构正文；图 2-1 仍为生成前教学合同 |
| `problem-contract.yaml` | `PTW-PC-02` 可机读问题合同；不包含 ch12 的回答 TBox |
| `SOURCE-AUDIT.md` | Part 1/Part 10 与工程本体论方法的主张—来源账 |
| `examples/core-terms.txt` | 章内范畴与身份判断防错卡 |
| `examples/fault-error-failure.txt` | ISO fault/error/failure 的观察边界与条件化传播卡 |
| `examples/asil-explained.txt` | 迁移路由说明：ASIL 已退出本章，HARA 由 ch04 承担 |
| `../../outlines/ch02-outline.md` | 与当前 H1/H2、来源和出口同步的章级大纲 |
| `../../notes/ch02-gold-rebuild-contract.md` | 历史文件名下的本轮顺序重写记录；明确不再以 ch04 为模板 |

## 本章只拥有问题，不拥有回答本体

本章 owns：

- `EPS-RC17` 同名异物现场；
- `DUT-P07` / `SN-EPS-000417` / `EPS_ASSY_043` 异名同物候选；
- Same / Different / Unknown 的待解身份边界；
- 产品/系统/元素/工件/实例、功能/行为、concern/measure、state/role 的范畴区分；
- ISO 专用 fault/error/failure、safe state 与时间概念的精确保留；
- ENV-01 六种 concern 的非等价迁移；
- 三条镜像 CQ 草案与 ch12 验收准则。

本章不 owns：

- 通用业务 TBox、ABox、`owl:sameAs` 规则、SHACL、SPARQL 或 runner；
- ch12 独立 ontology package 的命名空间和实现；
- 任一真实 EPS/ENV-01 身份、可靠性、安全性或放行决定。

## 连续问题链

```text
一句“EPS-RC17 可靠”为什么同时进入六个工程世界？
  -> 同名为什么不等于同一对象，异名为什么也不等于不同对象？
  -> 功能、行为、concern、measure、state、role 为什么不能排成一棵树？
  -> fault/error/failure 换观察边界后怎样条件化变化？
  -> 同一个 100 ms 与 safe state 为什么仍有不同主体和端点？
  -> ENV-01 的准确、稳定、可靠、可用、可维护、数据完整怎样分问？
  -> 图中一个词落到多个对象时，缺的是什么？
  -> 冻结 PTW-PC-02，谁有权作身份裁决交给 ch03，怎样实现交给 ch12。
```

## 三种身份结果

| 结果 | 允许条件 | 禁止的默认推断 |
|---|---|---|
| Same | 类型和身份判据相容，范围/时段明确，桥接证据成立，冲突已处置 | 同名、近名或高相似度自动强合并 |
| Different | 身份判据不相容，或存在积极的区分证据 | 名称不同就判定不同实体 |
| Unknown | 类型、边界、持续条件、桥接证据缺失，或来源冲突未决 | 把未记录的 Same 当成 False，或为了“去重”强行合并 |

## ISO 精度边界

- Part 1 是 item/system/element、fault/error/failure、FDTI/FHTI/FRTI/FTTI 与 safe state 的规范术语来源。
- Part 10 §4.2、§4.3.1、§4.4 只作资料性解释；示例不能变成跨项目普遍公理。
- `element` 是统称而非固定中间层；fault/error/failure 是并列概念而非必然子类或传播链。
- FHTI 属于给定 safety mechanism，FTTI 属于 item/hazard 语境；相同单位和数值不建立 measure identity。
- 通用概念不得通过删除 `iso262:` 前缀生成；ISO 语义只能经显式、局部 adapter 进入 ch12。

## 图文状态

图 ID：`ch02-fig01-one-word-many-worlds`

视觉问题：同一个“可靠”标签落在不同对象层级、功能、行为和时间尺度时，为什么会指向不同事实？

当前必须出现的区分：车辆层 EPS 功能边界、EPS system、ECU component、软件构建 artifact、
运行 behavior、物理单件的 DUT role 与不同时间/度量线索。视觉邻近和共同标签不得暗示身份已经合并。

当前尚待：ImageGen 资产、生成事件、语义/视觉/打印/权利复核、正文实图替换、同树 PDF 和逐页 QA。

## 当前不能宣称

- 不能宣称图 2-1 已生成或读者已接受。
- 不能宣称 ch02 全文已获用户验收或出版放行。
- 不能宣称 `DUT-P07` 与任何序列/PLM 标识真实为同一单件。
- 不能宣称一个准确、稳定、在线或可修复结果等价于产品整体可靠。
- 不能宣称 ch12 已完成；ch02 候选冻结不依赖 ch12 完成。
