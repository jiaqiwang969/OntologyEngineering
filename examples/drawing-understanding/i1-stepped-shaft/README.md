# I1 阶梯轴：读图路线的公开输入

原创合成零件：三段同轴圆柱沿 +X，直径 30/20/12 mm，长度 20/40/20 mm，中段直径偏差 −0.01/+0.02 mm；不取自任何客户图纸。

| 文件 | 说明 |
|---|---|
| `drawing.dwg` | 原 DWG（SolidCraft 建模、CADCraft 出图后保存并重开），未改动 |
| `source.dxf` | 锁定 CADCraft 转出的 2018 DXF（不给字体） |
| `source.svg` | 同一转换器对该 DXF 的中性渲染（内置笔画字体） |
| `provenance.json` | 各文件摘要、转换器提交与命令 |

在一个已锚定的 workspace 中，以该 DXF 为当前 Project 的源，运行：

```
oev2 --root ROOT --project-home HOME drawing route --project project.json --object object.json \
  --dxf sources/source.dxf --svg sources/source.svg --output-directory runs/i1 --title "I1 阶梯轴"
```

测试 `crates/oe-cli/tests/drawing_cli.rs` 给出完整的 workspace 建法。在这张图上通过路线检查，不说明工程正确，也不说明另一张图的分类质量。
