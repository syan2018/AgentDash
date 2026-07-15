# 07-10 实现链路为何偏离设计

## 结论

原架构任务的高层合同没有要求重写Session UI，也明确写过delivery receipt不等于business terminal、一个RuntimeCommand只有一个acceptance/terminal、brokered tool必须保留完整坐标。偏离发生在并行workstream的组合接缝：各agent用局部fixture证明自己的层成立，却没有任何一个验收者运行“production catalog + Native mapper + ToolBroker + Runtime outbox + PostgreSQL + Session reducer”的完整链路。

## 形成错误实现的链路

1. Native adapter workstream从Agent Core事件映射出main风格tool presentation，局部fixture证明vendor stream能够生成正确body。
2. Business surface workstream把所有production tool声明为`ToolBroker` presentation，同时在provision阶段把带bootstrap context的`DynAgentTool`冻结进binding；局部catalog测试只证明16个工具名称和schema存在。
3. Integration contract在`DriverToolDefinition`中遗漏presentation route，导致Native adapter不知道production binding已经选择ToolBroker；Broker也不知道Native仍在发布vendor presentation。
4. Outbox workstream用“driver调用返回”代表“delivery accepted”，而Native driver直到完整`run_turn()`结束才记录receipt。任何接受后的projection错误都被重新解释为dispatch失败。
5. Persistence/terminal测试分别证明journal能写、turn能终结，却没有断言正常TurnTerminal之后operation也终结；于是悬空operation直到下次错误才一起变成Lost。
6. Frontend parity测试只喂入已整理好的同ID事件，证明reducer能合卡，却没有证明production producer实际生成同一个ID。

因此每个局部测试都可以通过，但组合后必然出现双producer、双identity、bootstrap execution context和整条prompt重放。

## 为什么后续点修仍持续返工

- 只恢复某一种frame或某一个tool，会让catalog“看起来接上”，但同一冻结context下的其余provider仍然缺scope。
- 只放宽`fs_glob.pattern`，只能把第一个projection错误向后移动；receipt位置和outbox分类仍会在下一个错误处重跑provider。
- 只让ToolBroker或Native少发一次start，无法修复shared item identity、tool result readable-ref和wrapper turn坐标。
- 只跑一句文本不会进入tool continuation、Hook Runtime、workspace visibility和post-acceptance error路径。

## 新任务的防偏离机制

本任务以固定main commit的protected event body、真实PostgreSQL状态和现有前端reducer作为同一个oracle。四个工作项共享一张差异矩阵和一个production composition harness；任何局部实现只有在完整链路中关闭对应矩阵项后才算完成。这样新Runtime内部仍可重构，但外部Session行为无法再被局部fixture悄悄改写。
