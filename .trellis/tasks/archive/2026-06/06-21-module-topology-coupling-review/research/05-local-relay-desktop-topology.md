# Research: local relay desktop topology

- Query: 盘查 Local backend / Relay / Desktop shell / workspace routing 主链路拓扑与耦合点，产出后续架构 review 问题清单。
- Scope: internal
- Date: 2026-06-21

## 1. 模块/子模块清单与一句话职责

### Cloud backend / relay bootstrap

- `crates/agentdash-api/src/bootstrap/relay.rs` - 创建 `BackendRegistry`、backend runtime event channel、MCP probe relay provider 和 shell output registry，是 cloud 侧 relay runtime 的装配入口。
- `crates/agentdash-api/src/bootstrap/session.rs` - 把 `RelayAgentConnector` 接入 `CompositeConnector`，并把 backend execution placement transport/lease repo 注入 session runtime builder。
- `crates/agentdash-api/src/bootstrap/runtime_gateway.rs` - 组合 setup action、session MCP action 与 extension action transport，形成 cloud runtime gateway。
- `crates/agentdash-api/src/app_state.rs` - API composition root，串起 relay/bootstrap/session/runtime gateway 等输出并注入 `AppState`。

### Cloud relay / backend registry

- `crates/agentdash-api/src/relay/registry.rs` - cloud 侧在线 backend registry，维护在线连接、pending command、session sink route、capability/executor snapshot。
- `crates/agentdash-api/src/relay/ws_handler.rs` - 本机 backend WebSocket 入口，验证首条 register、写 runtime health、注册/注销 registry 并驱动消息循环。
- `crates/agentdash-api/src/relay/mcp_relay_impl.rs` - cloud MCP relay provider adapter，通过 registry 下发 MCP list/call/probe command。
- `crates/agentdash-api/src/relay/extension_runtime_impl.rs` - extension runtime action/channel transport adapter，把 application payload 映射成 relay command。
- `crates/agentdash-api/src/workspace_resolution.rs` - API 层 relay transport adapter，给 workspace detect、prompt、cancel、terminal 等能力提供 backend registry 访问。

### Relay protocol

- `crates/agentdash-relay/src/protocol.rs` - relay 顶层 wire envelope，集中定义 register、prompt、workspace、tool、MCP、extension、terminal 等 command/response/event 类型。
- `crates/agentdash-relay/src/protocol/handshake.rs` - backend register / ack / capabilities payload。
- `crates/agentdash-relay/src/protocol/prompt.rs` - `command.prompt` payload，承载 session id、turn id、executor、`mount_root_ref`、working dir、MCP servers 与 restored state。
- `crates/agentdash-relay/src/protocol/tool.rs` - file/shell/search 等 local tool payload，使用 `mount_root_ref` 作为当前 workspace root 边界。
- `crates/agentdash-relay/src/protocol/workspace.rs` - workspace detect / browse payload。
- `crates/agentdash-relay/src/protocol/extension_runtime.rs` - extension action/channel payload，承载 package artifact 与 session workspace context。
- `crates/agentdash-relay/src/protocol/terminal.rs` - terminal spawn/input/resize/kill payload，使用 session id 与 mount root。

### Backend / workspace routing

- `crates/agentdash-api/src/routes/backends.rs` - backend 列表、runtime summary、local runtime ensure/claim、browse directory 等 HTTP API。
- `crates/agentdash-api/src/routes/backend_access.rs` - Project backend access、backend inventory registration、workspace candidates 与 binding sync HTTP API。
- `crates/agentdash-api/src/routes/workspaces.rs` - workspace create/update/detect/git detect 与 binding 维护 API。
- `crates/agentdash-application/src/workspace/detection.rs` - 通过 backend transport 做 workspace detect，并生成 `WorkspaceBinding` 与 identity payload。
- `crates/agentdash-application/src/workspace/resolution.rs` - 从 workspace bindings、授权 backend 集合和在线状态解析当前可用 binding。
- `crates/agentdash-application/src/workspace/backend_sync.rs` - 从 backend inventory 同步 workspace binding。
- `crates/agentdash-application/src/backend_execution_placement.rs` - 根据 explicit / workspace binding / auto idle intent 解析执行 backend placement，并 claim backend execution lease。

### Session runtime / relay connector

- `crates/agentdash-application/src/session/launch/planner.rs` - session launch 阶段解析 working dir、backend selection intent、backend execution placement。
- `crates/agentdash-application/src/session/launch/plan.rs` - 将已 claim 的 backend execution placement 写入 `ExecutionContext.session.backend_execution`。
- `crates/agentdash-application/src/relay_connector.rs` - `RelayAgentConnector`，用 `ExecutionContext.session.backend_execution` 选择 backend、注册 session sink、下发 prompt、释放 lease。

### Local backend runtime

- `crates/agentdash-local/src/runtime.rs` - 本机 runtime library，管理 `LocalRuntimeManager`、runtime lifecycle、workspace roots canonicalization、WS config 构建。
- `crates/agentdash-local/src/ws_client.rs` - 本机主动连接 cloud relay websocket，发送 register，接收 command，调用 local handler，并写回 response/event。
- `crates/agentdash-local/src/machine_identity.rs` - 本机 machine identity 生成与持久化事实源。
- `crates/agentdash-local/src/tool_executor.rs` - local file/shell/search tool 边界与 workspace root guard。
- `crates/agentdash-local/src/process_executor.rs` - extension/process host 使用的 process execution guard。

### Local command handlers

- `crates/agentdash-local/src/handlers/mod.rs` - local relay command 顶层 router，按 `RelayMessage` dispatch 到 domain handler。
- `crates/agentdash-local/src/handlers/prompt.rs` - prompt/cancel/steer/discovery/session notification forwarding。
- `crates/agentdash-local/src/handlers/workspace.rs` - workspace detect、detect_git、browse directory。
- `crates/agentdash-local/src/handlers/tool_calls.rs` - file/shell/search tool command。
- `crates/agentdash-local/src/handlers/mcp_relay.rs` - MCP probe/list/call/close command。
- `crates/agentdash-local/src/handlers/materialization.rs` - VFS materialization command。
- `crates/agentdash-local/src/handlers/extension.rs` - extension action/channel command，负责 artifact cache、activation context、workspace context 后进入 local TS extension host。
- `crates/agentdash-local/src/handlers/terminal.rs` - local terminal spawn/input/resize/kill command。

### Extension host execution side

- `crates/agentdash-local/src/extensions/artifact_cache.rs` - local extension artifact download/cache/unpack/digest validation。
- `crates/agentdash-local/src/extensions/host/manager.rs` - Node extension host lifecycle、activate/invoke action/channel、output schema validation。
- `crates/agentdash-local/src/extensions/host/process.rs` - Node stdio JSON line request/response process wrapper。
- `crates/agentdash-local/src/extensions/host/host_api.rs` - Rust Host API facade，按 active invocation 与 permission guard 执行 workspace/process/env/http/runtime/channel API。
- `crates/agentdash-local/src/extensions/host/permission_guard.rs` - action/channel runtime permission 裁决。
- `crates/agentdash-local/src/extensions/host/schema.rs` - local host output schema JSON Schema 子集校验。
- `crates/agentdash-local/src/extensions/host/runner/*.mjs` - JS runner、loader、context、host api client，承载 trusted local extension runtime。

### Desktop shell / dashboard host

- `crates/agentdash-local-tauri/src/main.rs` - Tauri command host，持有 `LocalRuntimeManager`，负责 profile、runtime、MCP、directory browse、desktop API lifecycle 与 local runtime ensure/claim。
- `packages/app-tauri/src/App.tsx` - Desktop app shell，注入 local runtime client 到 window，`DashboardHost` 等待 desktop API health 后渲染 Dashboard。
- `packages/app-tauri/src/runtimeApi.ts` - Tauri `invoke()` 到 `LocalRuntimeClient` port 的适配层。
- `packages/app-web/src/desktop/localRuntimeBridge.ts` - Web Dashboard 侧读取 desktop injected `LocalRuntimeClient` port。
- `packages/core/src/local-runtime/index.ts` - TS local runtime port / DTO 定义。

## 2. 主链路拓扑

### A. Desktop boot / cloud ensure / local runtime start

1. Tauri shell 内部默认启动 desktop API：`agentdash-local-tauri` 使用 `ApiServerOptions::desktop_localhost(DESKTOP_API_PORT)` 启动 API，并用 `/api/health` 做 ready probe（`crates/agentdash-local-tauri/src/main.rs:702`, `crates/agentdash-local-tauri/src/main.rs:794`）。
2. `packages/app-tauri/src/App.tsx` 在启动时把 `createTauriLocalRuntimeClient()` 注入 `window.__AGENTDASH_DESKTOP_LOCAL_RUNTIME__`，并由 `DashboardHost` 轮询 `desktop_api_snapshot` 与 `/api/health`；只有 health ready 才渲染 dashboard（`packages/app-tauri/src/App.tsx:29`, `packages/app-tauri/src/App.tsx:52`, `packages/app-tauri/src/App.tsx:77`, `packages/app-tauri/src/App.tsx:94`）。
3. local runtime start 前，Tauri 通过 `profile_load()` 读取 profile，并用 `load_or_create_machine_identity()` 覆盖 canonical machine id（`crates/agentdash-local-tauri/src/main.rs:141`, `crates/agentdash-local-tauri/src/main.rs:537`, `crates/agentdash-local-tauri/src/main.rs:560`）。
4. Tauri claim cloud local runtime：`POST /api/local-runtime/ensure` 请求带 `machine_id + share_scope_kind + share_scope_id + capability_slot`，响应返回 `backend_id + relay_ws_url + auth_token`（`crates/agentdash-local-tauri/src/main.rs:428`, `crates/agentdash-local-tauri/src/main.rs:433`, `crates/agentdash-local-tauri/src/main.rs:510`; cloud DTO 在 `crates/agentdash-api/src/dto/backend.rs:24`, `crates/agentdash-api/src/dto/backend.rs:50`）。
5. Cloud ensure endpoint 由 `routes/backends.rs` 实现，调用 `ensure_local_runtime_record` 后返回 server-side backend id、endpoint/relay ws url、auth token 与 share scope facts（`crates/agentdash-api/src/routes/backends.rs:431`, `crates/agentdash-api/src/routes/backends.rs:437`, `crates/agentdash-api/src/routes/backends.rs:461`）。
6. Tauri 用 claim response 创建 `LocalRuntimeConfig` 并调用 `LocalRuntimeManager.start()`（`crates/agentdash-local-tauri/src/main.rs:414`, `crates/agentdash-local-tauri/src/main.rs:416`; local runtime config 在 `crates/agentdash-local/src/runtime.rs:26`，manager 在 `crates/agentdash-local/src/runtime.rs:115`, start 在 `crates/agentdash-local/src/runtime.rs:142`）。

### B. Local backend registration / capability surface

1. `agentdash-local` 通过 `ws_client` 用 `relay_ws_url?token=...` 主动连接 cloud relay（`crates/agentdash-local/src/ws_client.rs:60`, `crates/agentdash-local/src/ws_client.rs:64`）。
2. 本机创建 `CommandHandler` 时注入 backend id、workspace roots、tool executor、session runtime、connector、MCP manager、workspace config、extension host、artifact API/token/cache root、event tx（`crates/agentdash-local/src/handlers/mod.rs:59`, `crates/agentdash-local/src/handlers/mod.rs:60`, `crates/agentdash-local/src/handlers/mod.rs:62`, `crates/agentdash-local/src/handlers/mod.rs:66`, `crates/agentdash-local/src/handlers/mod.rs:70`）。
3. 本机发送 `RelayMessage::Register`，payload 包含 backend id、name、capabilities、workspace roots（`crates/agentdash-local/src/ws_client.rs:120`, `crates/agentdash-local/src/ws_client.rs:122`, `crates/agentdash-local/src/ws_client.rs:127`; protocol payload 在 `crates/agentdash-relay/src/protocol/handshake.rs:9`）。
4. Cloud ws handler 要求首条消息是 register，校验授权 backend id，写 runtime health online，并注册 `ConnectedBackend` 到 registry（`crates/agentdash-api/src/relay/ws_handler.rs:58`, `crates/agentdash-api/src/relay/ws_handler.rs:78`, `crates/agentdash-api/src/relay/ws_handler.rs:108`, `crates/agentdash-api/src/relay/ws_handler.rs:121`, `crates/agentdash-api/src/relay/ws_handler.rs:144`）。
5. `BackendRegistry` 同时维护 online backend map、pending response map、session sink routes 与 executor snapshot（`crates/agentdash-api/src/relay/registry.rs:55`, `crates/agentdash-api/src/relay/registry.rs:58`, `crates/agentdash-api/src/relay/registry.rs:176`, `crates/agentdash-api/src/relay/registry.rs:230`）。

### C. Cloud command/control -> relay websocket -> local command handler/tool executor

1. Relay 顶层 command/response/event 都由 `RelayMessage` 承载，类型覆盖 `command.prompt`、workspace detect、tool file/shell/search、MCP、extension、terminal 等（`crates/agentdash-relay/src/protocol.rs:40`, `crates/agentdash-relay/src/protocol.rs:67`, `crates/agentdash-relay/src/protocol.rs:99`, `crates/agentdash-relay/src/protocol.rs:121`, `crates/agentdash-relay/src/protocol.rs:449`, `crates/agentdash-relay/src/protocol.rs:526`）。
2. Cloud 侧通过 `BackendRegistry::send_command_with_timeout` 将 command 写入目标 backend sender，并按 msg id 注册 pending response（`crates/agentdash-api/src/relay/registry.rs:326`, `crates/agentdash-api/src/relay/registry.rs:345`）。
3. 本机 ws loop 收到 relay message 后调用 `handler.handle(relay_msg).await`，再把 responses 写回 websocket；异步 session notification 等通过 `event_tx` 走同一写通道（`crates/agentdash-local/src/ws_client.rs:102`, `crates/agentdash-local/src/ws_client.rs:199`, `crates/agentdash-local/src/ws_client.rs:211`）。
4. Local `CommandHandler.handle()` 只 match envelope 并 dispatch 到 `prompt/workspace/tool/materialization/mcp/extension/terminal` handler（`crates/agentdash-local/src/handlers/mod.rs:117`, `crates/agentdash-local/src/handlers/mod.rs:130`, `crates/agentdash-local/src/handlers/mod.rs:147`, `crates/agentdash-local/src/handlers/mod.rs:162`, `crates/agentdash-local/src/handlers/mod.rs:192`, `crates/agentdash-local/src/handlers/mod.rs:207`, `crates/agentdash-local/src/handlers/mod.rs:220`, `crates/agentdash-local/src/handlers/mod.rs:236`）。
5. Tool payload 统一携带 `mount_root_ref`；shell exec 的 `cwd` 只能为空或解析到 `mount_root_ref` 边界内（`crates/agentdash-relay/src/protocol/tool.rs:10`, `crates/agentdash-relay/src/protocol/tool.rs:60`, `crates/agentdash-relay/src/protocol/tool.rs:63`）。
6. Local `ToolExecutor` 在 workspace roots 为空时允许 session mount root 成为边界；有 roots 时检查 mount root 落在 configured roots 下（`crates/agentdash-local/src/tool_executor.rs:66`, `crates/agentdash-local/src/tool_executor.rs:98`, `crates/agentdash-local/src/tool_executor.rs:584`）。

### D. Relay prompt / execution placement / lease route

1. Session launch planner 会解析 backend selection intent，并在 launch 阶段 claim backend execution placement（`crates/agentdash-application/src/session/launch/planner.rs:225`, `crates/agentdash-application/src/session/launch/planner.rs:264`, `crates/agentdash-application/src/session/launch/planner.rs:308`）。
2. `backend_execution_placement` 支持 explicit、workspace binding、auto idle 三类 intent，并校验目标 executor online/capable；auto idle 按 active leases 选择（`crates/agentdash-application/src/backend_execution_placement.rs:56`, `crates/agentdash-application/src/backend_execution_placement.rs:101`, `crates/agentdash-application/src/backend_execution_placement.rs:144`, `crates/agentdash-application/src/backend_execution_placement.rs:174`）。
3. 已 claim placement 被写入 `ExecutionContext.session.backend_execution`，包含 `backend_id + lease_id + selection_mode`（`crates/agentdash-spi/src/connector/mod.rs:79`, `crates/agentdash-spi/src/connector/mod.rs:87`; plan 映射在 `crates/agentdash-application/src/session/launch/plan.rs:304`）。
4. `RelayAgentConnector` 发送 prompt 时要求 `context.session.backend_execution` 已存在，使用其中 backend id 和 lease id，先注册 session sink，再下发 prompt（`crates/agentdash-application/src/relay_connector.rs:102`, `crates/agentdash-application/src/relay_connector.rs:107`, `crates/agentdash-application/src/relay_connector.rs:162`, `crates/agentdash-application/src/relay_connector.rs:173`）。
5. terminal completed/failed/interrupted 会释放 backend execution lease 并 unregister session route（`crates/agentdash-application/src/relay_connector.rs:209`, `crates/agentdash-application/src/relay_connector.rs:233`, `crates/agentdash-application/src/relay_connector.rs:264`）。

### E. Workspace detect / inventory / binding routing

1. Project backend inventory registration endpoint 读取 active backend access 后通过 Runtime Gateway 调用 `workspace.detect`，然后 upsert inventory（`crates/agentdash-api/src/routes/backend_access.rs:377`, `crates/agentdash-api/src/routes/backend_access.rs:381`, `crates/agentdash-api/src/routes/backend_access.rs:386`）。
2. Workspace create/update/detect 也通过 `ensure_project_backend_access` 和 relay workspace detect 确认 binding facts（`crates/agentdash-api/src/routes/workspaces.rs:267`, `crates/agentdash-api/src/routes/workspaces.rs:272`, `crates/agentdash-api/src/routes/workspaces.rs:367`, `crates/agentdash-api/src/routes/workspaces.rs:373`）。
3. Runtime Gateway setup provider 调用 application workspace detection，后者通过 backend transport 检查 online 并发 `workspace.detect`（`crates/agentdash-application/src/runtime_gateway/setup_actions.rs:209`, `crates/agentdash-application/src/workspace/detection.rs:64`, `crates/agentdash-application/src/workspace/detection.rs:70`）。
4. Local workspace handler 将 root path 解析为本机目录后执行 probe，目录浏览则是 setup 选择器能力，不依赖 session workspace roots（`crates/agentdash-local/src/handlers/workspace.rs:15`, `crates/agentdash-local/src/handlers/workspace.rs:30`, `crates/agentdash-local/src/handlers/workspace.rs:134`, `crates/agentdash-local/src/handlers/workspace.rs:178`）。
5. Application workspace resolution 使用 workspace bindings、allowed backend ids、online 状态和 resolution policy 选择 binding（`crates/agentdash-application/src/workspace/resolution.rs:55`, `crates/agentdash-application/src/workspace/resolution.rs:70`, `crates/agentdash-application/src/workspace/resolution.rs:105`, `crates/agentdash-application/src/workspace/resolution.rs:113`）。

### F. Extension action/channel relay -> local TS extension host

1. Cloud extension runtime route 目前接收 request `backend_id`，校验 Project backend access，并从 session VFS mount 中选择该 backend 的 workspace root（`crates/agentdash-api/src/routes/extension_runtime.rs:133`, `crates/agentdash-api/src/routes/extension_runtime.rs:139`, `crates/agentdash-api/src/routes/extension_runtime.rs:328`, `crates/agentdash-api/src/routes/extension_runtime.rs:339`）。
2. Application runtime gateway 验证 action/channel input schema 和权限分类，再把 package artifact、session id、backend id、workspace context 交给 extension action transport（`crates/agentdash-application/src/runtime_gateway/extension_actions.rs:160`, `crates/agentdash-application/src/runtime_gateway/extension_actions.rs:169`, `crates/agentdash-application/src/runtime_gateway/extension_actions.rs:170`, `crates/agentdash-application/src/runtime_gateway/extension_actions.rs:176`, `crates/agentdash-application/src/runtime_gateway/extension_actions.rs:197`）。
3. Relay payload 携带 `package_artifact` 与 `workspace { mount_id, root_ref }`（`crates/agentdash-relay/src/protocol/extension_runtime.rs:30`, `crates/agentdash-relay/src/protocol/extension_runtime.rs:36`, `crates/agentdash-relay/src/protocol/extension_runtime.rs:45`, `crates/agentdash-relay/src/protocol/extension_runtime.rs:49`）。
4. Local extension handler 下载/缓存 artifact，构造 activation：backend id、project/session id、default workspace root、registered workspace roots，然后调用 `LocalExtensionHostManager`（`crates/agentdash-local/src/handlers/extension.rs:191`, `crates/agentdash-local/src/handlers/extension.rs:206`, `crates/agentdash-local/src/handlers/extension.rs:210`, `crates/agentdash-local/src/handlers/extension.rs:213`, `crates/agentdash-local/src/handlers/extension.rs:245`, `crates/agentdash-local/src/handlers/extension.rs:282`）。
5. Artifact cache key 使用 `artifact_id + archive_digest`，下载后校验 sha256 digest 并解包到 cache package dir（`crates/agentdash-local/src/extensions/artifact_cache.rs:51`, `crates/agentdash-local/src/extensions/artifact_cache.rs:52`, `crates/agentdash-local/src/extensions/artifact_cache.rs:79`, `crates/agentdash-local/src/extensions/artifact_cache.rs:88`, `crates/agentdash-local/src/extensions/artifact_cache.rs:236`）。
6. Local host manager 执行 action/channel 后校验 output schema（`crates/agentdash-local/src/extensions/host/manager.rs:115`, `crates/agentdash-local/src/extensions/host/manager.rs:124`, `crates/agentdash-local/src/extensions/host/manager.rs:136`, `crates/agentdash-local/src/extensions/host/manager.rs:145`, `crates/agentdash-local/src/extensions/host/manager.rs:155`, `crates/agentdash-local/src/extensions/host/manager.rs:168`）。
7. Host API 不接受 raw `workspace_root` override，workspace/process API 都回到 active extension 的 default workspace root（`crates/agentdash-local/src/extensions/host/host_api.rs:86`, `crates/agentdash-local/src/extensions/host/host_api.rs:90`, `crates/agentdash-local/src/extensions/host/host_api.rs:94`, `crates/agentdash-local/src/extensions/host/host_api.rs:112`）。

## 3. 与其它模块的耦合点

### Session runtime

- Boundary: session launch planner 负责把 backend selection intent 解析成已 claim 的 backend execution placement；relay connector 只消费 `ExecutionContext.session.backend_execution`。
- Coupling point: `session/launch/planner.rs` 仍有从 VFS default mount 推导 preferred backend 的路径（`crates/agentdash-application/src/session/launch/planner.rs:391`, `crates/agentdash-application/src/session/launch/planner.rs:417`）。这可能是 workspace binding intent 的实现细节，也可能与“Relay connector 不再猜测 backend”的规格形成事实源边界问题。
- Coupling point: `RelayAgentConnector` 强依赖 lease repo 与 session sink route 生命周期，terminal/cancel 需要同一 route facts（`crates/agentdash-application/src/relay_connector.rs:162`, `crates/agentdash-application/src/relay_connector.rs:261`, `crates/agentdash-application/src/relay_connector.rs:264`）。

### VFS

- Boundary: relay tool/terminal/extension 都消费 session VFS 默认 mount 的 `root_ref`/`backend_id` 投影，但 VFS 本身不是执行 backend lease 的事实源。
- Coupling point: terminal target 直接从 session 默认 relay mount 读取 backend id/root ref（`crates/agentdash-api/src/routes/terminals.rs:300`, `crates/agentdash-api/src/routes/terminals.rs:307`, `crates/agentdash-api/src/routes/terminals.rs:315`）。这可作为 terminal-specific routing，但应与 backend execution lease / session placement 的边界明确。
- Coupling point: extension runtime route 通过 VFS mounts 为指定 backend 选择 workspace context（`crates/agentdash-api/src/routes/extension_runtime.rs:328`, `crates/agentdash-api/src/routes/extension_runtime.rs:339`, `crates/agentdash-api/src/routes/extension_runtime.rs:350`）。
- Coupling point: VFS materialization 是单独 relay command，local handler 只进入 materialization store（`crates/agentdash-local/src/handlers/mod.rs:192`, `crates/agentdash-local/src/handlers/materialization.rs:8`）。上层 VFS resource semantics 不应进入 local store。

### Extension host

- Boundary: RuntimeGateway 拥有 Project extension runtime projection、input schema 与 permission precheck；Local Extension Host 拥有 artifact cache、Node process lifecycle、Host API permission facade、output schema validation。
- Coupling point: extension invocation 仍从 frontend/API request 携带 `backend_id`，同时再用 session VFS 选择 workspace；这形成 “frontend target / Project access / session workspace / relay placement” 四个事实的交汇点（`packages/app-web/src/generated/extension-runtime-contracts.ts:50`, `crates/agentdash-api/src/routes/extension_runtime.rs:133`, `crates/agentdash-api/src/routes/extension_runtime.rs:139`）。
- Coupling point: local host activation 把 registered workspace roots 和 session default workspace root 合并给 tool/process executor（`crates/agentdash-local/src/extensions/host/manager.rs:367`, `crates/agentdash-local/src/extensions/host/manager.rs:368`）。

### Workspace routing

- Boundary: setup 阶段 directory browse/detect/register 只产生 backend inventory / workspace binding facts；执行阶段 placement 由 backend execution lease / allocator 表达。
- Coupling point: frontend workspace routing 仍在 `workspaceRouting.ts` 中从 bindings、authorized backend、online backend 计算 availability/resolution summary（`packages/app-web/src/features/workspace/model/workspaceRouting.ts:200`, `packages/app-web/src/features/workspace/model/workspaceRouting.ts:220`, `packages/app-web/src/features/workspace/model/workspaceRouting.ts:233`）。若 UI 展示“可分配/忙碌”，应确认是否消费 `/backends/runtime-summary` 而不是自行推导。
- Coupling point: backend inventory register 和 workspace create shortcut 都会触发 `workspace.detect`，后续 review 应确认 inventory 和 workspace binding 没有互相替代事实源（`crates/agentdash-api/src/routes/backend_access.rs:377`, `crates/agentdash-api/src/routes/workspaces.rs:367`）。

### Frontend desktop mode

- Boundary: `app-tauri` 是 DashboardHost shell，`app-web` 仍通过 HTTP API 访问 dashboard data；只有 Local Runtime panel 通过 injected `LocalRuntimeClient` 使用 Tauri invoke。
- Coupling point: Tauri profile/claim/start 逻辑与 local runtime library 的 machine identity、profile persistence 共同决定 backend identity；主事实应保持在 local library 和 server ensure/claim response（`crates/agentdash-local-tauri/src/main.rs:537`, `crates/agentdash-local-tauri/src/main.rs:554`, `crates/agentdash-local/src/machine_identity.rs:14`）。
- Coupling point: `packages/core/src/local-runtime/index.ts` 的 state union 包含 `stopping`，而 desktop spec 的 `desktop_api_snapshot.state` 只允许 `starting | running | error | stopped`；需要确认这是 LocalRuntime state 与 DesktopApiSnapshot state 的刻意差异，而不是 UI 状态契约漂移（`packages/core/src/local-runtime/index.ts:1`, `packages/app-tauri/src/App.tsx:12`）。

### Backend registry / runtime health / lease

- Boundary: registry online/capability 表示连接健康与 executor snapshot；`backend_execution_leases` 表示 session execution occupancy；runtime summary 汇总展示用投影。
- Coupling point: `BackendRegistry::find_backend_for_context` 仍包含 session route、VFS default mount、MCP server capability 多级 fallback（`crates/agentdash-api/src/relay/registry.rs:276`, `crates/agentdash-api/src/relay/registry.rs:287`, `crates/agentdash-api/src/relay/registry.rs:306`）。这可能只服务 MCP/tool legacy transport，但需要核对是否会绕过 launch placement。
- Coupling point: `/backends/runtime-summary` 将 online health 与 active leases 合并，供前端展示 active session count 与 allocatable 状态（`crates/agentdash-api/src/routes/backends.rs:63`, `crates/agentdash-api/src/routes/backends.rs:239`, `crates/agentdash-api/src/routes/backends.rs:286`, `crates/agentdash-api/src/routes/backends.rs:357`）。

## 4. 值得下一轮深挖的 review 问题

### P0

1. **执行 backend 事实源是否仍有多入口推导。**  
   Spec 要求 session launch 把 backend selection intent 解析成已 claim 的 `backend_id + lease_id + selection_mode`，relay connector 不再从 VFS mount 猜测。代码中 session launch planner 仍会从 VFS 推导 workspace binding backend（`crates/agentdash-application/src/session/launch/planner.rs:391`），registry 的 `find_backend_for_context` 也仍会从 context VFS default mount 回退推导 backend（`crates/agentdash-api/src/relay/registry.rs:287`）。下一轮应判定：哪些调用链允许“从 workspace binding/VFS 生成 selection intent”，哪些调用链只能消费已 claim placement；任何 prompt/cancel/tool/MCP/extension 不应绕过 lease placement。

2. **session route / backend lease / registry disconnect cleanup 是否是唯一 terminal cleanup 链。**  
   Registry unregister 会移除 pending 与 session routes（`crates/agentdash-api/src/relay/registry.rs:109`, `crates/agentdash-api/src/relay/registry.rs:115`, `crates/agentdash-api/src/relay/registry.rs:122`），RelayAgentConnector 在 prompt/cancel/terminal 释放 lease（`crates/agentdash-application/src/relay_connector.rs:209`, `crates/agentdash-application/src/relay_connector.rs:233`, `crates/agentdash-application/src/relay_connector.rs:267`）。需要核对 backend disconnect 时 active lease 是否也有统一 terminal/failure cleanup owner，避免 registry 只清 route/pending 而 lease 仍 active，导致 `/backends/runtime-summary` 忙碌事实漂移。

3. **extension invocation 的 backend target 是否应由 frontend request 直接提供。**  
   Extension runtime API request 要求 `backend_id`（`packages/app-web/src/generated/extension-runtime-contracts.ts:50`），route 校验 access 后从 session VFS 选 workspace（`crates/agentdash-api/src/routes/extension_runtime.rs:133`, `crates/agentdash-api/src/routes/extension_runtime.rs:139`, `crates/agentdash-api/src/routes/extension_runtime.rs:328`）。需要确认 action target 的事实源是 Project workspace binding、session current workspace、WorkspacePanel tab target，还是 frontend request。若 frontend request 是 command intent，也应明确 server-side resolver 如何防止与 session/default workspace/lease facts 不一致。

### P1

1. **Local `CommandHandler` 是否已经足够薄，还是仍有共享 dependency context 泄漏。**  
   顶层 router 已按 domain handler dispatch（`crates/agentdash-local/src/handlers/mod.rs:117`），但 config 仍集中拥有 prompt/session runtime、tool executor、MCP manager、extension host、artifact token/cache、terminal manager/event tx 等横向依赖（`crates/agentdash-local/src/handlers/mod.rs:59` 到 `crates/agentdash-local/src/handlers/mod.rs:70`）。下一轮不应重复“CommandHandler 太厚”的旧结论，而应逐项核对 domain handler 是否只持有本域依赖。

2. **DashboardHost / LocalRuntimeClient port 是否保持 thin shell，Tauri 是否仍承载过多 profile/claim 业务。**  
   DashboardHost health gate 符合 spec（`packages/app-tauri/src/App.tsx:52`, `packages/app-tauri/src/App.tsx:77`），但 Tauri main 内仍有 ensure payload/response DTO、claim retry、claim validation、profile normalization（`crates/agentdash-local-tauri/src/main.rs:322`, `crates/agentdash-local-tauri/src/main.rs:428`, `crates/agentdash-local-tauri/src/main.rs:475`, `crates/agentdash-local-tauri/src/main.rs:537`）。需要判定这些是 shell adapter 必需逻辑，还是应进一步下沉到 `agentdash-local` runtime library。

3. **workspace routing 前端展示是否混用 online / binding / allocatable。**  
   Spec 要求执行空闲/忙碌与可分配状态从 `/backends/runtime-summary` 消费。`workspaceRouting.ts` 当前从 workspace bindings、authorized backends、backend online 计算 summary（`packages/app-web/src/features/workspace/model/workspaceRouting.ts:200`, `packages/app-web/src/features/workspace/model/workspaceRouting.ts:233`, `packages/app-web/src/features/workspace/model/workspaceRouting.ts:244`）。需要确认该 helper 只负责目录 binding 可用性，而不展示 execution allocatable。

4. **terminal target 是否应绑定 session placement，还是允许从 VFS default mount 独立解析。**  
   Terminal route 从 session default relay mount 读 backend id/root ref（`crates/agentdash-api/src/routes/terminals.rs:300`, `crates/agentdash-api/src/routes/terminals.rs:307`, `crates/agentdash-api/src/routes/terminals.rs:315`）。如果 terminal 是 workspace utility，与 prompt execution lease 可独立；如果 terminal 属于同一 session execution surface，则应明确它和 backend execution lease 的关系。

5. **MCP relay backend selection 的事实源边界。**  
   MCP relay impl 仍会通过 registry 查找目标 MCP server 的 backend（`crates/agentdash-api/src/relay/mcp_relay_impl.rs:27`, `crates/agentdash-api/src/relay/mcp_relay_impl.rs:103`, `crates/agentdash-api/src/relay/mcp_relay_impl.rs:148`），而 MCP command payload 已包含 resolved transport。需要核对 backend selection 只使用 capability discovery，执行 transport 是否完全来自 payload，避免 local static MCP catalog 重新成为运行时 transport 事实源。

6. **Backend ensure/claim 是否存在第二套 machine identity/profile 事实。**  
   `agentdash-local` machine identity 是 local library 事实源（`crates/agentdash-local/src/machine_identity.rs:14`），Tauri load/save/start 都会重读并覆盖 machine id（`crates/agentdash-local-tauri/src/main.rs:537`, `crates/agentdash-local-tauri/src/main.rs:560`）。需要核对 dev-joint 或其它启动入口是否也复用同一 library，而不是复制 ensure/claim 或生成 backend id。

### P2

1. **Relay protocol 子模块拆分是否已覆盖所有 payload ownership。**  
   顶层 `RelayMessage` 保留 wire envelope 是合理的；下一轮可只检查新增 command 是否把 payload 放到对应子模块，而不是把业务 payload 写回 `protocol.rs`。

2. **Extension Host output schema 已在 local host 校验，input schema 已在 runtime gateway 校验；后续 review 聚焦 schema owner 文档化。**  
   代码已有 action/channel input schema precheck（`crates/agentdash-application/src/runtime_gateway/extension_actions.rs:170`, `crates/agentdash-application/src/runtime_gateway/extension_actions.rs:649`）和 local output schema 校验（`crates/agentdash-local/src/extensions/host/manager.rs:136`, `crates/agentdash-local/src/extensions/host/manager.rs:168`）。下一轮只需确认 schema 子集一致性，不应重复“schema 未执行”的旧问题。

3. **Desktop API snapshot state 与 LocalRuntime state union 的命名差异。**  
   `LocalRuntimeState` 包含 `stopping`（`packages/core/src/local-runtime/index.ts:1`），`DashboardApiState`/desktop snapshot 不包含（`packages/app-tauri/src/App.tsx:12`）。如只是两个不同状态机，记录边界即可；如同一概念，需统一。

4. **Browse directory setup 能力与 execution tool 能力分离。**  
   Local browse directory 可浏览全盘 setup 目录（`crates/agentdash-local/src/handlers/workspace.rs:134`），而 tool executor 以 session `mount_root_ref`/workspace roots 为执行边界（`crates/agentdash-local/src/tool_executor.rs:98`）。下一轮可补少量 contract check，避免 UI setup 选择器被误解成执行授权。

## 5. 不应重复 review 的内容

以下内容已由 `.trellis/tasks/06-14-module-overdesign-review/overdesign-review.md` 覆盖，本轮后续 review 不应再泛泛重复，只应验证这些方向是否已经按事实源收敛：

- **Local `CommandHandler` 作为全域 command hub 的泛化结论。** 06-14 报告已指出 `CommandHandler` 曾集中承载 prompt/tool/VFS/MCP/extension/terminal，并建议拆成 `LocalCommandRouter + domain handlers`。当前代码已经有 domain handler 文件与顶层 dispatch；下一轮应查“domain handler 是否仍跨域依赖”，而不是重复“handler 太厚”。
- **`RelayMessage` 顶层 enum 作为 wire envelope 合理。** 06-14 明确不建议把顶层 relay envelope 当问题；问题在 local 执行侧 command handler 边界。下一轮不要把 relay enum 大小本身列为问题。
- **`RelayRuntimeToolProvider` 是跨域 composer。** 06-14 已覆盖 VFS runtime tool provider 吸收 workflow/companion/workspace module 的问题。本轮只列 VFS/Local/Relay 边界，不再展开 tool composer 清理。
- **Extension Host 旧版 raw workspace/process/schema 问题。** 06-14 曾指出 raw `workspace_root`、process permission 与 schema 校验问题；当前代码显示 Host API 已拒绝 raw `workspace_root` override，input/output schema 校验也有 owner。下一轮应基于现状 review workspace context provenance 与 schema owner，而不是复述旧问题。
- **Tauri shell 需要保持 thin adapter 的方向。** 06-14 已指出 Tauri `main.rs` profile/claim 过厚。下一轮只需评估哪些 claim/profile 逻辑还必须留在 Tauri command adapter，哪些应下沉到 `agentdash-local`。

## Files Found

- `.trellis/tasks/06-21-module-topology-coupling-review/prd.md` - 当前任务 PRD，仍为占位，实际 scope 以用户 dispatch prompt 为准。
- `.trellis/spec/project-overview.md` - 项目云端/本机双后端模型、数据归属与核心抽象总览。
- `.trellis/spec/cross-layer/desktop-local-runtime.md` - Desktop/Tauri/local runtime/relay/extension host 跨层契约。
- `.trellis/spec/cross-layer/project-backend-workspace-routing.md` - backend access、workspace detect、inventory registration、workspace binding 与 runtime-summary 契约。
- `.trellis/spec/backend/architecture.md` - 后端分层、bootstrap、relay/local crate 职责总览。
- `.trellis/spec/backend/vfs/vfs-materialization.md` - VFS 本机物化路径、key、scope 与错误语义。
- `.trellis/tasks/06-14-module-overdesign-review/overdesign-review.md` - 上一轮模块过度设计综合 review，提供不应重复 review 的旧结论。
- `crates/agentdash-relay/src/protocol.rs` - Relay 顶层 wire envelope。
- `crates/agentdash-relay/src/protocol/*` - Relay 子协议 payload。
- `crates/agentdash-api/src/bootstrap/*` - Cloud API relay/session/VFS/runtime gateway 装配。
- `crates/agentdash-api/src/relay/*` - Cloud relay websocket、registry、MCP/extension transport adapters。
- `crates/agentdash-api/src/routes/backends.rs` - backend runtime list/summary/ensure/browse routes。
- `crates/agentdash-api/src/routes/backend_access.rs` - Project backend access 与 inventory registration routes。
- `crates/agentdash-api/src/routes/workspaces.rs` - Workspace binding/detect routes。
- `crates/agentdash-api/src/routes/extension_runtime.rs` - Extension runtime API route 与 workspace selection。
- `crates/agentdash-application/src/backend_execution_placement.rs` - Backend execution selection and lease placement。
- `crates/agentdash-application/src/relay_connector.rs` - Relay connector prompt/cancel/session route/lease integration。
- `crates/agentdash-application/src/session/launch/*` - Session launch plan/planner/orchestrator。
- `crates/agentdash-application/src/workspace/*` - Workspace detection/resolution/backend sync。
- `crates/agentdash-application/src/runtime_gateway/*` - RuntimeGateway setup/session/extension action providers。
- `crates/agentdash-local/src/runtime.rs` - Local runtime manager。
- `crates/agentdash-local/src/ws_client.rs` - Local backend relay websocket client。
- `crates/agentdash-local/src/handlers/*` - Local command router and domain handlers。
- `crates/agentdash-local/src/extensions/*` - Extension artifact cache and host execution side。
- `crates/agentdash-local-tauri/src/main.rs` - Tauri command host and desktop API/local runtime lifecycle。
- `packages/app-tauri/src/App.tsx` - Desktop DashboardHost。
- `packages/app-tauri/src/runtimeApi.ts` - Tauri invoke adapter for local runtime port。
- `packages/app-web/src/desktop/localRuntimeBridge.ts` - Web app desktop bridge。
- `packages/app-web/src/features/workspace/model/workspaceRouting.ts` - Frontend workspace binding/availability helper。

## Code Patterns

- Relay protocol keeps a single top-level tagged enum while payloads live in submodules (`crates/agentdash-relay/src/protocol.rs:40`; `crates/agentdash-relay/src/protocol/extension_runtime.rs:36`; `crates/agentdash-relay/src/protocol/tool.rs:60`).
- Cloud relay registry combines online backend map, pending command response map, session sink route map and executor snapshot (`crates/agentdash-api/src/relay/registry.rs:55`, `crates/agentdash-api/src/relay/registry.rs:58`, `crates/agentdash-api/src/relay/registry.rs:176`, `crates/agentdash-api/src/relay/registry.rs:230`).
- Backend execution placement is claimed in launch, then projected into `ExecutionContext.session.backend_execution` for relay connector consumption (`crates/agentdash-application/src/session/launch/planner.rs:225`; `crates/agentdash-spi/src/connector/mod.rs:79`; `crates/agentdash-application/src/relay_connector.rs:102`).
- Local command handling follows envelope dispatch to domain handler methods (`crates/agentdash-local/src/handlers/mod.rs:117`, `crates/agentdash-local/src/handlers/mod.rs:130`, `crates/agentdash-local/src/handlers/mod.rs:220`).
- Workspace setup detect/register flows route through RuntimeGateway / backend transport instead of cloud filesystem access (`crates/agentdash-api/src/routes/backend_access.rs:377`; `crates/agentdash-application/src/workspace/detection.rs:70`; `crates/agentdash-local/src/handlers/workspace.rs:30`).
- Desktop shell injects a local runtime port and waits for HTTP health before rendering the dashboard (`packages/app-tauri/src/App.tsx:29`, `packages/app-tauri/src/App.tsx:77`, `packages/app-tauri/src/App.tsx:94`).
- Extension runtime gateway validates input schema before transport, local host validates output schema after Node runner invocation (`crates/agentdash-application/src/runtime_gateway/extension_actions.rs:170`; `crates/agentdash-local/src/extensions/host/manager.rs:136`).
- Host API workspace/process calls reject raw `workspace_root` override and use activation/session workspace context (`crates/agentdash-local/src/extensions/host/host_api.rs:86`, `crates/agentdash-local/src/extensions/host/host_api.rs:112`).

## External References

- No external references used. 本轮为内部架构拓扑 research，事实源来自 `.trellis/spec/`、既有 task review 与仓库代码。
- Relevant internal versions from project specs: Rust + Axum + Tokio + SQLx; React 19 + TypeScript 5.9 + Vite 7 + Tailwind v4; Tauri desktop shell; relay protocol over WebSocket.

## Related Specs

- `.trellis/spec/project-overview.md`
- `.trellis/spec/cross-layer/desktop-local-runtime.md`
- `.trellis/spec/cross-layer/project-backend-workspace-routing.md`
- `.trellis/spec/backend/architecture.md`
- `.trellis/spec/backend/vfs/vfs-materialization.md`

## Caveats / Not Found

- `task.py current --source` returned no active task in this Codex session; user dispatch prompt explicitly supplied `.trellis/tasks/06-21-module-topology-coupling-review` and exact output path, so this file was written there.
- The task PRD is still placeholder text, so scope came from the user dispatch prompt and required spec set rather than PRD content.
- No tests were run; this is read-only architecture research.
- I did not inspect VFS/Extension upper business flows beyond boundary-level references, per user instruction that another subagent owns that scope.
- `scripts/dev-joint.js` was referenced by spec but not found at repo root in this inspection; claim/ensure evidence here is from Tauri and API/local runtime code.
