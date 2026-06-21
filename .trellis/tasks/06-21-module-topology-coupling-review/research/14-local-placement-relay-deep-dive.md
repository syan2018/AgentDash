# Research: local-placement-relay deep dive

- Query: 深挖 backend execution placement、lease cleanup、workspace routing、extension backend target、Relay/local/desktop boundary 的事实源与耦合，产出可拆后续任务候选。
- Scope: internal
- Date: 2026-06-21

## 结论摘要

1. **prompt execution placement 的权威事实已经收敛到 session launch claim。** `SessionLaunchPlanner` 在 launch 阶段把 explicit / workspace binding / auto idle intent 解析为 `ExecutionPlacementPlan`，创建 `BackendExecutionLease::claimed` 后把 `backend_id + lease_id + selection_mode` 投影进 `ExecutionContext.session.backend_execution`（`crates/agentdash-application/src/session/launch/planner.rs:292`, `crates/agentdash-application/src/session/launch/planner.rs:307`, `crates/agentdash-application/src/session/launch/planner.rs:310`, `crates/agentdash-application/src/session/launch/plan.rs:263`, `crates/agentdash-application/src/session/launch/plan.rs:302`）。`RelayAgentConnector` 发送 prompt 时强制要求该 placement 存在，不再从 VFS 猜测执行 backend（`crates/agentdash-application/src/relay_connector.rs:102`）。

2. **launch 仍允许从 VFS default mount 生成 workspace_binding selection intent。** 当 prompt input 没有显式 backend selection 且 relay executor 可用时，planner 会调用 `selection_request_from_vfs_hint`，优先取 `vfs.default_mount().backend_id`，否则 auto idle（`crates/agentdash-application/src/session/launch/planner.rs:298`, `crates/agentdash-application/src/session/launch/planner.rs:385`, `crates/agentdash-application/src/session/launch/planner.rs:391`, `crates/agentdash-application/src/session/launch/planner.rs:417`）。这属于 launch placement 的输入推导，不是 connector 执行期 fallback；但 spec 需要明确它是唯一允许的 VFS -> execution intent 转换点。

3. **lease terminal cleanup 不是单一函数 owner，但已有按场景分工。** connector 负责 prompt send failure、stream terminal、cancel 和 guard drop 后的 route cleanup（`crates/agentdash-application/src/relay_connector.rs:173`, `crates/agentdash-application/src/relay_connector.rs:183`, `crates/agentdash-application/src/relay_connector.rs:204`, `crates/agentdash-application/src/relay_connector.rs:232`, `crates/agentdash-application/src/relay_connector.rs:249`, `crates/agentdash-application/src/relay_connector.rs:264`, `crates/agentdash-application/src/relay_connector.rs:334`）；ws disconnect handler 负责 registry unregister、mark active leases lost、runtime health offline、terminal cache lost（`crates/agentdash-api/src/relay/ws_handler.rs:232`, `crates/agentdash-api/src/relay/ws_handler.rs:233`, `crates/agentdash-api/src/relay/ws_handler.rs:255`, `crates/agentdash-api/src/relay/ws_handler.rs:269`）。这回答了“是否有 terminal cleanup owner”：目前是**状态维度分散 owner**，不是统一 terminal cleanup service。

4. **registry disconnect cleanup 已覆盖 active lease lost，但 session sink 被直接 retain 删除，connector stream 的接收端可能只看到 channel close。** `BackendRegistry::unregister` 移除 backend、pending 和 session_sinks（`crates/agentdash-api/src/relay/registry.rs:109`, `crates/agentdash-api/src/relay/registry.rs:115`, `crates/agentdash-api/src/relay/registry.rs:119`）；ws handler 随后 `mark_lost_by_backend` 只更新 `claimed/running` leases（`crates/agentdash-infrastructure/src/persistence/postgres/backend_execution_lease_repository.rs:118`, `crates/agentdash-infrastructure/src/persistence/postgres/backend_execution_lease_repository.rs:131`）。需要后续验证 stream close 是否会投影为用户可见 terminal/failure，而不是只让 active lease 正确消失。

5. **MCP transport fact 已收敛，但 backend target 仍有多级 fallback。** MCP relay command payload 携带完整 `McpServerRelay { name, transport }`（`crates/agentdash-relay/src/protocol/mcp.rs:60`, `crates/agentdash-relay/src/protocol/mcp.rs:67`, `crates/agentdash-relay/src/protocol/mcp.rs:72`），local manager 用 payload transport 连接并按 protect mode 校验（`crates/agentdash-local/src/handlers/mcp_relay.rs:140`, `crates/agentdash-local/src/mcp_client_manager.rs:147`, `crates/agentdash-local/src/mcp_client_manager.rs:161`）。但 cloud 投递 backend 由 `resolve_backend_for_relay_mcp` 按 session route -> VFS default mount -> advertised catalog -> any online backend 选择（`crates/agentdash-api/src/relay/registry.rs:263`, `crates/agentdash-api/src/relay/registry.rs:274`, `crates/agentdash-api/src/relay/registry.rs:287`, `crates/agentdash-api/src/relay/registry.rs:306`, `crates/agentdash-api/src/relay/registry.rs:310`）。

6. **extension backend target 入口不一致。** HTTP/panel API contract 要求 frontend request 提供 `backend_id`，route 只校验 Project backend access 并从 session VFS 选择 workspace（`packages/app-web/src/generated/extension-runtime-contracts.ts:50`, `crates/agentdash-api/src/routes/extension_runtime.rs:133`, `crates/agentdash-api/src/routes/extension_runtime.rs:139`, `crates/agentdash-api/src/routes/extension_runtime.rs:140`）。Workspace module tool 入口则优先使用 `session.backend_execution.backend_id`，没有 placement 时才回退 VFS default mount（`crates/agentdash-application/src/workspace_module/mod.rs:40`, `crates/agentdash-application/src/workspace_module/mod.rs:51`, `crates/agentdash-application/src/workspace_module/runtime_tool_provider.rs:175`）。这会让同一 extension action/channel 在 panel 与 agent/tool 两条入口使用不同 target resolver。

7. **VFS file tools 与 terminal 明确按 mount backend 投递，不走 execution lease。** `relay_fs` provider 对 file read/write/search 等直接使用 `mount.backend_id + mount.root_ref`（`crates/agentdash-api/src/mount_providers/relay_fs.rs:70`, `crates/agentdash-api/src/mount_providers/relay_fs.rs:85`, `crates/agentdash-api/src/mount_providers/relay_fs.rs:136`）；terminal launch 也从 session default relay mount 读取 backend/root（`crates/agentdash-api/src/routes/terminals.rs:290`, `crates/agentdash-api/src/routes/terminals.rs:300`, `crates/agentdash-api/src/routes/terminals.rs:314`）。这不是 prompt execution placement 的重复事实源，但需要明确为 workspace utility target。

8. **Desktop/Tauri/dev script 的 machine identity 基本收敛到 `agentdash-local`，但 standalone local binary 仍有非 claim backend id 生成路径。** `agentdash-local` 负责生成/持久化 machine identity（`crates/agentdash-local/src/machine_identity.rs:14`, `crates/agentdash-local/src/machine_identity.rs:28`）；Tauri profile load/save/start 都覆盖 canonical machine id（`crates/agentdash-local-tauri/src/main.rs:141`, `crates/agentdash-local-tauri/src/main.rs:153`, `crates/agentdash-local-tauri/src/main.rs:536`, `crates/agentdash-local-tauri/src/main.rs:559`）；dev script 通过 `agentdash-local machine-identity` 读取同一事实（`scripts/dev-runtime.js:748`, `scripts/dev-runtime.js:793`）。但 `agentdash-local` standalone CLI 在缺少 `--backend-id` 时会生成随机 UUID（`crates/agentdash-local/src/main.rs:65`），与 desktop spec “backend_id 来自 server ensure/claim response” 只在 Tauri/dev-runtime claim 路径上完全一致。

## 主链路拓扑

### A. Prompt execution placement / lease / route

1. Launch planner 接收 prompt input 与 typed VFS，先解析 prompt payload、executor config、runtime context，再解析 backend execution placement（`crates/agentdash-application/src/session/launch/planner.rs:225`）。
2. Backend selection request 来源有三类：frontend/command 显式 selection、VFS hint 生成 workspace_binding、auto idle。解析器只验证在线 executor 与 active lease count，不直接读取 workspace inventory（`crates/agentdash-application/src/backend_execution_placement.rs:101`, `crates/agentdash-application/src/backend_execution_placement.rs:141`, `crates/agentdash-application/src/backend_execution_placement.rs:174`, `crates/agentdash-application/src/backend_execution_placement.rs:196`）。
3. Planner 创建 `BackendExecutionLease::claimed`，将 default mount root_ref 记录为 lease metadata，然后写 repository（`crates/agentdash-application/src/session/launch/planner.rs:310`, `crates/agentdash-application/src/session/launch/planner.rs:318`, `crates/agentdash-application/src/session/launch/planner.rs:323`）。
4. Launch plan 把已 claim 的 placement 投影到 `ExecutionContext.session.backend_execution`；缺少 lease id 会 panic/失败，说明 plan builder 视 lease 为必要前置（`crates/agentdash-application/src/session/launch/plan.rs:302`, `crates/agentdash-application/src/session/launch/plan.rs:307`）。
5. Relay connector 发送 prompt 前读取 `context.session.backend_execution`，注册 `RelaySessionRoute { session_id, backend_id, lease_id, tx }`，再向该 backend 下发 prompt（`crates/agentdash-application/src/relay_connector.rs:102`, `crates/agentdash-application/src/relay_connector.rs:160`, `crates/agentdash-application/src/relay_connector.rs:173`）。
6. Prompt send 成功后 lease activate；prompt send 失败 lease fail；terminal completed/failed/interrupted 或 cancel release；sink guard drop 只 unregister route（`crates/agentdash-application/src/relay_connector.rs:175`, `crates/agentdash-application/src/relay_connector.rs:184`, `crates/agentdash-application/src/relay_connector.rs:208`, `crates/agentdash-application/src/relay_connector.rs:232`, `crates/agentdash-application/src/relay_connector.rs:265`, `crates/agentdash-application/src/relay_connector.rs:334`）。

### B. Backend disconnect cleanup

1. WebSocket loop 退出后，ws handler 调用 registry unregister；registry 删除 online backend、pending response 与 session sinks（`crates/agentdash-api/src/relay/ws_handler.rs:232`, `crates/agentdash-api/src/relay/registry.rs:109`, `crates/agentdash-api/src/relay/registry.rs:115`, `crates/agentdash-api/src/relay/registry.rs:119`）。
2. 同一 disconnect handler 调用 `mark_lost_by_backend`，把该 backend 下 `claimed/running` lease 置为 `lost`，并写 release reason / released_at（`crates/agentdash-api/src/relay/ws_handler.rs:233`, `crates/agentdash-infrastructure/src/persistence/postgres/backend_execution_lease_repository.rs:125`, `crates/agentdash-infrastructure/src/persistence/postgres/backend_execution_lease_repository.rs:131`）。
3. Runtime health offline 与 terminal cache lost 在 ws handler 内继续执行（`crates/agentdash-api/src/relay/ws_handler.rs:255`, `crates/agentdash-api/src/relay/ws_handler.rs:269`）。
4. `/backends/runtime-summary` 只读取 active leases 聚合 active session count 与 allocatable，lost/released/failed 都不会继续构成 active session（`crates/agentdash-domain/src/backend/entity.rs:223`, `crates/agentdash-api/src/routes/backends.rs:237`, `crates/agentdash-api/src/routes/backends.rs:273`, `crates/agentdash-api/src/routes/backends.rs:278`）。

### C. MCP relay target vs transport

1. AgentFrame/session capability surface 提供 `RuntimeMcpServer`，cloud relay provider 对每个 server 先解析投递 backend（`crates/agentdash-api/src/relay/mcp_relay_impl.rs:25`, `crates/agentdash-api/src/relay/mcp_relay_impl.rs:27`, `crates/agentdash-api/src/relay/mcp_relay_impl.rs:103`）。
2. Backend selection 优先 session route，其次 VFS default mount，再按 online backend advertised MCP catalog，最后任意在线 backend（`crates/agentdash-api/src/relay/registry.rs:274`, `crates/agentdash-api/src/relay/registry.rs:287`, `crates/agentdash-api/src/relay/registry.rs:306`, `crates/agentdash-api/src/relay/registry.rs:310`）。
3. Relay payload 本身携带完整 resolved server transport，local handler 把 payload server 交给 local MCP manager；local manager 对 stdio cwd/env、HTTP/SSE headers 使用 payload transport，而不是本机静态 catalog 重建（`crates/agentdash-api/src/relay/mcp_relay_impl.rs:41`, `crates/agentdash-api/src/relay/mcp_relay_impl.rs:112`, `crates/agentdash-local/src/handlers/mcp_relay.rs:140`, `crates/agentdash-local/src/mcp_client_manager.rs:163`, `crates/agentdash-local/src/mcp_client_manager.rs:183`）。

### D. Extension action/channel target

1. HTTP/panel route 要求 request 带 `session_id + backend_id + action/channel key`，route 校验 Project access、解析 session VFS workspace，然后构造 RuntimeGateway request target（`packages/app-web/src/generated/extension-runtime-contracts.ts:50`, `crates/agentdash-api/src/routes/extension_runtime.rs:127`, `crates/agentdash-api/src/routes/extension_runtime.rs:133`, `crates/agentdash-api/src/routes/extension_runtime.rs:139`, `crates/agentdash-api/src/routes/extension_runtime.rs:159`）。
2. RuntimeGateway extension provider 校验 installation package artifact、permission、input schema，再通过 transport 向 request target backend 下发 relay command（`crates/agentdash-application/src/runtime_gateway/extension_actions.rs:160`, `crates/agentdash-application/src/runtime_gateway/extension_actions.rs:169`, `crates/agentdash-application/src/runtime_gateway/extension_actions.rs:170`, `crates/agentdash-application/src/runtime_gateway/extension_actions.rs:178`, `crates/agentdash-application/src/runtime_gateway/extension_actions.rs:195`）。
3. Relay extension payload 不含 backend_id 字段；backend 选择发生在 transport 调用参数，payload 只带 Project/session/action/artifact/workspace/trace（`crates/agentdash-api/src/relay/extension_runtime_impl.rs:24`, `crates/agentdash-api/src/relay/extension_runtime_impl.rs:88`, `crates/agentdash-relay/src/protocol/extension_runtime.rs:36`）。
4. Frontend webview bridge 用 runtime surface default mount 选择 backend，fallback 到 workspace backend，并把 backend_id 放进 invoke request（`packages/app-web/src/features/extension-runtime/model/webviewBridge.ts:97`, `packages/app-web/src/features/extension-runtime/model/webviewBridge.ts:112`, `packages/app-web/src/features/extension-runtime/model/webviewBridge.ts:208`）。
5. Workspace module tool 入口的 resolver 与 HTTP route 不同：它从 ExecutionContext 中解析 backend，优先 `session.backend_execution`，再 fallback VFS default mount，并把结果写 RuntimeGateway target 或 channel invoke request（`crates/agentdash-application/src/workspace_module/mod.rs:54`, `crates/agentdash-application/src/workspace_module/tools.rs:748`, `crates/agentdash-application/src/workspace_module/tools.rs:779`）。

### E. Workspace routing / utility commands

1. Workspace binding / inventory 表达目录事实，不表达 execution occupancy；spec 明确执行空闲/忙碌由 active backend execution leases 投影（`.trellis/spec/cross-layer/project-backend-workspace-routing.md:44`, `.trellis/spec/cross-layer/project-backend-workspace-routing.md:47`）。
2. Frontend Workspace routing helper 当前按 bindings、authorized backend、backend online 计算 workspace availability/resolution summary，只能解释目录 binding 可用性（`packages/app-web/src/features/workspace/model/workspaceRouting.ts:198`, `packages/app-web/src/features/workspace/model/workspaceRouting.ts:216`, `packages/app-web/src/features/workspace/model/workspaceRouting.ts:235`）。
3. Settings backend section 已从 `/backends/runtime-summary` 展示 active session count 与 allocatable（`packages/app-web/src/stores/coordinatorStore.ts:35`, `packages/app-web/src/features/settings/ui/SettingsSystemSections.tsx:235`, `packages/app-web/src/features/settings/ui/SettingsSystemSections.tsx:373`, `packages/app-web/src/features/settings/ui/SettingsSystemSections.tsx:519`）。
4. VFS file operations 与 terminal launch 都按 mount backend/root 投递，是 workspace utility target，不是 session execution placement（`crates/agentdash-api/src/mount_providers/relay_fs.rs:85`, `crates/agentdash-api/src/routes/terminals.rs:290`）。

### F. Desktop / LocalRuntime identity / claim

1. `agentdash-local` 持久化 machine identity，并提供 `machine-identity` CLI 输出（`crates/agentdash-local/src/machine_identity.rs:14`, `crates/agentdash-local/src/main.rs:54`）。
2. Tauri profile load/save/start 调用 `load_or_create_machine_identity()` 覆盖 request/profile 的 canonical `machine_id`，profile 只持久化 server/profile/workspace roots/backend claim snapshot/start 偏好（`crates/agentdash-local-tauri/src/main.rs:141`, `crates/agentdash-local-tauri/src/main.rs:153`, `crates/agentdash-local-tauri/src/main.rs:536`, `crates/agentdash-local-tauri/src/main.rs:559`）。
3. Tauri start 通过 server ensure/claim 获得 `backend_id + relay_ws_url + auth_token` 后创建 `LocalRuntimeConfig`（`crates/agentdash-local-tauri/src/main.rs:414`, `crates/agentdash-local-tauri/src/main.rs:428`, `crates/agentdash-api/src/routes/backends.rs:431`, `crates/agentdash-api/src/routes/backends.rs:461`）。
4. Dev runtime script 也调用 `/api/local-runtime/ensure`，machine identity 通过 local binary 读取（`scripts/dev-runtime.js:747`, `scripts/dev-runtime.js:749`, `scripts/dev-runtime.js:793`）。
5. Standalone local binary 仍允许 `--backend-id` 缺省时随机生成 backend id，适合作为边界例外记录或收敛候选（`crates/agentdash-local/src/main.rs:65`）。

## 耦合矩阵

| Coupling | From | To | Relationship | Evidence | Risk |
| --- | --- | --- | --- | --- | --- |
| VFS hint -> launch placement intent | Session launch planner | Backend execution placement | VFS default mount backend 可在 launch 期生成 workspace_binding intent，随后 claim lease；这是允许的输入推导，但必须唯一化。 | `planner.rs:298`, `planner.rs:385`, `planner.rs:417` | P1 |
| Prompt execution backend | Launch plan | RelayAgentConnector | Connector 只消费已 claim `backend_execution`，不再自行 resolve backend。 | `plan.rs:263`, `relay_connector.rs:102` | P0 if regressed |
| Session route + lease cleanup | RelayAgentConnector | BackendRegistry + lease repo | connector 管 release/fail/cancel，registry/ws handler 管 disconnect lost；owner 分散但状态维度清晰。 | `relay_connector.rs:208`, `ws_handler.rs:233`, `registry.rs:119` | P1 |
| Disconnect route removal without terminal event | BackendRegistry unregister | Connector stream / UI terminal state | registry 直接 retain 删除 session sinks；lease 会 lost，但 connector stream 可能只看到 channel close。 | `registry.rs:119`, `ws_handler.rs:233` | P1 |
| MCP backend target fallback | BackendRegistry | MCP relay provider | MCP transport 来自 payload，但投递 backend 仍可从 route/VFS/catalog/any online 解析。 | `registry.rs:263`, `mcp_relay_impl.rs:43` | P1 |
| Extension HTTP backend target | Frontend webview/canvas bridge | API route / RuntimeGateway | frontend request 直接携带 backend_id，server 校验 access 与 workspace 后按该 backend 执行。 | `extension-runtime-contracts.ts:50`, `webviewBridge.ts:97`, `extension_runtime.rs:133` | P0 |
| Extension workspace module backend target | ExecutionContext | Workspace module tool / RuntimeGateway | agent/tool 入口优先 session backend_execution，再 fallback VFS default mount。 | `workspace_module/mod.rs:40`, `runtime_tool_provider.rs:175` | P0 |
| VFS utility target | VFS mount | relay_fs provider / local tool handler | file operations 按 mount backend/root 投递，不占用 backend execution lease。 | `relay_fs.rs:70`, `relay_fs.rs:85` | P2 |
| Terminal utility target | Session default mount | terminal route / local terminal handler | terminal spawn 按 VFS default relay mount backend/root 投递，不走 execution lease。 | `terminals.rs:290`, `terminals.rs:300` | P1 |
| Runtime summary projection | Lease repo + registry health | Frontend settings | summary 合并 online/executors/active leases，前端 settings 已消费。 | `backends.rs:237`, `SettingsSystemSections.tsx:373` | P2 |
| Machine identity | agentdash-local | Tauri/dev-runtime | Tauri/dev script 复用 local library/CLI identity；server claim 返回 backend_id。 | `machine_identity.rs:14`, `main.rs:536`, `dev-runtime.js:793` | P2 |
| Standalone local backend id | agentdash-local CLI | Relay registration | 非 claim 路径可随机 backend_id，和 desktop spec 的 claim response authority 不同。 | `agentdash-local/src/main.rs:65` | P1 |

## P0 Backlog Candidates

### P0-1: 统一 extension invocation backend target resolver

- 问题：HTTP/panel extension runtime API 让 frontend request 直接提供 `backend_id`，workspace module tool 入口却优先用 `session.backend_execution`。同一 action/channel 的 target resolver 不一致，容易让 webview/panel 与 agent/tool 指向不同 backend。
- 影响范围：`packages/app-web/src/features/extension-runtime/model/webviewBridge.ts`、`canvasBridge.ts`、`crates/agentdash-api/src/routes/extension_runtime.rs`、`crates/agentdash-application/src/workspace_module/mod.rs`、`runtime_gateway/extension_actions.rs`。
- 建议 owner：Extension Runtime / RuntimeGateway + Frontend bridge。
- 验收方向：后端提供唯一 server-side resolver：优先级与 workspace module 保持一致，frontend 只传 command intent 或 workspace/tab context；route tests 覆盖 panel 与 workspace module 对同一 session 得到同一 backend/workspace。

### P0-2: 明确 relay command target taxonomy，并把 execution placement 与 workspace utility target 写成 contract

- 问题：prompt/cancel 使用 lease placement；MCP backend target、extension target、terminal、VFS file tool 仍按 session route / VFS / frontend request / mount backend 各自解析。部分是合理的 utility target，部分会影响 execution consistency。
- 影响范围：relay prompt、MCP relay、extension runtime、terminal routes、VFS relay_fs provider、cross-layer specs。
- 建议 owner：Relay/Application boundary。
- 验收方向：形成一张命令分类表：`execution-placement-bound`、`session-route-bound`、`mount-utility-bound`、`setup-bound`；对应调用点不再混用 fallback；新增 tests 至少覆盖 prompt 不可从 VFS 执行期 fallback、MCP target fallback 只在无 session route 场景发生。

## P1 Backlog Candidates

### P1-1: 收口 backend disconnect 的 session terminal projection

- 问题：disconnect 时 registry 删除 session sinks，lease mark lost，terminal cache mark lost；但 connector stream 可能只看到 channel close，用户 feed/AgentRun 状态是否收到明确 failed/lost terminal 事件需要验证。
- 影响范围：`BackendRegistry::unregister`、`relay/ws_handler.rs`、`RelayAgentConnector` stream handling、session event ingestion。
- 建议 owner：Session Runtime / Relay。
- 验收方向：backend disconnect 对 running prompt 产生明确 terminal/lost projection；active lease 从 runtime-summary 消失；session route 被清理；测试覆盖 disconnect 后 stream/UI 状态。

### P1-2: 限定 MCP backend target fallback

- 问题：MCP resolved transport 已随 payload 下发，但 backend target fallback 最后可落到 advertised catalog 或任意在线 backend。若 runtime-resolved MCP server 和 local protect mode/config 不匹配，fallback 可能把调用送到非 session 承载 backend。
- 影响范围：`BackendRegistry::resolve_backend_for_relay_mcp`、`McpRelayProvider`、MCP setup probe。
- 建议 owner：MCP Relay。
- 验收方向：session context 下必须优先并最好强制 session route/backend_execution；无 session context 的 setup/probe 才允许 catalog/any online fallback；测试覆盖 same server name 不同 transport 不跨 backend 复用。

### P1-3: 收敛 standalone local backend id 来源

- 问题：desktop/Tauri/dev runtime 都从 server ensure/claim 获得 backend_id，但 standalone `agentdash-local` 缺少 `--backend-id` 时生成随机 UUID。若该 CLI 仍是支持入口，会形成 backend identity 的第二事实源。
- 影响范围：`crates/agentdash-local/src/main.rs`、dev scripts、desktop-local-runtime spec。
- 建议 owner：Local Runtime。
- 验收方向：standalone CLI 要么明确降级为 internal/debug path 并要求显式 backend_id/token，要么也先走 ensure/claim；文档与 CLI validation 一致。

### P1-4: 明确 terminal 与 execution lease 的关系

- 问题：terminal launch 从 session default mount backend/root 投递，不占用 execution lease；如果 terminal 属于 workspace utility，这合理。如果产品把 terminal 视为 session execution surface，则它应该与 route/lease/active session projection 有关系。
- 影响范围：`routes/terminals.rs`、terminal cache、runtime-summary、WorkspacePanel terminal tab。
- 建议 owner：Terminal / WorkspacePanel。
- 验收方向：spec 标明 terminal 是 mount utility 还是 execution surface；如果是 utility，UI 不展示为 backend active session；如果是 execution surface，建立 lease/terminal lifecycle 投影。

### P1-5: 把 VFS hint -> workspace_binding selection intent 明确为 launch-only rule

- 问题：当前 planner 可以从 VFS default mount 推导 workspace_binding placement intent，这是 launch 前解析。需要防止其它 prompt/cancel/tool 入口在执行期重复推导 execution backend。
- 影响范围：`session/launch/planner.rs`、`workspace_resolution.rs`、relay connector tests/spec。
- 建议 owner：Session Launch。
- 验收方向：spec 写明 VFS 只能在 launch planner 生成 selection intent；connector 和 cancel 只能消费 claimed placement/route；新增 regression test 覆盖缺 placement 的 relay connector prompt 失败。

## P2 Backlog Candidates

### P2-1: 前端 workspace routing 文案区分 binding availability 与 execution allocatable

- 问题：`workspaceRouting.ts` 从 bindings/access/online 计算 workspace availability，这适合目录可用性；settings 已用 runtime-summary 展示 allocatable。需要避免 UI 文案把 online binding 说成可分配执行 backend。
- 影响范围：Workspace list/create/detail UI、Settings system section。
- 建议 owner：Frontend Workspace。
- 验收方向：Workspace routing helper 只输出 binding/readiness 语义；任何“执行占用/可分配”文案只从 runtime-summary 来。

### P2-2: 保持 extension relay payload 不携带 backend_id

- 问题：extension relay payload 当前不含 backend_id，target 由 transport 参数决定。这是好边界；后续 target resolver 改造时不要把 backend_id 再写进 payload。
- 影响范围：`agentdash-relay/src/protocol/extension_runtime.rs`、`relay/extension_runtime_impl.rs`。
- 建议 owner：Relay Protocol。
- 验收方向：protocol serialization tests 保持 payload 只含 action/session/artifact/workspace/trace；backend target 是 transport/routing 层事实。

### P2-3: Profile UI 不把 machine_id 输入当 authority

- 问题：LocalRuntimeView 会显示并构造 `machine_id`，但 Tauri save/start 会用 local library identity 覆盖。当前行为符合 spec，但 UI 容易让人误以为 machine_id 可编辑生效。
- 影响范围：`packages/views/src/local-runtime/LocalRuntimeView.tsx`、`packages/app-web/src/desktop/localRuntimeBridge.ts`、Tauri profile commands。
- 建议 owner：Desktop UI。
- 验收方向：UI 表达 machine identity 为只读 runtime fact，profile save/start 后 machine_id 与 local library 一致；不需要兼容旧 profile。

## 不重复项

- 不重复“RelayMessage 顶层 enum 过大”。既有 review 已判定 wire envelope 集中是合理边界；本轮只关注 target/placement 事实源。
- 不重复“Local CommandHandler 过厚”的泛化结论。当前已有 domain handlers；本轮只关注 prompt/MCP/extension/terminal/file command 的 target provenance。
- 不重复“Extension Host schema/permission 未执行”的旧问题。RuntimeGateway input schema/permission 与 local host output schema/permission guard 已有实现；本轮问题是 backend target resolver 不一致。
- 不重复“前端大组件拆分”。本轮只把 webview/canvas bridge 作为 backend target 来源证据，不评价组件大小。
- 不重复“workspace binding 等于 execution allocation”的误解。spec 已明确 binding/inventory 是目录事实，active lease/runtime-summary 是执行忙闲事实；后续只检查 UI 文案与调用点是否遵守。

## Files Found

- `.trellis/tasks/06-21-module-topology-coupling-review/prd.md` - 当前 review 编排 PRD，约束不改代码、输出可拆 backlog。
- `.trellis/tasks/06-21-module-topology-coupling-review/design.md` - review 分轮、slice、schema 与 subagent 协调规则。
- `.trellis/tasks/06-21-module-topology-coupling-review/research/05-local-relay-desktop-topology.md` - 第一轮 Local/Relay/Desktop 拓扑与深挖问题来源。
- `.trellis/tasks/06-21-module-topology-coupling-review/research/04-capability-permission-extension-vfs-topology.md` - Extension/VFS/RuntimeGateway 边界来源。
- `.trellis/tasks/06-21-module-topology-coupling-review/research/06-frontend-contracts-topology.md` - Frontend/generated contract/webview bridge 拓扑来源。
- `.trellis/spec/cross-layer/desktop-local-runtime.md` - Desktop/local runtime/relay prompt/lease/MCP/extension host contract。
- `.trellis/spec/cross-layer/project-backend-workspace-routing.md` - Backend inventory、workspace binding 与 runtime-summary contract。
- `crates/agentdash-application/src/backend_execution_placement.rs` - backend selection intent -> execution placement resolver。
- `crates/agentdash-application/src/session/launch/planner.rs` - launch-time placement claim owner。
- `crates/agentdash-application/src/session/launch/plan.rs` - placement -> `ExecutionContext.session.backend_execution` projector。
- `crates/agentdash-application/src/session/launch/orchestrator.rs` - launch preparation/start failure -> lease fail cleanup。
- `crates/agentdash-application/src/relay_connector.rs` - prompt/cancel/session route/lease runtime integration。
- `crates/agentdash-api/src/relay/registry.rs` - online backend registry、session route、MCP target resolver、disconnect route cleanup。
- `crates/agentdash-api/src/relay/ws_handler.rs` - backend websocket disconnect cleanup owner。
- `crates/agentdash-infrastructure/src/persistence/postgres/backend_execution_lease_repository.rs` - lease state transitions and lost cleanup SQL。
- `crates/agentdash-api/src/routes/backends.rs` - `/backends/runtime-summary` active lease projection。
- `crates/agentdash-api/src/relay/mcp_relay_impl.rs` - MCP relay provider backend target + payload transport bridge。
- `crates/agentdash-relay/src/protocol/mcp.rs` - MCP relay payload shape。
- `crates/agentdash-local/src/handlers/mcp_relay.rs` - local MCP relay command handler。
- `crates/agentdash-local/src/mcp_client_manager.rs` - resolved MCP transport connection and protect mode policy。
- `crates/agentdash-api/src/routes/extension_runtime.rs` - panel extension runtime HTTP target handling。
- `crates/agentdash-application/src/workspace_module/mod.rs` - workspace module invocation backend resolver。
- `crates/agentdash-application/src/workspace_module/tools.rs` - workspace module action/channel invocation target use。
- `crates/agentdash-application/src/runtime_gateway/extension_actions.rs` - RuntimeGateway extension provider admission and relay transport call。
- `packages/app-web/src/features/extension-runtime/model/webviewBridge.ts` - frontend extension webview backend target selector。
- `packages/app-web/src/features/extension-runtime/model/canvasBridge.ts` - Canvas extension channel frontend backend selector。
- `crates/agentdash-api/src/routes/terminals.rs` - terminal target from session default mount。
- `crates/agentdash-api/src/mount_providers/relay_fs.rs` - VFS file operations target from mount backend/root。
- `crates/agentdash-local/src/machine_identity.rs` - local machine identity authority。
- `crates/agentdash-local-tauri/src/main.rs` - Tauri profile normalization and local runtime claim。
- `scripts/dev-runtime.js` - dev runtime ensure/claim and machine identity retrieval。

## Code Patterns

- Launch-time placement pattern: infer/parse intent -> validate executor availability -> claim lease -> project into `ExecutionContext.session.backend_execution` -> connector consumes only placement.
- Lease cleanup pattern: connector handles prompt lifecycle terminal states; ws handler handles backend disconnect lost state; runtime-summary reads only active lease states.
- MCP split pattern: backend target resolver is registry-side; transport fact is payload-side and local manager applies payload transport.
- Extension target split pattern: HTTP/panel target comes from frontend request; workspace module target comes from ExecutionContext resolver; RuntimeGateway provider assumes target already resolved.
- Workspace utility target pattern: VFS file operations and terminal use mount backend/root and do not participate in backend execution lease occupancy.
- Desktop identity pattern: machine identity is local library fact; profile/request are normalized against it; backend id/token/relay ws are server claim response facts.

## External References

- None. 本轮为内部架构 deep-dive，未联网，事实源来自 Trellis specs、既有 research 和仓库代码。

## Related Specs

- `.trellis/spec/cross-layer/desktop-local-runtime.md`
- `.trellis/spec/cross-layer/project-backend-workspace-routing.md`
- `.trellis/spec/backend/session/runtime-execution-state.md`
- `.trellis/spec/frontend/type-safety.md`
- `.trellis/spec/frontend/state-management.md`

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` 返回 `Current task: (none)`；本文件依据用户显式给出的 task path 与输出路径写入。
- 未修改业务代码，未运行测试，未执行 git 操作。
- 本轮未全面审查所有 frontend UI 文案；只抽查 workspace routing、settings runtime summary 与 extension bridge target。
- 未证明 backend disconnect 后 UI feed 的实际最终表现；这里只确认 lease lost 与 terminal cache lost 的代码路径存在，并把 connector stream projection 作为 P1 follow-up。
