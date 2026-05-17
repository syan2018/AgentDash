# Project Backend Workspace Routing

## Scenario: 本机目录登记为 Backend Inventory

### 1. Scope / Trigger

- Trigger: Project 设置页允许用户在已授权 backend 上识别本机目录，并把结果登记为 backend inventory。
- 范围：UI -> REST API -> Runtime Gateway `workspace.detect` -> `backend_workspace_inventory` upsert。
- 边界：Workspace binding 维护不等于 backend inventory 登记；Advanced Maintenance 只改 Workspace 自身 bindings。

### 2. Signatures

- `POST /projects/{project_id}/backend-access/{access_id}/inventory/register`
- Request:
  ```json
  { "root_ref": "D:/Workspaces/example" }
  ```
- Response: `BackendWorkspaceInventory`
- Frontend service:
  ```ts
  registerBackendWorkspaceInventory(
    projectId: string,
    accessId: string,
    payload: { root_ref: string },
  ): Promise<BackendWorkspaceInventory>
  ```

### 3. Contracts

- `project_id` 必须是当前用户可编辑的 Project。
- `access_id` 必须属于该 Project，且 `ProjectBackendAccess.status === "active"`。
- `root_ref` 会 trim，不能为空。
- 后端必须通过 Runtime Gateway 调用 `workspace.detect`，不得由云端直接访问本机文件系统。
- detect 成功后 upsert `BackendWorkspaceInventory`，`source` 使用 `capability_expansion_ack`。
- 该 API 不扩大 local runtime 的 `accessible_roots`；目录不可访问时由 detect 失败返回。
- UI 登记成功后必须刷新 workspace candidates；如果 Backend Access 面板已有展开的 Inventory，也要重新拉取对应快照。

### 4. Validation & Error Matrix

- Project ID / Access ID 非法 -> `400 BadRequest`
- Project 无 edit 权限 -> 权限错误
- access 不属于 Project -> `404 NotFound`
- access 非 active -> `409 Conflict`
- `root_ref` 为空 -> `400 BadRequest`
- backend 离线、目录不可访问、detect 失败 -> Runtime Gateway 错误向 UI 透传
- detect response 无法反序列化 -> `500 Internal`

### 5. Good / Base / Bad Cases

- Good: 已授权本机 backend + 可访问目录 -> detect 成功，inventory upsert，candidates/展开面板刷新。
- Base: 只想直接创建 Workspace -> 走 Workspace create/update，不要求登记 inventory。
- Bad: 在 Advanced Maintenance 手填 binding 后期待 inventory 自动出现 -> 错误心智；必须使用“登记到 Backend Inventory”动作。

### 6. Tests Required

- Frontend: `pnpm --filter app-web check` 覆盖类型、lint、现有 routing helper tests。
- Backend: `cargo check -p agentdash-api` 与 `cargo test -p agentdash-api` 覆盖路由编译、现有权限/序列化回归。
- Build: `pnpm --filter app-web build` 确认页面 chunk 可生产构建。

### 7. Wrong vs Correct

#### Wrong

在 Workspace binding 编辑器里手填 `backend_id` / `root_ref`，并假设它会写入 backend inventory。

#### Correct

先在本机目录识别区选择已授权 backend 和目录，执行 detect，再点击 `登记到 Backend Inventory`；需要绑定到 Workspace 时再从 candidate 或 create/update 流程确认。
