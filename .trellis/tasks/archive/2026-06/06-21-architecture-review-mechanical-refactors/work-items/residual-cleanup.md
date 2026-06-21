# Residual Surface Cleanup Items

## M08 拆分 `types/index.ts`

- Scope: `packages/app-web/src/types/index.ts`.
- Acceptance: generated aliases、shared view model、legacy wire gaps 分文件；service wire types 不从 UI view model 反推。
- Validation: `pnpm run frontend:check`.

## M09 确认 SessionExecutionState 消费面

- Scope: `/sessions/{id}/state`, frontend session service/types。
- Acceptance: 若仍是 control UI 事实则 contract 化；若只剩诊断/legacy endpoint，则标注 route-local wrapper。
- Validation: `rg "SessionExecutionState|fetchSessionState|/state"` plus targeted typecheck.

## M10 移除或封装 `AgentRunSteeringService`

- Scope: `crates/agentdash-application/src/agent_run/steering.rs`, exports, tests.
- Acceptance: 产品路径无可直接 import 的 AgentRun direct steer service；测试需求移入 test support 或改为 mailbox envelope。
- Validation: `rg "AgentRunSteeringService|agent_run::steering"`, `cargo test -p agentdash-application`.

## M11 清理 AppState 中未公开消费的 `StoryActivityActivationService`

- Scope: `crates/agentdash-api/src/app_state.rs`, `crates/agentdash-application/src/task/service.rs`.
- Acceptance: 无公开 route 消费则移除 AppState 字段；保留时必须有明确 use case。
- Validation: `rg "StoryActivityActivationService|get_task_execution_view"`, `cargo check`.

## M12 raw anchor repository API 与 application selection API 分层命名

- Scope: `RuntimeSessionExecutionAnchorRepository::latest_for_agent` 及 callsites。
- Acceptance: repository 方法名表达 raw order；业务 selection 留给 application service。
- Validation: `rg "latest_for_agent"`, `cargo check`.

## M13 RuntimeGateway `surface_for` debug 入口守卫

- Scope: RuntimeGateway public methods and product route callsites.
- Acceptance: 产品 route 只调用 `surface_for_actor`；`surface_for` 命名/可见性表达 debug/internal。
- Validation: `rg "surface_for\\("`, `cargo test -p agentdash-application`.

