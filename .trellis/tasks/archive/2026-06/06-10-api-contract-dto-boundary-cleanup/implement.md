# API contract DTO 边界收敛实施计划

## Phase 1: Lifecycle run response slice

- [x] 确认 `LifecycleRunView` 已在 `agentdash-contracts::workflow` 与 generated TS 中存在。
- [x] 将 `POST /api/lifecycle-runs` response 从 domain `LifecycleRun` 改为 `LifecycleRunView`。
- [x] 将 `GET /api/lifecycle-runs/{id}` response 从 domain `LifecycleRun` 改为 `LifecycleRunView`。
- [x] 在 workflow route 内提供 `lifecycle_run_to_contract_view` mapper 入口。
- [x] 将前端 lifecycle fetch 指向 canonical `/lifecycle-runs/{id}` endpoint，并继续使用 generated `LifecycleRunView`。

## Phase 2: Definition DTO follow-up

- [x] 为 `AgentProcedure` 和 `WorkflowGraph` 定义 browser-facing contract DTO，避免 list/get/create/update 直接返回 domain entity。
- [x] 将 workflow definition mapper 放在 API 层，request 仍通过现有 route DTO 创建 domain aggregate。
- [x] 将前端 workflow service 改为直接消费 generated `AgentProcedureResponse` / `WorkflowGraphResponse`。

## Phase 3: Canvas runtime snapshot follow-up

- [x] 为 canvas runtime snapshot 定义 contract DTO，覆盖 files、bindings、runtime bridge 与 VFS-derived surface。
- [x] 将 `GET /api/canvases/{id}/runtime-snapshot` 从 application snapshot 收敛到 contract DTO。
- [x] 将 `POST /api/canvases/{id}/runtime-invoke` 从 application runtime invocation result 收敛到 contract DTO。
- [x] 将前端 canvas runtime snapshot/invoke service 改为直接消费 generated canvas DTO。

## Checks

- [x] `pnpm run frontend:check`
- [x] `pnpm run contracts:check`
- [x] `cargo check -p agentdash-api`
