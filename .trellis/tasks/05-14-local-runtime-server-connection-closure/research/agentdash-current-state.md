# AgentDash 当前状态证据

## Tauri

`crates/agentdash-local-tauri/src/main.rs`：

- `RuntimeStartRequest` 需要 `cloud_url/token/backend_id/name/accessible_roots/executor_enabled`。
- `LocalRuntimeProfile` 仅保存在本地 app config 下的 `desktop-runtime-profile.json`。
- `runtime_start` 在 `backend_id` 缺失时使用随机 UUID：
  - 这与 server relay 的 token 绑定校验冲突。
  - 随机 backend_id 没有对应 `backends` row，注册无法成为稳定闭环。
- `DesktopApiManager` 可以启动 embedded API，但没有自动创建/领取 backend token，也没有把 embedded API 与 local runtime 绑定。

`packages/app-tauri/src/App.tsx`：

- 当前 `DesktopView = 'runtime' | 'dashboard'`。
- Dashboard view 通过 `<WebDashboardApp />` 嵌入 web app。
- Runtime view 单独渲染 `<LocalRuntimeView />`。
- 这会产生两个信息架构问题：Dashboard 像外接页面，Local runtime 像另一个应用，而不是 desktop settings 能力。

## Local runtime

`crates/agentdash-local/src/runtime.rs`：

- `LocalRuntimeManager::start` 构造 `ws_client::Config`，启动 `run_until_shutdown`。
- 本地 runtime 能管理 accessible roots、MCP config、SessionHub、executor enabled。
- 它假设调用者提供有效 `cloud_url/token/backend_id`，不负责 server provisioning。

`crates/agentdash-local/src/ws_client.rs`：

- 连接 URL 是 `${cloud_url}?token=${token}`。
- 首包发送 `RelayMessage::Register`，payload 包含 `backend_id/name/version/capabilities/accessible_roots`。
- 等待 `register_ack` 或 error。

## Server relay

`crates/agentdash-api/src/relay/ws_handler.rs`：

- `/ws/backend?token=...` 通过 `backend_repo.get_backend_by_auth_token` 授权。
- 第一条 WS 消息必须是 `register`。
- `validate_register_payload` 强制 `payload.backend_id == authorized_backend.id`。
- 注册成功后写入 `runtime_health` online。
- 断开时 unregister 并 mark offline。

结论：当前 relay 的安全模型是合理的，缺的是“谁来创建/领取 authorized backend + token”。

## Backend API

`crates/agentdash-api/src/routes/backends.rs`：

- `POST /api/backends` 可以创建 backend，若不传 token 会生成 token。
- 该接口更像通用管理接口，需要调用者传 id/name/endpoint，不适合 Tauri 启动时自动 claim 本机 runtime。
- 当前没有一个面向 desktop 的 ensure API，一次性返回 `backend_id + auth_token + relay_ws_url`。

`crates/agentdash-domain/src/backend/entity.rs`：

- `BackendConfig` 已包含 `owner_user_id`。
- `RuntimeHealth` 已包含 `profile_id`。
- 这两个字段足以承接“当前用户的 desktop profile local backend”语义，但 `backends` 缺少 `profile_id/device_id` 来做唯一定位。

## 结论

最小正确改法不是让用户继续手填 profile，也不是放松 relay 校验，而是新增 server ensure 控制面，让 Tauri 每次启动都从 server 领取权威 backend 身份，再启动 local runtime。
