# AgentRun Delivery List Projection Convergence

## Goal

AgentRun 状态列表必须稳定收敛到后端真实 AgentRun delivery 状态。Companion / SubAgent 被创建后，后续从 ready/idle 进入 running、terminal、lost 等状态时，列表和快捷入口都应通过 project 级投影失效事件刷新，而不是依赖 workspace 打开、composer submit、turn end 或手动刷新路径。

## Background

当前用户观察到：创建 Companion 新会话后，列表能出现对应 AgentRun，但该 Companion 长期显示待机；实际会话已经进入正常运行状态。

已确认代码事实：

- AgentRun 列表状态来自 `AgentRunWorkspaceQueryService::resolve_list_item`，该 read model 从当前 `AgentRunDeliveryBinding` 派生 `shell.delivery_status`。
- 前端 `agent-run-list-state-store` 只在 project event 的 `ControlPlaneProjectionChanged(projection=agent_run_list)` 或 `StateChanged` 时刷新列表。
- Companion 创建路径会通过 lineage 变更发布 `agent_run_list` 投影失效，因此新 Companion 能出现在列表。
- running / terminal 等 delivery 状态写入分散在 `launch_commit`、`delivery_state`、`terminal_convergence` 等路径，投影失效不是由 delivery binding 状态事实源统一发布。
- 现有前端 first-page refresh 有 in-flight 去重；如果 refresh 正在进行时又收到失效事件，可能 await 旧请求后结束，留下旧快照。

## Requirements

1. Delivery 状态写入与 AgentRun list 投影失效必须在后端同一事实边界收敛。
   - 写入当前 `AgentRunDeliveryBinding` 为 running / terminal 等列表可见状态后，应发布 project 级 `ControlPlaneProjectionChanged(projection=agent_run_list)`。
   - 事件 payload 必须携带 run、agent、frame、delivery runtime session 信息，方便前端和调试定位。

2. AgentRun list 刷新语义必须由 project 投影失效驱动。
   - 列表 UI 不根据 workspace runtime stream、keyboard submit 或 Companion 创建动作推断状态。
   - 列表 store 收到 `agent_run_list` 投影失效后重新拉取后端列表 projection。

3. 前端列表刷新必须抗竞态。
   - 如果刷新请求 in-flight 时收到新的 `agent_run_list` 失效，当前请求结束后必须再次刷新，直到没有未应用失效。
   - 连续多个失效事件应合并为串行收敛，不产生并发 first-page 请求风暴。

4. 回归覆盖 Companion / child AgentRun 状态收敛。
   - 子 AgentRun 创建后的 lineage 刷新继续可用。
   - 子 AgentRun accepted turn 后，列表应能通过 delivery 状态投影失效收敛到 running。
   - terminal transition 后，列表应能通过同一机制收敛到 terminal 状态。

5. 保留当前预研项目的正确模型。
   - 不增加兼容旧事件的回退路径。
   - 不让前端写入或缓存第二套 AgentRun 状态事实源。

## Acceptance Criteria

- [ ] 后端 running delivery binding transition 发布 `ControlPlaneProjectionChanged(projection=agent_run_list)`，且测试覆盖 run/agent/frame/runtime refs。
- [ ] 后端 terminal delivery transition 仍发布 `agent_run_list` 投影失效，且路径与 running transition 的投影通知语义一致。
- [ ] Companion / child AgentRun list item 在 delivery 状态变化后能通过 project event 刷新为 running。
- [ ] 前端 `agent-run-list-state-store` 在 first-page refresh in-flight 时收到新的 list invalidation，会在当前请求完成后再执行一次刷新并应用最新数据。
- [ ] 前端列表 store 仍忽略非 `agent_run_list` projection 对列表的刷新请求。
- [ ] 相关 Rust 测试、前端 store 测试通过；全局检查至少覆盖 affected Rust crate 和 frontend type/lint/test 的相关范围。

## Out Of Scope

- 重新设计 AgentRun list UI 视觉。
- 改变 RuntimeSession event stream 协议。
- 引入轮询作为状态收敛主路径。
- 重构 unrelated lifecycle/task/story 状态投影。
