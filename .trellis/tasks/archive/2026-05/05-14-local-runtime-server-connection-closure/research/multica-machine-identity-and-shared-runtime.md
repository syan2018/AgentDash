# multica 机器身份与共享 runtime 评估

## 关键发现

multica 后期把 runtime 身份从 hostname 迁到了“机器级持久 UUID”。这不是实现细节，而是产品模型修正：

- hostname / device name 只适合做展示标签，不适合做身份键。
- daemon id 存在 `~/.multica/daemon.id`，同一台机器跨 CLI profile 复用一个 UUID。
- profile 只表达“连接哪个 server / account / workspace”，不表达物理机器身份。
- server runtime row 仍按 workspace 和 provider 切分，唯一键是 `(workspace_id, daemon_id, provider)`。
- runtime row 有 `owner_id` 和 `visibility`。默认 `private`，owner / admin 可编辑；切到 `public` 后 workspace 成员都可绑定使用。

对应代码：

- `references/multica/server/internal/daemon/identity.go`
  - `EnsureDaemonID(profile)` 生成并保存机器级 UUID。
  - `LegacyDaemonIDs(hostname, profile)` 输出 hostname、`.local`、profile suffix 等历史身份。
  - `LegacyDaemonUUIDs()` 扫描旧 per-profile UUID，给 server 做合并。
- `references/multica/server/internal/handler/daemon.go`
  - `DaemonRegister` 注册时带 `daemon_id`、`legacy_daemon_ids`、`device_name`、`launched_by`。
  - server 按 workspace/provider upsert runtime 后，调用 `mergeLegacyRuntimes` 把旧 hostname row 合并到新 UUID row。
- `references/multica/server/pkg/db/queries/runtime.sql`
  - `UpsertAgentRuntime` 使用 `(workspace_id, daemon_id, provider)`。
  - `UpdateAgentRuntimeVisibility` 在 `private/public` 间切换。
  - `ListAgentRuntimesByOwner` 支持“只看我的 runtime”。
- `references/multica/apps/desktop/src/main/daemon-manager.ts`
  - Desktop profile 名称从目标 API host 派生，例如 `desktop-<host>`。
  - profile 只保存 `server_url` 和 token，不污染用户手工 CLI profile。
  - token 绑定当前登录用户；用户切换时 mint 新 PAT 并重启 daemon。

## 对 AgentDash 当前方案的影响

当前 AgentDash 的第一版实现使用：

```text
backend_id = hash(owner_user_id, profile_id, device_id)
unique(backends.owner_user_id, profile_id, device_id)
```

它能解决“个人登录后本机自动上线”，但会把三个概念缠在一起：

- `device_id` 既像机器身份，又按 desktop profile 生成。
- `owner_user_id` 被写入 backend id，导致同一台机器换用户会变成另一个 backend。
- `profile_id` 既像 server target profile，又被纳入 server 侧身份唯一键。

这会影响未来“共用本机”：

- 同一台物理机器无法自然聚合多个 personal backend 与 shared backend。
- UI 只能看到一堆 backend id，不能以“机器标签”聚合展示。
- 共享/私有只能靠 owner_user_id 是否为空来猜，缺少显式 `visibility/share_scope`。
- 迁移 hostname/device id 漂移时没有 legacy identity 合并通道。

## 建议模型

把模型拆成三层：

1. `machine`
   - 表示本地安装/物理机器身份。
   - 本地生成稳定 `machine_id`，长期存储在 Tauri app config 或 OS user config 下。
   - `machine_label` / hostname 只作为展示标签。
   - 可带 `legacy_machine_ids`，用于从 hostname、旧 per-profile device_id 合并。

2. `backend`
   - 表示 server 可调度的 runtime endpoint。
   - 一个 machine 可以有多个 backend slot：
     - personal slot：`scope_kind = user`，`scope_id = owner_user_id`
     - shared project slot：`scope_kind = project`，`scope_id = project_id`
     - shared system slot：`scope_kind = system`，`scope_id = null`
   - `backend_id = hash(machine_id, slot_kind, slot_scope_id, capability_slot)`，而不是直接 hash owner/profile/device。

3. `claim / credential`
   - 表示某个已认证用户或共享策略对某个 backend 的领取动作。
   - server 生成 relay token。
   - token 绑定 backend row；WS register 仍强校验 `payload.backend_id == token.backend_id`。

## 字段建议

短期可以在 `backends` 上补足字段，不急着拆新表：

```text
machine_id TEXT
machine_label TEXT
legacy_machine_ids JSONB DEFAULT []
owner_user_id TEXT
visibility TEXT CHECK ('private', 'shared', 'system')
share_scope_kind TEXT CHECK ('user', 'project', 'system')
share_scope_id TEXT
profile_id TEXT
device JSONB
last_claimed_at TIMESTAMPTZ
```

唯一键建议从当前：

```text
owner_user_id + profile_id + device_id
```

演进为：

```text
machine_id + share_scope_kind + COALESCE(share_scope_id, '') + capability_slot
```

其中 personal backend 的 `share_scope_kind = 'user'`、`share_scope_id = owner_user_id`。这样“个人绑定”和“未来共享”共用一套模型。

## API 建议

`POST /api/local-runtime/ensure` 请求应从 `device_id` 语义调整为 machine/slot 语义：

```json
{
  "machine_id": "uuid-v7",
  "machine_label": "LIAOYIHAO-P",
  "legacy_machine_ids": ["old-host", "old-host.local"],
  "profile_id": "desktop-127.0.0.1-3001",
  "scope": {
    "kind": "user",
    "id": "local-user"
  },
  "capability_slot": "default",
  "name": "LIAOYIHAO-P / Personal",
  "accessible_roots": [],
  "executor_enabled": true,
  "device": {}
}
```

响应仍保持启动 runtime 所需信息：

```json
{
  "backend_id": "local_...",
  "machine_id": "uuid-v7",
  "scope": { "kind": "user", "id": "local-user" },
  "relay_ws_url": "ws://127.0.0.1:3001/ws/backend",
  "auth_token": "server-issued-token"
}
```

后续共享本机可以用同一个接口，只把 `scope.kind` 改为 `project/system`，并在 handler 层校验当前用户是否有创建/管理共享 runtime 的权限。

## UI 建议

Settings / LocalRuntime 不应该只显示 backend 列表，而应以 machine 为主：

```text
本机设备
  LIAOYIHAO-P
  machine_id: 0192...
  当前连接: dev-local

运行时 Scope
  Personal / Local User / private / online
  Project Shared / AgentDash / shared / offline
```

Dashboard 和 Workspace binding 选择 backend 时，应展示：

- 机器标签：`LIAOYIHAO-P`
- scope：个人 / 项目共享 / 系统共享
- owner 或 share target
- capability 摘要与在线状态

不要让用户直接面对 `local_82f22...` 这种 backend id。

## 结论

AgentDash 不应照搬 multica 的 workspace + provider runtime 维度，但应吸收它的身份分层：

- 机器身份必须稳定且与 hostname 解耦。
- 个人绑定是 machine 上的一个 private scope，不等同于 machine 本身。
- 共享本机是同一 machine 上的 shared scope，不应另起一套模型。
- server 仍是 token 和 runtime row 的权威，本机只保存 machine id、profile 偏好和可恢复配置。
