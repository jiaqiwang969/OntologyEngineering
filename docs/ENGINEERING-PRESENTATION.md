# 有源工程图示：Anatomy 方法吸收第一阶段

`oev2 presentation` 把工程图示接入既有 Project、来源、几何与文档层。此阶段提供离散状态的离线 HTML/SVG，不改变原生 CAD、BOM 或工程采用结论。代码版本、默认程序及本次实际验收见 `var/maintenance/status.json`；本文件描述合同范围，不授予尚未取得的浏览器或原生资格。

## 责任与合同

- `oe-engineering::presentation` 复用 `ArtifactReference`、`ObjectIdentity`、`ProjectReviewContext` 和 `oe-kernel::project::check_current_context`，将图示绑定到原件中的明确 JSON pointer。
- `specs/modules/cad-engineering-binding.toml` 的 `Presentation*1` 类型经锁定 ggen 生成。图示与 CAD 来源关联共享已有边界，没有新增 Project 权威。
- `oe-geometry::presentation` 用 Rust 计算固定 45°/30° 正交投影、盒体、竖直圆柱和单轴平移；所有离散状态使用同一相机范围。几何与读数取同一组参数原值，显示坐标舍入不回写原件。
- `oe-docs::presentation` 输出不可覆盖的新目录。浏览器原生 radio 控件切换预计算的 SVG 和读数，离线使用，不需要 JavaScript、网络、模型或后台服务。
- `oe-cli` 的现有编译来源检查在命令入口执行；交付回执另绑定三层实现、生成合同、原件、Project 上下文和各输出字节。

场景最多 64 个部件、8 个位移参数、32 个状态。支持 `mm`、`m`、`illustration_unit`；参数与几何单位必须一致。部件必须明确 `illustration` 或 `analytic_proxy`。初期不接收声称为已验证 native CAD 的部件。圆柱尺寸是外接盒尺寸，X/Y 均为直径；原点是底部外接盒最小点。每个状态恰好给出所有参数一次。静止部件轴为零；运动部件绑定一个参数和单位方向，位置是 `origin + axis * value`。

全部已列状态中的全部部件对接受严格正体积 AABB 重叠筛查。接触不算正重叠；包围盒重叠只产生候选，不能证明实体相交。没有重叠只描述这些离散状态的解析图元。无矛盾的包围盒分离平面先确定部件顺序，歧义/环及部件内部面仍使用深度启发式；排序是显示方法，任意遮挡、连续碰撞、真实时间、动力学、制造精度及原生几何另验。

## Agent 操作

用户用自然语言表达目的；agent 整理原件、对象与条件。原件的 `/scene` 使用生成的 `PresentationScene1`。参考 `examples/presentation/linear-stage.json`，此例为新编写的示意滑台，尺寸不来自真实设备。

以下 `OEV2_BIN` 必须来自当前维护登记，`OEV2_ROOT` 为同一明确根。Project 已含对象修订、所选条件、原件 SHA256 和依赖；目录必须在该根 `var/` 下。

```sh
"$OEV2_BIN" --root "$OEV2_ROOT" presentation bind \
  --project var/projects/example/project.json \
  --source examples/presentation/linear-stage.json \
  --object linear-stage --condition illustrative-discrete \
  --title '直线滑台 · 离散状态图示' --output var/projects/example/request.json
"$OEV2_BIN" --root "$OEV2_ROOT" presentation generate \
  --project var/projects/example/project.json \
  --input var/projects/example/request.json --output var/presentations/stage-01
"$OEV2_BIN" --root "$OEV2_ROOT" presentation verify \
  --project var/projects/example/project.json --directory var/presentations/stage-01
```

交付目录有 `index.html`、每个状态的 `state-NN.svg`、请求、回执和上游许可证。verify 重算全部输出，不仅比较回执里自报的摘要；额外文件、缺失文件、符号链接、篡改字节或回执、当前来源/条件/对象/项目修订变化均阻止通过。图示是生成时快照，没有自动刷新；来源变化后保留旧交付和旧上下文，用新的原件与 Project 重新 bind，再输出到新目录。`verify` 成功只表示合同、来源与字节当前有效，不能替代浏览器观察或工程采用。

## 后续吸收范围

1. 将原生 CAD occurrence、解析近似和讲解性部件显式映射，并绑定实际几何回读。
2. 吸收 lathe/tube、组关系和更可靠的遮挡排序；逐项保留独立几何反例。
3. 将真实定时运动与无时间的有序几何样本分别接入，验证连续运动与机构读数。
4. 评估连续旋转、WebGL、选中/剖切等能力的 Rust/WASM 或受控显示运行时边界；不得另建工程状态权威。
5. 由现有 `oe-browser` / Peekaboo 获取后台视觉和交互证据，再把实际反馈交给现有 evolution/version owners。尚无这类证据时不声称浏览器验收或学习收益。

上游固定为 [Anatomy](https://github.com/wheresryan22/anatomy) `bc943e4`。本轮吸收了 `kit/iso-kit.mjs` 正交投影公式和有限实体图示方法，未复制或运行其 headless Chrome 工具；保留 [MIT 许可证](third-party/ANATOMY-LICENSE.txt)。未吸收部分继续保留在上游来源中。
