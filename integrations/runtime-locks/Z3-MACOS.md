# Jacobian 的 Z3 macOS arm64 安装修正

从零安装发现，锁定的 `z3-solver==5.1.0.0` 官方 wheel 文件名标为
`macosx_13_0_arm64`，内部 WHEEL 却标为 `macosx_13_3_arm64`，使当前 pip 的
`pip check` 报“不支持本平台”。两个原生 dylib 的 LC_BUILD_VERSION 最低系统是
13.3，不能直接改成 13.0。这里提供保守的 **macOS 14 及以上、arm64** 元数据修正；
不改变 Z3 版本、Python 代码或原生库，不据此宣称所有 macOS 版本均已验证。

原始失败与修正后的检查分别记录。其他平台使用对应上游发行包；本修正不适用于
Intel、Linux、Windows 或 macOS 13。Python 版本仍须符合 Jacobian 的要求。

先按主安装说明创建 Jacobian 环境，再执行以下步骤。输出目录必须不存在，失败时
保留输出以便检查，重试使用新的目录。下载仍来自公开 PyPI，脚本强制核原包 SHA256、
内部元数据及全部 RECORD；只改变 WHEEL 和重新计算的 RECORD，输出逐文件保全回执。

```sh
mkdir -p "$ONTOLOGY_ENGINEERING_ROOT/var/downloads"
curl --fail --location 'https://files.pythonhosted.org/packages/01/9a/cdb6db09d6aff6a803a94505aa24666db5e47df7aad0f2b1b0ddcb52ed12/z3_solver-5.1.0.0-py3-none-macosx_13_0_arm64.whl' \
  --output "$ONTOLOGY_ENGINEERING_ROOT/var/downloads/z3_solver-5.1.0.0-py3-none-macosx_13_0_arm64.whl"
python3 "$ONTOLOGY_ENGINEERING_ROOT/integrations/runtime-locks/repack_z3_macos_wheel.py" \
  --wheel "$ONTOLOGY_ENGINEERING_ROOT/var/downloads/z3_solver-5.1.0.0-py3-none-macosx_13_0_arm64.whl" \
  --output "$ONTOLOGY_ENGINEERING_ROOT/var/repacked-z3"
"$ONTOLOGY_TOOL_ENVS/jacobian-0.23.0/bin/python" -m pip install --no-index --no-deps --force-reinstall \
  "$ONTOLOGY_ENGINEERING_ROOT/var/repacked-z3/z3_solver-5.1.0.0-1oe1-py3-none-macosx_14_0_arm64.whl"
"$ONTOLOGY_TOOL_ENVS/jacobian-0.23.0/bin/python" -m pip check
"$ONTOLOGY_TOOL_ENVS/jacobian-0.23.0/bin/python" -c 'import z3; x=z3.Int("x"); s=z3.Solver(); s.add(x==2); assert s.check()==z3.sat; s.add(x==3); assert s.check()==z3.unsat; print(z3.get_version_string())'
```

`z3-macos-wheel.json` 固定原件与变更范围，`../patches/z3-solver-5.1.0.0-macos-wheel.patch`
展示最小元数据 diff。新包带本地 build 标识 `1oe1`；其来源是已核官方包的本地重打包，
不能标记为官方原包。保留 `REPACK.json`、安装报告、pip check 及实际求解结果。
安装后仍须执行 Jacobian 实际工具案例和同一 Project 的结果回交。
