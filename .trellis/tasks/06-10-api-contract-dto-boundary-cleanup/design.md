# API contract DTO 边界收敛设计

## Scope

本阶段已从 workflow lifecycle run 继续推进到 workflow definition 与 canvas runtime 边界：

- workflow lifecycle run HTTP 边界返回 `LifecycleRunView` contract。
- workflow definition browser-facing routes 返回 `AgentProcedureResponse` / `WorkflowGraphResponse` contract，由 API 层从 domain entity 显式映射。
- canvas runtime browser-facing routes 返回 `CanvasRuntimeSnapshotDto` / `RuntimeInvocationResultDto` contract，由 API 层从 application runtime snapshot / invocation result 显式映射。

## Boundary

- API route 的 `POST /api/lifecycle-runs` 和 `GET /api/lifecycle-runs/{id}` 返回 `agentdash_contracts::workflow::LifecycleRunView`。
- route 内部仍可读取 domain `LifecycleRun` 完成授权、调度和投影输入，但 HTTP response 不暴露 domain aggregate。
- API 层用 `lifecycle_run_to_contract_view` 作为 route-owned response mapper 入口，统一调用 lifecycle read projection builder。
- 前端 lifecycle service 直接消费 generated `LifecycleRunView`，canonical fetch endpoint 指向 `/lifecycle-runs/{id}`。
- API route 的 `/api/agent-procedures` 和 `/api/workflow-graphs` list/get/create/update 返回 `agentdash_contracts::workflow::{AgentProcedureResponse, WorkflowGraphResponse}`，不直接序列化 `agentdash_domain::workflow::{AgentProcedure, WorkflowGraph}`。
- workflow response DTO 保留 browser UI 需要的 `target_kinds` 派生字段，原因是当前 workflow definition domain entity 尚未持久化 target kind，而 UI 编辑状态需要稳定 contract 字段。
- API route 的 `GET /api/canvases/{id}/runtime-snapshot` 返回 `agentdash_contracts::canvas::CanvasRuntimeSnapshotDto`，不直接序列化 `agentdash_application::canvas::CanvasRuntimeSnapshot`。
- API route 的 `POST /api/canvases/{id}/runtime-invoke` 返回 `agentdash_contracts::canvas::RuntimeInvocationResultDto`，不直接序列化 application runtime gateway result。
- 前端 workflow service 直接消费 generated workflow response DTO；canvas runtime snapshot/invoke service 直接消费 generated canvas DTO。

## Non-Goals

- 不迁移 legacy identity、extension manifest、workspace tab、relay protocol。
- 不在本阶段重构已有 `/lifecycle-runs/{id}/view` view endpoint；它继续作为兼容的 read view alias，后续阶段可统一路由表。
- 不迁移 Canvas CRUD `CanvasResponse`；本 slice 只收敛 runtime snapshot/invoke 这两个 application runtime 边界。

## Validation

- `pnpm run contracts:check` 确认 generated contract 无 drift。
- `cargo check -p agentdash-api` 确认 route response type 与 mapper 编译通过。
- `pnpm run frontend:check` 确认前端 service 消费 generated DTO 通过类型检查。
