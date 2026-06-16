# W7 Workflow Fanout

## 状态

done

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

- 新增 `agentdash-application::task::fanout` 应用层基础：
  - `TaskFanoutSource` 支持从 root `LifecycleRun` 或 Story Task projection 读取候选 Task。
  - `TaskFanoutSelector` 支持按 Task id、计划状态、owner、assigned agent 与 archived 可见性筛选。
  - `fanout_tasks` 通过 `SubjectExecutionIntent(subject_ref=task)` 调用 `LifecycleDispatchService`，创建 child AgentRun 或 workflow graph append dispatch。
  - dispatch 成功后只回写 `LifecycleRun.tasks[].assigned_agent_id`，复用单个 assignment link 语义；不写 runtime status、failed/cancelled、artifacts、批次、重试或审计到 Task facts。
- focused tests 覆盖 root selector、Story projection selector、fanout dispatch 后 assignment link 写回，以及 Task plan status 不因 dispatch 自动变化。

## 风险与交接

- 当前只实现 application 层 selector / command；尚未接 HTTP/UI workflow fanout 入口，W5/W8 合流时需要确认前端入口是否消费该命令或另行补 route。
- `rg` 仍能找到旧 `dispatch_preference` surface，主要在 legacy Story-owned Task、旧 Task context/config、前端 Story/Task UI 与文档中；这属于 W5/W6/W8 清理面，不由 W7 恢复兼容。
- Permission / approval gate 只保留在 dispatch policy 入口形状中，默认开放；后续 permission 收束任务接管策略。
