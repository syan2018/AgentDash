# Research: failure placement characterization

- Query: RF01/RF03 当前行为 characterization：backend disconnect 后 running prompt/session feed/AgentRun/runtime-summary 行为，session context 下 MCP target fallback 行为，以及 standalone local backend id 来源。
- Scope: internal
- Date: 2026-06-21

## Findings

### Summary

当前行为存在一个关键不一致：backend WebSocket disconnect 会把 active backend execution lease 标为 `lost`，并让 `/backends/runtime-summary` 不再计入该 active session；但同一次 disconnect 先从 registry 移除该 backend 的 session sink，relay connector 的 stream 只看到 channel close，session ingestion 在没有 error/cancel 的情况下把它解析为 `turn_completed`。因此 running prompt 的 session feed / AgentRun projection 可能显示 completed，而 runtime-summary / lease 层显示 lost/offline。

MCP relay 当前在 session context 下不是强制 session backend。resolver 优先在线 session route；route 缺失或离线后继续 fallback 到 VFS default mount backend、advertised MCP catalog、任意在线 backend。setup/probe 另走 `find_any_online_backend()`，与设计中的 setup/probe fallback 边界一致。

standalone local backend id 当前有两条来源：Tauri/dev-runtime 经 `/api/local-runtime/ensure` 获取 server 生成的 stable local backend id；`agentdash-local` standalone CLI 缺少 `--backend-id` 时会本地随机 UUID。relay handshake 会校验 token 绑定 backend id 与 register payload 一致，因此随机 id 不会绕过 server token authority，但 standalone CLI 仍是 backend id 的本地第二来源。

### Files Found

- `AGENTS.md` - 项目工作约束：中文交流、不改业务代码、小规模避免过度测试、不要 commit。
- `.trellis/workflow.md` - Trellis Phase 1 research 要求：研究产物必须写入 task `research/`。
- `.trellis/tasks/06-21-runtime-failure-placement-convergence/prd.md` - 本 task 的目标、scope、open decisions、acceptance criteria。
- `.trellis/tasks/06-21-runtime-failure-placement-convergence/design.md` - 当前 placement/failure 边界草案，仍需 characterization 后决策。
- `.trellis/tasks/06-21-runtime-failure-placement-convergence/work-items/index.md` - RF01/RF03/RF05 当前 work item 定义。
- `.trellis/tasks/06-21-module-topology-coupling-review/research/14-local-placement-relay-deep-dive.md` - 既有 deep dive，指出 disconnect lost 与 stream projection、MCP fallback、standalone id 风险。
- `.trellis/spec/cross-layer/desktop-local-runtime.md` - desktop/local runtime、relay session sink、lease、MCP transport、backend id authority 约束。
- `.trellis/spec/cross-layer/project-backend-workspace-routing.md` - workspace binding/inventory 与 execution lease/runtime-summary 的分层约束。
- `crates/agentdash-application/src/relay_connector.rs` - prompt placement consumption、session sink registration、stream terminal/release behavior。
- `crates/agentdash-api/src/relay/registry.rs` - backend registry unregister、session route、MCP target resolver。
- `crates/agentdash-api/src/relay/ws_handler.rs` - backend WebSocket register/disconnect cleanup。
- `crates/agentdash-infrastructure/src/persistence/postgres/backend_execution_lease_repository.rs` - `mark_lost_by_backend` SQL 行为。
- `crates/agentdash-api/src/routes/backends.rs` - `/backends/runtime-summary` active lease projection 与 local-runtime ensure route。
- `crates/agentdash-api/src/relay/mcp_relay_impl.rs` - MCP list/call/probe 调用 resolver 与下发 relay command。
- `crates/agentdash-application/src/session/launch/ingestion.rs` - ExecutionStream close/error 到 turn terminal 的解析。
- `crates/agentdash-application/src/session/turn_processor.rs` - terminal notification 持久化与 active turn cleanup。
- `crates/agentdash-application/src/session/hub_support.rs` - `turn_terminal` session meta event 形态。
- `crates/agentdash-application/src/agent_run/workspace/projection.rs` - AgentRun workspace delivery/runtime command projection。
- `packages/app-web/src/features/session/ui/SessionChatViewModel.ts` - 前端识别 `turn_terminal` 终态事件。
- `packages/app-web/src/features/settings/ui/SettingsSystemSections.tsx` - 前端 settings 消费 runtime-summary active session/allocatable。
- `crates/agentdash-local/src/main.rs` - standalone local CLI 的 `--backend-id` 缺省随机 UUID。
- `crates/agentdash-local/src/machine_identity.rs` - 本机 machine identity 持久化来源。
- `crates/agentdash-local-tauri/src/main.rs` - Tauri start/profile normalize/claim local runtime。
- `scripts/dev-runtime.js` - dev runtime ensure 后把 server 返回的 backend id 传给 `agentdash-local`。
- `crates/agentdash-application/src/backend/management.rs` - server ensure 生成 stable local backend id。

### RF01: backend disconnect 当前行为链路

1. Relay prompt 启动时，connector 不再从 VFS 猜执行 backend，而是要求 `ExecutionContext.session.backend_execution` 已存在；缺失时直接 `InvalidConfig`（`crates/agentdash-application/src/relay_connector.rs:102`）。随后取 `backend_id + lease_id`，注册 `RelaySessionRoute { session_id, backend_id, lease_id, tx }`（`crates/agentdash-application/src/relay_connector.rs:107`, `crates/agentdash-application/src/relay_connector.rs:160`, `crates/agentdash-application/src/relay_connector.rs:162`）。
2. `relay_prompt` 成功后 lease activate；prompt send 失败会 `lease_repo.fail(...)` 并 drop sink guard（`crates/agentdash-application/src/relay_connector.rs:173`, `crates/agentdash-application/src/relay_connector.rs:175`, `crates/agentdash-application/src/relay_connector.rs:183`, `crates/agentdash-application/src/relay_connector.rs:184`, `crates/agentdash-application/src/relay_connector.rs:188`）。
3. 正常 terminal event 会经 connector stream 释放 lease：`Failed` 会 yield `Err(ConnectorError::Runtime(...))`，`Completed/Interrupted` 会 release 并结束 stream（`crates/agentdash-application/src/relay_connector.rs:204`, `crates/agentdash-application/src/relay_connector.rs:208`, `crates/agentdash-application/src/relay_connector.rs:217`, `crates/agentdash-application/src/relay_connector.rs:224`, `crates/agentdash-application/src/relay_connector.rs:232`, `crates/agentdash-application/src/relay_connector.rs:236`）。
4. 但 stream 接收端关闭时，connector 只 `sink_guard.take()` 并返回 `None`，没有 failed/lost projection，也没有 release/fail lease（`crates/agentdash-application/src/relay_connector.rs:238`, `crates/agentdash-application/src/relay_connector.rs:239`, `crates/agentdash-application/src/relay_connector.rs:240`）。
5. Backend WebSocket disconnect 时，ws handler 先调用 `BackendRegistry::unregister(&bid)`（`crates/agentdash-api/src/relay/ws_handler.rs:232`）。`unregister` 移除 backend、pending command、以及该 backend 的所有 `session_sinks`（`crates/agentdash-api/src/relay/registry.rs:109`, `crates/agentdash-api/src/relay/registry.rs:115`, `crates/agentdash-api/src/relay/registry.rs:119`）。
6. `unregister` 删除 route 会 drop sink tx；connector stream 读到 `rx.recv().await == None` 后正常结束（`crates/agentdash-application/src/relay_connector.rs:200`, `crates/agentdash-application/src/relay_connector.rs:238`）。session ingestion 在 stream 无 error 结束时调用 `resolve_stream_terminal(..., None)`，若不是 cancel，就返回 `TurnTerminalKind::Completed`（`crates/agentdash-application/src/session/launch/ingestion.rs:74`, `crates/agentdash-application/src/session/launch/ingestion.rs:96`, `crates/agentdash-application/src/session/launch/ingestion.rs:103`, `crates/agentdash-application/src/session/launch/ingestion.rs:114`, `crates/agentdash-application/src/session/launch/ingestion.rs:117`）。
7. Turn processor 把 terminal kind 持久化为 `Platform(SessionMetaUpdate { key: "turn_terminal", value: { terminal_type } })`，然后清理 active turn（`crates/agentdash-application/src/session/turn_processor.rs:124`, `crates/agentdash-application/src/session/turn_processor.rs:132`, `crates/agentdash-application/src/session/turn_processor.rs:141`; `crates/agentdash-application/src/session/hub_support.rs:85`, `crates/agentdash-application/src/session/hub_support.rs:90`, `crates/agentdash-application/src/session/hub_support.rs:91`）。`TurnTerminalKind::Completed` 的 event type 是 `turn_completed`（`crates/agentdash-application/src/session/hub_support.rs:312`, `crates/agentdash-application/src/session/hub_support.rs:314`）。
8. 前端 session chat view 把 `turn_terminal.terminal_type` 中的 `turn_completed/turn_failed/turn_interrupted` 识别为 turn lifecycle event（`packages/app-web/src/features/session/ui/SessionChatViewModel.ts:46`, `packages/app-web/src/features/session/ui/SessionChatViewModel.ts:57`, `packages/app-web/src/features/session/ui/SessionChatViewModel.ts:59`, `packages/app-web/src/features/session/ui/SessionChatViewModel.ts:67`）。
9. AgentRun workspace projection 对 `SessionExecutionState::Completed` 映射 `delivery_status="completed"`，对 `Failed` 映射 `failed`，对 `Interrupted` 映射 `interrupted`（`crates/agentdash-application/src/agent_run/workspace/projection.rs:76`, `crates/agentdash-application/src/agent_run/workspace/projection.rs:80`, `crates/agentdash-application/src/agent_run/workspace/projection.rs:81`, `crates/agentdash-application/src/agent_run/workspace/projection.rs:82`）。现有 projection tests 覆盖 completed/failed 状态（`crates/agentdash-application/src/agent_run/workspace/projection.rs:231`, `crates/agentdash-application/src/agent_run/workspace/projection.rs:250`）。
10. 同一 disconnect handler 在删除 session route 后，调用 `mark_lost_by_backend` 把 active `claimed/running` lease 更新为 `lost`（`crates/agentdash-api/src/relay/ws_handler.rs:233`, `crates/agentdash-api/src/relay/ws_handler.rs:236`; `crates/agentdash-infrastructure/src/persistence/postgres/backend_execution_lease_repository.rs:125`, `crates/agentdash-infrastructure/src/persistence/postgres/backend_execution_lease_repository.rs:131`）。因此 DB lease 状态会是 lost，而 session feed/AgentRun 当前可能走 completed。
11. `/backends/runtime-summary` 只读取 `backend_execution_lease_repo.list_active()`，再用 active lease 数量填 `active_session_count`（`crates/agentdash-api/src/routes/backends.rs:237`, `crates/agentdash-api/src/routes/backends.rs:240`, `crates/agentdash-api/src/routes/backends.rs:273`, `crates/agentdash-api/src/routes/backends.rs:286`）。lost lease 不再是 active，所以 runtime-summary 会减少 active session，且 ws handler 同时 mark runtime health offline（`crates/agentdash-api/src/relay/ws_handler.rs:255`, `crates/agentdash-api/src/relay/ws_handler.rs:258`, `crates/agentdash-api/src/relay/ws_handler.rs:267`）。
12. ws handler 还会把 terminal cache 中该 backend 的 terminal 标为 lost，并尝试通过 `backend_registry.feed_session_event` 推送 `TerminalStateChanged { state: "lost" }`（`crates/agentdash-api/src/relay/ws_handler.rs:269`, `crates/agentdash-api/src/relay/ws_handler.rs:284`, `crates/agentdash-api/src/relay/ws_handler.rs:286`, `crates/agentdash-api/src/relay/ws_handler.rs:294`）。但由于 session_sinks 已在第 5 步被移除，该推送对同 backend 的 active session 当前没有可用 route；这一路径不是 running prompt terminal projection 的可靠来源。

当前 RF01 结论：disconnect 后 lease/runtime-summary 与 feed/AgentRun 的语义冲突。lease/runtime-summary 是 lost/offline；running prompt 的 stream close 链路会被解析成 completed，除非 connector 收到 explicit failed terminal 或用户 cancel。

### RF03: session context 下 MCP target fallback 当前行为链路

1. MCP list tools 和 call tool 都先调用 `resolve_backend_for_relay_mcp(server_name, context.as_ref())` 决定投递 backend，再下发 `CommandMcpListTools` 或 `CommandMcpCallTool`（`crates/agentdash-api/src/relay/mcp_relay_impl.rs:25`, `crates/agentdash-api/src/relay/mcp_relay_impl.rs:27`, `crates/agentdash-api/src/relay/mcp_relay_impl.rs:41`, `crates/agentdash-api/src/relay/mcp_relay_impl.rs:103`, `crates/agentdash-api/src/relay/mcp_relay_impl.rs:112`）。
2. resolver 在有 context 时，优先查 `session_route(context.session_id)`；如果 route backend 在线，直接返回该 backend（`crates/agentdash-api/src/relay/registry.rs:274`, `crates/agentdash-api/src/relay/registry.rs:275`, `crates/agentdash-api/src/relay/registry.rs:276`, `crates/agentdash-api/src/relay/registry.rs:277`）。
3. 如果 session route 指向 backend 已离线，只记录 warn，不失败；继续后续 fallback（`crates/agentdash-api/src/relay/registry.rs:279`, `crates/agentdash-api/src/relay/registry.rs:283`）。
4. resolver 接着尝试 `context.vfs.default_mount().backend_id`；若该 backend 在线，返回它（`crates/agentdash-api/src/relay/registry.rs:287`, `crates/agentdash-api/src/relay/registry.rs:290`, `crates/agentdash-api/src/relay/registry.rs:294`, `crates/agentdash-api/src/relay/registry.rs:295`）。
5. 如果没有可用 VFS backend，继续按 advertised MCP catalog 查找 server name；还没有则返回任意在线 backend（`crates/agentdash-api/src/relay/registry.rs:306`, `crates/agentdash-api/src/relay/registry.rs:310`）。
6. 无 session context 的 MCP setup/probe 使用任意在线 backend，这符合 setup/probe 可以 fallback 的当前边界（`crates/agentdash-api/src/relay/mcp_relay_impl.rs:144`, `crates/agentdash-api/src/relay/mcp_relay_impl.rs:148`, `crates/agentdash-api/src/relay/mcp_relay_impl.rs:151`）。
7. 现有 unit tests 覆盖“session route 不需要 MCP catalog 也优先使用”和“无 context 可 fallback 到 advertised catalog”（`crates/agentdash-api/src/relay/registry.rs:658`, `crates/agentdash-api/src/relay/registry.rs:681`, `crates/agentdash-api/src/relay/registry.rs:684`, `crates/agentdash-api/src/relay/registry.rs:702`）。尚未看到覆盖“有 session context 但 route 离线后 fallback 到 VFS/catalog/any”的现成测试；该行为由代码路径直接推出。

当前 RF03 结论：session context 当前不是 hard binding。只要 session route 不存在或 route backend 离线，MCP list/call 会继续跨到 VFS default mount、catalog 或任意在线 backend。

### Standalone local backend id 来源当前行为

1. `agentdash-local machine-identity` 由 `load_or_create_machine_identity()` 输出本机机器身份；机器身份不存在时生成并持久化 `machine_id = UUID` 和 machine label（`crates/agentdash-local/src/main.rs:54`, `crates/agentdash-local/src/main.rs:55`, `crates/agentdash-local/src/machine_identity.rs:14`, `crates/agentdash-local/src/machine_identity.rs:28`, `crates/agentdash-local/src/machine_identity.rs:29`）。
2. Server ensure path 用 `machine_id + share_scope_kind + share_scope_id + capability_slot` 生成 stable local backend id：`stable_local_backend_id(...) -> local_<sha256-prefix>`（`crates/agentdash-application/src/backend/management.rs:165`, `crates/agentdash-application/src/backend/management.rs:179`, `crates/agentdash-application/src/backend/management.rs:188`, `crates/agentdash-application/src/backend/management.rs:357`, `crates/agentdash-application/src/backend/management.rs:372`）。API response 返回该 `backend_id`、`relay_ws_url`、`auth_token`（`crates/agentdash-api/src/routes/backends.rs:431`, `crates/agentdash-api/src/routes/backends.rs:438`, `crates/agentdash-api/src/routes/backends.rs:461`, `crates/agentdash-api/src/dto/backend.rs:50`, `crates/agentdash-api/src/dto/backend.rs:53`）。
3. Tauri start 会先 normalize request 的 machine identity，再 `claim_local_runtime()` 调用 `/api/local-runtime/ensure`，然后用 response 中的 `claim.backend_id` 创建 `LocalRuntimeConfig`（`crates/agentdash-local-tauri/src/main.rs:414`, `crates/agentdash-local-tauri/src/main.rs:415`, `crates/agentdash-local-tauri/src/main.rs:418`, `crates/agentdash-local-tauri/src/main.rs:428`, `crates/agentdash-local-tauri/src/main.rs:457`, `crates/agentdash-local-tauri/src/main.rs:559`, `crates/agentdash-local-tauri/src/main.rs:560`）。
4. dev runtime 同样先调用 `/api/local-runtime/ensure`，校验 response 有 `backend_id/auth_token/relay_ws_url`，再把 server 返回的 `backend_id` 作为 `--backend-id` 传给 `agentdash-local`（`scripts/dev-runtime.js:135`, `scripts/dev-runtime.js:140`, `scripts/dev-runtime.js:747`, `scripts/dev-runtime.js:749`, `scripts/dev-runtime.js:779`）。
5. standalone `agentdash-local` CLI 当前只要求 `--cloud-url` 和 `--token`；`--backend-id` 缺省时直接 `Uuid::new_v4().to_string()`（`crates/agentdash-local/src/main.rs:60`, `crates/agentdash-local/src/main.rs:63`, `crates/agentdash-local/src/main.rs:65`, `crates/agentdash-local/src/main.rs:67`）。
6. Relay handshake 通过 token 查到 authorized backend，并校验 register payload 的 `backend_id` 必须等于 token 绑定 backend id（`crates/agentdash-api/src/relay/ws_handler.rs:629`, `crates/agentdash-api/src/relay/ws_handler.rs:652`, `crates/agentdash-api/src/relay/ws_handler.rs:657`）。因此 standalone 随机 UUID 如果不匹配 token 绑定 backend，会被拒绝；但从 CLI contract 看，backend id 仍可在本地生成。

当前 RF05 相关结论：正式 desktop/dev 路径已经由 server ensure/claim 授权 backend id；standalone CLI 保留随机 id 的 debug/internal 形态，但这与 spec “backend_id 来自 server ensure/claim response” 不一致，需要设计确认是删除、强制显式传入，还是把 standalone 明确降级为 internal/debug。

## Code Patterns

- Execution placement consumption: launch 阶段产生 `session.backend_execution`，relay connector 只消费该 placement；prompt path 缺 placement 直接失败（`crates/agentdash-application/src/relay_connector.rs:102`）。
- Session route owner: relay connector 注册 route，registry unregister 按 backend 批量删除 route，cancel/steer 再从 route 读 backend（`crates/agentdash-application/src/relay_connector.rs:162`; `crates/agentdash-api/src/relay/registry.rs:119`; `crates/agentdash-application/src/relay_connector.rs:257`）。
- Disconnect split-brain pattern: ws handler 把 lease 标 lost；但 route tx drop 让 stream close 被 session ingestion 当 completed（`crates/agentdash-api/src/relay/ws_handler.rs:236`; `crates/agentdash-application/src/session/launch/ingestion.rs:96`; `crates/agentdash-application/src/session/launch/ingestion.rs:117`）。
- Runtime summary projection: backend settings UI 不从 runtime health 推断 active sessions，而是消费 `/backends/runtime-summary` 的 active lease 投影（`crates/agentdash-api/src/routes/backends.rs:237`; `packages/app-web/src/features/settings/ui/SettingsSystemSections.tsx:373`）。
- MCP target resolver pattern: execution-time MCP payload carries resolved server transport, but backend target is registry-side route/VFS/catalog/any fallback（`crates/agentdash-api/src/relay/mcp_relay_impl.rs:44`; `crates/agentdash-api/src/relay/registry.rs:269`）。
- Local identity pattern: machine identity 是 local library fact；backend id/token/ws 是 server ensure fact in Tauri/dev path；standalone CLI can still local-generate backend id（`crates/agentdash-local/src/machine_identity.rs:14`; `crates/agentdash-application/src/backend/management.rs:165`; `crates/agentdash-local/src/main.rs:65`）。

## Direct Tests vs Design-Confirmed Work

可以直接测试/已可 characterization 的项：

- `BackendRegistry::unregister` 会删除对应 backend 的 session route；现有 test 已覆盖（`crates/agentdash-api/src/relay/registry.rs:626`）。
- MCP session route 优先级与无 context catalog fallback；现有 tests 已覆盖（`crates/agentdash-api/src/relay/registry.rs:658`, `crates/agentdash-api/src/relay/registry.rs:684`）。
- `mark_lost_by_backend` 只更新 active leases；现有 repository test 已覆盖但本轮未运行（`crates/agentdash-infrastructure/src/persistence/postgres/backend_execution_lease_repository.rs:482`）。
- 可以新增/补充 characterization tests：stream channel close -> `turn_completed`；session context route 离线后 fallback 到 VFS/catalog/any；disconnect ordering 导致 terminal lost event feed 无 route；standalone CLI 缺 `--backend-id` 生成随机 UUID 且 handshake mismatch 被拒绝。

需要设计确认后再实现/改测试期望的项：

- backend disconnect 对 running prompt 的产品语义：新增 `lost` terminal，还是映射 `turn_failed` / `turn_interrupted`，以及 feed 文案、mailbox pause、AgentRun delivery_status 如何表达。
- disconnect cleanup 顺序与投递机制：是否在删除 session sink 前投 lost terminal，或引入非 relay-sink 的 server-side terminal event persistence。
- runtime-summary 是否只隐藏 lost lease，还是要短期保留 lost session diagnostic。
- session context 下 MCP route 缺失/离线时是否必须失败，还是允许某些明确 fallback。
- setup/probe 与 execution MCP call 的 resolver 是否拆分成不同 API/类型。
- standalone `agentdash-local` 是否正式支持；若支持，是否也必须走 ensure/claim，或要求 `--backend-id` 显式且 token 绑定一致。

## External References

- None. 本轮未联网，事实源来自 task artifacts、Trellis specs、既有 research 和仓库代码。

## Commands Run

- `python ./.trellis/scripts/task.py current --source` -> `Current task: (none)`, `Source: none`。本轮依据用户显式给出的 task/output path 继续写入。
- `mkdir -p .trellis/tasks/06-21-runtime-failure-placement-convergence/research` -> 成功，目录已存在/创建。
- `cargo test -p agentdash-api relay_mcp_backend_resolution --lib` -> 通过；2 tests passed，0 failed；首次编译耗时 4m43s。
- `cargo test -p agentdash-api unregister_drops_session_routes_for_that_backend_only --lib` -> 通过；1 test passed，0 failed；耗时 1.28s。
- 多条 `Get-Content` / `rg` / PowerShell line-range read 命令用于读取上述文件和定位行号，无写入业务代码。

## Related Specs

- `.trellis/spec/cross-layer/desktop-local-runtime.md`
- `.trellis/spec/cross-layer/project-backend-workspace-routing.md`
- `.trellis/spec/cross-layer/frontend-backend-contracts.md`
- `.trellis/spec/backend/session/runtime-execution-state.md`
- `.trellis/spec/frontend/state-management.md`

## Caveats / Not Found

- `task.py current --source` 未解析到 active task；本文件按用户明确指定的 `.trellis/tasks/06-21-runtime-failure-placement-convergence/` 写入。
- 未修改业务代码，未执行 git 操作，未新增测试文件。
- 未运行真实 `pnpm dev` / browser E2E disconnect 场景；RF01 的 completed-vs-lost 结论来自代码链路和 unit-level behavior 推导。
- 未找到现成测试覆盖“session context 下 route 离线后 fallback 到 VFS/catalog/any”或“backend disconnect 后 session feed/AgentRun 最终显示 completed”这两个端到端组合。
