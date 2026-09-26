# 雨刮连杆运动学 Python/NumPy 验证版

这个目录是对归档 MATLAB 运动学代码的独立、可重复开源运行时验证。它目前覆盖“两杆公转/双曲柄”拓扑，并固化了 D003 和 J71E 两个历史 MATLAB Publish 案例。

> **权属与许可证状态：待确认。** 原始 PPT、RAR、MATLAB 源码、车型坐标和历史结果的对外发布授权尚未确认。在获得明确授权和选定许可证之前，请只用于私有环境内部验证，不要公开上传、转载或宣称已开源。

## 范围边界

这些代码处理雨刮机构的几何与运动学：

- 电机轴局部坐标系标定；
- 电机曲柄圆轨迹；
- 摇臂圆与连杆长度球面的两支交点；
- 装配分支选择；
- 输出轴角度、摆角和杆长闭环误差。

它不是电机控制器固件，不包含 PPT 中描述的 PWM/PI、LIN 通信、堵转/热/电压保护或状态机代码。

## 来源与数学对应

归档 `20171225_Matlab code update.rar` 内的 MATLAB Publish HTML 保留了当时的完整脚本和运行结果：

- `汽车四连杆(约定统一)/RESULT/D003/D003.html`，标记为 MATLAB 7.11；
- `汽车四连杆(约定统一)/RESULT/J71E/J71E.html`，标记为 MATLAB 7.11。

历史页面给出的摆角是本验证版的回归基准：

| 案例 | `thetaS` | `thetaL` |
| --- | ---: | ---: |
| D003 | 84.3934° | 84.2716° |
| J71E | 84.8999° | 85.5999° |

注意：这里的 `RESULT/D003/D003.html` 是 MATLAB 7.11 的历史发布快照；它和根目录较晚版本 `D003.m` 对应的 `RESULT/2` 不是同一套参数/采样基线。前者用于 Python 移植回归，后者用于 Octave 原脚本回归，数值不可直接混用。

对于输入铰点 `N`，在摇臂轴局部坐标中求输出铰点 `Q=(x,y,0)`：

```text
|Q| = R
|Q - N| = L
Q · (Nx, Ny) = (R² + |N|² - L²) / 2
```

因此问题等价于平面圆与直线的两个交点。`core.py` 使用这个紧凑几何形式，与归档 `CalcTransferCoor.m` 中的符号展开式等价，并保留相同的分支次序。角度由 `atan2` 计算，与原 `acos + y 象限判断` 等价，但数值上更直接。

## 运行

只依赖 Python 3.10+ 和 NumPy 1.24+。在本目录下执行：

```bash
PYTHONPATH=. python3 -m wiper_kinematics --pretty
```

在本 skill 的独立运行环境中，推荐直接运行包内门禁：

```bash
./run-tests.sh
```

该脚本使用 skill 自带的 `.venv`，运行单元回归和 CLI 合同检查，不依赖原雨刮
仓库、Nix 或未随包分发的私有归档。

只跑一个案例：

```bash
PYTHONPATH=. python3 -m wiper_kinematics --case D003 --pretty
```

同时把 JSON 保存到指定文件：

```bash
PYTHONPATH=. python3 -m wiper_kinematics --output validation_results.json --pretty
```

CLI 始终在标准输出产生同一份 JSON；任一回归或不变量超差时返回非零状态码。

## 测试

```bash
PYTHONPATH=. python3 -m unittest discover -s tests -v
```

测试门禁包括：

- D003/J71E 各 12,501 个采样点全部为有限实数；
- 电机曲柄半径、输出摇臂半径、两根连杆长度的最大闭环误差不超过 `1e-9 mm`；
- 与历史 MATLAB 页面中四个摆角的差值不超过 `1e-4 degree`；
- JSON 结果可序列化，且总体 `passed=true`。

完整工作区的额外集成回归 `validation/compare_python_archive.py` 还会逐文件比较两个案例的 `Pin01/Pin02/Pin1`、角速度、角加速度和四项倾角，共 22 个历史 MATLAB 输出文件。该检查依赖未获公开授权的原始归档，因此没有放进可独立分发的 Python 包内。

`validation_results.json` 是 DGX 上的一次实际运行记录；完整工作区内的本机结果和逐文件比较分别保存到 `validation/results/python-kinematics-local.json` 与 `validation/results/python-archive-regression.json`。
