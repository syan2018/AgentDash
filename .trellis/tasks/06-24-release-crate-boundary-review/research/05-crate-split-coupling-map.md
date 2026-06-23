# Research: crate split coupling map

- Query: Cargo/module coupling and crate split candidates for release boundary review.
- Scope: mixed internal code, Trellis specs, Cargo metadata.
- Date: 2026-06-24

## Findings

### Files Found

- `Cargo.toml` - workspace membership and internal workspace dependency aliases.
- `crates/*/Cargo.toml` - crate-level normal/dev dependency direction.
- `crates/agentdash-application/src/lib.rs` - application crate root facade.
- `crates/agentdash-application/src/session/mod.rs` - current RuntimeSession facade and broad re-export surface.
- `crates/agentdash-application/src/agent_run/mod.rs` - AgentRun facade, runtime surface, frame, mailbox and workspace exports.
- `crates/agentdash-application/src/lifecycle/mod.rs` - Lifecycle facade, dispatch/orchestration/surface exports.
- `crates/agentdash-application/src/runtime_gateway/mod.rs` - RuntimeGateway provider/action facade.
- `crates/agentdash-application/src/vfs/mod.rs` - VFS provider/service/surface facade.
- `crates/agentdash-application/src/runtime_tools/mod.rs` - runtime tool composer facade that re-exports cross-domain providers.
- `crates/agentdash-application-ports/src/lib.rs` - existing pure port crate entry.
- `crates/agentdash-domain/src/workflow/runtime_session_anchor.rs` - RuntimeSession to control-plane anchor model.
- `crates/agentdash-spi/src/session_persistence.rs` - RuntimeSession persistence DTO/port surface.
- `crates/agentdash-api/src/app_state.rs` and `crates/agentdash-api/src/bootstrap/*.rs` - current composition root and service wiring.
- `crates/agentdash-api/src/session_construction.rs`, `routes/vfs_surfaces/resolver.rs`, `routes/canvases.rs`, `routes/extension_runtime.rs`, `routes/terminals.rs`, `routes/lifecycle_agents.rs` - API consumers of current surface/session/runtime data.

### Cargo Workspace Map

`cargo metadata --no-deps --format-version 1` normal internal dependency summary:

- `agentdash-api -> agentdash-agent, agentdash-agent-protocol, agentdash-application, agentdash-application-ports, agentdash-contracts, agentdash-domain, agentdash-executor, agentdash-first-party-integrations, agentdash-infrastructure, agentdash-integration-api, agentdash-mcp, agentdash-relay, agentdash-spi`
- `agentdash-application -> agentdash-agent-protocol, agentdash-agent-types, agentdash-application-ports, agentdash-contracts, agentdash-domain, agentdash-relay, agentdash-spi`
- `agentdash-application-ports -> agentdash-agent-protocol, agentdash-agent-types, agentdash-domain, agentdash-relay, agentdash-spi`
- `agentdash-executor -> agentdash-agent, agentdash-agent-protocol, agentdash-agent-types, agentdash-application-ports, agentdash-domain, agentdash-mcp, agentdash-spi`
- `agentdash-infrastructure -> agentdash-agent-protocol, agentdash-domain, agentdash-spi`
- `agentdash-local -> agentdash-agent-protocol, agentdash-application, agentdash-domain, agentdash-executor, agentdash-infrastructure, agentdash-mcp, agentdash-relay, agentdash-spi`
- `agentdash-mcp -> agentdash-application, agentdash-domain, agentdash-spi`
- `agentdash-spi -> agentdash-agent-protocol, agentdash-agent-types, agentdash-domain`

Workspace evidence:

- Workspace members include `agentdash-domain`, `agentdash-application-ports`, `agentdash-application`, `agentdash-infrastructure`, `agentdash-spi`, `agentdash-executor`, `agentdash-contracts`, `agentdash-api`, `agentdash-mcp`, `agentdash-agent-types`, `agentdash-relay`, and `agentdash-agent-protocol` at `Cargo.toml:3-21`.
- Workspace aliases define `agentdash-application-ports`, `agentdash-application`, `agentdash-domain`, `agentdash-spi`, `agentdash-executor`, `agentdash-mcp`, `agentdash-agent-types`, `agentdash-relay`, `agentdash-contracts`, and `agentdash-agent-protocol` at `Cargo.toml:71-93`.
- `agentdash-application` has normal deps on `agentdash-agent-types`, `agentdash-application-ports`, `agentdash-contracts`, `agentdash-domain`, `agentdash-relay`, `agentdash-spi`, and `agentdash-agent-protocol` at `crates/agentdash-application/Cargo.toml:7-15`.
- `agentdash-application` only has `agentdash-agent` and `agentdash-infrastructure` as dev-deps at `crates/agentdash-application/Cargo.toml:42-47`.
- `agentdash-application-ports` currently exposes only `backend_transport`, `extension_runtime`, `mcp_discovery`, and `vfs_materialization` modules at `crates/agentdash-application-ports/src/lib.rs:1-4`; it has no AgentRun/Lifecycle/runtime-session port yet.
- `agentdash-api` is the current composition root and depends on both application and adapter crates at `crates/agentdash-api/Cargo.toml:16-24`.

Verdict: Cargo dependency direction is mostly clean at crate level today. The split blocker is not a Cargo cycle yet; it is intra-`agentdash-application` module coupling and broad public facades. Physical extraction before facade/visibility cleanup would create cycles between AgentRun, Lifecycle, RuntimeSession, VFS and RuntimeGateway.

### Module Facade And `pub use` Hotspots

Facade counts from `rg -n "^(pub mod|pub use|pub(crate) use)"`:

- `crates/agentdash-application/src/lib.rs`: `pub mod=39`, `pub use=3`; crate root publicly exposes almost every application module at `lib.rs:1-39`.
- `crates/agentdash-application/src/session/mod.rs`: `pub mod=29`, `pub use=29`; public modules include `baseline_capabilities`, `bootstrap`, `construction_planner`, `context`, `runtime_transition_service`, `continuation`, `control`, `core`, `effects_service`, `eventing`, `hook_delegate`, `hook_events`, `hooks_service`, `launch`, `persistence`, `plan`, `runtime_builder`, `runtime_commands`, `runtime_control`, `runtime_services`, `stall_detector`, `terminal_cache`, `terminal_effects`, `title_generator`, `title_service`, `tool_result_cache`, `turn_processor`, and `types` at `session/mod.rs:3-57`.
- `session/mod.rs` also re-exports non-session owners: `AgentFrameHookRuntime` from AgentRun at `session/mod.rs:59` and `WorkflowApplicationError` from Lifecycle at `session/mod.rs:60`.
- `session/mod.rs` re-exports service and DTO surface such as `SessionControlService`, `SessionCoreService`, `SessionRuntimeService`, `SessionRuntimeTransitionService`, `SessionRuntimeServices`, `SessionLaunchService`, `SessionPersistence`, `SessionToolResultCache`, `TurnEvent`, and `AgentFrameRuntimeTarget` at `session/mod.rs:74-120`.
- `crates/agentdash-application/src/agent_run/mod.rs`: `pub mod=7`, `pub use=13`; public modules include `frame`, `mailbox`, `message_delivery`, `runtime_capability`, `runtime_capability_projection`, `runtime_surface`, and `workspace` at `agent_run/mod.rs:5-14`.
- `agent_run/frame/mod.rs` publicly exposes all frame submodules and re-exports `AgentFrameBuilder`, `AgentFrameHookRuntime`, `FrameLaunchEnvelope`, `FrameRuntimeSurface`, `AgentFrameSurfaceExt`, and frame services at `agent_run/frame/mod.rs:1-17`.
- `crates/agentdash-application/src/lifecycle/mod.rs`: `pub mod=9`, `pub use=14`, `pub(crate) use=5`; it exposes dispatch, execution log, gate, orchestrator, projection, run command, run view, surface, and tools at `lifecycle/mod.rs:3-17`.
- `crates/agentdash-application/src/vfs/mod.rs`: `pub mod=27`, `pub use=24`; it exposes provider internals, mount builders, materialization, mutation, surface query, tools and low-level types at `vfs/mod.rs:1-94`.
- `crates/agentdash-application/src/runtime_gateway/mod.rs`: `pub use=10`; implementation modules stay private, but facade re-exports provider/action types including `CurrentSurfaceRuntimeMcpAccess`, `RuntimeProvider`, session MCP providers, setup providers, adapter and DTOs at `runtime_gateway/mod.rs:11-39`.
- `crates/agentdash-application/src/runtime_tools/mod.rs` re-exports providers from companion, lifecycle, task and workspace module at `runtime_tools/mod.rs:4-12`, making runtime tool assembly a cross-domain aggregator.

Verdict: boundary facade first should not mean adding more root re-exports. It should introduce narrow facades and then remove direct exports of construction/planner/hub/internal surface types from `session`, `vfs`, and `agent_run::frame`.

### Application Import Hotspots

Cross-module import/reference counts inside `crates/agentdash-application/src` from `rg` matrix:

- `agent_run -> session`: 51
- `session -> agent_run`: 47
- `agent_run -> lifecycle`: 32
- `agent_run -> vfs`: 10
- `workspace_module -> vfs`: 9
- `lifecycle -> vfs`: 9
- `hooks -> lifecycle`: 9
- `workflow -> lifecycle`: 8
- `workspace_module -> session`: 8
- `lifecycle -> workflow`: 8
- `permission -> agent_run`: 7
- `canvas -> vfs`: 7
- `session -> vfs`: 7
- `lifecycle -> agent_run`: 7
- `session -> lifecycle`: 6
- `vfs -> lifecycle`: 6
- `workspace_module -> canvas`: 6
- `vfs -> session`: 5
- `lifecycle -> session`: 5
- `workspace_module -> agent_run`: 5
- `workspace_module -> runtime_gateway`: 5

Key code patterns:

- AgentRun runtime surface query already has the correct target shape: it depends on `RuntimeSessionExecutionAnchorRepository`, `LifecycleRunRepository`, `LifecycleAgentRepository`, and `AgentFrameRepository` at `agent_run/runtime_surface.rs:41-54`; the port returns current runtime surface by `runtime_session_id` at `agent_run/runtime_surface.rs:57-68`.
- `AgentRunRuntimeSurfaceQuery::resolve_surface` starts from `RuntimeSessionExecutionAnchorRepository::find_by_session` at `agent_run/runtime_surface.rs:81-98`, then loads `LifecycleRun`, `LifecycleAgent`, and current `AgentFrame` at `agent_run/runtime_surface.rs:100-180`.
- The query returns a DTO with `runtime_session_id`, `run_id`, `project_id`, `agent_id`, `runtime_address`, `surface_frame_id`, `capability_state`, `vfs`, `mcp_servers`, backend anchor and provenance at `agent_run/runtime_surface.rs:221-245` and `agent_run/runtime_surface.rs:287-302`.
- Runtime surface update depends on a query port, `AgentFrameRepository`, `VfsService`, and an `AgentRunActiveRuntimeSurfaceAdopter` at `agent_run/runtime_surface_update.rs:23-47`; adoption currently still targets `AgentFrameRuntimeTarget` from session at `agent_run/runtime_surface_update.rs:62-69`.
- `AgentFrameRuntimeTarget` is currently defined under `session::types` even though it expresses an AgentFrame/live runtime adoption target at `session/types.rs:62-70`.
- `SessionRuntimeInner` implements `AgentRunActiveRuntimeSurfaceAdopter` at `session/hub/tool_builder.rs:317-324`, so AgentRun update writes are coupled back into SessionHub live runtime internals.
- SessionHub comments state it still owns tool building, hook dispatch, runtime context transition and that these should keep moving to concrete services/packages at `session/hub/mod.rs:1-12`.
- SessionHub holds `AgentFrameRepository`, `RuntimeSessionExecutionAnchorRepository`, `LifecycleAgentRepository`, `PermissionGrantRepository`, and mailbox runtime adapter fields at `session/hub/mod.rs:84-94`, confirming that live runtime cache currently knows too much about AgentRun/Lifecycle control plane.
- AgentRun frame construction consumes Lifecycle projector facts, capability resolver, context builder, session construction planner, session plan fragments and VFS service at `agent_run/frame/construction/request_assembler.rs:41-69`, and calls `session::plan::build_session_plan_fragments` at `agent_run/frame/construction/request_assembler.rs:575-581`.
- Lifecycle dispatch defines `RuntimeSessionCreator` at `lifecycle/dispatch_service.rs:43-48`, implements it with `SessionPersistence` at `lifecycle/dispatch_service.rs:51-68`, and writes `RuntimeSessionExecutionAnchor` on workflow/plain dispatch at `lifecycle/dispatch_service.rs:411-418`, `lifecycle/dispatch_service.rs:498-506`, and `lifecycle/dispatch_service.rs:597-604`.
- `RuntimeSessionExecutionAnchor` is a domain workflow launch-evidence model, not a session facade detail: comments define it as RuntimeSession to control-plane anchor at `agentdash-domain/src/workflow/runtime_session_anchor.rs:20-28`, and fields hold runtime session, run, launch frame, agent and optional orchestration node at `runtime_session_anchor.rs:29-43`.
- SPI already owns session persistence substrate DTOs: `SessionMeta` at `agentdash-spi/src/session_persistence.rs:304-321`, `PersistedSessionEvent` at `session_persistence.rs:531-544`, and `SessionPersistence` store trait aggregation at `session_persistence.rs:942-951`.

Verdict: the highest-risk cycle is `AgentRun <-> SessionHub/RuntimeSession`. The second is `AgentRun <-> Lifecycle`. These should be broken by moving DTO/port ownership before moving files across crates.

### RuntimeGateway And Current Surface

- `RuntimeGateway` facade exports `CurrentSurfaceRuntimeMcpAccess` and session/setup providers at `runtime_gateway/mod.rs:23-39`.
- Session MCP action port is `RuntimeSessionMcpAccess` with `list_mcp_tools(session_id)` and `call_mcp_tool(session_id, input)` at `runtime_gateway/session_actions.rs:65-77`.
- Session MCP providers are `RuntimeProvider`s for `SessionRuntime` at `runtime_gateway/session_actions.rs:79-105` and `runtime_gateway/session_actions.rs:160-183`.
- `CurrentSurfaceRuntimeMcpAccess` depends on `AgentRunRuntimeSurfaceQueryPort` and `McpToolDiscovery` at `runtime_gateway/mcp_access.rs:11-25`.
- MCP discovery requires `current_runtime_surface_with_backend(session_id, RuntimeSurfaceQueryPurpose)` at `runtime_gateway/mcp_access.rs:39-50`, then calls MCP discovery with the closed surface/backend context at `runtime_gateway/mcp_access.rs:52-55`.
- Tests assert idle MCP listing uses runtime surface backend anchor and disabled capability tools are not exposed at `runtime_gateway/mcp_access.rs:455-510`.
- API composes `AgentRunRuntimeSurfaceQuery` and `CurrentSurfaceRuntimeMcpAccess` in `app_state.rs:238-249`, then registers MCP and extension providers in `bootstrap/runtime_gateway.rs:35-42`.

Verdict: RuntimeGateway is already close to a clean crate/module. The remaining split blocker is that its MCP access implementation imports the AgentRun query port from `agentdash-application::agent_run`; move the query trait/DTO or a reduced gateway-facing port into `agentdash-application-ports` before physical extraction.

### VFS / Resource Surface

- VFS architecture requires AgentRun workspace resource surface to come from current `AgentFrame` typed VFS plus lifecycle projection, not raw session inventory.
- `VfsSurfaceRuntimeProjection` is currently in application VFS and asks the API composition root for backend online/edit capability facts at `vfs/surface_query.rs:11-15`.
- `build_surface_summary` consumes an inline file repository, runtime projection, `ResolvedVfsSurfaceSource`, and `Vfs` to build a browser-facing surface summary at `vfs/surface_query.rs:17-74`.
- API implements `VfsSurfaceRuntimeProjection` in `ApiVfsSurfaceRuntimeProjection`, backed by `BackendRegistry` and `MountProviderRegistry`, at `agentdash-api/src/vfs_surface_runtime.rs:10-43`.
- AgentRun workspace query consumes `VfsSurfaceRuntimeProjection` and session control/core services at `agent_run/workspace/query.rs:39-55`, builds summaries from `ResolvedVfsSurfaceSource::AgentRun` at `agent_run/workspace/query.rs:86-99`, and projects read surface from `AgentRunLifecycleSurfaceProjector` at `agent_run/workspace/query.rs:355-369`.
- API VFS surface resolver still imports `session::construction_planner::resolve_project_workspace` at `routes/vfs_surfaces/resolver.rs:3-5` and uses `SessionMountTarget` for Project/Story/Task surfaces at `resolver.rs:48-140`, while session runtime surfaces use `RuntimeSurfaceQueryPurpose::resource_surface()` at `resolver.rs:210-304`.

Verdict: keep physical VFS provider/service extraction later. First, move "AgentRun resource surface query" to an AgentRun/Lifecycle application facade and keep VFS as provider/summary/mutation infrastructure inside application. `VfsSurfaceRuntimeProjection` is a good candidate for `agentdash-application-ports` because API/local implements it and application consumes it.

### API / Composition Coupling

- `AppState` imports AgentRun runtime surface query/update, RuntimeGateway, Session services and VFS services directly at `agentdash-api/src/app_state.rs:9-29`.
- `AppState` wires one `AgentRunRuntimeSurfaceQuery` for RuntimeGateway MCP access at `app_state.rs:238-249`.
- Session bootstrap wires another `AgentRunRuntimeSurfaceQuery` into `AgentRunRuntimeSurfaceUpdateService` with SessionRuntimeBuilder as active adopter at `bootstrap/session.rs:205-222`.
- RuntimeGateway bootstrap registers setup providers, MCP providers, and extension dynamic provider in API composition root at `bootstrap/runtime_gateway.rs:20-42`.
- Terminal, extension runtime, canvas, lifecycle agents and vfs surface routes import `RuntimeSurfaceQueryPurpose` or current surface DTOs directly: `routes/terminals.rs:8`, `routes/extension_runtime.rs:19`, `routes/canvases.rs:7`, `routes/lifecycle_agents.rs:57`, and `routes/vfs_surfaces/resolver.rs:3-7`.

Verdict: API is allowed to compose concrete services, but routes should not continue to know session construction planner or domain `AgentFrame` details for current surface reads. Add application facade DTOs for route-level read/use cases before crate extraction.

## Split Candidate Evaluation

| Candidate | Boundary Verdict | Evidence | Release Recommendation |
| --- | --- | --- | --- |
| AgentRun application crate/module | Accept as first owning facade; physical crate only after import cleanup. | Current surface query already starts from runtime session anchor and returns a closed DTO at `agent_run/runtime_surface.rs:57-68` and `agent_run/runtime_surface.rs:287-302`; AgentRun workspace query owns resource surface read model at `agent_run/workspace/query.rs:39-55`. | Create a narrow `agent_run` facade/port for current surface query, update, effective capability and workspace snapshot. Hide frame builder/surface internals from API. Extract as `agentdash-application-agentrun` only after `AgentFrameRuntimeTarget` and active adoption port leave `session`. |
| Lifecycle application crate/module | Accept, but split after AgentRun facade because current Lifecycle and AgentRun mutually depend. | Lifecycle dispatch owns RuntimeSession creation and anchor writes at `lifecycle/dispatch_service.rs:43-68`, `411-418`, `597-604`; workflow specs define LifecycleRun/orchestration control plane. | Keep physical extraction later. First expose Lifecycle dispatch/run-view/surface projector through facades and move `RuntimeSessionCreator` to a lower port. |
| RuntimeSession substrate | Accept as lower-level substrate, but not as public business crate. | SPI owns `SessionMeta`, `PersistedSessionEvent`, `SessionPersistence` at `agentdash-spi/src/session_persistence.rs:304-321`, `531-544`, `942-951`; SessionHub comment says remaining internals should move down at `session/hub/mod.rs:1-12`. | Rename/scope internally as runtime-session delivery/trace. Extract only eventing, runtime registry/control, persistence adapters, turn processing and connector delivery. Do not include current surface query, AgentFrame writes, Permission, VFS resource surface, or Gateway providers. |
| RuntimeGateway | Accept as a late low-risk extraction. | RuntimeGateway modules are private with a narrow facade at `runtime_gateway/mod.rs:1-39`; MCP access consumes a query port at `runtime_gateway/mcp_access.rs:23-50`; API registers providers at `bootstrap/runtime_gateway.rs:20-42`. | Move gateway-facing AgentRun surface port/DTO into `application-ports`, then extract `agentdash-application-runtime-gateway`. Keep API bootstrap as composition root; do not move `agentdash_infrastructure::RmcpProbeTransport` into gateway crate. |
| VFS/resource surface | Keep VFS providers in application for now; extract resource-surface facade under AgentRun first. | VFS summary port is generic at `vfs/surface_query.rs:11-74`; API implements runtime projection at `api/vfs_surface_runtime.rs:10-43`; AgentRun workspace query owns AgentRun resource surface at `agent_run/workspace/query.rs:86-99`, `355-369`. | First split AgentRun resource-surface facade from generic VFS provider/service. Physical VFS crate extraction comes after VFS no longer imports session/lifecycle and API routes stop using session construction planner. |
| Application ports | Accept as the first crate-level expansion point. | Existing ports crate is pure and small at `agentdash-application-ports/src/lib.rs:1-4`; executor and application already depend on it through normal Cargo deps. | Add `agent_run_surface`, `runtime_session_delivery`, and `vfs_surface_runtime` ports/DTOs here before physical crate extraction. Avoid putting application services or repository sets in ports. |

## Release Split Batches

### Batch 1: boundary facade first

Goal: make the intended dependency direction visible while everything still compiles in one application crate.

1. Add/settle AgentRun facade APIs:
   - `AgentRunRuntimeSurfaceQueryPort` + DTOs for RuntimeGateway/API current surface.
   - `AgentRunRuntimeSurfaceUpdateService` as the only surface-changing command path for Canvas, Permission, WorkspaceModule, MCP/VFS/Skill changes.
   - `AgentRunEffectiveCapabilityService` as the only execution capability/admission read path.
2. Move ownership of `AgentFrameRuntimeTarget` out of `session::types` or re-export it from AgentRun while call sites migrate; current definition at `session/types.rs:62-70` is semantically AgentRun active-runtime adoption.
3. Move `RuntimeSessionCreator` out of Lifecycle implementation details into a runtime-session/application-port boundary; Lifecycle should consume a creation port, not own session persistence details forever.
4. Add gateway-facing port in `agentdash-application-ports` so RuntimeGateway can depend on `RuntimeSessionMcpAccess` + AgentRun current-surface DTO without importing `agentdash-application::agent_run`.
5. Add VFS surface runtime port to `agentdash-application-ports`; API already implements the shape in `api/vfs_surface_runtime.rs:10-43`.
6. Introduce a RuntimeSession facade module that intentionally exposes only delivery/trace/turn/event/resume/debug/persistence services, not current business surface.

Batch 1 compile/test gates:

- `cargo metadata --no-deps --format-version 1`
- `cargo check -p agentdash-application`
- `cargo check -p agentdash-api`
- `cargo test -p agentdash-application runtime_gateway::session_actions`
- `cargo test -p agentdash-application runtime_gateway::mcp_access`
- `cargo test -p agentdash-application agent_run::runtime_surface`
- `cargo test -p agentdash-application agent_run::runtime_surface_update`

### Batch 2: visibility/import cleanup second

Goal: remove accidental public/internal import paths so physical crate extraction is mechanical.

1. Reduce application root public modules from broad `pub mod` exposure at `lib.rs:1-39` toward user-facing application facades.
2. Make session internals private or `pub(crate)`: `construction_planner`, `plan`, `runtime_builder`, `runtime_transition_service`, `hook_delegate`, `hook_events`, `hooks_service`, `tool_result_cache`, `terminal_cache`, and `types` should not be route-level APIs.
3. Remove `session` re-exports of AgentRun/Lifecycle types (`AgentFrameHookRuntime`, `WorkflowApplicationError`) from `session/mod.rs:59-60`.
4. Replace API imports of `session::construction_planner::resolve_project_workspace` and `SessionMountTarget` in `routes/vfs_surfaces/resolver.rs:3-140` with project/workspace or VFS facades.
5. Replace API current-surface route imports of AgentFrame/current-frame internals with AgentRun DTOs; `routes/lifecycle_agents.rs` still has test/read-model references to `AgentFrame` and `RuntimeSessionExecutionAnchor` at `routes/lifecycle_agents.rs:1680-2028`.
6. Break `agent_run -> session` and `session -> agent_run` direct imports by moving shared DTO/ports to `agentdash-application-ports` or a `runtime_session` internal facade.
7. Split `runtime_tools` cross-domain provider re-exports into explicit composition root exports; do not make VFS/tools own workflow/collaboration/workspace module provider surfaces.

Batch 2 gates:

- `cargo check -p agentdash-application`
- `cargo check -p agentdash-api`
- `cargo check -p agentdash-local`
- `cargo check -p agentdash-mcp`
- `rg -n "agentdash_application::session::construction_planner|agentdash_application::session::plan|agentdash_application::session::AgentFrameRuntimeTarget" crates/agentdash-api/src crates/agentdash-mcp/src crates/agentdash-local/src` should return no production call sites.
- `rg -n "use crate::session::|crate::session::" crates/agentdash-application/src/agent_run crates/agentdash-application/src/lifecycle` should trend down to explicit runtime-session facade imports only.
- Re-run hotspot matrix; target no direct `agent_run <-> session` business-surface dependency.

### Batch 3: physical crate extraction third

Goal: create crates only when imports already express the desired graph.

Recommended extraction order:

1. Expand `agentdash-application-ports`.
   - Add pure DTO/trait modules for AgentRun runtime surface, runtime session delivery/adoption, and VFS runtime projection.
   - Expected deps stay similar to current ports crate: domain/spi/agent protocol/agent types/relay only.
2. Extract RuntimeSession substrate as `agentdash-application-runtime-session` or `agentdash-runtime-session`.
   - Depends on domain, spi, agent-protocol/agent-types, relay/ports as needed.
   - Must not depend on AgentRun, Lifecycle, RuntimeGateway, VFS providers, Permission, Canvas, WorkspaceModule or API.
3. Extract AgentRun as `agentdash-application-agentrun`.
   - Depends on domain, spi, application-ports, contracts/protocol where needed, runtime-session ports, VFS surface provider interfaces, and lifecycle ports/DTOs only.
   - Does not depend on full Lifecycle implementation crate.
4. Extract Lifecycle as `agentdash-application-lifecycle`.
   - Depends on domain, application-ports, AgentRun facade/ports and RuntimeSession creation port.
   - Owns dispatch/orchestration/reducer/surface projector; consumes runtime delivery through ports.
5. Extract RuntimeGateway as `agentdash-application-runtime-gateway`.
   - Depends on application-ports, domain, spi and MCP discovery/extension transport ports.
   - Does not depend on AgentRun implementation; consumes query/admission ports.
6. Defer VFS physical extraction until `vfs -> session`, `vfs -> lifecycle`, and API route direct VFS internals are reduced.

Batch 3 gates:

- `cargo metadata --no-deps --format-version 1` must show no new application crate cycles.
- `cargo check --workspace` after each crate extraction.
- `cargo test -p agentdash-application-runtime-session` for delivery/event/turn/persistence substrate.
- `cargo test -p agentdash-application-agentrun` for current surface query/update/effective capability/workspace snapshot.
- `cargo test -p agentdash-application-lifecycle` for dispatch/orchestration/runtime node reducer.
- `cargo test -p agentdash-application-runtime-gateway` for provider admission/MCP/extension/setup actions.
- `cargo check -p agentdash-api -p agentdash-local -p agentdash-mcp` after API composition rewiring.

## Risks

- Physical extraction now would force cycles: current `agent_run -> session` count is 51 and `session -> agent_run` count is 47; current `agent_run -> lifecycle` is 32 and `lifecycle -> agent_run` is 7.
- `SessionHub` is still the live runtime adoption implementation for AgentRun surface updates at `session/hub/tool_builder.rs:317-324`; extracting AgentRun first requires inverting this through a port.
- `AgentFrameRuntimeTarget` lives under `session::types` at `session/types.rs:62-70`, which makes AgentRun/Lifecycle callers import session for an AgentFrame runtime concept.
- API routes and bootstrap use broad application modules directly. This is acceptable for composition root, but route-level current surface reads should use application facades rather than session construction helpers.
- `vfs/mod.rs` exposes provider internals and service internals broadly at `vfs/mod.rs:1-94`; extracting VFS early risks pulling lifecycle/session/agentrun with it.
- `runtime_tools/mod.rs` aggregates providers from companion, lifecycle, task and workspace module at `runtime_tools/mod.rs:4-12`; moving it to a crate before facades would centralize cross-domain coupling rather than reduce it.
- `agentdash-mcp` currently depends on `agentdash-application` for task plan projections. This is outside the session/AgentRun split but must be considered when splitting application into multiple crates.

## Child Task Candidates

1. `agentrun-current-surface-facade`
   - Scope: move/settle current runtime surface query/update/effective capability DTOs and ports; make RuntimeGateway/API consume facade contracts.
   - Depends on: this research file, specs for runtime-gateway/session/capability/vfs.
   - Gate: `cargo test -p agentdash-application agent_run::runtime_surface runtime_gateway::mcp_access`.

2. `runtime-session-substrate-facade`
   - Scope: rename/scope session public surface to runtime-session delivery/trace; move `AgentFrameRuntimeTarget` ownership out of session; hide session hub internals.
   - Depends on: AgentRun current-surface facade.
   - Gate: `cargo check -p agentdash-application`; `rg` proves API no longer imports session planner/surface internals.

3. `lifecycle-runtime-session-port`
   - Scope: move `RuntimeSessionCreator` and launch evidence writes behind port/facade; keep Lifecycle dispatch owning run/agent/frame/anchor materialization semantics.
   - Depends on: runtime-session substrate facade.
   - Gate: lifecycle dispatch/orchestration tests plus `cargo check -p agentdash-application`.

4. `runtime-gateway-port-boundary`
   - Scope: move gateway-facing AgentRun surface/MCP access contracts to `agentdash-application-ports`; keep providers private behind RuntimeGateway.
   - Depends on: AgentRun current-surface facade.
   - Gate: `cargo test -p agentdash-application runtime_gateway`.

5. `vfs-resource-surface-facade`
   - Scope: separate AgentRun resource surface query from generic VFS provider/service/summary; move `VfsSurfaceRuntimeProjection` or equivalent to ports.
   - Depends on: AgentRun facade and API route cleanup.
   - Gate: `cargo check -p agentdash-api`; VFS surface route tests if present.

6. `application-public-visibility-cleanup`
   - Scope: reduce `pub mod` and `pub use` at `lib.rs`, `session/mod.rs`, `vfs/mod.rs`, `agent_run/frame/mod.rs`; preserve only intended facades.
   - Depends on: first facade tasks.
   - Gate: `cargo check -p agentdash-api -p agentdash-local -p agentdash-mcp`.

7. `physical-crate-extraction-wave-1`
   - Scope: expand `agentdash-application-ports`, then extract RuntimeSession and RuntimeGateway if import graph is clean.
   - Depends on: visibility cleanup.
   - Gate: `cargo metadata --no-deps --format-version 1` and `cargo check --workspace`.

8. `physical-crate-extraction-wave-2`
   - Scope: extract AgentRun and Lifecycle crates after ports prevent cycles.
   - Depends on: wave 1 and reduced `agent_run <-> lifecycle/session` matrix.
   - Gate: workspace check plus AgentRun/Lifecycle targeted tests.

## Related Specs

- `.trellis/spec/backend/architecture.md` - clean architecture dependency direction and current crate roles.
- `.trellis/spec/backend/directory-structure.md` - crate/layer baseline and `agentdash-application-ports` role.
- `.trellis/spec/backend/session/architecture.md` - `Session` target semantics as `RuntimeSession`, AgentFrame as effective surface fact source, RuntimeSession as delivery/trace substrate.
- `.trellis/spec/backend/runtime-gateway.md` - RuntimeGateway actor/context admission and Session MCP action current-surface query contract.
- `.trellis/spec/backend/workflow/architecture.md` - LifecycleRun, AgentRun runtime address, RuntimeSessionExecutionAnchor and AgentRun resource surface ownership.
- `.trellis/spec/backend/vfs/architecture.md` - VFS provider/resource surface semantics and AgentRun resource surface resolver expectation.
- `.trellis/spec/backend/capability/architecture.md` - AgentRun effective capability/admission as runtime capability read entry.
- `.trellis/spec/backend/permission/architecture.md` - grant effects and active runtime adoption failure visibility.

## External References

- No web references used.
- Tooling evidence:
  - `cargo metadata --no-deps --format-version 1`
  - `cargo 1.96.0-nightly (f298b8c82 2026-02-24)`
  - `rustc 1.96.0-nightly (69370dc4a 2026-03-05)`

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` returned no active task in this shell; the user supplied the exact task path, so this report was written there.
- This research did not modify business source code or other research files.
- This report does not assert the exact file moves for physical extraction; it defines the dependency blockers and release-safe order. Exact crate boundaries should be locked after Batch 1/2 `rg` matrices show the cycles are gone.
- No database migration was inspected because this is research-only and no source/schema edits were made.
