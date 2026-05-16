# Session 重构收尾清理

## Goal

让当前 session 重构结果更贴近 `docs/reviews/AgentDash_session_refactor_plan.md` 中的目标状态，优先处理已经有架构骨架但仍缺少最后一层职责收口的低风险点。

## Confirmed Facts

- 生产 prompt 入口已经基本统一到 `SessionLaunchService::launch_command`。
- `LaunchCommand` / `LaunchExecution` / `SessionLaunchPlanner` 已存在，并能记录 VFS、MCP、capability、follow-up 等来源摘要。
- `SessionRuntimeRegistry` / `TurnSupervisor` 已存在，但 `TurnSupervisor` 目前主要管理 claim、active turn 与 `processor_tx`，stream adapter task 仍由 prompt pipeline 直接 spawn 后丢弃 handle。
- terminal effect 已有 durable outbox；本轮不重拆 processor 与 effect router，避免扩大风险。
- pending capability transitions 已迁移为 runtime commands；本轮不再触碰数据库模型。

## Requirements

- 将本轮清理范围限定为 session runtime / launch pipeline 的收尾，不做 API/数据库兼容层。
- 让 stream adapter task 的生命周期由 `TurnSupervisor` 可见，减少 prompt pipeline 对后台任务监督职责的直接持有。
- cancel / terminal 清理时应释放或中止已登记的 adapter task，避免后台 stream adapter 在 turn 结束后继续悬挂。
- 保持现有 prompt、hook、terminal effect 行为不变。
- 为新增 supervisor 行为补充单元测试。

## Acceptance Criteria

- [x] `TurnSupervisor` 能登记当前 turn 的 stream adapter abort handle。
- [x] `clear_active_turn` / `clear_turn_and_hook` 清理 active turn 时会中止对应 adapter task。
- [x] `prompt_pipeline` 不再直接 fire-and-forget stream adapter task；spawn 后通过 supervisor 登记。
- [x] 现有关键 launch/provider/bootstrap 测试仍通过。
- [x] 新增测试覆盖 adapter handle 登记与清理中止。

## Out of Scope

- 不在本轮彻底拆分 `SessionHub`。
- 不在本轮重写 `SessionLaunchPlanner` 的 hook runtime 解析。
- 不在本轮把 terminal effect dispatcher 改成事件订阅式 router。
- 不在本轮引入新的数据库迁移。
