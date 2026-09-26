# 木镜框案例：图生 STL 如何成为参数解析的桥梁

本案例支持[从中间模型识别核心控制参数](inverse-shape-parameter-discovery.md)的操作方法。它记录一个对象上的发现过程，不表示该参数族已在其他形状上验证。

## 关键转变

用户要求按单张木镜框照片重构，并反复指出高低变化、曲面转折与最高棱线不对。早期仅依靠轮廓或偏置构造不能充分表达这些关系；其中一版虽为闭合网格，正面投影仍出现翻折。这说明网格有效性与形状吻合是两个问题。

用户随后提供混元生成的 STL，并明确指出它也不准确。它使原先隐含在照片里的空间猜测变成可切片、可测量的对象。按用户要求沿厚度中面保留前半后，通过截面和等高线分析，得到有用的候选关系：内边、外边和峰顶轨迹可以分别控制；峰顶在截面宽度中的位置变化，会使两侧曲面的宽窄、陡缓交替。

实验由固定截面逐步加入移动峰顶、谐波项、角度变化，再尝试单调截面。关键收获是找到能表达目标变化的生成关系，而不是把全部网格顶点保留下来。实验记录中的拟合误差衡量的是与图生 STL 的差异，不是实物精度。

有了可控的导线和截面后，工作重新以原图为依据：提取轮廓和木纹、生成曲面、固定视角比较，再调整参数与材质。原图没有突然提供更多深度信息；模型使“哪里不同”可以转化成“应检验哪个控制量”。

用户最后要求用对称性修复转弯处的最高棱线。简单左右、上下平均会保留局部拟合偏差；最终把重复部位共同拟合到共享曲线，再生成连续截面，并用相同相机、材质和光照检查前后差异。

## 最终模型采用的具体选择

- 内边、外边、峰顶三条导线各自采用 `r(θ)=c0+c8*cos(8θ)+c16*cos(16θ)`，再保留纵向比例。
- 截面两侧使用七次 Bézier，在峰顶连接处匹配位置与一至三阶导数。固定的是径向截面控制柄，不能由此宣称整张曲面处处具有相同的法向曲率半径。
- 约束 `c8<0` 且 `-c8>4*abs(c16)` 排除归一化导线中的额外径向极值。修复前、后采样都得到八个极大值和八个极小值；本次改善是棱线位置与曲率一致性，不是实测波峰数量减少。
- 非等比拉伸后的成品具有左右、上下反射对称；八瓣旋转等价是在撤销该拉伸后成立。自然木纹没有同步做镜像。

峰顶高度 `0.078 × 参考宽度` 保留为设计假设，未由单张照片实测。原 STL 的数值尺度没有被擅自解释为毫米。实际厚度、木种和制造工艺仍未知。

## 验证与复现入口

本地案例根目录：`/Users/<user>/171-王杰-设计`。下列相对路径以此为根；方法本身不依赖这些案例文件才能使用。

| 证据或入口 | 用途 |
|---|---|
| `hunyuan_reference/19e4e0674d7a14eb6a125165982deefd.stl` | 用户提供的中间空间假设 |
| `upper_half_final/cut_verification.json`、`upper_half_final/export_readback_verification.json` | 厚度中面裁切及实际导出检查 |
| `analytic_reconstruction/experiment_ledger.json`、`analytic_reconstruction/round2/experiment_ledger.json`、`analytic_reconstruction/round3/experiment_ledger.json` | 候选生成关系与对桥梁网格的拟合实验 |
| `symmetric_refinement_v2/reference_photo.png` | 最终视觉校准使用的照片 |
| `symmetric_refinement_v2/symmetry_specification.json`、`symmetric_refinement_v2/symmetry_verification.json` | 共享曲线、连续性及对称检查 |
| `symmetric_refinement_v2/symmetry_before_after.png` | 同条件下的无纹理整体与局部比较 |
| `symmetric_refinement_v2/photo_matched_render.png`、`symmetric_refinement_v2/photo_model_oblique.png` | 带材质的正视与斜视结果 |
| `symmetric_refinement_v2/delivery_readback_verification.json` | 导出 STL、GLB 和 Blender 文件的重新读取结果 |
| `refit_symmetric_ridge.py`、`build_photo_reconstruction.py` | 最终几何及材质生成入口 |
| `verify_photo_delivery.py`、`render_ridge_comparison.py` | 导出检查及诊断渲染入口 |

重新读取最终 STL 得到一个闭合连通木框，无边界边、非流形边或零面积三角形，背面共面，中央开孔探针未命中。全部导出顶点的左右/上下反射距离最大约为参考宽度的 `1.1e-7`。这些是模型及导出检查，未执行完整三角形自交检查，也不证明单视图深度准确或可直接加工。

2026-09-18 核对的 SHA-256：

```text
中间 STL  43704d76c94615022c1382cbc4e08cdd45a4f7ad5040638e5ccab104e7e14cdf
参考照片  07632f78c5ff9f6580b6aa40673b837692c5c94f28ec89d045a2cfef34052e29
最终 STL  06c0c379400166ff5230bca7b1ac55f21f9697b9d91985620a9e7ae447cec7ae
```

本案例沉淀为可检索的操作指导；八瓣谐波、恒定峰顶高度及 Bézier 次数只属于此次设计选择。
