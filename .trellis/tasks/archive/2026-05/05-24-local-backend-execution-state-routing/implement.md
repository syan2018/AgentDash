# 实施计划：本机后端执行状态与分配治理

## Phase 1: Domain 与持久化

- [x] 新增 `BackendExecutionLease` 领域对象、状态枚举、selection mode。
- [x] 新增 `BackendExecutionLeaseRepository` trait。
- [x] 新增 PostgreSQL migration：`backend_execution_leases` 表、约束、索引。
- [x] 实现 Postgres repository：claim、activate、release、mark_lost、list_active、count_active_by_backend。
- [x] 补 repository round-trip 与 active count 测试。

## Phase 2: Allocator 与 placement

- [x] 新增 relay prompt placement 逻辑，组合 online executor snapshot 与 active lease counts。
- [x] 定义 `BackendSelectionRequest` / `ExecutionPlacementPlan`。
- [x] 自动分配第一版按 active lease count 升序、backend_id 稳定排序。
- [x] 调整 launch command / user prompt input / construction / launch plan，携带 selection intent 与 placement result。
- [x] relay connector 改为消费 `target_backend_id`，不再把 VFS mount 推断作为主选择逻辑。
- [x] 覆盖 auto idle multiple candidate 测试。

## Phase 3: Relay route 与释放闭环

- [x] 扩展 `BackendRegistry` session route：记录 backend_id、lease_id、sender。
- [x] `RelayAgentConnector.prompt()` 注册 route 时写入 backend/lease。
- [x] prompt response 成功后 activate lease；失败时 mark failed。
- [x] terminal event 到达时 release lease。
- [x] cancel 使用 route 精确 backend；取消完成释放或标记失败原因。
- [x] unregister backend 时关闭该 backend routes。
- [x] backend disconnect 时 active lease 标记 lost。
- [x] 添加 terminal、cancel、prompt failure 测试。
- [x] 添加 backend unregister route cleanup 测试。
- [ ] 添加 WebSocket disconnect 标记 lease lost 测试。

## Phase 4: API / Frontend 投影

- [x] 新增或扩展 backend runtime summary API DTO。
- [x] 更新 TS 类型与前端 service。
- [x] 在 backend/local runtime UI 或执行器选择相关 UI 展示 active count / allocatable 状态。
- [x] 前端不再从 runtime health 或 executor list 自行推断 idle/busy。

## Phase 5: 文档与验证

- [x] 更新 `.trellis/spec/cross-layer/desktop-local-runtime.md`：relay session route 与 lease 生命周期。
- [x] 更新 `.trellis/spec/cross-layer/project-backend-workspace-routing.md`：workspace binding 与 execution placement 分离。
- [x] 更新 `.trellis/spec/backend/session/runtime-execution-state.md`：lease 与 session active turn 的关系。
- [x] 运行后端格式化、编译检查与相关测试。
- [x] 运行前端类型检查。

## 验证命令

```powershell
cargo fmt --all --check
cargo test -p agentdash-domain -p agentdash-infrastructure -p agentdash-application -p agentdash-api
pnpm --filter app-web typecheck
pnpm --filter app-web test
```

## 风险文件

- `crates/agentdash-domain/src/backend/*`
- `crates/agentdash-infrastructure/migrations/*`
- `crates/agentdash-infrastructure/src/persistence/postgres/*`
- `crates/agentdash-application/src/backend_execution_placement.rs`
- `crates/agentdash-application/src/backend_transport.rs`
- `crates/agentdash-application/src/relay_connector.rs`
- `crates/agentdash-application/src/session/**`
- `crates/agentdash-api/src/relay/registry.rs`
- `crates/agentdash-api/src/relay/ws_handler.rs`
- `crates/agentdash-api/src/workspace_resolution.rs`
- `packages/app-web/src/**`

## Rollback 点

- migration 与 repository 可先独立合入，不接入 launch。
- allocator 可先只用于 relay connector，前端 summary 后置。
- 若 release 闭环不稳定，优先保留 lease 表但暂停 auto idle 策略，只允许 explicit backend。
