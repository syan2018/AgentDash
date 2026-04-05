# Symphony Milestone: 运行时活跃对账

## 目标

周期性检查 running session 对应的 task/story 是否仍处于 eligible 状态。
若外部状态已变更（用户手动 cancel、story 完成等），及时停止不再需要的 session。

## 核心行为

```
reconcile(running_sessions):
  对每个 running session:
    task = load_task(session.task_id)
    story = load_story(task.story_id)

    if task.status in [Completed, Failed, Cancelled]:
      → stop session, release slot
    if story.status in [Completed, Failed, Cancelled]:
      → stop session, release slot
    else:
      → 更新 in-memory 状态快照
```

## 与 Stall Detection 的关系

两者都是"安全网"，但检查维度不同：
- **Stall Detection**: 检查 session 自身健康（是否有活动）
- **Runtime Reconcile**: 检查业务状态（task/story 是否仍需要执行）

可以在同一个检查循环中执行，也可以独立。

## 架构定位（设计对账 #2 结论）

> **事件驱动实现。当 task/story 状态变更时主动通知 session hub 停止相关 session。**
> **不需要周期性轮询。**

## 推荐实现

在 task/story 状态变更的写入路径上，增加 session 生命周期联动：

```
task_status_changed(task_id, new_status):
  if new_status in [Completed, Failed, Cancelled]:
    if task.session_id is not None:
      session_hub.cancel(task.session_id)
      release_concurrency_slot(task.project_id)

story_status_changed(story_id, new_status):
  if new_status in [Completed, Failed, Cancelled]:
    for task in story.running_tasks():
      session_hub.cancel(task.session_id)
      release_concurrency_slot(task.project_id)
```

## 现有基础

- `TaskStateReconciler` 已有 boot-time 对账逻辑
- `SessionHub.cancel()` 已有会话取消能力
- `StateChange` 事件流记录所有状态变更
- Story/Task status 变更已有完整 API

## 待讨论

- [ ] 选项 A / B / C？根据设计对账 #1 的结论，A 最自然但 C 最及时
- [ ] 是否 A + C 结合：事件驱动做"及时停止"，Agent tick 做"兜底检查"？
- [ ] 对于外部取消的 session，是否需要 cleanup workspace（参考 Symphony）？
- [ ] AgentDash 没有 Symphony 的"terminal state workspace cleanup" 需求（workspace 是逻辑实体，不是临时目录）

## 依赖

- symphony-tick-loop（若选 A，嵌入 Agent tick）
- symphony-concurrency-governor（释放槽位）

## 参考

- Symphony spec §8.5 Part B (Tracker State Refresh)
- `crates/agentdash-application/src/task/state_reconciler.rs`
