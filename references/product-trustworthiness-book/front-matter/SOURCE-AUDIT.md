# 前言来源边界记录

更新日期：2026-08-19

## Semantica 架构声明的来源

前言新增的“书是人可读规范、Semantica 是唯一执行语义”不是 ISO 26262 内容，
而是本版软件/出版架构。其机器事实必须从源锁定 Semantica registry 与 manifest
核对：卷二章包为 `semantica.chapter_packages.vol2.ch01` 至 `vol2.ch20`，规范转述
domain package 为 `semantica.chapter_packages.vol2.normative`；当前均声明
`status=partial`、`release_status=blocked`。本记录只说明书稿为何这样表述，不能代替
package registry 验证、wheel 哈希、scenario receipt 或 release verdict。

前言开场的雨刷企业缘起来自用户以作者身份提供的直接叙事，并已记录为
`EVT-20260815-BOOK-ORIGIN-STORY-001`、`EVD-BOOK-ORIGIN-USER-001` 与
`CLM-BOOK-ORIGIN-001`。

作者随后补充的黑板闭环叙事已记录为
`EVT-20260815-ENTERPRISE-ONTOLOGY-LOOP-001`、`EVD-ENTERPRISE-ONTOLOGY-LOOP-USER-001` 与
`CLM-ENTERPRISE-ONTOLOGY-LOOP-001`。该补充明确：讨论从雨刷产品出发，但闭环中心是企业级工程本体，
研发、试验、质量、制造、销售、服务与员工经验都围绕共同对象和关系开展活动，并将有边界的新证据或知识候选回流。

该来源可以支持以下内容：

- 一家专门做汽车雨刷系统的企业找到作者；
- 对方认为工程标准有价值，希望把它们做成 Agent，并看见其中的商业价值；
- 作者长期从事 AI 原生工程本体，双方一拍即合；
- 作者进一步把问题深化为一门以工程本体论为根的 AI 原生工程学科，并由此形成写书动机。
- 当时的黑板讨论把企业级工程本体放在中心，并把研发、销售、员工学习等企业活动理解为读取共同状态、
  产生新证据、核验后更新共同状态的闭环。

该来源不能支持企业名称、人物身份、商务合同、金额、市场规模、已经交付的 Agent 能力、
真实产品配置、测试、缺陷、放行或认证状态。前言因此保持匿名，不补造项目结果。

用户提供的储罐研发闭环图只支持上述抽象思想，不支持把图中的储罐、材料层级、仿真、实验装置、曲线、
箭头或阶段关系迁移为雨刷企业事实。该图不进入当前读者视图，任何后续企业闭环图都必须在相关章节语义冻结后
重新建立对象表、邻接表和 Figure Contract。

前言对工程本体论、LLM/Skill/Agent 分工和 Unknown 边界的方法来源，与第 1 章
正文及 `semantica.chapter_packages.vol2.ch01` 的 `source-audit` 迁移资产一致。汽车功能安全只作为严格纵向样板自然说明，
不显示内部条款坐标，也不把安全推广为全部产品可信问题。

前言只负责作者缘起、全书动机、范围和读法，不承担第 1 章 EPS 合成案例的事实来源，
也不承担任何章节完成、产品放行或出版发布结论。
