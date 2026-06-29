# Project Backend Workspace Routing

本 appendix 定义 Project backend access、workspace detect 与 backend inventory registration 的跨层契约。

## Backend Inventory Registration

用户在 Project 设置页选择已授权 backend 和本机目录后，后端通过 Runtime Gateway 调用 `workspace.detect`，再把结果登记为 backend inventory。目录浏览和 detect/register 是 setup 能力：本机默认允许用户浏览本机目录来选择候选 workspace，detect 只校验目标目录存在、是目录且可读取。

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
- detect 成功后 upsert `BackendWorkspaceInventory`，`source` 使用 `manual_register`。
- detect 成功登记的 `root_ref` 是后续 Workspace Inventory / Workspace Binding 的目录事实；目录不可访问时由 detect 失败返回。
- `workspace.detect` 的成功结果进入后端后统一投影为 Workspace directory fact，同时维护 `BackendWorkspaceInventory` 与 `WorkspaceBinding`，原因是机器详情、候选目录和 Workspace 运行落点必须共享同一份目录事实，不能让不同入口分别拼装 binding / inventory 状态。
- UI 登记成功后必须刷新 workspace candidates；如果 Backend Access 面板已有展开的 Inventory，也要重新拉取对应快照。
- Workspace binding 维护不等于 backend inventory 登记；Advanced Maintenance 只改 Workspace 自身 bindings。
- Workspace binding / inventory 只表达目录事实与已确认 workspace root，不表达执行空闲状态。session 执行 backend placement 由 backend execution lease / allocator 维护，原因是同一个 workspace root 的 backend 可能正在执行其它 session。
- `workspace_roots` 为空不表示本机不能浏览或不能 detect；空集合表示本机没有显式预登记 roots，执行类能力以 session `mount_root_ref` 自身作为当前 workspace 边界。
- Frontend 展示 backend 是否可分配时读取 `/backends/runtime-summary` 的 `active_session_count`、executor `active_session_count` 与 `allocatable`，原因是该投影已经合并 runtime health、registry executor snapshot 与 active backend execution leases。
- WorkspacePanel extension webview 的 action target 可由 Project workspace binding 提供；前端按 session/story/project default workspace 解析当前 workspace，再读取其默认 binding 与在线状态，原因是插件 tab 属于 Project runtime projection，而可执行本机 host 由 Project workspace/backend 授权关系承载。
- `ProjectBackendAccess` 是 project→backend 的权威授权层：runner backend 为机器级 `user`-scope 身份（不再 project-baked），一台 runner 通过多行 active grant 复用到多个 project；鉴权对 user-scope backend 在 owner 之外额外按 active grant 放行给对应 project 成员（详见 cross-layer/desktop-local-runtime.md 的 Runner Registration Token Enrollment）。
- 工作空间设置区按用户心智分三组：**运行环境**（可用机器，显式区分「本机/这台设备」desktop runtime 与「服务器 runner」，依据 `registration_source`；含「接入新服务器」即 runner token 管理子块）/ **工作空间**（保留命名，弱化「代码来源」主语，内联展示各机器落点）/ **高级**（本机目录定位、Workspace Modules 诊断）。grant 状态、priority、inventory 等后台词降到展开/次级，不作为主线术语。完整的多 project grant 管理（priority/policy/跨 owner/审计反向视图）属于独立任务 `06-27-runner-multi-project-access`。

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

## Scenario: Project Identity Discovery Local Binding

### 1. Scope / Trigger

- Trigger: Project 设置页支持按 Workspace identity 在本机 backend 上发现候选目录，并确认后写入 Workspace binding。
- Scope: HTTP API、Runtime Gateway setup action、Relay command、local backend discovery strategy、Workspace binding、Backend inventory source。

### 2. Signatures

Setup action:

```text
workspace.discover_by_identity
```

Relay:

```text
command.workspace_discover_by_identity
response.workspace_discover_by_identity
```

Project API:

```text
POST /projects/{project_id}/workspaces/discover-local-bindings
POST /projects/{project_id}/workspaces/bind-discovered
```

DB source:

```text
backend_workspace_inventory.source = "identity_discovery"
```

### 3. Contracts

`discover-local-bindings` request:

```json
{ "backend_id": "local-runtime-id" }
```

`discover-local-bindings` response:

```json
{
  "backend_id": "local-runtime-id",
  "candidates": [
    {
      "workspace_id": "uuid",
      "workspace_name": "Main",
      "root_ref": "D:/ws/main",
      "identity_kind": "p4_workspace",
      "identity_payload": {},
      "detected_facts": {},
      "confidence": "high",
      "client_name": "local-client",
      "server_address": "ssl:p4:1666",
      "stream": "//Depot/main",
      "warnings": []
    }
  ],
  "skipped": [
    {
      "workspace_id": "uuid",
      "workspace_name": "Docs",
      "identity_kind": "git_repo",
      "reason": "unsupported_identity_kind",
      "message": "当前版本尚未支持该 Workspace identity 类型的本机发现"
    }
  ],
  "warnings": []
}
```

`bind-discovered` request:

```json
{
  "bindings": [
    { "workspace_id": "uuid", "backend_id": "local-runtime-id", "root_ref": "D:/ws/main" }
  ]
}
```

`bind-discovered` response returns updated `WorkspaceResponse[]`, bound workspace ids, created/updated binding counts, upserted inventory items, and warnings.

### 4. Validation & Error Matrix

| 条件 | 语义 |
| --- | --- |
| Project 无 edit 权限 | 权限错误 |
| `backend_id` 为空 | `400 BadRequest` |
| backend 未授权给 Project | `403 Forbidden` |
| backend 不是 `local` | `400 BadRequest` |
| access 非 active | `409 Conflict` |
| identity kind 无 discovery strategy | `skipped.reason = "unsupported_identity_kind"` |
| P4 identity 缺少 server/stream | `skipped.reason = "invalid_identity_payload"` |
| P4 CLI 不可用或 client 查询失败 | skipped，不阻断其它 workspace |
| bind 时 root 不可读或 detect 失败 | Runtime Gateway 错误向 UI 透传 |
| bind 时 detect identity 不匹配 | `400 BadRequest` |

### 5. Good/Base/Bad Cases

- Good: `p4_workspace` 的 `server_stream` / `server_stream_client` identity 在 online local backend 上返回候选，用户确认后写入 ready binding 和 `identity_discovery` inventory。
- Base: `git_repo`、`local_dir` 在 v1 中返回 skipped，P4 discovery 继续执行。
- Bad: 前端传回 stale root 时，`bind-discovered` 重新 detect 并拒绝不匹配结果。

### 6. Tests Required

- Local strategy registry: unsupported identity 进入 `skipped`，不进入 `candidates`。
- P4 parser: `p4 clients -ztag` 多 record 与 `p4 client -o` Root/AltRoots/Stream 解析。
- P4 discovery match: `server_stream_client` 在发现阶段按 server+stream 匹配，client_name 只用于展示和消歧。
- API binding: 重复 `backend_id + root_ref` 更新已有 binding，新增 root 创建 binding，并 upsert `identity_discovery` inventory。
- Frontend: Project 设置页直接展示 discovery 面板，绑定成功刷新 Workspace 列表与 Backend Access inventory。

### 7. Wrong vs Correct

#### Wrong

```text
discover-local-bindings 直接写入 Workspace binding
```

该形状让 discovery 的本机扫描副作用过大，用户没有机会消歧多个 P4 client root。

#### Correct

```text
discover-local-bindings -> candidates/skipped
bind-discovered -> workspace.detect -> identity match -> binding/inventory upsert
```

该形状把“发现候选”和“确认写库”拆开，同时保证写库时重新校验本机目录事实。
