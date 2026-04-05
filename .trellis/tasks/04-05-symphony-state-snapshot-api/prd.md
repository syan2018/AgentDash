# Symphony Milestone: Orchestrator 运行时状态快照 API

## 目标

为前端 Dashboard 和调试提供项目级编排运行时的统一快照视图。

## 核心数据

快照应包含：
- **Running sessions**: task_id, session_id, started_at, last_event_at, turn_count, tokens
- **Retry queue**: task_id, attempt, due_at, error reason
- **Aggregate totals**: total_input_tokens, total_output_tokens, total_runtime_seconds
- **Orchestrator status**: 是否 enabled, last_tick_at, next_tick_at

## 是否画蛇添足？

这个 task 需要评估是否当前阶段真的需要：

**现有能力**:
- Session 事件已有实时 SSE 推送
- Task 状态已有 API 查询
- StateChange 日志已有完整历史

**新增价值**:
- 聚合视图（不需要逐 session 查询）
- 运行时诊断（retry queue、token totals 等当前无法直接获取的信息）
- Orchestrator 自身健康状态

## 待讨论

- [ ] 当前阶段是否需要？还是等 tick-loop 和 auto-continuation 稳定后再补？
- [ ] 复用现有 `/api/stories/:id/tasks` 路由 + 增加聚合字段？还是新路由？
- [ ] 前端展示需求：是否需要单独的 "Orchestrator Dashboard" 页面？

## 依赖

- symphony-tick-loop + symphony-concurrency-governor（需要 orchestrator 运行时状态已存在）

## 参考

- Symphony spec §13.3 (Runtime Snapshot)
- Symphony spec §13.7 (Optional HTTP Server)
