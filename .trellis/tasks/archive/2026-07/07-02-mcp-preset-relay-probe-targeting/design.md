# 技术设计

## Boundary

MCP Preset probe 是 Runtime Gateway 的 Setup Action，不进入 Session Runtime Surface。它需要在 API route 保持薄入口的同时，把 probe target 作为 setup context 的一部分传给 application/provider。

运行时 session 的 MCP list/call 继续使用 `RuntimeBackendAnchor`，本任务只收束配置期/资产期 probe。

## Proposed Flow

1. 前端发起 probe 时提交：
   - `transport`
   - `route_policy`
   - `runtime_binding`
   - `probe_target`（新增，表达当前用户默认本机 backend，或显式 backend_id）
2. API route 校验 Project view 权限后进入 `RuntimeGateway::invoke()`。
3. `McpProbeTransportInput` 保留 `route_policy`，新增 target projection。
4. application probe：
   - `route_policy.uses_relay(transport) == false`：继续直连 HTTP/SSE；direct stdio 返回 unsupported。
   - `route_policy.uses_relay(transport) == true`：进入 relay provider，并要求 resolver 得到明确 backend id。
5. cloud relay provider 使用后端 helper 解析 target：
   - default-user-local target：从当前用户可见 backend 中筛选 `backend_type=Local`、`owner_user_id=current_user`、`share_scope=User(current_user)`、`device.registration_source=desktop_access_token`、在线且 enabled 的候选。
   - explicit-backend target：使用 `BackendAuthorizationService::require_backend(identity, backend_id, BackendPermission::View)` 校验，再检查 online / enabled。
   - 无可用 target：返回 probe result error/unsupported，不抛 command transport failure。
6. local relay handler 使用现有 one-shot probe，直接消费传入 transport。

## Contracts

新增 probe target 应是跨层 DTO，避免前端用隐式缓存或当前 Project workspace 推断：

```rust
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum McpProbeTargetDto {
    DefaultUserLocal,
    Backend { backend_id: String },
}
```

后端内部可映射为 application-port 类型，避免 API DTO 泄漏到 application crate。

## Backend Helper

新增一个后端侧 resolver/helper，作为 relay setup probe 唯一的 backend 选择入口：

```rust
pub enum McpProbeBackendTarget {
    DefaultUserLocal,
    Backend { backend_id: String },
}

pub async fn resolve_mcp_probe_backend_target(
    identity: &AuthIdentity,
    target: McpProbeBackendTarget,
) -> Result<ResolvedMcpProbeBackendTarget, McpProbeTargetError>
```

helper 依赖：

- `BackendRepository::list_backends()` / `get_backend()` 获取配置。
- `BackendAuthorizationService` 复用 owner、user scope、ProjectBackendAccess 和管理员/Personal 模式授权规则。
- `BackendRegistry::is_online()` 或 `list_online_ids()` 校验 relay 连接在线。

候选筛选规则：

- 默认目标只考虑 Desktop enrollment 产生的个人本机 backend：`BackendType::Local`、`enabled=true`、`owner_user_id=current_user.user_id`、`share_scope_kind=User`、`share_scope_id=current_user.user_id`、`device.registration_source="desktop_access_token"`。
- runner enrollment 产生的 shared local backend 不进入默认目标，即使它归属于当前用户；需要 UI 显式选择。
- 显式 backend 可以是 runner/backend，只要当前用户拥有 View 权限且 online。
- 默认目标没有候选时返回 `Unavailable(CurrentUserLocalBackendOffline)`。
- 默认目标存在多个候选时选择 `last_claimed_at` 最新的在线候选；没有 `last_claimed_at` 时使用稳定排序兜底，不能随机从 online map 中取一个。

## UI Shape

- Assets MCP Preset 卡片和详情默认使用 default-user-local target。
- 如果当前用户没有在线 Desktop local runtime，按钮仍可点击，但结果提示“当前客户端未连接本机 runtime”。
- 默认 probe 不增加运行环境选择器；显式 runner/backend probe 后续由已有 backend 管理/选择入口承接。

## Risk

- 同一用户可能有多台在线 Desktop local runtime。默认 helper 按最近 claimed 的可用 backend 选择，换取更少交互；localhost probe 在多设备场景下可能落到用户最近连接的另一台机器。
- 如果用户在浏览器 Web profile 而非桌面端使用，default-user-local target 只能表达“当前用户的在线本机 backend”，不能天然表达“当前浏览器所在机器”。

## Rollback

- 保留 `route_policy` 分流修复。
- 如 target 选择实现出现问题，可先只对默认目标缺失返回 unsupported，避免退回任意在线 backend。
