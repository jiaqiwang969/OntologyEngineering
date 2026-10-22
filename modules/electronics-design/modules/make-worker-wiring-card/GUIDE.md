---
name: make-worker-wiring-card
description: Create evidence-backed, worker-facing wiring instructions from actual hardware photos, cable colors, connector labels, silkscreen markings, and an approved connection map. Use for shop-floor wiring cards, sensor harnesses, module-to-controller hookups, soldering guides, one-page color XeLaTeX/PDF wiring sheets, or when a technical schematic must be translated into direct “object + wire color + printed terminal” actions. Also use to simplify or audit an existing wiring guide before workers build or power it.
---

# Make Worker Wiring Card

本文件是 [electronics-design](../../SKILL.md) 的内部专业指南；从总入口按需读取，脚本与资源路径均相对本模块目录。

## v2 工程接续

用于 v2 管理的工程项目时，由 `ontology-engineering-v2` 主导，先按
[electronics-design 接续约定](../../references/v2-handoff.md)
核同一 Project、对象修订、条件、来源和本次证据缺口；以下专业操作保留其实际工具边界。
结果连同原生文件、输入/工具版本、来源摘要、覆盖、失败与未知交回原 Project，
不另建决策账本，不把操作完成或本工具检查通过当成工程采用。
用户当前授权及后台不抢焦点要求优先；需要前台才能完成的项保持未执行并说明限制，
不能依据下文历史操作方法自动激活应用。若单独分发缺少上述接续文件，读取已安装的
`electronics-design` / v2 正本；仍缺失则说明缺项，不声称 v2 接入已完成。

本系统执行限制：浏览器/桌面沿当前用户指定的 Peekaboo 现有后台能力，观察禁网页聚焦回退，
交互禁自动聚焦；不按旧参考自举 Chrome 调试实例、使用 Chrome 插件或 osascript，
不读取登录存储/浏览器凭据，不静默重启浏览器、改设置或重装连接器。
任务临时服务仅在当前任务需要且已授权时启动，保存归属；用完核归属后清退本任务进程，
不停止其他会话或用户已有服务，不设置自启/自动重连。
在 v2 项目内使用裸 JS/API 桥前，先核主工具 typed action 确实未覆盖且当前授权包含该
debug 路径；沿已有授权执行，不重复索要许可，也不通过子 skill 绕过主工具阶段检查。

Produce a card a worker can follow without understanding the protocol or circuit theory. Treat the card as an execution layer derived from verified evidence, not as a substitute for an engineering schematic.

## Establish the evidence

1. Inspect the actual controller, intermediate boards, connectors, probes, cable labels, and wire colors. Prefer original photos and manufacturer markings for the exact purchased item.
2. Orient every board with a visible anchor such as `VIN at upper left`, keyed connector notch, or mounting hole. Transcribe silkscreen in its physical order.
3. Build a connection matrix before drawing. Start from [assets/connection-matrix-template.csv](assets/connection-matrix-template.csv). Give every physical wire one row.
4. Record the evidence for each non-obvious mapping with [assets/evidence-register-template.csv](assets/evidence-register-template.csv).
5. Never infer wire function from customary colors. If a label, datasheet, continuity result, or exact-module evidence does not establish the mapping, mark it `HOLD` and request the missing evidence.
6. Distinguish identical words on different objects. Write `ESP32 VIN`, `multiplexer VIN`, and `probe VCC`, never an unqualified `VIN` where confusion is possible.

For electrical design, voltage, pull-up, address, protection, or pin-function review, also use `electronics-design`. For a formal schematic or PCB, route to the appropriate EDA skill. Do not present a worker illustration as DRC or schematic proof.

## Freeze the connection matrix

Before layout, verify:

- source object and visible source marking;
- physical wire or sleeve color;
- destination object and visible destination marking;
- voltage domain and shared-return requirements;
- repeated-channel numbering and installed role;
- unused pins versus pins that workers leave at a verified factory default;
- evidence status for every row.

Resolve every `HOLD` before issuing a buildable card. Keep unknowns explicit; do not replace them with a typical wiring convention.

## Translate engineering facts into worker actions

Use these presentation rules:

- Keep the normal card to one A4 landscape page and require color printing.
- Lead with the exact physical objects: `controller → purple board → probe 1`.
- Show each action as `wire color → printed terminal`, with both color fill and text.
- Repeat all wires inside every repeated channel block. Do not make the worker infer channel 2 from channel 1.
- Number both ends of every repeated cable or probe before connection.
- Show board orientation and all used silk labels at readable size.
- Put evidence-photo crops beside the diagram with a caption. Do not make them look like components on the illustrated board.
- State power-off and wrong-voltage prohibitions using fully qualified object names.
- For shared power or ground, specify a terminal block or splice. Do not instruct workers to force many conductors into one plated hole.
- Use a red cross to mean `worker does not connect this terminal`. Only call a pin electrically `NC` when the exact hardware evidence proves it.
- If a board relies on onboard address or reset resistors, write `keep factory default` and retain the exact-module evidence.
- Remove protocol addresses, software setup, theory, calibration discussion, and research prose from the worker page unless they change a physical connection.

Read [references/card-contract.md](references/card-contract.md) for the mandatory content and acceptance checklist. Use [assets/worker-card-layout.tex](assets/worker-card-layout.tex) only as a layout starting point; replace every placeholder and rebuild from the frozen matrix.

## Render and verify

When delivering a PDF, also use the native XeLaTeX/PDF workflow available in the environment.

1. Compile with XeLaTeX in a temporary build directory. Stop on errors and disable shell escape.
2. Confirm one page, A4 landscape, embedded fonts, and no missing-character, overfull, underfull, or package errors.
3. Render at 300 dpi and inspect the whole page visually for clipping, overlap, tiny text, ambiguous arrows, and color-to-terminal alignment.
4. Extract PDF text and check every connection row against the frozen matrix. Specifically audit common reversals such as data/clock, positive/negative, transmit/receive, and channel numbers.
5. Obtain an independent read-only review when the card will control soldering or first power-on.
6. Deliver the print PDF, editable source, frozen connection matrix, and evidence register. Preserve the engineering version separately when one exists.

Do not authorize first power solely because the PDF renders correctly. Require a physical continuity/polarity review appropriate to the hardware and have a responsible person approve energization.
