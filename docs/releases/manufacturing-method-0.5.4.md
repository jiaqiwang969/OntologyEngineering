# 制造工艺与成本管理方法 0.5.4

本版承接公开的 [0.5.3](manufacturing-method-0.5.3.md)，将 CAD Agent 的原生几何证据接入工程本体论和制造工艺判断。源码合入 `main`，以 `manufacturing-v0.5.4` 标签固定；独立核心 ZIP 作为同一 GitHub Release 的附件提供。

制造方法在原有“要求—工艺—资源—成本—反馈”闭环中增加因果与反证关口：工艺替代先逐项核对原工艺承担的必要功能，专家建议保留为可质疑的候选主张，几何、仿真和试验中的挑战沿依赖关系回写到方案。CAD 模块核对模型来源、原生回读、孔／界面／焊缝可达性与版本变化，向工艺模块交接项目证据和双向问题；正式语义执行仍只由 Semantica 承担。

| 组件 | 本版状态 |
| --- | --- |
| 制造方法 | 0.5.4 |
| 两份记录模板／其他模板 | 0.2.1／0.2.0 |
| 六张评审卡 | 1.4.1 |
| 报告与工艺图模板 | 1.0.0 |
| 冻结制造语义包 | 0.1.1，保持候选状态 |
| Semantica 运行时 | 0.6.5+oe.6，精确来源及 wheel 锁定 |
| Fusion 执行适配器 | 0.1.0+oe.1，组件版本独立于方法版本 |

公开核心附件按 99 项精确白名单生成，排除客户图纸、模型、对话、真实项目参数、历史 CAD 案例、两卷书稿与未清权插图。包内 `PORTABLE-MANIFEST.json` 固定每个文件的 SHA-256；[发布资产台账](https://github.com/jiaqiwang969/OntologyEngineering/releases/download/manufacturing-v0.5.4/ontology-engineering-core-v0.5.4-assets.json)说明来源、许可依据与公开批准。组件权利见 [COMPONENT-NOTICE](../COMPONENT-NOTICE.md)。`main` 保留原有两卷书来源与历史文件；其[整体发布状态](../PUBLIC-RELEASE-STATUS.md)不因本次核心包改变。

独立核心在全新目录通过文件闭包与哈希、Semantica 唯一后端严格门禁、68 个冻结合成制造案例、6 项 CAD／工艺交接测试及 3 项制造投影测试。发布 ZIP 及三个嵌套 ZIP/wheel 共 1,082 个成员完成直接标识扫描，PDF 元数据经检查；Fusion 在线业务操作不在便携验收范围内。这些检查不代表客户工艺或产品放行。

独立核心 ZIP SHA-256：`ff9bc9592e495039202bdc899fb188e58e2ed4e6ddd06d826657298d74c023d7`。
