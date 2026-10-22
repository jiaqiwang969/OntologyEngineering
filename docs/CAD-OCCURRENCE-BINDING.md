# 当前装配实例与原型特征绑定

`cad evidence bind-occurrence --input <源请求> --output <新私有目录>` 接收显式 `oev2.cad-occurrence-binding/1`。agent根据工程问题整理输入；用户不用填写规范文件。原型来源使用原 `CadCorrespondenceState1`、实际inspect_detailed及独立圆边意图；根装配另给完整inspect_assembly原件、ObjectIdentity和同Project的原context。

Rust复用原receipt、source、圆边和装配/运动owner：重读两个任务的完整来源，确认原型PRT字节与装配中实际引用相同，按完整父路径唯一解析实例，核父级组合/原始矩阵/当前根位姿。当前profile仅接整个零件reference set和proper rigid transform。两个相同原型可以得到各自实例使用；换路径但沿用旧位姿、错原型、缺路径或重复同一实例特征不得通过。

输出中的根坐标圆由实际原型圆加实际装配变换推导，保留各自进程的原tag与观察摘要；它不是原生occurrence edge的读回，不可拿原型edge tag向根装配发命令。旧correspondence/1与图纸消费者继续按原合同，不能读一份JSON报告恢复verified能力。后续跨版本对应应复用本owner，并继续取证移动、复制、删除、split/merge、原生图纸与完整Project变化反馈。

生产入口使用原Project owner检查两个对象/context、来源登记及依赖，再次读回后输出当前绑定与影响。它不改写Project或adopted。材料、载荷、制造、配合和实际项目最终的专属精度要求仍需各自工程证据。

本轮实际能力与未验范围看维护回执。已曝光旧source04 native数据属于development_replay，重读不算新的原生执行或当前平台接收。原16项、63目标、完整Rust Semantica及19/19/22/13494范围保持，完整发布/独立审查与RRSI收益分别验收。


Project结论使用原有固定值，说明文字留在目的/条件；上下文必须通过实际`project context`生成，不在外部编排中复刻Project规范化。发现无效回放输入时保留原冻结输入与失败，建立明确的新记录和上下文；不放宽原Project/几何判据。三个源/登记/条件拒绝沿既有安全错误owner仅公开固定原因，完整私有错误不进入CLI输出。是否实际通过仍以当前程序回执为准。


[显式跨版本实例对应/2](CAD-INSTANCE-CORRESPONDENCE.md)复用本owner；内部空选定使用端仍核完整原型/装配来源，旧公开入口维持非空要求。Project检查由同一CLI helper复用，不能拿不同请求与旧verified值配对。实际当前原生、跨平台、施工图及原16/63验收继续。
