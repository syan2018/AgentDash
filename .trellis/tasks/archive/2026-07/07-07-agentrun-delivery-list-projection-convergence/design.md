# Design

## Architecture

AgentRun list 是后端 read model projection，事实源是 lifecycle tree、AgentLineage、current `AgentRunDeliveryBinding`、runtime session meta/title 和 subject association。前端只消费 project event stream 上的投影失效并重新读取列表 projection。

本任务收束两个 seam：

1. 后端 delivery 状态写入 seam：`AgentRunDeliveryStateService`
2. 前端列表收敛 seam：`agent-run-list-state-store`

## Backend Contract

`AgentRunDeliveryStateService` 应承担 current delivery binding 状态变化的写入与投影失效发布。调用方不再在写完 binding 后各自拼 `ProjectProjectionInvalidation`。

目标接口形态：

```rust
AgentRunDeliveryStateService::mark_running_from_accepted_turn(...)
AgentRunDeliveryStateService::mark_terminal_from_runtime_session(...)
```

每个方法负责：

1. 解析 runtime session anchor / current binding。
2. 验证 stale runtime 不覆盖当前 binding。
3. 通过 `AgentRunDeliveryBindingRepository::upsert_if_current_runtime_session` 条件写入 `AgentRunDeliveryBinding`。
4. 只在条件写入成功后发布 `ProjectProjectionInvalidation::agent_run_list(...)`。
5. 返回 typed result，供调用方继续做 frame commit、mailbox terminal effect 等后续业务。

投影失效 reason 使用现有 `AgentRunActivityChanged` / `DeliveryTerminal` 即可；本任务不强制新增协议 enum。关键合同是：任何会改变 `AgentRunWorkspaceListEntry.shell.delivery_status` 或 shell activity 的 delivery binding 写入，都必须发布 `projection=agent_run_list`。

仓储接口必须显式实现条件写，而不是继承读后写默认行为。原因是旧 runtime 可能在读到自己仍为 current 后，被新 runtime 抢先替换 current binding；只有数据库/仓储层条件写才能保证旧 runtime 不覆盖新 binding，也不发布误导性的列表失效事件。

## Frontend Contract

`agent-run-list-state-store` 是列表投影的唯一前端收敛 module。它继续通过 `fetchProjectAgentRuns` 拉后端权威列表，但 first-page refresh 需要支持 dirty generation：

```text
invalidateProject(project)
  -> dirtyGeneration++
  -> ensure refresh loop scheduled

refresh loop
  -> snapshot dirtyGeneration as target
  -> fetch first page
  -> appliedGeneration = target
  -> if dirtyGeneration > appliedGeneration, repeat
```

这样项目事件可以密集到达，但 first-page 请求始终串行，且不会因为已有请求正在进行而丢掉后续失效。

## Data Flow

```text
Runtime accepted turn
  -> launch commit / accepted turn lifecycle
  -> AgentRunDeliveryStateService.mark_running_from_accepted_turn
  -> AgentRunDeliveryBindingRepository.upsert_if_current_runtime_session(status=Running)
  -> ProjectProjectionInvalidation(agent_run_list, AgentRunActivityChanged)
  -> ProjectEventStream
  -> agent-run-list-state-store invalidateProject
  -> GET /agent-runs
  -> list row delivery_status=running
```

```text
Runtime terminal
  -> AgentRunTerminalConvergenceService
  -> AgentRunDeliveryStateService.mark_terminal_from_runtime_session
  -> AgentRunDeliveryBindingRepository.upsert_if_current_runtime_session(status=Terminal)
  -> ProjectProjectionInvalidation(agent_run_list, DeliveryTerminal)
  -> ProjectEventStream
  -> agent-run-list-state-store convergence refresh
```

## Testing Strategy

Backend:

- `AgentRunDeliveryStateService` unit tests assert running and terminal transitions publish list invalidations after conditional binding persistence.
- Existing stale runtime tests continue to assert no stale binding overwrite and no projection notification for ignored stale transitions.
- Repository tests assert conditional writes reject stale runtime rows and accept the current runtime row.
- Existing launch commit tests are updated to observe delegated delivery-state projection behavior.

Frontend:

- Store tests keep existing project event refresh behavior.
- Add in-flight invalidation test with controlled `fetchProjectAgentRuns` promises to prove a second refresh runs after the first settles.
- Keep non-list projection ignore test.

## Tradeoffs

This keeps project event stream as the list convergence mechanism instead of wiring runtime session stream into global list state. The reason is that AgentRun list can contain many roots/children across runtime sessions, and its facts are project-scoped; runtime stream remains workspace-local detail.

The backend may still have module-local helper methods for resolving frame/project refs, but delivery binding write + list invalidation should stay behind one service interface so future state changes do not spread notification responsibilities across callers.
