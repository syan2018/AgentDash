# Relay 路径 TurnMonitor 迁移到 Hook Effect 管道

## Goal

将 relay 执行路径（通过 WebSocket 中继到本机后端的任务）的 TurnMonitor 逻辑迁移到 Hook Effect 管道，实现 cloud-native 和 relay 两条路径统一由 Hook 驱动 Task lifecycle。

## Background

当前 cloud-native 路径已经通过 `PostTurnHandler` + `TaskHookEffectExecutor` 完全替代了 TurnMonitor。但 relay 路径仍通过 `spawn_turn_monitor` → `run_turn_monitor` 的旧逻辑运行，导致：

- 同一个 Task lifecycle 逻辑存在两套实现（hook preset vs TurnMonitor 硬编码）
- relay 场景下 task 状态转换行为可能与 cloud-native 不一致
- `TurnDispatcher` trait 仍保留 `spawn_turn_monitor` 方法，增加维护负担

## Requirements

- [ ] Relay session 的事件也能经过 Hook 评估，产出 effects
- [ ] `TurnDispatcher` trait 移除 `spawn_turn_monitor` 方法
- [ ] `run_turn_monitor` 及相关旧代码清理
- [ ] Relay 路径的 artifact 持久化也经由 `PostTurnHandler.on_event`
- [ ] 确认 relay session notification bridge 已能正确触发 hook evaluation

## Technical Notes

- Relay session 的事件通过 notification bridge 转发到 SessionHub，但这些 session 没有 hook snapshot 上下文——需要确认 relay session 的 binding 和 owner 信息是否完整
- 可能需要在 relay dispatch 时也构造并传递 `PostTurnHandler`
- `schedule_auto_retry` 中的逻辑也需要迁移到 effect executor

## Acceptance Criteria

- [ ] relay 路径的 task 状态转换与 cloud-native 路径一致
- [ ] `spawn_turn_monitor` 和 `run_turn_monitor` 代码完全移除
- [ ] 两条路径统一走 Hook Effect 管道
