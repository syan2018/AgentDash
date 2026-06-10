# API contract DTO 边界收敛设计

## Scope

本阶段先收敛 workflow lifecycle run HTTP 边界，不一次性迁移 workflow definition、agent procedure 或 canvas runtime snapshot。原因是 lifecycle run 已经存在 browser-facing `LifecycleRunView` contract，且 application 侧已有 read projection builder，route 可以用同一投影替代直接序列化 domain aggregate。

## Boundary

- API route 的 `POST /api/lifecycle-runs` 和 `GET /api/lifecycle-runs/{id}` 返回 `agentdash_contracts::workflow::LifecycleRunView`。
- route 内部仍可读取 domain `LifecycleRun` 完成授权、调度和投影输入，但 HTTP response 不暴露 domain aggregate。
- API 层用 `lifecycle_run_to_contract_view` 作为 route-owned response mapper 入口，统一调用 lifecycle read projection builder。
- 前端 lifecycle service 直接消费 generated `LifecycleRunView`，canonical fetch endpoint 指向 `/lifecycle-runs/{id}`。

## Non-Goals

- 不迁移 legacy identity、extension manifest、workspace tab、relay protocol。
- 不在本阶段重构已有 `/lifecycle-runs/{id}/view` view endpoint；它继续作为兼容的 read view alias，后续阶段可统一路由表。
- 不迁移 canvas runtime snapshot；该 snapshot 需要先定义完整 browser-facing canvas DTO。

## Validation

- `pnpm run contracts:check` 确认 generated contract 无 drift。
- `cargo check -p agentdash-api` 确认 route response type 与 mapper 编译通过。
- `pnpm run frontend:check` 确认前端 service 消费 generated DTO 通过类型检查。
