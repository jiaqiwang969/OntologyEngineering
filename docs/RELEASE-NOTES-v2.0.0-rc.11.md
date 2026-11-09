# OntologyEngineering v2.0.0-rc.11 — CAXA 图纸的尺寸文字与检查页

**用 CAXA 等软件画的图纸，读出的尺寸文字与 AutoCAD 打开时显示的一致；检查页不再用色块盖住尺寸数字；锁定的转换器可以从干净克隆构建。读图规则只改了一处：嵌套块里的尺寸文字。**

这是 v2.0.0 的第 11 个整合候选（预发布，重构中）。rc.10 发布后，我们用它读了四张没见过的图纸，其中两张由 CAXA 绘制，查出转换器、检查页和一条读图规则的问题。本版修正这些问题，读图知识和其他规则不变。

## 这一版有什么

| 内容 | 位置 |
|---|---|
| 检查页：尺寸文字背后的遮罩（DIMTFILL 背景、擦除）保持纸面颜色，画在遮罩上的内容保持自己的颜色。离线浏览器检查在每种页面模式下核对这两点 | `crates/oe-drawing/assets/expression-review.html`、`crates/oe-drawing-check` |
| 嵌套块里的尺寸文字：尺寸块里没有直接文字时，读嵌套块参照里的文字（最多 4 层，套用插入点、比例和旋转），按文字方向排序。主文字行上下各一段的偏差写成叠放公差。表达记 `display_text_nested_blocks`；有直接文字的尺寸读法不变 | `crates/oe-drawing/src/engineering_expression.rs` |
| 转换器锁定：CADCraft `cadcraft-cli 0.15.1-aixcam.8 (dev)`，fork `fix/caxa-dwg-fidelity` 提交 `085d8c7` | `integrations/runtime-locks/cadcraft-source.json` |

## 转换器的变化（CADCraft aixcam.6 至 aixcam.8）

- **尺寸块：** 尺寸自己的匿名块按读入时的样子保留（CAXA 用 `*X`，不是 `*D`），没改过的尺寸不再按几何重新生成文字。
- **前缀：** DIMPOST 前缀（`R<>`、`%%C<>`）取代默认的 R 或直径符号，不再出现重复前缀。
- **遮罩：** SVG 给遮罩加上 `data-mask="background"`。
- **字体包：** `cadcraft.font-pack/2` 可以加入 TrueType 字体文件（.ttf、.ttc、.otf，每个不超过 32 MiB），并声明替代字体；渲染结果会列出用到的替代。字体不随仓库分发。
- **形位公差字体：** 用 gdt、amgdt、CAXA 的 CXGDT 写的多行文字，画成形位公差符号。
- **缺字：** TrueType 字体缺的字符（如 `%%c` 的直径符号）改用 CADCraft 的制图字体画，不再显示缺字方框。
- **acadrust 补丁：** acadrust 0.6.6 加了一个补丁，R2007 以前保存的 TrueType 字体名按图纸代码页读取，GB2312 的中文字体名不再变成乱码。补丁发布在 fork `jiaqiwang969/acadrust` 的 `aixcam/legacy-typeface-codepage` 分支，CADCraft 按固定提交引用，所以能从干净克隆构建。

**升级后要按锁重新构建 cadcraft-cli，并写新的运行记录。** aixcam.7 及以前的运行记录会被路线拒绝。

构建时请直接用 1.95.0 工具链自带的 cargo 和 rustc。用非默认的 `CARGO_HOME` 运行 `rustup run 1.95.0 cargo` 时，PATH 上的其他 rustc 可能被用上；craft-pdf 要求 rustc 1.95，构建会失败。我们复核时第一次就是这样失败的。

## 效果

在六张图纸上（都不随仓库分发）：

- **四张新图：** 转换器修正后，四张新图上的尺寸文字与 AutoCAD 打开原图时显示的一致，包括写在嵌套块里的文字。嵌套规则修正后，一个带公差的直径尺寸从“只读出测量值的线性尺寸”变为带上下偏差的直径尺寸。
- **另两张图：** 用来写规则的两张开发图纸，读出的关系图逐字节不变。
- **局限：** 这四张图都用来发现和修正问题，结果不说明能用到别的图上；专家对这四张图的复核尚未完成。

## 验证

- **生成：** 锁定真实 ggen 重生比对 `consistent`、`drift=[]`，经验库合同 `matched`，八个程序按锁定依赖构建。从 GitHub 干净克隆发布提交后，再次核对生成一致。
- **测试：** 工作区单元 5 项、来源缓存 3 项、工作区与任务视图集成 55 项（另 1 项 ignored）、复核接续 12 项、CLI 失败原因 8 项、读图 148 项、独立复核 2 项、读图 CLI 7 项、义务 6 项、engineering-memory-runtime 109 项（另 2 项 ignored），全部通过；ignored 的既有案例不计通过。
- **转换器：**
  - 从 GitHub 干净克隆 CADCraft `085d8c7`，用空的非默认 `CARGO_HOME`、1.95.0 工具链构建，161 秒完成。
  - 换成从 fork 取补丁后，六张图转出的 DXF、SVG 和预览图，与原先用本地补丁构建的程序逐字节相同。
- **读图：** 发布构建读六张图，关系图与发布前的内部运行逐字节相同，两遍结果相同。
- **管理程序：**
  - 在一个 326 次提交的长期 Project（不随仓库分发）的克隆副本上，本版与 rc.10 的完整恢复输出逐字节相同；本版写入的 `/6` 提交，两者读出也逐字节相同。
  - 本机另外 20 个 Project 的克隆副本上，两者结果全部相同（其中 4 个两者都拒绝，原因与本版无关）。

## 已知限制

- **不是工程采用：** 识别结果是待复核的候选。
- **未读懂的内容：** 四张新图里还有不少没有读懂，例如手画的形位公差框、明细表、部分注释，等专家复核后再纠正。
- **字体：** 字体包不随仓库分发。没有字体包时，渲染仍可用 CADCraft 自带的制图字体，中文字宽可能与原图不同，个别文字会压线。
- **其他：** [rc.10 发布说明](RELEASE-NOTES-v2.0.0-rc.10.md)列出的已知限制仍然有效。

## 安装与升级

```sh
cd "$HOME/OntologyEngineering"
git fetch --tags && git checkout v2.0.0-rc.11
sh scripts/install.sh
"$HOME/OntologyEngineering/var/install/bin/oev2" --root "$HOME/OntologyEngineering" --version
```

- 安装器保留上一安装为 `var/install-previous.*`，私有数据目录 `var/` 与 `sources/` 不受影响。
- 已有工作区无需迁移。
- 已打开的会话要重新打开，才会加载新版技能和 MCP 服务。
- 读图前，按 `integrations/runtime-locks/cadcraft-source.json` 构建 cadcraft-cli，并写运行记录（见[读懂已有工程图纸](DRAWING-UNDERSTANDING.md)）。

本版不授予工程采用，项目的全部根验收仍按各自证据判断。
