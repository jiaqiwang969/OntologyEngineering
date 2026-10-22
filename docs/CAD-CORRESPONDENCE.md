# CAD 变化对应与当前工程复核

本增量沿用同一 Project、CAD evidence、原生观察及变化 owner。`cad evidence correspond --input <request> --output <new-private-dir>` 重新读取旧、新工程状态与有来源的特征意图、变化声明，并调用原 `Project::reconcile` 及两端依赖影响分析。输出是复核报告；命令执行成功不表示几何对应、工程审查或采用通过，Project 原件保持不变。

## 第一版的明确范围

`oev2.cad-correspondence/1` 绑定两端完整 Project 原件、原 `ProjectReviewContext`、`ObjectIdentity`、完成的 NX `inspect_detailed` 任务、输入零件与 native-result。特征意图通过 SHA256 和严格 JSON 指针定位，并复用 `CircularEdgeIntentV2` 的独立圆心与半径容差、完整候选检查。变化声明绑定两端完整意图引用；新声明只要求登记于当前 Project，旧记录保留当时内容。

`retained` 的相同工程特征 ID、相同工程对象 ID，加上两端各自完整且唯一的圆边匹配，可形成 `bounded_declared_correspondence`。其含义是有来源的工程身份声明得到这两次几何观察支持。原生 tag 仅属于本次观察；此检查未证明创建到检查的谱系，也未实现任意拓扑的持久命名。

实例路径和变换可以明确声明，但第一版的实例几何对应仍为 `unresolved`；SolidWorks 对应、split/merge 也保留未合格状态。新增、复制和删除保留 `declared_only`，复制要求新特征 ID；没有匹配不能自动推断删除。缺失观察、歧义和部分失败保留原原因，不靠最近几何猜测消除未知。

## 如何进入原工程流程

来源、对象、原生结果、意图或相关设计字段变化时，复用两端依赖和原 Project 决定，报告需要重审的节点。删除新图中的边不能隐藏旧图中的依赖。工程条件变化仍由原 reconciliation 检查；圆边形状一致不表示材料、载荷或适用性一致。

设计字段变化与证据重新绑定分别报告。同一零件文件改动后，几何值未变的其它特征仍要重新绑定本次观察。完全相同输入可报告 `no_correspondence_delta`；该字段只描述此对应检查，不关闭其它工程义务。

图纸作为第二个生产消费者，显式使用：

```text
oev2 --root <skill-root> cad drawing check-references \
  --input <drawing-request> --project <project> --project-context <context> \
  --correspondence <correspondence-request> --output <new-private-report.json>
```

提供该选项时输出 `oev2.current-drawing-reference/2`。它重新执行对应检查，核当前 Project 完整字节、对象、原生观察和每个圆边选择条件，再执行原图纸意图、来源与几何检查；任一项不满足仍阻断当前使用。未提供选项时保留原 `/1` 行为。`engineering_adoption` 保持 `not_established`，通过后仍需原工程审查。

## 验证和平台

规范正本为 `specs/modules/cad-engineering-binding.toml`，类型经过锁定 ggen 生成；算法在 `oe-cad::evidence::correspondence`，生产入口在 `oe-cli`，决定仍由 `oe-kernel::project` 维护。文件和源集合有界，最终重读原件，不允许用反序列化报告构造已验证能力。

合成磁盘回执仅检验软件边界；实际 NX 创建、保存、独立进程重开、创建输出与检查输入的闭包另行取证。macOS 软件检查、Windows 构建/接收、原生工具和工程交付分别按当前源的维护回执登记。本增量未关闭原 16 组 CAD 预期、完整 Semantica、原 63 项目标或实际项目实物验收；孔板/定位销案例属于 development_replay，不能证明 RRSI 方法净收益。


当前实例使用的必要基础由显式[当前装配绑定](CAD-OCCURRENCE-BINDING.md)接续，复用本source/circle owner及原装配/Project检查。该新入口不改变本`correspondence/1`的实例未合格含义；完整跨版本对应及原生occurrence edge/图纸消费者继续取证。


显式[实例对应/2](CAD-INSTANCE-CORRESPONDENCE.md)组合本变化来源/覆盖owner与当前实例owner，另由Project检查完整旧新依赖；本/1和原图纸合同保持。新版真实执行与平台/native资格仍按其回执，不能给旧通过改签。
