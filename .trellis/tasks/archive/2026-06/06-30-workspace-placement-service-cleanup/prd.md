# WorkspacePlacementService directory fact transaction

## Goal

实现 design backlog Slice 8 / D10：把 `workspace.detect -> WorkspaceDirectoryFact ->
BackendWorkspaceInventory / WorkspaceBinding` 这条事务从 API route-local helper 收束到
application 层 `WorkspacePlacementService`。API route 只保留鉴权、DTO parsing 和响应映射，不再
直接调用 runtime gateway detect、拼装 directory fact 或分散写 inventory/binding。

## Requirements

- 新增 application-level `WorkspacePlacementService`，统一持有 workspace detect invocation、
  directory fact projection、inventory upsert、workspace binding apply/update 事务。
- Service 必须表达清晰 placement intents，至少覆盖：
  - manual backend inventory register；
  - bind discovered workspace bindings；
  - create workspace with initial bindings / shortcut binding；
  - update workspace bindings；
  - sync candidate inventory to workspace bindings；
  - advanced binding-only shape when caller only edits Workspace bindings without detect side effect。
- `crates/agentdash-api/src/routes/backend_access.rs` 不再包含 route-local `invoke_workspace_detect`
  或直接 `workspace_inventory_from_detection` + repo upsert 事务。
- `crates/agentdash-api/src/routes/workspaces.rs` 不再直接执行 detect/fact/inventory/binding
  transaction；复杂 identity validation 和 relaxed P4 matching 可以作为 service helper 或
  service policy 留在 application 层。
- 保持 ProjectBackendAccess 授权、local backend restriction、identity mismatch、inactive access、
  empty root 等现有用户语义。
- 清理旧问题优先于加法：不得只新增 service 但保留 route-local detect/helper 作为活动路径。
- 本项目预研期不保留兼容路径；若某 route 语义需要改名或 API shape 更正，按当前正确模型处理。
- Subagent 执行约束：实现 worker 不跑大规模 Rust 编译或 broad suites；允许 scoped `rg`、
  `cargo fmt`、小型定向 Rust tests。最终集成 check 统一决定更大范围编译。

## Acceptance Criteria

- [x] 存在 `WorkspacePlacementService` 或等价 application use case owner，显式表达 placement intents。
- [x] backend inventory manual register 通过 service 完成 detect + inventory upsert。
- [x] workspace create/update bindings 通过 service 完成 detect + fact hydration + inventory upsert。
- [x] bind-discovered 通过 service 完成 redetect + identity match + binding apply + inventory upsert。
- [x] sync candidate inventory 的 binding apply 逻辑与 service/fact owner 对齐，不再散落 route。
- [x] API route 只做 auth/DTO/error mapping；静态搜索无 route-local `invoke_workspace_detect` 活动 helper。
- [x] Tests 覆盖 manual register、bind-discovered、create/update binding hydration、identity mismatch、
  inactive/access denied 的关键路径。
- [x] Spec 更新记录 workspace placement transaction owner 与 route/application 分工。

## Notes

- Source: `.trellis/tasks/06-30-design-backlog-review/design-review.md#d10-workspaceplacementservice`.
- This is Slice 8 from `.trellis/tasks/06-30-design-backlog-review/implementation-slices.md`.
- Completed shape:
  - `WorkspacePlacementService` owns manual inventory register, create workspace placement,
    update workspace placement and bind-discovered write transactions.
  - API routes retain auth, DTO parsing and response mapping.
  - Setup detect endpoints remain route-level runtime queries because they do not write
    `BackendWorkspaceInventory` or `WorkspaceBinding`.
