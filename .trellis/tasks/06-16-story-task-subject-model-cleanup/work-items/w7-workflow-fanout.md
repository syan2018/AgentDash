# W7 Workflow Fanout

## 状态

pending

## 依赖

- W4 done

## 目标

让 dynamic workflow 可以把 Task 集合作为 fanout 数据源，同时保持 Task 只表达计划项事实，workflow runtime 负责运行、依赖、批次、重试和审计。

## 输入

- W4 backend command / read model。
- `.trellis/spec/backend/workflow/architecture.md`
- `.trellis/spec/backend/workflow/lifecycle-run-link.md`
- `crates/agentdash-application/src/lifecycle/dispatch_service.rs`
- workflow / orchestration 相关 command 与 selector 代码。

## 范围

- 定义 Task selector 从 root LifecycleRun / Story projection 读取 Task 集合。
- `fanout_tasks` 基于已选 Task 创建 child AgentRun 或 workflow node dispatch。
- 单个 Task assign 与 workflow fanout 复用 assignment link 语义。
- Task facts 只保存计划层关系和必要来源，runtime evidence 从 Lifecycle projection 查询。

## 范围边界

- Task 只作为 fanout 数据源和计划项事实，原因是 workflow 依赖、批次、重试和审计属于 orchestration runtime。
- fanout 消费 W4 的 command / association，原因是单个 assign 与 workflow 批量派发需要共享同一解释链路。

## 验收

- workflow fanout 可以选择 Task 集合作为输入。
- fanout 创建的 child run 能通过 linked runs / SubjectExecutionView 被观察。
- Task 状态不会因 runtime failed / cancelled 自动变成执行态。
- assignment link 与单个派发路径一致。

## 产出记录

- 待填写。

## 风险与交接

- W8 需要 fanout 旧 dispatch preference surface 的清理结果。
