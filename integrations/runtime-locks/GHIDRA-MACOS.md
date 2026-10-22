# REA 原生分析：Ghidra 12.1.4 / macOS arm64

本次公开安装使用 Ghidra 12.1.4 的官方发行 ZIP 与 JDK 21。要求以固定版本的
[README](https://github.com/NationalSecurityAgency/ghidra/blob/Ghidra_12.1.4_build/README.md)
和 [Getting Started](https://github.com/NationalSecurityAgency/ghidra/blob/Ghidra_12.1.4_build/GhidraDocs/GettingStarted.md)
为准，不能把上游 master 的工具链要求直接套到这个版本。

官方资产为
[`ghidra_12.1.4_PUBLIC_20260921.zip`](https://github.com/NationalSecurityAgency/ghidra/releases/download/Ghidra_12.1.4_build/ghidra_12.1.4_PUBLIC_20260921.zip)，
本次取得的 SHA256 为 `ddac49f903da9d5bac833e5cc79395098b9c33cfd3279be5f31bd00387d2d4db`。
保存原 ZIP 与校验记录，解压到新目录。不要用源码归档代替发行包，也不要覆盖已有安装。

该包未提供 `Ghidra/Features/Decompiler/os/mac_arm_64/decompile`。
包内原 Makefile 在 macOS 默认选择 x86_64；arm64 构建须显式选择架构。
这是平台原生组件准备，不需要改 REA、Ghidra 源码或借用维护者的旧程序。
本次先核包内源码，再以以下参数构建；默认架构的先前尝试也保留了原记录。

以下示例要求已设置本次真实的 `ONTOLOGY_ENGINEERING_ROOT`、`GHIDRA_INSTALL_DIR`，
并已有 Xcode Command Line Tools 的 Clang 和 make。使用尚不存在的构建目录；
示例只为这个官方版本补建 decompiler，不声称完成全部 Ghidra 原生组件。

```sh
set -e
GHIDRA_NATIVE_BUILD="$ONTOLOGY_ENGINEERING_ROOT/var/ghidra-native-build"
GHIDRA_RELEASE_ZIP="$ONTOLOGY_ENGINEERING_ROOT/var/downloads/ghidra_12.1.4_PUBLIC_20260921.zip"
test "$(shasum -a 256 "$GHIDRA_RELEASE_ZIP" | cut -d ' ' -f 1)" = ddac49f903da9d5bac833e5cc79395098b9c33cfd3279be5f31bd00387d2d4db
test ! -e "$GHIDRA_NATIVE_BUILD"
mkdir "$GHIDRA_NATIVE_BUILD"
unzip -q "$GHIDRA_RELEASE_ZIP" \
  'ghidra_12.1.4_PUBLIC/Ghidra/Features/Decompiler/src/decompile/*' \
  -d "$GHIDRA_NATIVE_BUILD"
GHIDRA_CPP="$GHIDRA_NATIVE_BUILD/ghidra_12.1.4_PUBLIC/Ghidra/Features/Decompiler/src/decompile/cpp"

make -C "$GHIDRA_CPP" -j2 ghidra_opt \
  'ARCH_TYPE=-arch arm64' \
  'ADDITIONAL_FLAGS=-mmacosx-version-min=11.0' \
  OSDIR=mac_arm_64
file "$GHIDRA_CPP/ghidra_opt"

# 确认 file 输出为 Mach-O arm64，保留构建日志与原件后安装本次产物。
GHIDRA_NATIVE_DEST="$GHIDRA_INSTALL_DIR/Ghidra/Features/Decompiler/os/mac_arm_64"
test ! -e "$GHIDRA_NATIVE_DEST/decompile"
mkdir -p "$GHIDRA_NATIVE_DEST"
install -m 755 "$GHIDRA_CPP/ghidra_opt" "$GHIDRA_NATIVE_DEST/decompile"
shasum -a 256 "$GHIDRA_NATIVE_DEST/decompile"
```

在新 shell 脚本中运行时使用 `set -e`，任一步失败即停止；已有产物先核来源，不盲目覆盖。
构建摘要与编译器版本记录为本机派生组件身份，不把新文件写成官方 ZIP 自带文件。
JDK 路径显式传给公开 `rea-launcher.cjs`，配置见 [WORKFLOW.md](../bindings/WORKFLOW.md)。

随后用自有小型制品实际运行原生分析，独立核制品摘要、符号与函数行为，并核清退。
仅文件存在、启动器检查或 JavaScript 分析通过，都不代替这一步。Ghidra 由上游
启动器在任务私有 home 中运行属于其原有隔离行为，不改变 Codex 的 HOME/CODEX_HOME。

先补齐并核验原生组件，再启动 REA MCP。本次已有 MCP 进程保留了补建前的缺组件诊断；原生 CLI 通过后，另起任务专属 stdio MCP 进程，实际打开、反编译与关闭同一制品也通过。不要为刷新组件状态重启别人的会话；现有会话保留失败记录，新会话使用已经准备好的安装。
