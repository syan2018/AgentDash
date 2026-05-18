# Backend 能力扩展治理设计

## 目标边界

本任务补齐 server 侧发起、批准、下发、确认、撤销 backend capability / accessible root 扩展的治理闭环。它不负责 ProjectBackendAccess 自动路由本身；自动路由消费已经生效并已被 backend 上报/确认的能力。

现有基础：

- `BackendConfig` 已有 `visibility`、`share_scope_kind`、`share_scope_id`、`capability_slot`、`machine_id`、`owner_user_id`。
- `RuntimeHealth` 已保存 backend 上报的 `accessible_roots` 与 `capabilities`。
- `ProjectBackendAccess` 与 `BackendWorkspaceInventory` 已存在，后者的 `source` 已包含 `capability_expansion_ack`。
- `workspace.detect` / `workspace.browse_directory` 已通过 Runtime Gateway 调用本机 backend，cloud 不直接访问本地文件系统。
- `ensure_local_runtime` 目前只允许 `user` scope，project/system shared runtime 创建入口尚未开放。

## 核心决策

### D1. Backend control mode 一等化

新增 `BackendControlMode`，作为 `BackendConfig.control_mode` 持久字段：

- `personal`：个人本机 backend。server 可以创建请求，但 backend / 本机 UI 是最终裁决者。
- `project_managed`：Project 托管 backend。Project owner / editor 可按 Project policy 扩展能力。
- `system_managed`：系统托管 backend。server admin 可批量扩展能力。

`visibility` / `share_scope_kind` 仍表达可见范围与身份 scope，不承担裁决语义。迁移时按现有字段初始化：

- `share_scope_kind=user` -> `personal`
- `share_scope_kind=project` -> `project_managed`
- `share_scope_kind=system` -> `system_managed`

预研期不保留兼容推导分支；迁移后业务代码读取 `control_mode`。

### D2. 能力扩展请求是 server 期望，不是已生效能力

新增领域实体 `BackendCapabilityExpansionRequest`。关键字段：

- `id`
- `target_backend_id`
- `target_scope_kind`: `backend` / `backend_scope` / `capability_slot`
- `capability_slot`
- `requested_by_user_id`
- `request_actor_kind`: `server_admin` / `project_owner` / `project_editor` / `user` / `automation`
- `source_kind`: `manual_admin` / `project_backend_access` / `workspace_prepare` / `worktree_prepare` / `automation`
- `project_id`
- `workspace_id`
- `task_id`
- `workflow_id`
- `requested_capabilities`: JSON object，表达 `browse` / `detect` / `execute` / `read_write` / `prepare` / `inventory_source` / `worktree_root`
- `requested_resources`: JSON object，首版至少表达 `{ "roots": [{ "root_ref": "...", "mode": "read_write" }] }`
- `reason`
- `status`
- `policy_decision`: JSON object，记录 server 裁决、命中规则、操作者
- `backend_ack`: JSON object，记录 backend 实际接受/拒绝/降级的内容
- `expires_at`
- `revoked_at`
- `created_at`
- `updated_at`

状态机：

- `pending_policy`：请求已创建，等待 server 侧裁决。
- `rejected`：server policy 拒绝或人工拒绝。
- `accepted`：server 已接受请求，但尚未发送或尚未 ack。
- `pending_backend_ack`：已向 backend 下发，等待 ack / reject。
- `applied`：backend ack 成功，实际能力已生效或会在下一次 health 上报体现。
- `failed`：下发失败、超时、ack payload 非法或 backend 处理失败。
- `revoked`：server 撤销已接受/已生效请求。
- `expired`：超过 `expires_at` 且未续租。

任何读取侧都只能把 `applied` 且未过期/未撤销的请求视为可消费事实。`accepted` / `pending_backend_ack` 只是 server 期望。

### D3. 下发机制采用持久请求 + 在线 relay apply + backend ack

流程：

1. UI / 自动化创建 `BackendCapabilityExpansionRequest`。
2. server 根据 `BackendConfig.control_mode`、Project 权限、admin 权限、ProjectBackendAccess 等做 policy decision。
3. 若通过，server 将请求置为 `accepted`。
4. 若 backend 在线，server 通过 relay 下发 `command.capability_expansion_apply`，状态转为 `pending_backend_ack`。
5. backend 校验 requested resources 是否可接受：
   - personal：新增任意绝对路径默认需要 Tauri Local Runtime 面板确认。
   - project/system managed：可按本机 profile / managed policy 自动接受。
6. backend 返回 ack / reject。server 写入 `backend_ack` 并更新为 `applied` / `rejected` / `failed`。
7. backend 后续 runtime health 上报真实 `accessible_roots` / `capabilities`。server 不在 ack 前伪造 health。

离线 backend：

- 请求可创建并进入 `accepted`。
- UI 显示“等待 backend 在线下发”。
- backend 下一次在线后，server 可在 register/health 后主动重试下发。
- 首版可以先提供 `retry` API，不要求实现完整后台队列调度；但数据模型必须支持重试。

Relay 协议草案：

```json
{
  "type": "command.capability_expansion_apply",
  "id": "...",
  "payload": {
    "request_id": "...",
    "control_mode": "personal",
    "capability_slot": "default",
    "requested_capabilities": {},
    "requested_resources": {},
    "expires_at": null,
    "reason": "..."
  }
}
```

Response:

```json
{
  "type": "response.capability_expansion_apply",
  "id": "...",
  "ok": true,
  "payload": {
    "request_id": "...",
    "decision": "applied",
    "applied_capabilities": {},
    "applied_resources": {},
    "message": "..."
  }
}
```

Reject / downgrade 必须写明原因：

```json
{
  "decision": "rejected",
  "reason": "personal backend requires local confirmation"
}
```

### D4. personal backend 的确认入口

personal backend 不允许 server 静默新增任意绝对路径。首版确认入口放在 Tauri Local Runtime 面板：

- server 设置页显示 pending 请求、目标 backend、目标 path/capability、来源 project/workspace/task。
- Local Runtime 面板显示“来自 server 的能力扩展请求”。
- 用户在本机确认后，runtime 更新本机 profile / runtime manager 的 accessible roots，再 ack server。
- 如果请求只使用已有 accessible roots 内的资源，可由 backend 直接 ack `applied`，不弹确认。

首版不要求系统通知；后续可在 Tauri 层加通知提醒，但不能替代 Local Runtime 面板中的最终确认记录。

### D5. TTL / lease 策略

请求支持 `expires_at`：

- admin / managed 持久 policy 可以为空，靠手动 revoke。
- `workspace_prepare` / `worktree_prepare` 必须传 `expires_at`，默认建议 24 小时或跟随 workspace prepare 生命周期。
- 过期处理至少在读取 active requests 时过滤；后台清理可后续做。

撤销：

- `revoked` 必须发 `command.capability_expansion_revoke` 或在下一次 sync 中让 backend 移除能力。
- revocation ack 后，runtime health / backend inventory / ProjectBackendAccess 消费侧不得继续使用该能力。

### D6. 与现有 ProjectBackendAccess / inventory 的关系

`ProjectBackendAccess` 仍表达 Project 可以使用哪个 backend / backend scope。能力扩展请求表达 backend 能力如何被扩大。

关系规则：

- `ProjectBackendAccess` 不等于 capability expansion request。
- `ProjectBackendAccess.capability_policy` 可以引用已 `applied` 的 request id 或 capability tags。
- `BackendWorkspaceInventory.source = capability_expansion_ack` 表示 inventory 来源于 backend ack 后的 detect/register，不表示 server 单方面授权。
- workspace detect / browse / runtime resolution 仍必须经过 ProjectBackendAccess；capability expansion 只扩大 backend 可上报/可执行的资源边界。

### D7. API 草案

Project 范围创建请求：

- `POST /projects/{project_id}/backend-access/{access_id}/capability-requests`
- `GET /projects/{project_id}/backend-access/{access_id}/capability-requests`

全局/设置页管理：

- `GET /backend-capability-requests?backend_id=&status=&project_id=`
- `POST /backend-capability-requests/{request_id}/approve`
- `POST /backend-capability-requests/{request_id}/reject`
- `POST /backend-capability-requests/{request_id}/revoke`
- `POST /backend-capability-requests/{request_id}/retry`

Backend ack 由 relay response 进入 server；如果未来需要 HTTP pull/ack，可补：

- `GET /local-runtime/capability-requests`
- `POST /local-runtime/capability-requests/{request_id}/ack`

首版优先走 relay，HTTP pull/ack 作为后续增强，不阻塞模型。

### D8. UI 信息架构

Server 设置页：

- Backend list：显示 `control_mode`、scope、capability slot、runtime health、accessible roots。
- Backend detail：显示 capability expansion requests、policy decision、backend ack、revoke/retry 操作。
- Project Workspace / Backend Access：在已授权 backend 下显示“请求扩展能力”入口，以及请求状态。

Tauri Local Runtime 面板：

- Profile roots 管理仍是本机事实源。
- 新增 pending expansion requests 区块。
- personal backend 的本机确认/拒绝从这里完成。

前端 DTO 字段保持 snake_case，不做 camelCase 兼容。

## 数据库与迁移方向

### backends

新增：

- `control_mode TEXT NOT NULL`

迁移规则按 `share_scope_kind` 填充。预研期可直接要求所有读取路径使用新字段。

### backend_capability_expansion_requests

建议字段：

```sql
id TEXT PRIMARY KEY
target_backend_id TEXT NOT NULL
target_scope_kind TEXT NOT NULL
capability_slot TEXT NOT NULL DEFAULT 'default'
requested_by_user_id TEXT
request_actor_kind TEXT NOT NULL
source_kind TEXT NOT NULL
project_id TEXT
workspace_id TEXT
task_id TEXT
workflow_id TEXT
requested_capabilities JSONB NOT NULL DEFAULT '{}'::jsonb
requested_resources JSONB NOT NULL DEFAULT '{}'::jsonb
reason TEXT
status TEXT NOT NULL
policy_decision JSONB NOT NULL DEFAULT '{}'::jsonb
backend_ack JSONB NOT NULL DEFAULT '{}'::jsonb
expires_at TEXT
revoked_at TEXT
created_at TEXT NOT NULL
updated_at TEXT NOT NULL
```

索引：

- `(target_backend_id, status)`
- `(project_id, status)`
- `(expires_at)`

状态枚举必须在 domain 层类型化，不以裸字符串散落在 route 中。

## 风险与取舍

- **personal 确认链路会跨 server UI 与 Tauri UI**：首版保持确认入口单一在 Local Runtime 面板，server UI 只展示请求和状态，降低误判。
- **relay command 只能触达在线 backend**：持久请求和 retry API 先兜住离线场景，后台自动重放可后续补。
- **capability/root policy 易变成第二套授权系统**：读取侧坚持只消费 `applied` ack 与 runtime health；ProjectBackendAccess 仍是 Project 使用权限边界。
- **JSON policy 容易漂移**：domain 层提供 typed wrapper / validator，route 不直接拼 JSON。

## 非目标

- 不在本任务中实现完整 workspace/worktree provisioner。
- 不把 personal backend 改造成 server 可静默管理的设备。
- 不绕过 Runtime Gateway / relay 让 cloud 访问本地文件系统。
- 不做旧 API / 旧字段兼容层。
