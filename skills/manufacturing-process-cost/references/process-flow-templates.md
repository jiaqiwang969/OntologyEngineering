# 可复用工艺图模板

这些是原生 TikZ/XeLaTeX 图形模板，使用合成节点、状态和费用标识。先选图的用途，再用本项目记录替换节点与连线。原图、企业、产品编号和具体工艺参数没有移入。

可先打开 [五页模板预览 PDF](../assets/report-template/flowcharts/preview.pdf)，再选择下面的可编辑图源。预览展示模板自身，不是任何客户的实际制造方案。

## 选择图形

| 模板源文件 | 用来回答什么 | 必须补齐什么 |
| --- | --- | --- |
| [顺序工序与状态](../assets/report-template/flowcharts/01-sequential.tex) | 当前路线先做什么，检验哪个状态？ | 对象版次、输入/输出状态、检验阶段和进入条件 |
| [并行加工与汇合](../assets/report-template/flowcharts/02-parallel-merge.tex) | 哪些对象分别准备，何时可以装配或连接？ | 每个分支的对象身份、汇合条件及装配后的新对象 |
| [检验与返工复验](../assets/report-template/flowcharts/03-inspection-rework.tex) | 不符合时怎么办，返工后检查什么？ | 判据、隔离、处置批准、返工路线及受影响特性 |
| [可选工序与未决分支](../assets/report-template/flowcharts/04-conditional-operation.tex) | 资料尚不全时，哪些路线保持候选？ | 适用条件、要求依据、采用状态和未决项 |
| [工序、资源与成本关系](../assets/report-template/flowcharts/05-process-resource-cost.tex) | 费用和资源判断依据从哪里来？ | 对象/工序/资源/费用/反馈标识及每条关系的含义 |

前四类图表达路线或条件分支；第五类是知识关系图，箭头不能当作制造顺序。蓝框、琥珀框等样式只是阅读提示，不替代关键工序认定、批准或质量状态。候选虚线改成实线之前先更新项目采用记录。

图中没有预设某种表面处理应位于某个“检测”之前或之后。实际使用时先明确受检对象、状态、阶段和判据，检查后续处理是否改变已检特性；具体工艺依赖由项目证据确定。

## 怎样改成自己的图

1. 将需要的 `.tex` 图源和 [样式文件](../assets/report-template/flowcharts/styles.tex) 复制到项目私有报告目录。保留分发版原件，在新文档版次修改。
2. 用本项目稳定标识替换 OP/IN/OBJ 等示例编号；填写对象与状态，再按依赖修改连线。相同工序名不保证相同输入、输出或检验覆盖。
3. 用 [图形对应记录](../assets/report-template/flowcharts/flow-record-template.json) 留下图号/版次、所用快照、节点到项目记录的对应、边的含义和来源。此文件是填写模板，不会自动绘图、推理或验证工程语义。
4. 在报告正文说明每张图回答的问题、适用范围和重新核查条件；详细参数、费用和来源留在对应记录或表格中。
5. 编译后检查图与记录是否一致、箭头是否有歧义、分支是否遗漏；逐页看图，冻结图源、样式、PDF 和实际快照摘要。工程语义验证按根 skill 的 Semantica 流程另行完成。

`nodes[].inspection` 只在检验节点填写；不适用时可设为 `null` 并说明原因，不能把尚未知晓写成不适用。节点状态、边条件和采用记录分别维护。图形顺序、费用分组、能力关系不相互替代。

## XeLaTeX 使用

[catalog.tex](../assets/report-template/flowcharts/catalog.tex) 是五页图册入口，每页包含一张图及填写说明；各图文件也能独立嵌入普通报告。编译需要 TeX 发行版中的 TikZ、ctex、Fandol 和 TeX Gyre，不依赖外部图片、公司字体或另一 skill。

在图源目录运行，输出到已创建的项目目录；示例中的输出路径自行替换：

```bash
xelatex -no-shell-escape -interaction=nonstopmode -halt-on-error \
  -output-directory=/path/to/project/flow-preview catalog.tex
```

在已有 XeLaTeX 报告的导言区加载 `styles.tex`，正文用 `\input{02-parallel-merge.tex}` 插入图。普通竖版页面较窄时，优先重新分行布局或设置横向图页；整体缩小后必须检查最小字仍可读。

这些图源当前由调用者编辑，不是 `manufacturing_report.py` 的自动视图类型。需要将图接入冻结报告生成器时，应先设计节点/连线投影、版面规则、资源冻结和图文对应验证，不能把手工插图声称为已自动同步本体。
