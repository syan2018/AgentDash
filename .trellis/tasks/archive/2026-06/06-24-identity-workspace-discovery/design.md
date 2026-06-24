# 技术设计：按 Identity 策略发现本机 Workspace

## Architecture

新增 discovery-by-identity setup 能力，沿用现有跨层边界：

- API 层只编排 Project 权限、backend access、Workspace 列表、binding 写入。
- Runtime Gateway 负责把 discovery 请求路由到 local backend。
- local backend 按 identity kind 分派发现策略。
- 云端不直接访问本机文件系统。

## Contracts

新增 setup action：

```text
workspace.discover_by_identity
```

输入包含：

- `backend_id`
- `workspaces`: `workspace_id`、`identity_kind`、`identity_payload`

输出包含：

- `candidates`: discovery 候选
- `skipped`: 未支持或无法发现的 workspace 记录

新增 Project API：

```text
POST /projects/{project_id}/workspaces/discover-local-bindings
POST /projects/{project_id}/workspaces/bind-discovered
```

`discover-local-bindings` 读取 Project 内 Workspace 与已授权本机 backend，调用 Runtime Gateway，返回前端可展示候选。

`bind-discovered` 接收用户选中的候选，重新 detect / match 后写入 Workspace binding，并 upsert backend inventory。

## Strategy Model

local backend 内新增 discovery strategy registry：

- `WorkspaceIdentityDiscoveryStrategy`：以 `WorkspaceIdentityKind` 为分派键。
- `P4WorkspaceDiscoveryStrategy`：实现 `p4_workspace`。
- 未注册 kind 统一写入 `skipped`，reason 为 `unsupported_identity_kind`。

P4 策略只消费可归一化的 P4 identity contract：

- `server_stream`
- `server_stream_client`

`server_client` 与 `path_key` 不适合作为 Project 级反向发现的主路径，v1 作为 skipped 或低置信 unsupported reason 处理。

## P4 Discovery Data Flow

1. 从 identity payload 读取 `server_address` 和 `stream`。
2. 在 local backend 上读取当前 P4 user / server facts。
3. 用 P4 CLI 查找候选 client：
   - `p4 clients -S <stream> --me`
   - 必要时用当前 user 作为 `-u` 限定。
4. 对每个 client 执行 `p4 client -o <client>`，读取 `Root`、`AltRoots`、`Host`、`Stream`。
5. 仅保留当前机器可读目录。
6. 对候选 root 复用现有 `detect_workspace`，并用领域层 identity match 做二次确认。
7. 返回候选，`client_name` 仅用于展示和消歧。

## Persistence

绑定写入遵循现有 Workspace 聚合：

- 已存在同一 `backend_id` + `root_ref` binding 时更新 status、detected_facts、last_verified_at、priority。
- 不存在时新增 binding。
- Workspace status 根据 bindings 派生并刷新 default binding。

Backend inventory 同步 upsert：

- 新增 `BackendWorkspaceInventorySource::IdentityDiscovery`。
- 保持 `(backend_id, root_ref)` 幂等。

## Frontend UX

Project 设置 `workspace` tab 新增 `LocalWorkspaceDiscoveryPanel`：

- 展示可用于 discovery 的本机 backend。
- 展示 Project 内可发现 Workspace 数量与 skipped 摘要。
- 点击“发现本机 Workspace”后展示按 Workspace 分组的候选。
- 唯一候选显示“一键绑定”。
- 多候选通过 radio/selector 选择后绑定。
- 绑定成功后刷新 Workspace 列表、Backend Access inventory 与 candidates。

## Error Handling

- 未授权或离线 backend：API 返回可展示错误。
- Unsupported identity：返回 skipped，不作为错误。
- P4 CLI 不可用或未登录：返回 warning / skipped。
- root 不存在、不可读、identity mismatch：候选跳过并记录 warning。
