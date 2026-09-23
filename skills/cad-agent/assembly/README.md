# 装配专项检查

先读[装配与力学通用判断内核](../references/assembly-and-physics-kernel.md)。`nominal_solver.py` 是公开的**有界几何筛查器**：对已经给定最终 AABB、先后约束和有限候选方向的实例，搜索固定朝向直线插入顺序，并返回具体方向或“所声明域内无路径”。它不会根据功能意图创造接口、求接触力或评价真实强度。定义/实例、完整界面图、逐状态支撑、载荷路径和仿真边界仍按方法内核及 CAD 证据合同独立记录。

```bash
python3 assembly/nominal_solver.py assembly/synthetic-example.json
```

真实任务应使用经原生回读的实例包围盒，说明坐标、容差、工具/夹具和装入方向的来源。AABB 的保守性、转动路径和弹性压合可能改变结论；输出只对声明的 AABB 与候选域有效。
