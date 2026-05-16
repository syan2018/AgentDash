# Session 重构彻底收尾

## Goal

把 `docs/reviews/AgentDash_session_refactor_plan.md` 中 session 目标态未完成的部分全部纳入本 task：彻底删除跨 crate `SessionHub` 入口，继续拆分内部 runtime 装配；不再把 `SessionLaunchPlanner` hook runtime 纯化、terminal effect router、数据库/runtime command 清理列为 out-of-scope，而是逐项收敛到可测试的生产结构。

## Confirmed Facts

- 生产 prompt 入口已经基本统一到 `SessionLaunchService::launch_command`。
- `LaunchCommand` / `SessionConstructionPlan` / `LaunchExecution` 已存在，但 `SessionLaunchPlanner` 仍直接解析 hook runtime 并在失败时清理 turn。
- `SessionRuntimeRegistry` / `TurnSupervisor` 已存在；上一提交已让 stream adapter abort handle 纳入 supervisor。
- terminal effect durable outbox 已改为显式 deps/ports，dispatcher 不再依赖内部 runtime 装配对象。
- `SessionHub` 符号已删除；跨 crate 调用面改为 `SessionRuntimeBuilder` / `SessionRuntimeServices`。application 内部的私有 `SessionRuntimeInner` 仍承载少量 hook/capability/tool 装配实现，后续可继续下沉。
- terminal effect / runtime command 表已经存在；若彻底清理需要存储契约调整，可以新增 migration，不再回避数据库变更。

## Requirements

- 彻底删除 `SessionHub` 公开/内部符号；生产业务服务依赖明确服务/依赖包，内部装配只允许以 `SessionRuntimeInner` 形式存在并保持 crate-private。
- 重写 `SessionLaunchPlanner` 的 hook runtime 解析边界：planner 不直接清理 turn，不在规划阶段执行不必要副作用；hook plan 作为 `LaunchExecution` 的一部分交给 executor 执行或显式准备。
- 将 terminal effect dispatcher 改成事件/事实驱动的 router 形态：processor 只产出 terminal fact 与 dispatch input，effect router/dispatcher 负责 enqueue、replay、execute。
- 清理 pending/runtime command 和 terminal effect 的存储边界；如果现有 schema 或 repository API 阻碍正确边界，允许新增 migration。
- 保持现有 HTTP、Task、Workflow、Routine、Companion、Hook auto-resume、Local relay prompt 行为等价或更严格。
- 每个结构性迁移都要有针对性单元测试或现有回归测试覆盖。

## Acceptance Criteria

- [x] `TurnSupervisor` 能登记当前 turn 的 stream adapter abort handle。
- [x] `clear_active_turn` / `clear_turn_and_hook` 清理 active turn 时会中止对应 adapter task。
- [x] `prompt_pipeline` 不再直接 fire-and-forget stream adapter task；spawn 后通过 supervisor 登记。
- [x] `SessionEffectsService` / terminal effect dispatcher 不再依赖完整 runtime 装配对象。
- [x] terminal effect 以明确 router/deps 形式处理 hook effects、terminal callback、hook auto-resume，并支持 replay。
- [x] `SessionLaunchPlanner` 不再在 hook runtime resolve 失败时直接执行 turn 清理；清理由 executor/turn supervisor 统一处理。
- [x] hook runtime 解析的副作用边界在 `LaunchExecution` 或 executor 中显式可见。
- [x] `SessionHub` 符号已删除；API `ServiceSet` 与本机 relay 均只持有明确 services，跨 crate 只暴露 builder/services。
- [x] runtime command / terminal effect store API 与 schema 命名表达事实状态，不再携带旧 pending transition 语义。
- [x] 关键 session launch、terminal effect replay、runtime command、hook auto-resume 回归测试通过。

## Out of Scope

- 无。用户明确要求彻底完成本重构，不再为上述项目保留“本轮不做”列表。
