# Research: import graph crate review

- Query: 从 Cargo graph 与 `agentdash-application` Rust module import 图复核 application crate split draft 的可推行分阶段方案。
- Scope: internal
- Date: 2026-06-25

## Findings

### Files Found

- `.trellis/tasks/06-24-release-crate-boundary-review/research/05-crate-split-coupling-map.md` - 前置 crate split coupling map，给出旧版 import 热点和 batch 建议。
- `.trellis/tasks/06-24-release-crate-split-draft/prd.md` - 当前 split draft 的目标、非目标和 acceptance criteria。
- `.trellis/tasks/06-24-release-crate-split-draft/design.md` - 当前候选 crate 图、依赖方向、waves 与 blocking conditions。
- `.trellis/tasks/06-24-release-crate-split-draft/implement.md` - 未来 checklist 与验证命令草案。
- `Cargo.toml` - workspace members 与 workspace dependency aliases。
- `crates/*/Cargo.toml` - crate-level internal dependency direction。
- `crates/agentdash-application/src/lib.rs` - application crate 根 public facade。
- `crates/agentdash-application/src/session/mod.rs` - RuntimeSession/session facade 与 re-export surface。
- `crates/agentdash-application/src/agent_run/mod.rs` - AgentRun facade、frame/runtime surface/mailbox/workspace exports。
- `crates/agentdash-application/src/lifecycle/mod.rs` - Lifecycle dispatch/orchestration/surface exports。
- `crates/agentdash-application/src/runtime_gateway/mod.rs` - RuntimeGateway provider/action facade。
- `crates/agentdash-application/src/vfs/mod.rs` - VFS provider/service/surface facade。
- `crates/agentdash-application-ports/src/lib.rs` - existing pure port crate entry。
- `crates/agentdash-application-ports/src/runtime_gateway_mcp_surface.rs` - 已新增的 RuntimeGateway MCP current-surface port。
- `.trellis/spec/backend/directory-structure.md` - crate/layer baseline 与 `agentdash-application-ports` 职责。
- `.trellis/spec/backend/architecture.md` - 后端 clean architecture 依赖方向与 current baseline。
- `.trellis/spec/backend/session/architecture.md` - RuntimeSession / AgentFrame / AgentRun surface 语义。
- `.trellis/spec/backend/runtime-gateway.md` - RuntimeGateway admission 与 MCP current surface query boundary。
- `.trellis/spec/backend/vfs/architecture.md` - VFS provider/resource surface 与 runtime tool composition boundary。
- `.trellis/spec/backend/workflow/architecture.md` - LifecycleRun / AgentFrame / RuntimeSessionExecutionAnchor / AgentRun resource surface 语义。

### Existing Cargo Graph

`cargo metadata --no-deps --format-version 1` 的 workspace normal internal dependency 摘要：

- `agentdash-api -> agentdash-agent, agentdash-agent-protocol, agentdash-application, agentdash-application-ports, agentdash-contracts, agentdash-domain, agentdash-executor, agentdash-first-party-integrations, agentdash-infrastructure, agentdash-integration-api, agentdash-mcp, agentdash-relay, agentdash-spi`
- `agentdash-application -> agentdash-agent-protocol, agentdash-agent-types, agentdash-application-ports, agentdash-contracts, agentdash-domain, agentdash-relay, agentdash-spi`
- `agentdash-application-ports -> agentdash-agent-protocol, agentdash-agent-types, agentdash-domain, agentdash-relay, agentdash-spi`
- `agentdash-executor -> agentdash-agent, agentdash-agent-protocol, agentdash-agent-types, agentdash-application-ports, agentdash-domain, agentdash-mcp, agentdash-spi`
- `agentdash-infrastructure -> agentdash-agent-protocol, agentdash-domain, agentdash-spi`
- `agentdash-local -> agentdash-agent-protocol, agentdash-application, agentdash-domain, agentdash-executor, agentdash-infrastructure, agentdash-mcp, agentdash-relay, agentdash-spi`
- `agentdash-mcp -> agentdash-application, agentdash-domain, agentdash-spi`
- `agentdash-spi -> agentdash-agent-protocol, agentdash-agent-types, agentdash-domain`

Cargo-level facts:

- Workspace members include `agentdash-domain`, `agentdash-application-ports`, `agentdash-application`, `agentdash-infrastructure`, `agentdash-spi`, `agentdash-executor`, `agentdash-api`, `agentdash-mcp`, `agentdash-agent-types`, `agentdash-agent`, `agentdash-relay`, `agentdash-local`, `agentdash-local-tauri`, and `agentdash-agent-protocol` at `Cargo.toml:4-21`.
- Workspace dependency aliases define `agentdash-domain`, `agentdash-application-ports`, `agentdash-application`, `agentdash-spi`, `agentdash-executor`, `agentdash-mcp`, `agentdash-agent-types`, `agentdash-agent`, `agentdash-relay`, `agentdash-local`, and `agentdash-agent-protocol` at `Cargo.toml:71-93`.
- `agentdash-application` normal deps are `agentdash-agent-types`, `agentdash-application-ports`, `agentdash-contracts`, `agentdash-domain`, `agentdash-relay`, `agentdash-spi`, and `agentdash-agent-protocol` at `crates/agentdash-application/Cargo.toml:8-15`; `agentdash-agent` / `agentdash-infrastructure` are dev-deps at `crates/agentdash-application/Cargo.toml:46-47`.
- `agentdash-application-ports` depends only on protocol/types/domain/relay/spi plus basic async/error/runtime crates at `crates/agentdash-application-ports/Cargo.toml:8-12`.
- `agentdash-api`, `agentdash-local`, and `agentdash-mcp` still depend on the monolithic `agentdash-application` at `crates/agentdash-api/Cargo.toml:20`, `crates/agentdash-local/Cargo.toml:22`, and `crates/agentdash-mcp/Cargo.toml:8`.

Verdict: 当前 Cargo graph 本身不是 split 的第一阻塞；阻塞来自 `agentdash-application` 内部双向 module graph 和 broad facade。若现在直接新增 application 子 crate，API/local/MCP 仍会把大 application crate 拖入组合图，且被移动的实现会因为现有双向 imports 产生 Cargo cycle。

### Application Module Facade State

Facade counts from `rg -n "^(pub mod|pub\\(crate\\) mod|mod |pub use|pub\\(crate\\) use)"`:

| File | `pub mod` | `pub(crate) mod` | private `mod` | `pub use` | `pub(crate) use` |
| --- | ---: | ---: | ---: | ---: | ---: |
| `crates/agentdash-application/src/lib.rs` | 39 | 1 | 0 | 3 | 0 |
| `crates/agentdash-application/src/session/mod.rs` | 10 | 24 | 20 | 27 | 0 |
| `crates/agentdash-application/src/agent_run/mod.rs` | 7 | 1 | 10 | 17 | 0 |
| `crates/agentdash-application/src/lifecycle/mod.rs` | 9 | 2 | 6 | 14 | 5 |
| `crates/agentdash-application/src/runtime_gateway/mod.rs` | 0 | 0 | 9 | 10 | 1 |
| `crates/agentdash-application/src/vfs/mod.rs` | 22 | 7 | 0 | 24 | 1 |
| `crates/agentdash-application/src/runtime_tools/mod.rs` | 2 | 0 | 0 | 6 | 0 |
| `crates/agentdash-application-ports/src/lib.rs` | 5 | 0 | 0 | 0 | 0 |

Key patterns:

- Application root still exports 39 top-level modules at `crates/agentdash-application/src/lib.rs:1-39`; this keeps every business area available as public cross-crate API.
- `session` is improved versus the previous coupling map: many internals are now `pub(crate)` (`baseline_capabilities`, `bootstrap`, `runtime_transition_service`, `plan`, `runtime_builder`, `runtime_commands`, `runtime_services`, etc.) at `crates/agentdash-application/src/session/mod.rs:3-56`. It still re-exports runtime/service DTOs broadly at `session/mod.rs:60-123`.
- `AgentFrameRuntimeTarget` has moved out of `session::types` into `agent_run::runtime_target` and is re-exported from `agent_run/mod.rs:118`; its definition now lives at `crates/agentdash-application/src/agent_run/runtime_target.rs:9-13`. This removes one old semantic mismatch but not the bidirectional dependency.
- `runtime_gateway` implementation modules are private at `crates/agentdash-application/src/runtime_gateway/mod.rs:1-9`, with a narrow public facade at `runtime_gateway/mod.rs:11-47`.
- `vfs` remains a broad facade: 22 public submodules and 24 public re-exports at `crates/agentdash-application/src/vfs/mod.rs:1-94`.
- `runtime_tools` is a cross-domain aggregator re-exporting companion/lifecycle/task/workspace module providers at `crates/agentdash-application/src/runtime_tools/mod.rs:4-12`.

### Application Internal Import Matrix

Direct `crate::<module>` references inside `crates/agentdash-application/src` for key horizontal modules:

| Source | Target | Count |
| --- | --- | ---: |
| `session` | `agent_run` | 48 |
| `session` | `lifecycle` | 2 |
| `session` | `vfs` | 7 |
| `agent_run` | `session` | 45 |
| `agent_run` | `lifecycle` | 36 |
| `agent_run` | `vfs` | 16 |
| `agent_run` | `canvas` | 3 |
| `agent_run` | `permission` | 1 |
| `agent_run` | `workspace_module` | 1 |
| `lifecycle` | `session` | 4 |
| `lifecycle` | `agent_run` | 8 |
| `lifecycle` | `vfs` | 9 |
| `vfs` | `session` | 5 |
| `vfs` | `lifecycle` | 6 |
| `vfs` | `canvas` | 2 |
| `canvas` | `agent_run` | 1 |
| `canvas` | `runtime_gateway` | 2 |
| `canvas` | `vfs` | 7 |
| `permission` | `agent_run` | 6 |
| `workspace_module` | `session` | 3 |
| `workspace_module` | `agent_run` | 10 |
| `workspace_module` | `runtime_gateway` | 5 |
| `workspace_module` | `vfs` | 10 |
| `workspace_module` | `canvas` | 6 |

The matrix still shows two hard cycles:

- `session <-> agent_run` is the largest bidirectional edge.
- `agent_run <-> lifecycle` remains a second hard cycle.

### Key Blockers In Current Module Graph

1. `SessionRuntimeInner` still depends on AgentRun frame/runtime concepts.
   - `session/hub/mod.rs` imports `AgentRunMailboxRuntimeAdapter` and `SharedFrameLaunchEnvelopeProvider` at `crates/agentdash-application/src/session/hub/mod.rs:19-20`.
   - `SessionRuntimeInner` still stores `vfs_service`, frame launch envelope provider, `AgentFrameRepository`, `RuntimeSessionExecutionAnchorRepository`, `LifecycleAgentRepository`, `PermissionGrantRepository`, and mailbox adapter at `session/hub/mod.rs:53-91`.
   - The hub comment explicitly says `tool / hook / transition / launch / effects` internals should continue moving down to concrete services or dependency packages at `session/hub/mod.rs:1-12`.
   - `SessionRuntimeInner` implements `AgentRunActiveRuntimeSurfaceAdopter` at `crates/agentdash-application/src/session/hub/tool_builder.rs:319-324`, so AgentRun surface update still calls back into session live runtime implementation.

2. AgentRun still consumes Session implementation details.
   - `agent_run/frame/launch_envelope_provider.rs` imports `LaunchCommand`, `RuntimeCommandRecord`, and `RuntimeTraceLaunchState` from session at `crates/agentdash-application/src/agent_run/frame/launch_envelope_provider.rs:13-15`.
   - `agent_run/frame/construction/request_assembler.rs` implements a companion facts provider for `SessionRuntimeTransitionService` at `request_assembler.rs:111`, and calls `session::plan::build_session_plan_fragments` at `request_assembler.rs:574-577`.
   - `agent_run/project_agent_start.rs` holds `SessionCoreService`, `SessionControlService`, `SessionEventingService`, and `SessionLaunchService` dependencies at `crates/agentdash-application/src/agent_run/project_agent_start.rs:36` and `project_agent_start.rs:126-129`.
   - `agent_run/workspace/query.rs` still consumes `SessionCoreService`, `SessionExecutionState`, and `SessionControlService` at `crates/agentdash-application/src/agent_run/workspace/query.rs:24` and `workspace/query.rs:42-50`.

3. AgentRun and Lifecycle still import each other directly.
   - AgentRun consumes `resolve_current_frame_from_delivery_trace_ref` at `crates/agentdash-application/src/agent_run/delivery_runtime_selection.rs:12` and `delivery_runtime_selection.rs:165`.
   - AgentRun runtime surface imports `AgentRunRuntimeAddress` and `AgentRunLifecycleSurfaceProjector` from lifecycle at `crates/agentdash-application/src/agent_run/runtime_surface.rs:21-27`.
   - Lifecycle dispatch imports `AgentFrameBuilder` at `crates/agentdash-application/src/lifecycle/dispatch_service.rs:24` and creates launch anchors with it at `dispatch_service.rs:397`, `dispatch_service.rs:778`, and `dispatch_service.rs:794`.
   - Lifecycle defines `RuntimeSessionCreator` and `SessionPersistenceRuntimeSessionCreator` inside `dispatch_service.rs:43-68`, while API bootstrap imports the concrete creator at `crates/agentdash-api/src/bootstrap/repositories.rs:7` and constructs it at `bootstrap/repositories.rs:56`.

4. RuntimeGateway is partly decoupled, but setup actions are still tied to application helpers.
   - `agentdash-application-ports` now exposes `runtime_gateway_mcp_surface` at `crates/agentdash-application-ports/src/lib.rs:4`.
   - The port contains `RuntimeGatewayMcpSurfaceQueryPort` and DTOs at `crates/agentdash-application-ports/src/runtime_gateway_mcp_surface.rs:6-66`.
   - `runtime_gateway/mcp_access.rs` consumes only the port and MCP discovery at `crates/agentdash-application/src/runtime_gateway/mcp_access.rs:8-44`; tests assert production code does not import session/frame boundaries at `mcp_access.rs:455-510` in the previous map and current source still keeps production imports clean.
   - AgentRun implements the RuntimeGateway MCP port as an adapter at `crates/agentdash-application/src/agent_run/runtime_surface.rs:707-755`.
   - New blocker: `runtime_gateway/setup_actions.rs` still imports `crate::mcp_preset::probe_transport_without_runtime_context` and `crate::workspace::detect_workspace_from_backend` at `crates/agentdash-application/src/runtime_gateway/setup_actions.rs:9-10`. Therefore RuntimeGateway extraction cannot start just because MCP access is port-mediated.

5. VFS is still both generic VFS core and owner-specific provider bundle.
   - `vfs/provider.rs` accepts `SessionPersistence` and `SessionToolResultCache` from session at `crates/agentdash-application/src/vfs/provider.rs:97-98`.
   - `vfs/provider_lifecycle.rs` imports lifecycle execution log/journey types and session persistence/cache at `crates/agentdash-application/src/vfs/provider_lifecycle.rs:28-35`.
   - `agent_run/runtime_surface.rs` uses `PROVIDER_RELAY_FS` from VFS at `runtime_surface.rs:27`, and AgentRun resource surface projection consumes lifecycle projector facts at `runtime_surface.rs:21-27`.
   - API VFS surface resolver still consumes application VFS DTOs and calls `build_surface_summary` directly at `crates/agentdash-api/src/routes/vfs_surfaces/resolver.rs:3-7` and `resolver.rs:244-253`.

6. API route/helper layer still consumes internal application DTOs.
   - API current-surface adapter imports AgentRun concrete query and DTOs at `crates/agentdash-api/src/agent_run_runtime_surface.rs:3-8`.
   - Canvas / extension runtime / terminals routes import `RuntimeSurfaceQueryPurpose` directly at `crates/agentdash-api/src/routes/canvases.rs:7`, `routes/extension_runtime.rs:19`, and `routes/terminals.rs:8-9`.
   - VFS route DTO/helper code directly names `agentdash_application::vfs::ResolvedVfsSurfaceSource`, `ResolvedVfsSurface`, `ResolvedMountSummary`, and `ResolvedMountPurpose` at `crates/agentdash-api/src/routes/vfs_surfaces/dto.rs:16-17`, `dto.rs:67-69`, `dto.rs:119-157`, and `routes/vfs_surfaces/helpers.rs:14-20`.
   - API bootstrap may compose concrete services; route/helper direct imports should become a block condition for physical crate extraction because they freeze internal module paths as cross-crate API.

### Future Crate Graph

Recommended target graph:

```text
agentdash-api / agentdash-local / agentdash-mcp
  -> application facade crates
  -> agentdash-application-ports
  -> agentdash-domain / agentdash-spi / protocol/type crates

agentdash-application-lifecycle
  -> agentdash-application-agentrun facade or agent-frame-materialization port
  -> runtime-session creation/delivery ports
  -> domain/spi

agentdash-application-agentrun
  -> runtime-session delivery/adoption ports
  -> lifecycle read/projection ports or domain workflow repositories
  -> VFS surface contracts
  -> domain/spi/protocol

agentdash-application-runtime-session
  -> runtime-session ports + domain/spi/protocol
  -> no AgentRun/Lifecycle implementation crate

agentdash-application-runtime-gateway
  -> RuntimeGateway ports + backend/setup/action transports
  -> domain/spi/protocol
  -> no AgentRun implementation crate
```

Candidate crate/module ownership:

1. `agentdash-application-ports`
   - Include existing modules: `backend_transport`, `extension_runtime`, `mcp_discovery`, `runtime_gateway_mcp_surface`, `vfs_materialization` at `crates/agentdash-application-ports/src/lib.rs:1-5`.
   - Add pure contracts before implementation moves:
     - `agent_run_surface`: reduced query DTOs/traits for API/RuntimeGateway/current-surface consumers. Do not require callers to import `agent_run::runtime_surface::RuntimeSurfaceQueryPurpose`.
     - `runtime_session_delivery`: `RuntimeSessionCreator`, creation request/result, delivery command/result, accepted runtime session refs.
     - `runtime_surface_adoption`: active runtime adoption trait and target DTOs currently represented by `AgentRunActiveRuntimeSurfaceAdopter` and `AgentFrameRuntimeTarget`.
     - `frame_launch_envelope`: launch-envelope provider trait and input/output DTOs if RuntimeSession crate must consume launch-ready facts without depending on AgentRun implementation.
     - `vfs_surface_runtime`: `VfsSurfaceRuntimeProjection`-like API/local implemented port.
     - `runtime_gateway_setup`: setup action backing ports for MCP probe and workspace detection, replacing direct `runtime_gateway/setup_actions.rs -> mcp_preset/workspace` imports.
   - Exclude application services, `RepositorySet`, `AppState`, `AgentFrameBuilder`, `SessionRuntimeBuilder`, `VfsService` concrete implementation, API DTOs, and repository adapters.

2. `agentdash-application-runtime-session`
   - Include RuntimeSession substrate modules after their public contracts point to ports:
     - `session/core.rs`, `control.rs`, `eventing.rs`, `continuation.rs`, `persistence.rs`.
     - `session/runtime_control.rs`, `runtime_commands.rs`, `runtime_registry.rs`, `runtime_services.rs`, `runtime_builder.rs`.
     - `session/turn_processor.rs`, `turn_supervisor.rs`, `post_turn_handler.rs`, `terminal_effects.rs`, `terminal_cache.rs`, `tool_result_cache.rs`, `stall_detector.rs`.
     - `session/launch/*` and `session/hub/*` only after `SharedFrameLaunchEnvelopeProvider`, `AgentRunMailboxRuntimeAdapter`, and `AgentRunActiveRuntimeSurfaceAdopter` are replaced by ports.
     - session context/projection/lineage/compaction modules that belong to delivery/trace projection, not business owner bootstrap.
   - Exclude AgentRun current surface query/update, `AgentFrameBuilder`, AgentFrame write ownership, Lifecycle orchestration reducer/dispatch implementation, Permission/Canvas/WorkspaceModule business modules, generic VFS providers, RuntimeGateway providers, and API bootstrap.
   - `session::plan::build_session_plan_fragments` should not remain a public cross-crate dependency from AgentRun; move the required planning fragments either into AgentRun frame construction or a lower context/launch contract.

3. `agentdash-application-agentrun`
   - Include:
     - `agent_run/runtime_surface.rs`, `runtime_surface_update.rs`, `runtime_target.rs`.
     - `agent_run/effective_capability.rs`, `runtime_capability.rs`, `runtime_capability_projection.rs`.
     - `agent_run/frame/*` including frame builder, surface service, construction, launch commit, launch envelope contracts if not placed in ports.
     - `agent_run/mailbox.rs`, `mailbox_runtime_adapter.rs`, `message_delivery.rs`, `delivery_runtime_selection.rs`.
     - `agent_run/presentation_read_model.rs`, `workspace/*`, `project_agent_context.rs`, `project_agent_start.rs`, `permission_runtime_surface_update.rs`.
   - Move or port before extraction:
     - `AgentRunLifecycleSurfaceProjector` and `AgentRunRuntimeAddress` currently imported from lifecycle by AgentRun should either move into AgentRun resource-surface ownership or be expressed as a lifecycle projection port.
     - Session service/control dependencies in project-agent start, workspace command policy, mailbox runtime adapter, and frame construction should become runtime-session ports.
   - Exclude RuntimeSession live registry/hub internals, Lifecycle reducer/orchestrator implementation, generic VFS provider/service implementation, Permission service persistence logic, Canvas domain service, and API route DTOs.

4. `agentdash-application-lifecycle`
   - Include:
     - `lifecycle/dispatch_service.rs`, `execution_log.rs`, `gate_service.rs`, `orchestrator.rs`, `projection.rs`, `run_command_service.rs`, `run_view_builder.rs`.
     - `lifecycle/session_association.rs`, `session_run_context_resolver.rs`, `subject_context_assignment.rs`, `subject_execution_control.rs`.
     - lifecycle surface/node projection modules that own LifecycleRun / orchestration evidence.
   - Move or port before extraction:
     - `RuntimeSessionCreator` / `RuntimeSessionCreationRequest` to runtime-session delivery ports.
     - Direct `AgentFrameBuilder` use in lifecycle dispatch to an AgentRun frame materialization facade/port.
     - Current-frame resolver imports by AgentRun/session to a read-model port or AgentRun-owned helper.
   - Exclude session persistence concrete implementation, RuntimeSession storage implementation, API route DTOs, VFS provider implementation, and RuntimeGateway providers.

5. `agentdash-application-runtime-gateway`
   - Include:
     - `runtime_gateway/error.rs`, `extension_actions.rs`, `gateway.rs`, `mcp_access.rs`, `provider.rs`, `session_actions.rs`, `setup_actions.rs`, `tool_adapter.rs`, `types.rs`.
   - Move or port before extraction:
     - `setup_actions.rs` backing calls to MCP preset probe and workspace detection into ports, because current production imports at `setup_actions.rs:9-10` would otherwise pull the monolithic application crate.
   - Exclude AgentRun query implementation, `mcp_preset` implementation, workspace detection implementation, infrastructure probe/relay adapters, API routes/bootstrap, and runtime tool composition.

6. Future `agentdash-application-vfs`
   - Include only generic VFS core after owner-specific dependencies are cut:
     - `vfs/path.rs`, `types.rs`, `provider.rs`, `service.rs`, `surface.rs`, `surface_query.rs`, `materialization.rs`, `mutation_dispatcher.rs`, `apply_patch.rs`, `binding_resolver.rs`, `rewrite.rs`, `search.rs`.
     - generic mount helpers where they do not import Canvas/Lifecycle/Session owner modules.
     - VFS-only runtime tools (`mounts_list`, `fs.*`, `shell.exec`) without workflow/collaboration/workspace module providers.
   - Keep owner-specific providers with their owners or adapter crates until dependency direction is clean:
     - `provider_lifecycle`, `mount_lifecycle` with Lifecycle/AgentRun surface owner.
     - `provider_canvas`, `mount_canvas` with Canvas.
     - `provider_skill_asset`, `provider_routine` with their owner modules if they keep business repository dependencies.
   - Exclude AgentRun resource surface ownership, Lifecycle node state ownership, SessionRuntimeToolComposer cross-domain tool assembly, and business module runtime tool providers.

### Recommended Extraction Waves

#### Wave 0: Current Draft Preconditions Refresh

Status: must precede physical extraction. Some old preconditions are partially complete, but new concrete blockers remain.

- Treat `AgentFrameRuntimeTarget` move to AgentRun as done: it now lives at `agent_run/runtime_target.rs:9-13`.
- Replace draft wording that says “RuntimeGateway can be extracted after MCP surface port” with a stricter requirement: setup action backing dependencies must also be port-mediated because `setup_actions.rs:9-10` imports `mcp_preset` and `workspace`.
- Mark API route/helper direct VFS DTO and AgentRun query imports as extraction blockers, while allowing API bootstrap/composition root to hold concrete service constructors.

Gate:

```powershell
cargo metadata --no-deps --format-version 1
rg -n "use crate::(mcp_preset|workspace)::" crates/agentdash-application/src/runtime_gateway -g '*.rs'
rg -n "agentdash_application::agent_run::RuntimeSurfaceQueryPurpose|agentdash_application::vfs::ResolvedVfsSurfaceSource|agentdash_application::vfs::build_surface_summary" crates/agentdash-api/src -g '*.rs'
```

Block condition:

- The second command returns production imports.
- The third command returns route/helper call sites rather than only composition/bootstrap adapters.

#### Wave 1: Ports First, No Implementation Moves

Move or add pure trait/DTO contracts first. Implementations stay in `agentdash-application`.

Order:

1. Add `runtime_session_delivery` ports for `RuntimeSessionCreator` and creation request/result.
2. Add `runtime_surface_adoption` port for live adoption target/trait currently represented by `AgentFrameRuntimeTarget` and `AgentRunActiveRuntimeSurfaceAdopter`.
3. Add `agent_run_surface` current/resource surface query DTOs for API/RuntimeGateway route consumers. Keep `runtime_gateway_mcp_surface` as the gateway-specific reduced DTO; it already exists.
4. Add `frame_launch_envelope` port if RuntimeSession launch/hub must consume launch-ready frame facts without depending on AgentRun implementation.
5. Add `runtime_gateway_setup` ports for MCP probe and workspace detection backing actions.
6. Add `vfs_surface_runtime` port for API/local runtime projection facts consumed by VFS surface summary.

Why ports first: every current hard cycle has a concrete implementation on both sides. Lower ports let Session/RuntimeGateway/Lifecycle depend on contracts while API/local composition root injects AgentRun/RuntimeSession implementations.

Gate:

```powershell
cargo check -p agentdash-application-ports
cargo check -p agentdash-application
cargo check -p agentdash-api
cargo test -p agentdash-application runtime_gateway::mcp_access
```

Block condition:

- Any new port imports `agentdash-application`, `agentdash-api`, `agentdash-infrastructure`, or concrete application services.
- Port DTOs expose `AgentFrameBuilder`, `SessionRuntimeInner`, `AppState`, route DTOs, or repository concrete implementations.

#### Wave 2: Import Cleanup And Facade Contraction

Still no physical crate moves. Change consumers to depend on ports/facades, then shrink public module exposure.

Order:

1. RuntimeGateway: replace `setup_actions.rs` imports of `mcp_preset`/`workspace` with setup ports.
2. Lifecycle: move `RuntimeSessionCreator` contract out of `lifecycle/dispatch_service.rs`; keep lifecycle dispatch consuming the port.
3. AgentRun: remove direct `crate::session::plan`, `crate::session::runtime_commands`, `crate::session::types`, and concrete session service imports from frame construction/project-agent/workspace/mailbox paths.
4. Session: replace direct AgentRun implementation imports in hub/runtime_builder/hooks_service with ports for launch envelope, mailbox runtime adapter, surface adoption, and effective capability reads.
5. AgentRun/Lifecycle: move or port `AgentRunLifecycleSurfaceProjector`, `AgentRunRuntimeAddress`, and `resolve_current_frame_from_delivery_trace_ref` so AgentRun no longer depends on Lifecycle implementation while Lifecycle can consume AgentRun materialization/update boundaries.
6. API routes/helpers: replace direct `agentdash_application::agent_run::*` and `agentdash_application::vfs::*` route-level DTO imports with application facade DTOs or contracts. Keep concrete constructors in bootstrap/AppState.
7. Reduce `lib.rs`, `session/mod.rs`, `agent_run/mod.rs`, and `vfs/mod.rs` public surfaces to intended facades. The goal is not zero exports; the goal is that exports match future crate APIs.

Gate:

```powershell
cargo check -p agentdash-application
cargo check -p agentdash-api
cargo check -p agentdash-local
cargo check -p agentdash-mcp
rg -n "crate::session::(plan|runtime_commands|types|hub|Session.*Service|LaunchCommand)" crates/agentdash-application/src/agent_run -g '*.rs'
rg -n "crate::agent_run::frame::builder|AgentFrameBuilder" crates/agentdash-application/src/lifecycle -g '*.rs'
rg -n "crate::lifecycle::resolve_current_frame_from_delivery_trace_ref" crates/agentdash-application/src/agent_run crates/agentdash-application/src/session -g '*.rs'
rg -n "use crate::(mcp_preset|workspace)::" crates/agentdash-application/src/runtime_gateway -g '*.rs'
rg -n "agentdash_application::session::(construction|plan|types|hub)|agentdash_application::agent_run::frame|agentdash_application::vfs::ResolvedVfsSurfaceSource|agentdash_application::vfs::build_surface_summary" crates/agentdash-api/src crates/agentdash-local/src crates/agentdash-mcp/src -g '*.rs'
```

Block condition:

- `agent_run -> session` remains a business-surface dependency rather than a runtime-session port dependency.
- `session -> agent_run` remains an implementation dependency instead of launch/adoption/mailbox/effective-capability ports.
- Lifecycle still constructs AgentFrames directly through `AgentFrameBuilder`.
- RuntimeGateway production code imports non-gateway application modules.
- API route/helper code depends on session internals, AgentRun frame internals, or VFS internal DTOs.

#### Wave 3: Extract Low-Risk Implementations

Start with crates whose implementation imports are already port-mediated.

Order:

1. Extract `agentdash-application-runtime-gateway` after both MCP surface and setup actions use ports. RuntimeGateway should depend on `agentdash-application-ports`, domain, spi, agent types/protocol, and basic serde/async crates.
2. Extract `agentdash-application-runtime-session` only after SessionHub launch/adoption/mailbox/effective-capability dependencies are ports. RuntimeSession may consume launch envelope and surface adoption contracts but must not depend on AgentRun/Lifecycle implementation crates.

Gate:

```powershell
cargo metadata --no-deps --format-version 1
cargo check -p agentdash-application-runtime-gateway
cargo test -p agentdash-application-runtime-gateway runtime_gateway
cargo check -p agentdash-application-runtime-session
cargo test -p agentdash-application-runtime-session
cargo check -p agentdash-api -p agentdash-local -p agentdash-mcp
```

Block condition:

- `cargo metadata` shows a new application crate depending on `agentdash-application` just to reach helper implementations.
- RuntimeSession crate imports AgentRun/Lifecycle implementation crates.
- RuntimeGateway crate imports AgentRun implementation, `mcp_preset`, `workspace`, API, infrastructure adapters, or runtime tool composition.

#### Wave 4: Extract Control-Plane Implementations

Order:

1. Extract `agentdash-application-agentrun` after AgentRun no longer imports Session implementation or Lifecycle implementation. It may depend on domain workflow repositories and runtime-session/lifecycle projection ports.
2. Extract `agentdash-application-lifecycle` after Lifecycle consumes AgentRun frame materialization/update through a facade/port and consumes RuntimeSession creation through ports.
3. Keep VFS physical extraction deferred unless owner-specific providers are separated.

Gate:

```powershell
cargo metadata --no-deps --format-version 1
cargo check -p agentdash-application-agentrun
cargo test -p agentdash-application-agentrun agent_run
cargo check -p agentdash-application-lifecycle
cargo test -p agentdash-application-lifecycle lifecycle
cargo check --workspace
```

Block condition:

- `agentdash-application-agentrun` depends on `agentdash-application-lifecycle` while lifecycle depends on AgentRun materialization/update.
- AgentRun still imports Session services or `session::plan`.
- Lifecycle still imports `AgentFrameBuilder` directly instead of a materialization/update boundary.

#### Wave 5: VFS Core Extraction

Only start after AgentRun resource surface and owner-specific providers have clean dependencies.

Order:

1. Split VFS core contracts and generic services.
2. Keep lifecycle/canvas/routine/skill-asset providers with their owner modules or adapter crates until their dependencies are directional.
3. Keep `SessionRuntimeToolComposer` as session/runtime composition, not VFS core.

Gate:

```powershell
cargo check -p agentdash-application-vfs
cargo test -p agentdash-application-vfs vfs
cargo check -p agentdash-application-agentrun -p agentdash-application-lifecycle -p agentdash-api
rg -n "crate::session::|crate::lifecycle::|crate::canvas::" crates/agentdash-application-vfs/src -g '*.rs'
```

Block condition:

- VFS core still imports session persistence/cache, Lifecycle journey/execution log, Canvas domain types, or cross-domain runtime tool providers.

### Draft Rewrite Suggestions

Current `design.md` and `implement.md` are directionally correct but too coarse in five places:

1. Split “Wave 1: Ports” into named port modules.
   - Current draft says “Add AgentRun current/resource surface ports; Add RuntimeSession delivery/adoption ports; Add VFS runtime projection ports; Add gateway-facing MCP/current surface contracts.”
   - Rewrite to name the actual modules/contracts: `agent_run_surface`, `runtime_session_delivery`, `runtime_surface_adoption`, `frame_launch_envelope`, `runtime_gateway_setup`, `vfs_surface_runtime`, plus existing `runtime_gateway_mcp_surface`.

2. RuntimeGateway extraction condition must mention setup actions.
   - Current draft says RuntimeGateway extracts after it consumes only ports.
   - Rewrite with specific blocker: `runtime_gateway/setup_actions.rs` must not import `crate::mcp_preset` or `crate::workspace`; backing probe/detect behavior must be injected through ports.

3. RuntimeSession candidate must list module-level includes/excludes.
   - Current draft says RuntimeSession owns delivery/trace substrate.
   - Rewrite to include current `session/core/control/eventing/persistence/runtime_* / turn_processor / terminal_* / tool_result_cache / launch` only after launch/adoption/mailbox dependencies are ports, and to explicitly exclude AgentRun current surface query/update, Lifecycle orchestration, generic VFS providers, Permission, Canvas, WorkspaceModule, and RuntimeGateway providers.

4. AgentRun/Lifecycle dependency direction must be executable.
   - Current draft says “AgentRun and Lifecycle after interaction is port-mediated.”
   - Rewrite with concrete blockers: `AgentRunLifecycleSurfaceProjector`, `AgentRunRuntimeAddress`, `resolve_current_frame_from_delivery_trace_ref`, `RuntimeSessionCreator`, and `AgentFrameBuilder` direct imports must each have a new owner/port before extraction.

5. API route imports need a separate gate.
   - Current draft mentions “API route layer still chooses anchors/current frames.”
   - Rewrite to also block direct route/helper imports of `RuntimeSurfaceQueryPurpose`, VFS `Resolved*` DTOs, and `build_surface_summary`, while allowing AppState/bootstrap to compose concrete services.

Recommended `implement.md` replacement skeleton:

```markdown
### Wave 1: Ports Only
- [ ] Add `runtime_session_delivery` port for RuntimeSession creation/delivery.
- [ ] Add `runtime_surface_adoption` port for active-runtime adoption target/trait.
- [ ] Add `agent_run_surface` DTO/port for API current/resource surface consumers.
- [ ] Keep existing `runtime_gateway_mcp_surface`; add `runtime_gateway_setup` for MCP probe/workspace detect.
- [ ] Add `frame_launch_envelope` if RuntimeSession launch must not depend on AgentRun implementation.
- [ ] Add `vfs_surface_runtime` projection port.

### Wave 2: Import Cleanup
- [ ] RuntimeGateway setup actions consume ports, not `mcp_preset` / `workspace`.
- [ ] Lifecycle consumes RuntimeSession creation port and AgentRun materialization facade/port.
- [ ] AgentRun no longer imports `session::plan`, session service implementations, or lifecycle implementation helpers.
- [ ] SessionHub/runtime_builder consume launch/adoption/mailbox/effective-capability ports.
- [ ] API routes/helpers consume facade DTOs; bootstrap remains composition root.

### Wave 3: RuntimeGateway / RuntimeSession Extraction
- [ ] Extract RuntimeGateway first if setup + MCP dependencies are port-mediated.
- [ ] Extract RuntimeSession after launch/adoption/mailbox dependencies are ports.

### Wave 4: AgentRun / Lifecycle Extraction
- [ ] Extract AgentRun after lifecycle projection/current-frame/session dependencies are port-mediated.
- [ ] Extract Lifecycle after AgentFrame materialization and RuntimeSession creation are ports.

### Wave 5: VFS Core Extraction
- [ ] Extract only generic VFS core after owner-specific providers no longer import session/lifecycle/canvas.
```

### Related Specs

- `.trellis/spec/backend/directory-structure.md` - `agentdash-application-ports` is the pure port crate for API/local implementations and application-consumed boundaries.
- `.trellis/spec/backend/architecture.md` - clean architecture dependency direction and current crate roles.
- `.trellis/spec/backend/session/architecture.md` - RuntimeSession is delivery/trace substrate; AgentFrame is effective surface fact source; AgentRun frame surface command boundary owns runtime surface updates.
- `.trellis/spec/backend/runtime-gateway.md` - RuntimeGateway MCP provider consumes query-backed current surface port; Gateway provider must not read `SessionHub` or `AgentFrame` directly.
- `.trellis/spec/backend/vfs/architecture.md` - VFS bootstrap owns VFS service/materialization/registry; session bootstrap owns cross-domain runtime tool composer.
- `.trellis/spec/backend/workflow/architecture.md` - LifecycleRun/AgentFrame/RuntimeSessionExecutionAnchor roles and AgentRun resource surface ownership.

### External References

- No web references used.
- Tooling evidence:
  - `cargo metadata --no-deps --format-version 1`
  - `cargo 1.96.0-nightly (f298b8c82 2026-02-24)`
  - `rustc 1.96.0-nightly (69370dc4a 2026-03-05)`

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` returned `Current task: (none)` in this shell. The user supplied `.trellis/tasks/06-24-release-crate-split-draft` explicitly, so this report was written there.
- This review did not modify production source, Cargo manifests, specs, or task planning docs.
- I did not run `cargo check`/tests; listed commands are gates for future implementation waves. The only Cargo command executed was metadata inspection.
- Counts are direct `crate::<module>` reference counts and include test code when it lives under the same module files. They are useful for trend/blocker checks, not as an exact semantic dependency classifier.
