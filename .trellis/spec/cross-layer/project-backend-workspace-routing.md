# Project Backend Workspace Routing

本 appendix 定义 Project backend access、workspace detect 与 backend inventory registration 的跨层契约。

## Backend Inventory Registration

用户在 Project 设置页选择已授权 backend 和本机目录后，后端通过 Runtime Gateway 调用 `workspace.detect`，再把结果登记为 backend inventory。

## Signatures

API:

```text
POST /projects/{project_id}/backend-access/{access_id}/inventory/register
```

Request:

```json
{ "root_ref": "D:/Workspaces/example" }
```

Response: `BackendWorkspaceInventory`

Frontend service:

```ts
registerBackendWorkspaceInventory(
  projectId: string,
  accessId: string,
  payload: { root_ref: string },
): Promise<BackendWorkspaceInventory>
```

## Contract

- `project_id` 必须是当前用户可编辑的 Project。
- `access_id` 必须属于该 Project，且 `ProjectBackendAccess.status === "active"`。
- `root_ref` 会 trim，不能为空。
- 后端必须通过 Runtime Gateway 调用 `workspace.detect`，不得由云端直接访问本机文件系统。
- detect 成功后 upsert `BackendWorkspaceInventory`，`source` 使用 `capability_expansion_ack`。
- 该 API 不扩大 local runtime 的 `accessible_roots`；目录不可访问时由 detect 失败返回。
- UI 登记成功后必须刷新 workspace candidates；如果 Backend Access 面板已有展开的 Inventory，也要重新拉取对应快照。
- Workspace binding 维护不等于 backend inventory 登记；Advanced Maintenance 只改 Workspace 自身 bindings。
- Workspace binding / inventory 只表达目录事实与可访问根，不表达执行空闲状态。session 执行 backend placement 由 backend execution lease / allocator 维护，原因是同一个可访问 workspace 的 backend 可能正在执行其它 session。
- Frontend 展示 backend 是否可分配时读取 `/backends/runtime-summary` 的 `active_session_count`、executor `active_session_count` 与 `allocatable`，原因是该投影已经合并 runtime health、registry executor snapshot 与 active backend execution leases。

## Validation And Errors

| 条件 | 语义 |
| --- | --- |
| Project ID / Access ID 非法 | `400 BadRequest` |
| Project 无 edit 权限 | 权限错误 |
| access 不属于 Project | `404 NotFound` |
| access 非 active | `409 Conflict` |
| `root_ref` 为空 | `400 BadRequest` |
| backend 离线、目录不可访问、detect 失败 | Runtime Gateway 错误向 UI 透传 |
| detect response 无法反序列化 | `500 Internal` |

## User Flow

正确心智是：先在本机目录识别区选择已授权 backend 和目录，执行 detect，再点击“登记到 Backend Inventory”；需要绑定到 Workspace 时再从 candidate 或 create/update 流程确认。
