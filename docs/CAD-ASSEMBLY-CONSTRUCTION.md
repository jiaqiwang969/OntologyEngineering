# 有来源的装配构造与独立回读

`cad evidence verify-construction --input <请求> --output <新私有目录>` 使用显式 `oev2.cad-assembly-construction/1` 或 `/2`。它连接原构造job、构造意图、实际AddComponent诊断、保存前后观察、全部PRT输出和当前独立inspect_assembly，不启动CAD，也不自动采用工程结论。

`/1` 保留原字符串相等判据。NX 2412 官方 `UGOPEN/uf_assem.h` 在 `UF_ASSEM_ask_component_data` 文档中明确：整件选择通过该API返回 `None`；定义instance和实际part occurrence的参考集还可能不同。NXOpen则提供实际 `ReferenceSet()`、`EntirePartRefsetName()` 和 `SetEntirePartRefset()`。因此不把不同API/作用域的名称直接比较，也不将历史失败改签。

`/2` 要求创建端原NXOpen结果等于该次实际读取的vendor整件名；创建/保存观察中的原UF定义名称须为 `None`。独立检查另使用 `inspect_assembly` 的显式 `oev2.nx-assembly-reference-sets-params/2` 和 `capture_reference_sets=true`，在同一原生进程分别读取每个definition instance与实际part occurrence。`oev2.nx-assembly-reference-sets/2` 保留全部原字符串、两种tag和完整库存对应；两种当前选择均须为UF整件 `None`。捕获前后完整库存与参考集重读一致，缺失、重复、错tag、非整件、错误API或未请求捕获均拒绝。没有归一化名称、放宽几何容差或省略原来源、顺序、记账、保存、进程和Project检查。

原 `inspect_assembly` `/1` 不请求该新增数据，序列化和输出保持。原worker合同每个action只声明一个参数schema；部署时为新 `/2` 创建独立、完整绑定的worker descriptor，保留旧descriptor，不向同一descriptor插入重复action。源码可编译、平台接收、真实原生读取和当前安装使用分别登记；旧资料缺新观察时，`/2` 拒绝，不能靠补造回执取得通过。

复用现有原生receipt、event_index、source_assembly、assembly和当前实例owner；Project仍是唯一工程决定来源。新不透明能力只能由重新读取原件的Rust检查产生，序列化报告不能恢复。两端使用相同声明工程对象ID、revision和条件文本，保存CAD文件的实际SHA另外连接；构造意图中的对象来源声明与产生后的CAD文件摘要分别保存，不强行等同。

所有直接叶实例必须各有且仅有一项完整AddComponent结果，父级、原型、definition-instance和part-occurrence仅在构造进程内连接；保存和独立回读按完整路径、原型、文件闭包、单位、姿态及原几何持久化检查重新连接。全部保存文件必须与检查输入逐字节对应。独立性核host、pid和创建时刻，不靠tag是否改变判断。根文件相同但原型文件改变，同样拒绝。

生产caller重核完整Project、对象/条件与来源登记，构造及检查的job/native回执和原构造意图分别绑定，再交原Project::impact列出重审义务。结束前再次核原件与实际编译执行身份；报告不写Project、不继承原工程通过。

本合同提供构造到保存文件及独立回读的来源链；复制/删除的工程声明、原生Copy/Delete操作谱系、split/merge和任意持久拓扑命名分别验证，不能由构造成功自动推出。原correspondence/1及实例/2保持；后续消费者必须组合这些原owner及各自必要检查。

规范正本为 `specs/modules/cad-engineering-binding.toml`，共享输入由锁定真实ggen生成。合成磁盘回执只检验软件合同；历史SOURCE05原生资料按曝光的development_replay重新读取，不能授予当前源码或平台的原生资格。Mac/Windows、新原生运行、安装/选用/恢复及工程采用各看当前源回执。完整CAD16、63目标、Semantica19/19/22/13494、制造检验和实际项目实物计量继续保留。
