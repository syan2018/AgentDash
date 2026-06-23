# Research: session-hub-inventory

- Query: 调查 `session/hub` 当前职责、模块边界和外部调用点，输出迁移分类。
- Scope: internal
- Date: 2026-06-23

## Findings

### Files Found

| Path | Description |
| --- | --- |
| `crates/agentdash-application/src/session/hub/mod.rs` | `SessionRuntimeInner` 装配对象；当前仍持有 connector、runtime registry、turn supervisor、stores、frame/anchor/lifecycle repos、VFS/skill/MCP/permission/mailbox 依赖。 |
| `crates/agentdash-application/src/session/hub/factory.rs` | `SessionRuntimeInner` 的 service factory、builder 注入、ready gate。 |
| `crates/agentdash-application/src/session/hub/facade.rs` | hub 内部 helper 与测试入口，包含 eventing/runtime command/notification/transcript/test launch 等转发。 |
| `crates/agentdash-application/src/session/hub/tool_builder.rs` | runtime tool assembly、active tool refresh、MCP discovery、AgentFrame adoption、runtime surface query 混合点。 |
| `crates/agentdash-application/src/session/hub/hook_dispatch.rs` | hook trigger、runtime context update injection、hook auto-resume 调度。 |
| `crates/agentdash-application/src/session/hub/runtime_context_transition.rs` | live/pending runtime context transition frame 构造、通知、pending transition apply-on-turn。 |
| `crates/agentdash-application/src/session/capability_service.rs` | 当前混合 facade：MCP access、capability query、runtime backend query、AgentFrame target resolve、Canvas exposure/adoption、runtime command outbox、pending transition apply。 |
| `crates/agentdash-application/src/session/runtime_builder.rs` | 对外 builder facade，包装 `SessionRuntimeInner` factory 输出与 ready gate。 |
| `crates/agentdash-api/src/app_state.rs` | 将 `SessionCapabilityService` 注入为 `Arc<dyn RuntimeSessionMcpAccess>`，再构建 RuntimeGateway。 |
| `crates/agentdash-api/src/bootstrap/session.rs` | session runtime builder 的生产装配点；向 hub 注入 VFS、runtime tool provider、MCP discovery、backend placement、AgentFrame/anchor/lifecycle/permission/mailbox repos。 |
| `crates/agentdash-api/src/bootstrap/runtime_gateway.rs` | `mcp.list_tools` / `mcp.call_tool` provider 只依赖 `RuntimeSessionMcpAccess`，不应知道 hub。 |
| `crates/agentdash-api/src/session_construction.rs` | 当前手写 `runtime_session_id -> anchor -> current AgentFrame -> VFS`，但 backend anchor 又回头调用 active-turn-only helper。 |
| `crates/agentdash-api/src/routes/canvases.rs` | Canvas runtime invoke/surface/snapshot 调用 RuntimeGateway 与 `resolve_session_frame_vfs`。 |
| `crates/agentdash-api/src/routes/extension_runtime.rs` | extension action/channel 通过 `resolve_session_frame_vfs` 获取 VFS/backend anchor，然后设置 backend target/workspace context。 |
| `crates/agentdash-api/src/routes/vfs_surfaces/resolver.rs` | VFS surface `SessionRuntime` source 通过 `resolve_session_frame_vfs`，AgentRun source 另有独立 lifecycle projector。 |
| `crates/agentdash-api/src/routes/permission_grants.rs` | PermissionGrant apply/revoke 后直接调用 `session_capability.adopt_persisted_agent_frame_revision`。 |
| `crates/agentdash-application/src/workspace_module/tools.rs` | WorkspaceModule tool 通过 session capability facade 查询 effective capability view、Canvas mount expose/adopt，以及测试中读取 active capability state。 |
| `crates/agentdash-application/src/canvas/tools.rs` | Canvas expose tool 通过 session capability facade 写 AgentFrame revision 并 adopt active runtime。 |
| `crates/agentdash-application/src/agent_run/frame/runtime_launch.rs` | `FrameLaunchSurface` 是 typed closed launch surface，并可从 VFS default mount 派生 `RuntimeBackendAnchor`。 |
| `crates/agentdash-application/src/agent_run/frame/construction/mod.rs` | `build_envelope_from_frame` / `close_frame_launch_surface` 已有 closed surface 与 backend anchor 生成路径。 |
| `crates/agentdash-application/src/agent_run/frame/surface.rs` | `AgentFrameSurfaceExt` 统一反序列化 AgentFrame 的 capability/VFS/MCP/execution profile surface。 |

### Related Specs

- `.trellis/spec/backend/session/architecture.md`: Session 语义目标是 `RuntimeSession`，只拥有 turn/tool/event/resume/debug/projection/trace lineage，不拥有 Agent effective surface；AgentFrame 是 capability/context/VFS/MCP 的事实源。
- `.trellis/spec/backend/session/runtime-execution-state.md`: 明确 `SessionRuntimeRegistry`、`TurnSupervisor`、connector live session、active turn、backend execution lease 是不同边界；MCP/工具热更新可从 active turn 快照读取，但 idle/runtime surface 要回到 AgentFrame。
- `.trellis/spec/backend/session/execution-context-frames.md`: `ExecutionContext` 是 connector-facing projection；active turn 可 clone frame 做 tool hot update，但 application facts 不应从 connector projection 反写。
- `.trellis/spec/backend/runtime-gateway.md`: `RuntimeSessionMcpAccess` 是 RuntimeGateway MCP provider 进入 session 的端口；MCP runtime server surface 来源于当前 AgentFrame / active execution surface，具体工具暴露必须经过 capability state 裁决。
- `.trellis/spec/backend/capability/architecture.md`: AgentRun effective capability/admission 是 runtime 能力读取唯一入口；工具 schema 暴露和执行入口必须消费 AgentRun final visible/admission 输出。
- `.trellis/spec/backend/session/session-startup-pipeline.md`: `FrameLaunchSurface` / `FrameLaunchEnvelope` 是 launch-ready facts，VFS/MCP/capability/executor 必须同源闭合；runtime tool assembly 和 hub runtime refresh 共享 `session::tool_assembly`。

### External References

- None. 本调查只依赖仓库内代码与 Trellis specs；未使用外部文档。

### Current Public / `pub(crate)` Surface Inventory

#### `SessionRuntimeInner` factory / builder output

| Method | Visibility | Current callers | Classification |
| --- | --- | --- | --- |
| `core_service` | `pub` at `session/hub/factory.rs:21`; wrapped by `runtime_builder.rs:110` | `bootstrap/session.rs:181`, `runtime_services.rs:41`, `session/launch/deps.rs:82`, tests/workspace fixtures | Keep as composition while hub exists; not business logic. |
| `branching_service` | `pub` at `factory.rs:29`; wrapped `runtime_builder.rs:114` | `bootstrap/session.rs:182`, `runtime_services.rs:42` | Keep as builder output. |
| `eventing_service` | `pub` at `factory.rs:33`; wrapped `runtime_builder.rs:118` | `bootstrap/session.rs:183`, `runtime_services.rs:43`, `launch/deps.rs:81`, tests | Keep as builder output. |
| `runtime_service` | `pub` at `factory.rs:41`; wrapped `runtime_builder.rs:122` | `bootstrap/session.rs:184`, `runtime_services.rs:44` | Keep as builder output. |
| `control_service` | `pub` at `factory.rs:50`; wrapped `runtime_builder.rs:126` | `bootstrap/session.rs:185`, `runtime_services.rs:46`, mailbox deps in `factory.rs:232` | Keep as builder output. |
| `launch_service` | `pub` at `factory.rs:54`; wrapped `runtime_builder.rs:130` | `bootstrap/session.rs:186`, `launch/deps.rs` consumers, hook auto-resume `hook_dispatch.rs:309`, mailbox deps `factory.rs:234` | Keep; launch is live runtime delivery. |
| `hook_service` | `pub` at `factory.rs:58`; wrapped `runtime_builder.rs:134` | `bootstrap/session.rs:187`, `launch/deps.rs:83`, active adoption `tool_builder.rs:267`, tests | Keep service output; hook runtime cache remains hub-adjacent. |
| `effects_service` | `pub` at `factory.rs:62`; wrapped `runtime_builder.rs:142` | `bootstrap/session.rs:189`, `launch/deps.rs:85`, `terminal_effects.rs:542` | Keep service output. |
| `title_service` | `pub` at `factory.rs:74`; wrapped `runtime_builder.rs:146` | `bootstrap/session.rs:190` | Keep service output. |
| `capability_service` | `pub` at `factory.rs:81`; wrapped `runtime_builder.rs:138` | `bootstrap/session.rs:188`, `launch/deps.rs:84`, `bootstrap/session.rs:222` via session tool services | Split: current facade is too broad; keep only capability/runtime-transition app service, move surface query/MCP access/adoption use cases out. |
| `new_with_hooks_and_persistence` | `pub` at `factory.rs:85`; wrapped `runtime_builder.rs:28` | `bootstrap/session.rs:156`, `runtime_services.rs:24` | Keep as constructor. |
| `with_*` injection methods | `pub` at `factory.rs:123-253`; wrapped `runtime_builder.rs:42-180` | `bootstrap/session.rs:159-178`, tests/workspace fixtures | Keep as construction plumbing until dependencies move to new resolver/service constructors. |
| `set_terminal_callback`, `set_hook_effect_handler_registry`, `set_frame_launch_envelope_provider`, `set_context_audit_bus`, `assert_ready_for_app_state` | `pub` at `factory.rs:266-307`; wrapped `runtime_builder.rs:188-213` | `bootstrap/session.rs:207-212`, `app_state.rs:299`, `app_state.rs:359`, `app_state.rs:364`, tests | Keep as late binding/ready gate, but ready gate should eventually validate new runtime surface resolver/MCP access deps outside hub. |
| `current_context_audit_bus` | `pub(crate)` at `factory.rs:299` | `hook_dispatch.rs` sink construction | Keep private/internal; live hook audit plumbing. |

#### `SessionRuntimeInner` live/session helper methods

| Method | Visibility | Current callers | Classification |
| --- | --- | --- | --- |
| `emit_context_frame` | `pub(crate)` at `facade.rs:41` | `launch/commit.rs:56`, `launch/preparation.rs` via context frames, `hook_dispatch.rs:265`, `runtime_context_transition.rs:142/157/245/353`, tests | Keep in session eventing or a small runtime event port; live/runtime notification coordination is valid hub-adjacent. |
| `enqueue_runtime_delivery_command` | `pub(crate)` at `facade.rs:52` | `capability_service.rs:191`, `runtime_context_transition.rs:212`, tests | Move store access out of hub facade to `SessionRuntimeCommandStore`/transition use case; hub should not be generic outbox facade. |
| `has_live_executor_session` | `pub` at `facade.rs:69` | `hook_dispatch.rs:122`; public surface likely redundant with `SessionCoreService::has_live_executor_session` | Keep as live runtime coordination, preferably through `SessionCoreService`. |
| `persist_notification` | `pub(crate)` at `facade.rs:132` | `hook_dispatch.rs:125`, `launch/commit.rs:97/137/145`, `launch/connector_start.rs:74`, `launch/deps.rs:257`, `runtime_control.rs:111/144`, `title_service.rs:59`, `mailbox_delegate.rs:151` | Keep in `SessionEventingService`; avoid routing new code through hub. |
| Test-only `create_session`, `ensure_session`, `build_projected_transcript`, `start_prompt`, `inject_notification` | `#[cfg(test)] pub` / `pub(crate)` at `facade.rs:32-122` | hub tests and fixtures; API relay calls `.inject_notification` on service, not hub | Keep test helper or migrate tests to service facades. |

#### `SessionRuntimeInner` tool/surface/adoption methods

| Method | Visibility | Current callers | Classification |
| --- | --- | --- | --- |
| `get_runtime_mcp_servers` | `pub` at `tool_builder.rs:27` | `SessionCapabilityService::get_runtime_mcp_servers` at `capability_service.rs:39`; no production consumer found beyond wrapper | Migrate to current runtime surface resolver. Current method reads active turn, then idle AgentFrame MCP only (`tool_builder.rs:40-57`). |
| `get_current_capability_state` | `pub` at `tool_builder.rs:61` | wrapper at `capability_service.rs:43`; workspace module tests at `workspace_module/tools.rs:1902` and `2117` | Keep active-turn-only internal helper for tests/live refresh; business/query code should use AgentRun effective capability/surface resolver. |
| `get_latest_capability_state` | `pub` at `tool_builder.rs:74` | wrapper at `capability_service.rs:47`; `SessionRequestAssembler` companion parent facts at `assembler.rs:111-117` | Split. Companion parent runtime snapshot may stay as live/session fact; idle fallback to AgentFrame (`tool_builder.rs:92-108`) belongs in resolver. |
| `get_current_runtime_backend_anchor` | `pub` at `tool_builder.rs:111` | wrapper at `capability_service.rs:51`; `session_construction.rs:73` | Migrate to current runtime surface resolver. Current helper only reads active turn cache (`tool_builder.rs:115-123`) and fails idle, so API/business consumers must not use it. |
| `resolve_runtime_session_frame_id` | `pub(crate)` at `tool_builder.rs:139` | wrapper at `capability_service.rs:66`, then `resolve_runtime_session_target` at `capability_service.rs:80` | Migrate to AgentRun/AgentFrame runtime target resolver. Current implementation already resolves anchor/current frame (`tool_builder.rs:143-175`). |
| `adopt_persisted_agent_frame_revision` | `pub(crate)` at `tool_builder.rs:183` | wrapper at `capability_service.rs:92`; `permission_grants.rs:291`; Canvas expose indirectly; tests | Move behind a surface update/adoption use case. The primitive itself coordinates live connector update, runtime cache and hook notification (`tool_builder.rs:250-328`), so its low-level live portion may remain private in hub. |
| `assemble_tools_for_execution_context` | `pub(crate)` test-facing at `tool_builder.rs:347` | hub tests only | Keep test helper or use `session::tool_assembly` directly. |
| `assemble_tool_surface_for_execution_context` | `pub(crate)` at `tool_builder.rs:357` | active adoption `tool_builder.rs:290`, launch preparation path via `SessionLaunchDeps`, tests | Keep as live/launch tool assembly coordinator if it only consumes a closed `ExecutionContext`; do not let it resolve idle AgentFrame. |
| `discover_runtime_mcp_tool_entries` | `pub(in crate::session)` at `tool_builder.rs:374` | `SessionCapabilityService` RuntimeGateway impl at `capability_service.rs:270` and `295` | Migrate to RuntimeGateway MCP backing access that consumes new current surface resolver. Active turn branch may remain as helper; idle branch currently drops backend anchor/identity (`tool_builder.rs:428-435`) then fails at `tool_builder.rs:437-446`. |

#### `SessionRuntimeInner` hook / transition methods

| Method | Visibility | Current callers | Classification |
| --- | --- | --- | --- |
| `emit_session_hook_trigger` | `pub(in crate::session)` at `hook_dispatch.rs:55` | `SessionHookService::emit_session_hook_trigger` at `hooks_service.rs:295-301`; launch preparation `preparation.rs:217`; terminal effects trait impl `hook_dispatch.rs:330` | Keep in hub/hook service; live hook runtime dispatch and trace persistence are runtime coordination. |
| `collect_runtime_context_update_injections` | `pub(crate)` at `hook_dispatch.rs:159` | runtime transition `runtime_context_transition.rs:124/319/342`, tests | Keep internal; hook injection sink is live runtime/context event coordination. |
| `request_hook_auto_resume` | `pub(in crate::session)` at `hook_dispatch.rs:189`; trait impl at `hook_dispatch.rs:348` | terminal effects `terminal_effects.rs:363`, tests | Keep in hub/hook service; auto-resume scheduling depends on runtime registry reservation and launch service. |
| `schedule_unanchored_hook_auto_resume` | `pub(crate)` at `hook_dispatch.rs:294` | `request_hook_auto_resume` fallback `hook_dispatch.rs:210`, tests | Keep internal; should remain strict launch via provider. |
| `build_initial_capability_state_frame` | `pub(crate)` free function at `runtime_context_transition.rs:73` | launch preparation `preparation.rs:240`, tests | Keep with runtime context transition frame builder or move to a dedicated context-frame module. |
| `emit_adopted_runtime_context_transition` | `pub(crate)` at `runtime_context_transition.rs:97` | active adoption `tool_builder.rs:315` | Keep internal as part of live adoption primitive; not business/API surface. |
| `enqueue_pending_runtime_context_transition` | `pub(crate)` at `runtime_context_transition.rs:178`, currently `#[cfg(test)]` via capability wrapper | tests only through `capability_service.rs:207` | Store/outbox primitive; prefer transition use case or store boundary, not hub facade. |
| `apply_pending_runtime_context_transitions_on_turn` | `pub(crate)` at `runtime_context_transition.rs:261` | `SessionCapabilityService` wrapper at `capability_service.rs:254`; launch preparation `preparation.rs:183-194` | Keep in launch/live runtime preparation or transition service; not general capability facade. |

#### `SessionCapabilityService` current facade

| Method / impl | Current callers | Classification |
| --- | --- | --- |
| `get_runtime_mcp_servers` (`capability_service.rs:39`) | No production caller found beyond wrapper | Remove or move to surface resolver query. |
| `get_current_capability_state` (`capability_service.rs:43`) | Workspace module tests at `workspace_module/tools.rs:1902`, `2117` | Do not expose as business API; test/live helper only. |
| `get_latest_capability_state` (`capability_service.rs:47`) | `CompanionParentFactsProvider` at `assembler.rs:111-117` | Split live companion parent fact vs idle AgentFrame surface query. |
| `get_current_runtime_backend_anchor` (`capability_service.rs:51`) | `session_construction.rs:73` | Move to resolver; current helper is active-turn-only. |
| `resolve_runtime_session_frame_id` / `resolve_runtime_session_target` (`capability_service.rs:66`, `80`) | Canvas exposure/effective view internal to this service | Move to AgentRun/AgentFrame runtime target resolver. |
| `adopt_persisted_agent_frame_revision` (`capability_service.rs:92`) | `permission_grants.rs:291`, Canvas exposure internal | Public wrapper should be removed from API/business paths; expose a higher-level surface update use case. |
| `expose_canvas_mount_revision_and_adopt` (`capability_service.rs:103`) | `canvas/tools.rs:262`, `workspace_module/tools.rs:682` | Move to Canvas/WorkspaceModule surface update use case. It writes AgentFrame revision (`capability_service.rs:136-148`) then adopts active runtime (`capability_service.rs:150-154`). |
| `effective_capability_view_for_runtime_session` (`capability_service.rs:161`) | `workspace_module/tools.rs:90-94` | Move to AgentRun effective capability service/query facade. |
| `list_requested_runtime_commands` / `enqueue_runtime_delivery_command` (`capability_service.rs:179`, `191`) | launch/persistence/tests via stores/wrapper | Move to runtime command store/service; not capability service. |
| `apply_pending_runtime_context_transitions_on_turn` (`capability_service.rs:254`) | `launch/preparation.rs:183-194` | Keep in launch prep deps, but expose through transition service, not mixed capability facade. |
| `impl RuntimeSessionMcpAccess` (`capability_service.rs:265`) | Wired in `app_state.rs:232`, providers in `runtime_gateway.rs:20-22` | Move to new RuntimeGateway MCP access implementation backed by current runtime surface resolver. |

### Code Patterns

#### Active turn live cache vs idle AgentFrame query are currently mixed

- `get_runtime_mcp_servers` first reads active turn `turn.session_frame.mcp_servers` (`tool_builder.rs:28-37`), then falls back to `RuntimeSessionExecutionAnchor -> AgentFrame` and returns `frame.typed_mcp_servers()` (`tool_builder.rs:40-57`).
- `get_latest_capability_state` first reads active turn or `runtime.session_profile` (`tool_builder.rs:75-89`), then falls back to current AgentFrame projection (`tool_builder.rs:92-108`).
- `get_current_runtime_backend_anchor` does not have the same fallback; it only reads active turn `turn.session_frame.runtime_backend_anchor` (`tool_builder.rs:115-123`), causing idle API consumers to miss the backend anchor.

Migration signal: live cache reads may stay in hub, but any idle/current-frame fallback should move to a single current runtime surface resolver.

#### RuntimeGateway provider boundary is already narrow

- `McpListToolsProvider` only parses input, validates session context, calls `RuntimeSessionMcpAccess::list_mcp_tools`, then filters by server names (`runtime_gateway/session_actions.rs:85-145`).
- `McpCallToolProvider` only parses/validates target input, extracts session id, then calls `RuntimeSessionMcpAccess::call_mcp_tool` (`runtime_gateway/session_actions.rs:166-218`).
- API bootstrap passes `Arc<dyn RuntimeSessionMcpAccess>` into both providers (`bootstrap/runtime_gateway.rs:16-22`).

Migration signal: replace the access implementation/wiring, not the RuntimeGateway action model.

#### RuntimeGateway MCP access currently re-enters hub discovery

- `SessionCapabilityService` implements `RuntimeSessionMcpAccess` and delegates both list/call to `hub.discover_runtime_mcp_tool_entries(session_id)` (`capability_service.rs:265-306`).
- `app_state.rs` converts `session_capability.clone()` into `Arc<dyn RuntimeSessionMcpAccess>` (`app_state.rs:232`) and passes it to `build_runtime_gateway` (`app_state.rs:234-239`).

Migration signal: new `RuntimeSessionMcpAccess` should be built from an AgentRun/AgentFrame current runtime surface resolver plus `McpToolDiscovery`, not from `SessionCapabilityService`.

#### MCP discovery idle branch is the direct missing-anchor path

- Active branch captures `(mcp_servers, runtime_backend_anchor, vfs, identity, turn_id, capability_state)` from active turn (`tool_builder.rs:378-400`).
- Idle branch resolves current frame but returns `backend_anchor = None` and `identity = None` while taking `frame.typed_mcp_servers()`, `frame.typed_vfs()`, and projected capability state (`tool_builder.rs:402-435`).
- Immediately after that, discovery requires `backend_anchor` and emits `RuntimeBackendAnchorError::Missing { component: "runtime_mcp_tool_discovery", turn_id }` (`tool_builder.rs:437-446`).

Migration signal: idle MCP discovery must consume a closed surface that includes backend anchor derived from the same VFS/MCP/capability closure.

#### Existing frame launch code already knows how to close a surface

- `FrameLaunchSurface` requires non-optional `capability_state`, `vfs`, `mcp_servers`, and `execution_profile` (`runtime_launch.rs:92-98`).
- `FrameLaunchSurface::new` rejects mismatch between `capability_state.vfs.active` and VFS, and between `capability_state.tool.mcp_servers` and MCP servers (`runtime_launch.rs:131-148`).
- `FrameLaunchSurface::runtime_backend_anchor` derives a backend anchor from the default VFS mount (`runtime_launch.rs:187-200`).
- `build_envelope_from_frame` closes the surface, derives working directory, and computes `runtime_backend_anchor` from `closed_surface.launch_surface.runtime_backend_anchor(...)` (`construction/mod.rs:372-394`).
- `close_frame_launch_surface` replays pending runtime commands into final capability/VFS/MCP closure, then writes it back to the draft (`construction/mod.rs:423-472`).

Migration signal: the new query resolver should reuse or mirror this closed-surface contract. Prefer a query-specific `CurrentRuntimeSurface` over leaking `FrameLaunchEnvelope` launch-only semantics.

#### API routes already contain duplicated surface resolving

- `resolve_session_frame_vfs` resolves session meta, `RuntimeSessionExecutionAnchor`, lifecycle run, project permission, current AgentFrame, then returns `frame.typed_vfs()` (`session_construction.rs:22-76`).
- The same function obtains `runtime_backend_anchor` by calling `session_capability.get_current_runtime_backend_anchor(session_id).await.ok()` (`session_construction.rs:67-74`), which loses idle anchors.
- Canvas runtime invoke correctly constructs `RuntimeActor::UserCanvas` and `RuntimeContext::Session` then calls `runtime_gateway.invoke` (`routes/canvases.rs:384-400`).
- Canvas bridge surface uses `runtime_gateway.surface_for_actor` (`routes/canvases.rs:524-541`), which only reports action visibility.
- Canvas runtime snapshot VFS calls `resolve_session_frame_vfs(...).vfs` (`routes/canvases.rs:577-588`).
- Extension runtime action/channel both call `resolve_session_frame_vfs`, require backend anchor, then set `RuntimeTarget::Backend` or channel workspace context (`routes/extension_runtime.rs:132-154`, `184-216`, `304-318`).
- VFS surface `SessionRuntime` source calls `resolve_session_frame_vfs(...).vfs.unwrap_or_default()` (`routes/vfs_surfaces/resolver.rs:220-225`).

Migration signal: `resolve_session_frame_vfs` should disappear or become a thin adapter over the new resolver; Canvas, Extension and VFS session sources should share one surface closure.

#### Business mutation paths call adoption primitive directly

- Permission grant revoke/apply effect calls `state.services.session_capability.adopt_persisted_agent_frame_revision(...)` directly from API route (`routes/permission_grants.rs:279-303`).
- Canvas tool calls `session_services.capability.expose_canvas_mount_revision_and_adopt(session_id, canvas)` (`canvas/tools.rs:250-268`).
- WorkspaceModule invoke tool calls the same Canvas expose/adopt helper (`workspace_module/tools.rs:670-685`).
- `expose_canvas_mount_revision_and_adopt` resolves current frame, mutates VFS, derives skill baseline, writes new AgentFrame revision, appends visible refs, persists it, then calls `adopt_persisted_agent_frame_revision` (`capability_service.rs:103-159`).

Migration signal: business use cases should own frame-surface writes; the low-level active runtime adoption primitive should be private to a surface update service and only run after the use case has produced a validated target frame.

### Responsibility Classification

#### Keep In Hub: Live Runtime Coordination

These responsibilities are about current process/live turn/connector state and should remain in hub or immediately adjacent session runtime services:

- `runtime_registry` / active turn reads and mutation (`tool_builder.rs:28-37`, `61-68`, `75-89`, `299-311`).
- Connector live tool replacement via `connector.update_session_tools` during adoption/hot refresh (`tool_builder.rs:295-297`).
- Tool assembly for an already closed `ExecutionContext` (`tool_builder.rs:357-371`), because launch preparation and live refresh must use the same assembly helper.
- Hook runtime dispatch, hook trace persistence fallback, injection sink, auto-resume reservation and scheduling (`hook_dispatch.rs:55-188`, `189-313`).
- Runtime context transition event/frame emission and hook runtime capability updates during live adoption or launch turn preparation (`runtime_context_transition.rs:97-176`, `261-356`).
- Turn preparation pending transition application (`launch/preparation.rs:183-194`) if exposed through a focused transition service rather than broad `SessionCapabilityService`.
- Session eventing helpers (`emit_context_frame`, `persist_notification`) as `SessionEventingService` responsibilities, not as new hub public surface.

#### Migrate Out: AgentRun / AgentFrame Runtime Surface Query

These responsibilities answer "what is the current closed runtime surface for this AgentRun/runtime session?" and should move to a new application-layer resolver/facade, likely under `agent_run::frame::runtime_surface` or `agent_run::runtime_surface_query`:

- Idle/current-frame fallback in `get_runtime_mcp_servers` (`tool_builder.rs:40-57`).
- Idle/current-frame fallback in `get_latest_capability_state` (`tool_builder.rs:92-108`).
- `get_current_runtime_backend_anchor` API/business use (`tool_builder.rs:111-128`, `session_construction.rs:67-74`).
- `resolve_runtime_session_frame_id` / `resolve_runtime_session_target` (`tool_builder.rs:139-175`, `capability_service.rs:66-89`).
- `effective_capability_view_for_runtime_session` (`capability_service.rs:161-177`), which should call AgentRun effective capability service using run/agent/frame provenance from the resolver.
- `resolve_session_frame_vfs` (`session_construction.rs:22-76`) as an API helper; replace with resolver output containing frame, VFS, MCP servers, capability state, runtime backend anchor, identity/admission context, project/run/agent/frame provenance.
- SessionRuntime VFS source and Canvas/Extension runtime snapshot VFS (`routes/vfs_surfaces/resolver.rs:220-225`, `routes/canvases.rs:577-588`, `routes/extension_runtime.rs:132-136`, `184-198`).

Suggested resolver shape:

```rust
pub struct CurrentRuntimeSurface {
    pub runtime_session_id: String,
    pub run_id: Uuid,
    pub agent_id: Uuid,
    pub frame_id: Uuid,
    pub frame_revision: i64,
    pub project_id: Uuid,
    pub frame: AgentFrame,
    pub capability_state: CapabilityState,
    pub vfs: Vfs,
    pub mcp_servers: Vec<RuntimeMcpServer>,
    pub runtime_backend_anchor: Option<RuntimeBackendAnchor>,
    pub identity: Option<AuthIdentity>,
    pub resolution_trace: CurrentRuntimeSurfaceTrace,
}
```

For active turn reads, keep transient fields such as current turn id and live identity as optional overlay, but keep authoritative VFS/MCP/capability/backend closure from the same AgentFrame/current surface path unless the caller is specifically live connector refresh.

#### Migrate Out: RuntimeGateway MCP Action Backing Access

These responsibilities back `mcp.list_tools` / `mcp.call_tool` and should move into a new `RuntimeSessionMcpAccess` implementation, not remain in `SessionCapabilityService`:

- `impl RuntimeSessionMcpAccess for SessionCapabilityService` (`capability_service.rs:265-306`).
- `SessionRuntimeInner::discover_runtime_mcp_tool_entries` idle/current-frame logic (`tool_builder.rs:374-468`).
- `app_state.rs` wiring that currently casts `SessionCapabilityService` into `Arc<dyn RuntimeSessionMcpAccess>` (`app_state.rs:232`).

Suggested new owner:

- `agent_run::runtime_surface_query::RuntimeSessionSurfaceResolver` returns `CurrentRuntimeSurface`.
- `runtime_gateway::session_mcp_access::RuntimeSurfaceMcpAccess` or `agent_run::runtime_mcp_access::AgentRunRuntimeMcpAccess` consumes resolver + `McpToolDiscovery`.
- Provider remains unchanged (`runtime_gateway/session_actions.rs`).

Tests:

- `mcp.list_tools` idle Canvas/runtime session action resolves backend anchor from current frame VFS and succeeds.
- `mcp.call_tool` uses same resolver surface and capability policy as `mcp.list_tools`.
- Active turn branch still uses current active turn/ExecutionContext surface where appropriate and does not regress live tool calls.

#### Migrate Out / Privatize: Business Use Case Or Update/Adoption Primitive

These responsibilities mutate AgentFrame surface or project business policy and should not be directly exposed through session hub/capability facade:

- Canvas exposure: move `expose_canvas_mount_revision_and_adopt` (`capability_service.rs:103-159`) to a Canvas/WorkspaceModule surface update use case.
- Permission grant adoption: route `permission_grants.rs:279-303` through a Permission/AgentRun capability use case that writes/validates the effect frame and then invokes a private active runtime adoption port.
- `adopt_persisted_agent_frame_revision` (`tool_builder.rs:183-340`) should become private low-level primitive, e.g. `ActiveRuntimeSurfaceAdopter::adopt_current_frame(target)`, used only after a use case confirms the target is the current AgentFrame revision for the delivery runtime.
- Runtime command outbox helpers `list_requested_runtime_commands` / `enqueue_runtime_delivery_command` (`capability_service.rs:179-204`, `facade.rs:52-66`) should be exposed by store/service boundaries, not capability facade.
- Skill baseline derivation during Canvas/VFS transition (`capability_service.rs:221-251`) belongs with capability dimension/projection pipeline or the surface update use case, not SessionCapabilityService.

### File-Level Migration Matrix

| Old location | Current callers | Suggested new owner | Risk | Tests needed |
| --- | --- | --- | --- | --- |
| `session/hub/tool_builder.rs::get_runtime_mcp_servers` | `SessionCapabilityService::get_runtime_mcp_servers`; no production consumer found | `CurrentRuntimeSurfaceResolver::resolve(...).mcp_servers` for query; active-turn helper private for live refresh | Low if wrapper unused; medium if hidden downstream expects active cache | Unit test resolver idle/current frame MCP list; grep no public wrapper consumers. |
| `session/hub/tool_builder.rs::get_latest_capability_state` | `SessionRequestAssembler` companion parent facts through `SessionCapabilityService` (`assembler.rs:111-117`) | Split: live companion parent snapshot in session service; idle current frame capability in resolver | Medium: companion parent continuation may rely on cached active turn state | Existing companion parent tests; add idle parent frame capability query test. |
| `session/hub/tool_builder.rs::get_current_runtime_backend_anchor` | `session_construction.rs:67-74` | `CurrentRuntimeSurfaceResolver::resolve(...).runtime_backend_anchor` | High: Extension/Canvas/VFS idle surfaces currently miss anchor; wrong fallback can route to wrong backend | Regression: idle Canvas `mcp.list_tools`; extension action/channel backend target from frame default mount; VFS session runtime source includes same VFS/backend closure. |
| `session/hub/tool_builder.rs::resolve_runtime_session_frame_id` | `SessionCapabilityService::resolve_runtime_session_target`; Canvas/effective view internal | `AgentRunRuntimeSurfaceResolver::resolve_target` / `CurrentRuntimeSurface.target` | Medium: target must verify anchor/run/agent/current frame consistency | Unit tests missing anchor, stale frame, agent mismatch, current frame success. |
| `session/hub/tool_builder.rs::discover_runtime_mcp_tool_entries` | RuntimeGateway MCP access via `SessionCapabilityService` | New `RuntimeSessionMcpAccess` implementation backed by resolver + `McpToolDiscovery` | High: must preserve capability filtering and relay call context; current idle branch is broken | RuntimeGateway provider tests with fake resolver/discovery; integration-style Canvas idle list/call; active turn list/call with turn id. |
| `session/hub/tool_builder.rs::assemble_tool_surface_for_execution_context` | active adoption, launch prep/tests | Keep in hub/session launch only, consuming closed `ExecutionContext`; alternatively move to `session::tool_assembly` facade | Medium: launch and live refresh must share exact tool declaration behavior | Existing hub tool tests; launch preparation tests asserting schemas/tools same before prompt and after refresh. |
| `session/hub/tool_builder.rs::adopt_persisted_agent_frame_revision` | Permission grants, Canvas exposure, tests | Private active runtime adoption primitive behind AgentRun/Canvas/Permission surface update use cases | High: active turn update, hook runtime target, connector tool replacement and event frames must remain ordered | Tests: adopt current frame updates connector tools/runtime cache/hook target; rejects stale frame; no active turn path returns typed error; permission/canvas use cases call through new service. |
| `session/capability_service.rs::RuntimeSessionMcpAccess impl` | `app_state.rs:232`, `bootstrap/runtime_gateway.rs:20-22` | `RuntimeSurfaceMcpAccess` in runtime_gateway/agent_run surface layer | High because it fixes bug and changes core action backing | RuntimeGateway MCP list/call tests with active/idle; Canvas runtime invoke route regression. |
| `session/capability_service.rs::effective_capability_view_for_runtime_session` | `workspace_module/tools.rs:90-94` | AgentRun effective capability query facade using resolver target | Medium: WorkspaceModule visibility may change if grants/admission projected differently | WorkspaceModule visibility tests for visible refs/grant effects; permission admission tests. |
| `session/capability_service.rs::expose_canvas_mount_revision_and_adopt` | `canvas/tools.rs:262`, `workspace_module/tools.rs:682` | Canvas/WorkspaceModule runtime surface update use case | High: writes AgentFrame revision, VFS mount binding files, skill baseline, visible refs, then live adoption | Canvas expose tests; workspace module create/update tests; active turn cache/tool refresh assertions. |
| `session/capability_service.rs::list_requested_runtime_commands` / `enqueue_runtime_delivery_command` | launch/persistence/tests via store/service wrappers | `SessionRuntimeCommandStore` or dedicated `RuntimeDeliveryCommandService` | Medium: pending command ordering and requested/applied semantics are critical | Existing runtime command store tests; launch failure marks failed/applied behavior. |
| `session/hub/runtime_context_transition.rs::apply_pending_runtime_context_transitions_on_turn` | `launch/preparation.rs:183-194` through capability facade | `RuntimeContextTransitionService` used by launch preparation | Medium: context frames and hook capability updates must preserve order | Launch preparation tests for pending transitions; context frame delta tests already in module. |
| `session_construction.rs::resolve_session_frame_vfs` | Canvas snapshot, Extension runtime, VFS surfaces | Thin API adapter over `CurrentRuntimeSurfaceResolver`; ideally move to application layer | High: currently duplicates permission/anchor/frame lookup and active-turn anchor query | API tests for session permission, anchor missing, frame missing, backend anchor present idle, VFS same as resolver output. |
| `routes/canvases.rs::invoke_canvas_runtime_action` | Browser Canvas iframe runtime invoke | Keep route; RuntimeGateway access behind it changes | Medium: `surface_for_actor` still only action visibility, not tool readiness | Route test: Canvas actor/session context; idle `mcp.list_tools` succeeds with backend anchor. |
| `routes/extension_runtime.rs` action/channel backend resolution | Extension runtime panel/API | Use resolver output for VFS/backend/workspace context | High: wrong backend target runs extension on wrong local backend | Tests: backend target equals resolver anchor backend; workspace context uses anchor root_ref/default mount; missing anchor returns conflict. |
| `routes/vfs_surfaces/resolver.rs::SessionRuntime` source | VFS browser/session runtime source | Use resolver output; share AgentRun lifecycle projector where needed | Medium: current default-empty behavior may hide missing surface | Tests: session runtime source returns resolver VFS; missing anchor/frame maps explicit API error if desired by design. |
| `routes/permission_grants.rs::adopt_grant_effect` | Permission grant apply/revoke | Permission/AgentRun capability effect use case with private adoption port | High: API route currently orchestrates active adoption directly | Permission grant apply/revoke tests: effect frame current, stale frame rejected, active runtime tools updated. |
| `runtime_builder.rs` / `hub/factory.rs` builder deps | API bootstrap/session tests | Builder should expose new resolver/access services or take separate deps; hub ready gate should shrink as deps move | Medium: boot order may create cycles with RuntimeGateway handle/session services handle | Bootstrap test or `cargo check -p agentdash-api`; ready gate test for resolver deps. |

### Suggested Migration Order

1. Add `CurrentRuntimeSurfaceResolver` under AgentRun/AgentFrame application layer. It should resolve `runtime_session_id -> RuntimeSessionExecutionAnchor -> LifecycleRun/LifecycleAgent/current AgentFrame -> closed typed surface`, including `runtime_backend_anchor` from the same VFS closure.
2. Replace `session_construction::resolve_session_frame_vfs` internals with resolver output, then update Canvas snapshot, Extension runtime and VFS surface session source to consume the shared output.
3. Add new `RuntimeSessionMcpAccess` implementation using resolver + `McpToolDiscovery`; rewire `app_state.rs` / `bootstrap/runtime_gateway.rs` away from `SessionCapabilityService`.
4. Split `SessionCapabilityService`: keep/rename only transition/pending command pieces needed by launch, or replace with focused services. Remove MCP access and current surface query wrappers.
5. Move Canvas exposure and PermissionGrant adoption into use cases; make `adopt_persisted_agent_frame_revision` private low-level live runtime adoption.
6. Delete or privatize hub idle surface fallback helpers after all consumers use resolver.

### Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` returned no active task, so this research used the explicit task path provided by the dispatch prompt instead of a runtime active-task pointer.
- No production caller was found for `SessionCapabilityService::get_runtime_mcp_servers`; verify again after implementation branch changes before deletion.
- Several direct hub public methods are `#[cfg(test)]` helpers in `facade.rs`; they appear in method inventory because they are public in test builds, but they are not production API surface.
- I did not run tests; this was a read-only research task and only wrote this research artifact.
