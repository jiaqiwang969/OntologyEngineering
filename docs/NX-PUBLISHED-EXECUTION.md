# NX 显式 v3 发布、执行与结果取回

工程本体规定原生任务的对象、来源、版本与证据含义；specs中的传输/容量合同经ggen生成共享约束；v2 Rust协议、controller和worker执行；实际任务反馈进入同一Project及经验检验。程序仍由v2自己提供，旧cad-agent不作为执行入口。

旧v2归档暂存保留原100MiB边界，不能通过改schema把v3送入旧ZIP入口。显式oev2.nx-native-job/3分别声明输入/输出总量、单文件、文件数和单段预算并绑定job摘要。协议最大总量64GiB、单文件8GiB、文件数10000、单段100MiB；具体任务可以更小，动作内部的原生几何、单条记录和SDK边界仍分别检查。v3容量不是任意CAD模型均可完成的承诺。

按完整任务准备worker、运行库、构建/来源、参数及输入，不只传PRT。agent维护显式job和发布计划，调用本根oev2 cad nx transfer完成分段接收、重读验证、独占重建及不可变发布。发布身份绑定源计划、实际目录和完整文件摘要；来源根与执行根分别核物理身份。

cad nx stage-published使用显式请求执行prepare/inspect/recover。暂存核完整载荷和有界参数，保存来源绑定；只有本任务中断留下的可核记录可显式恢复，其余占用保持拒绝。不能把恢复历史当作当前运行状态，也不覆盖旧unknown。

cad nx run对v3流式核输入完整哈希、精确文件集合和参数，再由controller复核来源、登记单次派发意图、监督worker。Run返回待完成或观察中断时，继续查询同一job的status；不因超时再Run，不自动新建任务规避未知状态。

cad nx fetch-large固定完整输出清单、目录/文件及job/state身份，按有界片段传输，结束时再核远端完整清单和本地逐文件哈希。verify-fetched复查取回原件。单段校验、字节收齐、worker成功、独立重开、工程采用是不同结论；部分取回不写完整成功标志。

运动保存后的实际文件需要另一个明确job及新进程执行inspect_assembly_positions，保留worker、PID及创建时间。有限离散样本和包围几何核验不证明连续扫掠或完整整机。当前安装、实际执行组合和通过范围从maintenance/status.json及其原件读取，不继承候选或历史程序的资格。
