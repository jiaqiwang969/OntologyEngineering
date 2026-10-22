# 统一 RC 的范围与验收

当前源码候选为 **2.0.0-rc.6，重构中**。工程本体与 engineering-memory 共用 workspace、Cargo.lock 和语义实现，通过同一安装的 `oev2` 与 `oev2 memory` 使用。当前实际验证见 [UNIFIED-VERIFICATION.md](UNIFIED-VERIFICATION.md)。

本轮按用户要求移除 Jev 及相关专用判断服务的客户端、配置与自动调度。当前 Agent 承担开放式任务组织和资料审阅，Rust 执行明确检查。其余工程、语义、来源与采用判据保持；不能把已取消的服务目标重新列为必须恢复的依赖。

## 当前交付内容

| 内容 | 入口与范围 | 验收边界 |
|---|---|---|
| Project 与变化审查 | 对象、版本、条件、来源、决定、依赖、任务视图、路线锚定与重审 | 当前对象/条件必须一致，unknown、失败及独立采用批准保留 |
| 工具结果接收 | `project tool-result verify/register/compare/registry`、`project decide` | 重读原件和程序身份；登记不自动支持决定或授予采用；性能比较须有可比基线 |
| 内置工程记忆 | `memory` 的 record、recall、verify、history-index、session 与 Agent 审阅组织 | 原文、时间线、纠正、适用条件和因果未知保留；召回为待核候选，使用效果另验 |
| Project-memory 双侧交接 | `project memory-candidate`、`project memory-feedback`、`memory session` | 当前Project与经验修订双侧核验；记忆不能直接支持决定、关闭验收或冒作工具原件 |
| 来源与局部工程工作 | 本地知识登记/检索/原页、`project work`、明确计算及工程前提检查 | 检索命中与来源匹配不证明工程适用，空资料库正常报告缺项 |
| 完整过程记录 | Harness 显式 checkpoint、空候选归档 | 完整原件、预算、未知与失败保持；recorded不等于工程通过，非空候选不能免审 |
| Rust语义与生成 | 已实现的 kernel、source-session、规则、合同生成和版本检查 | 有限合同支持不等于全部Semantica兼容；实际生成、编译、行为与安装分开验证 |
| 六工具关联 | 上游安装、四份Skill diff、两份companion、公开小案例 | 每工具分别核版本、输入、结果与覆盖；缺工具时明确缺项，不虚构通过 |
| 可选模型、浏览器与CAD | 通用chat/embedding、显式模型辅助、原生观察和v2自有CAD执行 | 需实际资产、服务或SDK；本次整合不扩大其平台、动作授权与工程资格 |

发布源码不包含私人书籍、session、记忆库、原生原件、客户项目、设备配置、凭据或编译缓存。公开合成及有源推导 fixture 保留；冻结历史输入只作字节/来源回归，不能作为旧服务的执行入口。纯DOM观察算法保留合法上游署名和许可证，其来源名称不代表依赖模型服务。

## 安装与版本身份

新用户按 README / AGENTS 使用源码安装器，构建八程序：`oev2`、`oev2-design-bootstrap`、`oe-code-runner`、`oe-code-evaluator`、`oe-evolution-driver`、`oe-runtime-linker2`、`oe-runtime-rustc2`、`engineering-memory`。

`var/install/INSTALLATION.json` 使用新 `oev2.installation/1`，绑定真实根与实际程序摘要。默认development profile与优化release profile分别记录，不能混用构建资格。安装成功不表示案例、专业工具或工程验收已经通过。

历史D85与D86记录保持原版本、来源及输入身份。旧 `dist release` 的Mac /1、Windows /2仍是本体七程序发行格式，保留用于其历史范围；它们不代表本候选已完成统一八程序安装。全局旧 `oe` 不被替换。

本候选继续显式使用 `oev2.rc5-local-source/1` 的来源feature集合合同：CLI经kernel编入 `source-session-local-v1` 和 `rdflib760-nullable-internal`，独立库无默认feature。该profile名代表既有特性合同，不把当前rc.6改签为rc.5。公共接口版本及支持格式按[来源会话](SEMANTIC-SOURCE-SESSION.md)与其后续显式合同核验。

## 仍未闭合的工程范围

- 完整Semantica范围、原19模块/19协议/22 owner/13494条目的逐项行为、全库兼容及声明平台，按当前迁移台账继续验证。
- 六书与两卷方法书的完整知识/治理接续，不能由单页可读、OCR或检索覆盖证明完成。
- 全部原生CAD、装配运动、制造图、BOM、工艺、检验与实物计量，分别保留具体对象及独立验收。
- 供应商完整下载、浏览器流程、平台SDK与工作负载的接通，按当前版本、任务与工具证据判断。
- 未见任务的经验迁移收益、方法质量、成本及完整RRSI效果，不能由软件回归或一次开发案例证明。
- 其他平台、干净离线安装、原生双构建与完整发行包接收，按实际执行范围报告。

原工程目标的非服务部分保持原义；本次软件整合不重开已有历史工程结果，也不继承它们的通过。

## 本轮发布验收

1. 冻结统一源码、版本、规范、上游工具与diff身份；真实锁定ggen生成并实际重生比较。
2. 构建并运行受影响的本体、memory与安装检查；保留来源篡改、条件过期、缺工具、版本不符、预算及失败反例。
3. 在独立配置和空测试库执行完整 Project—工具—memory 回流，再由新会话召回；多个模块各自通过不能替代该联调。
4. 检查公开文件、许可证、来源边界及文档命令，更新原GitHub仓库并保留历史。
5. 保全旧安装与私人数据后，移除本次涉及的旧Skill；从固定远端提交重新安装，不借用旧入口或隐藏配置。
6. 对新安装重新执行核心闭环与六工具案例，报告真实覆盖、未运行和局限；新安装通过不授予未验证的工程能力。
