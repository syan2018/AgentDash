# Phase 0 Surface Fact Source Audit

- Task: `06-12-agentrun-runtime-session-surface-convergence`
- Date: 2026-06-12
- Scope: Evidence refresh only. No source code changes.

## Context Read

Task artifacts read:

- `.trellis/tasks/06-12-agentrun-runtime-session-surface-convergence/prd.md`
- `.trellis/tasks/06-12-agentrun-runtime-session-surface-convergence/design.md`
- `.trellis/tasks/06-12-agentrun-runtime-session-surface-convergence/implement.md`
- `.trellis/tasks/06-12-agentrun-runtime-session-surface-convergence/implement.jsonl`

Curated context read from `implement.jsonl`:

- `.trellis/spec/backend/session/session-startup-pipeline.md`
- `.trellis/spec/backend/session/execution-context-frames.md`
- `.trellis/spec/backend/runtime-gateway.md`
- `.trellis/spec/backend/capability/tool-capability-pipeline.md`
- `.trellis/tasks/archive/2026-06/06-12-mcp-runtime-binding/design.md`
- `.trellis/tasks/archive/2026-06/06-01-session-lifecycle-control-plane-refactor/target-state-blueprint.md`
- `.trellis/tasks/archive/2026-06/06-02-lifecycle-control-plane-frame-convergence/research/session-frame-launch-boundary.md`

Lightweight evidence commands used:

- `rg -n "RuntimeMcpServerDeclaration|RuntimeMcpServer|McpRuntimeBindingContext|RuntimeSessionMcpAccess" crates packages/app-web/src .trellis/spec`
- `rg -n "mcp_surface_json|RuntimeSessionExecutionAnchor|runtime_session_execution_anchor|execution_anchor" crates packages/app-web/src .trellis/spec`
- Focused `rg -n "^"` reads of the referenced files listed below.

## Current Name Distribution

| Name | Main definition | Main consumers | Current semantic boundary |
| --- | --- | --- | --- |
| `RuntimeMcpServerDeclaration` | `crates/agentdash-spi/src/connector/mod.rs:510` | SPI connector frames, capability resolver, frame construction, runtime MCP direct/relay, relay prompt serialization, local relay parser, session persistence | Runtime-resolved MCP declaration for the executable surface. It has no ownership/session identity field and is the canonical runtime declaration shape. |
| `RuntimeMcpServer` | `crates/agentdash-application/src/runtime.rs:11` | context/session plan summaries and conversion helpers in `runtime_bridge.rs` | Application-level summary/bridge shape for MCP runtime context. It is less complete than `RuntimeMcpServerDeclaration` for HTTP/SSE headers and relay identity, so it is not the canonical runtime declaration. |
| `McpRuntimeBindingContext` | `crates/agentdash-application/src/mcp_preset/runtime.rs:21` | MCP preset runtime binding resolver and capability resolver input | Runtime binding context carrying final VFS facts. It is not a runtime session object; it is a resolver context derived from frame construction facts. |
| `RuntimeSessionMcpAccess` | `crates/agentdash-application/src/runtime_gateway/session_actions.rs:66` | Runtime gateway `mcp.list_tools` / `mcp.call_tool`, implemented by `SessionCapabilityService` | Session action adapter over an active delivery runtime session. This name remains aligned with the runtime gateway boundary. |
| `RuntimeSessionExecutionAnchor` | `crates/agentdash-domain/src/workflow/runtime_session_anchor.rs:29` | dispatch/orchestration creation, session/frame lookup, AgentRun workspace, VFS surface resolution, runtime trace views | Launch evidence index from runtime session to run / agent / launch frame / orchestration node. It is not a surface fact source. |
| `AgentFrame.mcp_surface_json` | `crates/agentdash-domain/src/workflow/agent_frame.rs:21` | frame builder, frame surface reader, runtime launch, AgentRun workspace views, persistence | Persisted frame revision field for the MCP executable surface. This is already the closest durable surface fact location. |

## Current Surface Write Paths

### 1. Capability resolver produces MCP declarations, but does not persist them

`CapabilityResolverInput` carries `mcp_runtime_context: Option<McpRuntimeBindingContext>` at `crates/agentdash-application/src/capability/resolver.rs:84-92`.

For `mcp:<preset>` directives, the resolver calls `resolve_preset_mcp_declaration(preset, input.mcp_runtime_context.as_ref())` and pushes a `RuntimeMcpServerDeclaration` into `resolved_mcp_servers` at `crates/agentdash-application/src/capability/resolver.rs:259-270`.

This makes `CapabilityResolver` the producer of part of the runtime MCP surface. It remains an in-memory resolver output; persistence happens later through frame construction or launch commit.

### 2. MCP preset runtime binding resolves from final VFS

`McpRuntimeBindingContext` is only:

```rust
pub struct McpRuntimeBindingContext<'a> {
    pub vfs: Option<&'a Vfs>,
}
```

`resolve_preset_mcp_declaration()` applies `McpPreset.runtime_binding` against the selected final VFS mount and returns `RuntimeMcpServerDeclaration { name, uses_relay, transport }` at `crates/agentdash-application/src/mcp_preset/runtime.rs:68-80`.

The binding resolver reads mount metadata such as workspace id, binding id, identity payload, and detected facts through `apply_runtime_binding()` at `crates/agentdash-application/src/mcp_preset/runtime.rs:87-130`. This confirms that `McpRuntimeBindingContext` is actually an MCP runtime binding context over final VFS facts.

### 3. Session request assembly normalizes multiple MCP inputs into one projection

Owner bootstrap builds final VFS first, resolves capabilities with `mcp_runtime_context: Some(McpRuntimeBindingContext { vfs })`, then normalizes request MCP declarations, capability MCP output, and agent-level preset MCP into one server list:

- Resolver input: `crates/agentdash-application/src/session/assembler.rs:682-690`
- Final VFS and owner bootstrap flow: `crates/agentdash-application/src/session/assembler.rs:783-827`
- Builder receives both resolved capabilities and MCP servers: `crates/agentdash-application/src/session/assembler.rs:869-875`
- Normalization helper: `crates/agentdash-application/src/session/assembler.rs:1516-1539`

`normalize_owner_bootstrap_mcp_projection()` is the important sync point. It:

- Extends from request MCP declarations.
- Extends from `capability_state.tool.mcp_servers`.
- Resolves agent-level preset MCP declarations with the same runtime context.
- Deduplicates by server name.
- Inserts corresponding tool capabilities.
- Writes the final list back into `capability_state.tool.mcp_servers`.

This is currently the main pre-launch synchronizer for `FrameLaunchEnvelope.mcp_servers` and `CapabilityState.tool.mcp_servers`.

### 4. AgentFrameBuilder writes frame surface JSON

`AgentFrameBuilder::with_capability_state()` splits `CapabilityState` into `effective_capability_json`, `vfs_surface_json`, and `mcp_surface_json` through `capability_state_to_frame_surfaces()` at `crates/agentdash-application/src/workflow/frame_builder.rs:120-129`.

`AgentFrameBuilder::with_mcp_servers()` can also fill `mcp_surface_json` directly from `Vec<RuntimeMcpServerDeclaration>` at `crates/agentdash-application/src/workflow/frame_builder.rs:141-148`.

`with_surface_input()` composes capability state, VFS, MCP, execution profile, and context bundle summary into the pending frame at `crates/agentdash-application/src/workflow/frame_builder.rs:176-192`.

`build_uncommitted()` carries forward previous surface fields when a new field is not provided, including `mcp_surface_json` at `crates/agentdash-application/src/workflow/frame_builder.rs:226-248`.

### 5. Launch accept writes or rewrites AgentFrame surface

`TurnCommitter::commit_accepted_agent_frame()` is the accepted-boundary writer.

When there is a pending frame, it overwrites the pending frame surface from `prepared.accepted_capability_state` before `frame_repo.create()`:

- `effective_capability_json`: `crates/agentdash-application/src/session/launch/commit.rs:155-157`
- `vfs_surface_json`: `crates/agentdash-application/src/session/launch/commit.rs:158`
- `mcp_surface_json`: `crates/agentdash-application/src/session/launch/commit.rs:159`
- `current_frame_id` update: `crates/agentdash-application/src/session/launch/commit.rs:168-175`

When there is no pending frame, it resolves the current frame for the runtime session, builds a new revision with `with_capability_state(&prepared.accepted_capability_state)`, and updates the agent current frame at `crates/agentdash-application/src/session/launch/commit.rs:188-223`.

This makes accepted capability state another frame surface writer, not just a runtime cache update.

### 6. Live runtime context transitions write a new AgentFrame revision and sync the active turn

`SessionRuntimeInner::replace_current_capability_state()` is the live update primitive. It:

- Resolves `AgentFrameRuntimeTarget` from delivery runtime session and frame id.
- Verifies the delivery `RuntimeSessionExecutionAnchor` belongs to the same agent.
- Creates a new frame revision with `AgentFrameBuilder::with_capability_state(&state)`.
- Updates the connector tools.
- Updates `runtime.session_profile`, active turn `session_frame.mcp_servers`, active turn VFS, and active turn `capability_state`.

Evidence:

- Target and anchor validation: `crates/agentdash-application/src/session/hub/tool_builder.rs:116-161`
- New frame revision write: `crates/agentdash-application/src/session/hub/tool_builder.rs:172-189`
- In-memory and connector sync: `crates/agentdash-application/src/session/hub/tool_builder.rs:198-263`

This is the runtime-side synchronizer for live capability/VFS/MCP changes.

## Current Surface Read Paths

### 1. Frame surface typed readers

`AgentFrameSurfaceExt` reads typed surface fields from `AgentFrame`:

- `typed_capability_state()`: `crates/agentdash-application/src/workflow/frame_surface.rs:43-47`
- `typed_vfs()`: `crates/agentdash-application/src/workflow/frame_surface.rs:49-53`
- `typed_mcp_servers()`: `crates/agentdash-application/src/workflow/frame_surface.rs:55-60`
- `typed_execution_profile()`: `crates/agentdash-application/src/workflow/frame_surface.rs:62-66`

`project_capability_state_from_frame()` reconstructs a `CapabilityState` from `AgentFrame`, then lets `vfs_surface_json` and `mcp_surface_json` override embedded dimensions at `crates/agentdash-application/src/session/capability_state.rs:42-74`.

These are the cleanest current read APIs for treating AgentFrame as surface fact source.

### 2. Frame construction reads current frame, falling back to launch frame

`FrameConstructionService::construct_launch_envelope()` starts from the runtime session id, finds `RuntimeSessionExecutionAnchor`, loads `LifecycleAgent`, loads `LifecycleRun`, and then chooses `agent_frame_repo.get_current(agent.id)` or `agent_frame_repo.get(anchor.launch_frame_id)` at `crates/agentdash-application/src/workflow/frame_construction/mod.rs:83-146`.

For plain lifecycle launches, if the frame surface is ready, it directly builds the envelope from the frame at `crates/agentdash-application/src/workflow/frame_construction/mod.rs:148-159`.

`frame_surface_ready()` requires execution profile, capability state, and a non-empty VFS default mount root at `crates/agentdash-application/src/workflow/frame_construction/mod.rs:229-237`.

### 3. FrameLaunchEnvelope reads AgentFrame surface and may apply launch extras

`build_envelope_from_frame()` creates `FrameRuntimeSurface::from_frame(frame, runtime_session_id)` and reads typed VFS, execution profile, capability state, and MCP servers from the frame at `crates/agentdash-application/src/workflow/frame_construction/mod.rs:302-315`.

Then extras may override capability state, VFS, and MCP servers before the final `FrameLaunchEnvelope` is returned:

- capability state override: `crates/agentdash-application/src/workflow/frame_construction/mod.rs:337-339`
- VFS override: `crates/agentdash-application/src/workflow/frame_construction/mod.rs:340-348`
- MCP servers override: `crates/agentdash-application/src/workflow/frame_construction/mod.rs:349-351`

The envelope itself has `surface: FrameRuntimeSurface` plus operational fields `capability_state`, `vfs`, and `mcp_servers` at `crates/agentdash-application/src/workflow/runtime_launch.rs:80-99`.

This is a transitional shape: `FrameRuntimeSurface` is explicitly from persisted frame surface, while the launch-ready operational fields can be rewritten by construction extras before runtime launch.

### 4. LaunchPlan projects envelope MCP into connector ExecutionContext

`LaunchPlan::build()` clones `input.launch_envelope.mcp_servers`, `vfs`, `executor_config`, and `working_directory` at `crates/agentdash-application/src/session/launch/plan.rs:151-157`.

It writes `ExecutionSessionFrame.mcp_servers` at `crates/agentdash-application/src/session/launch/plan.rs:256-262`.

The SPI connector contract still names that field as session frame MCP servers:

- `ExecutionSessionFrame.mcp_servers: Vec<RuntimeMcpServerDeclaration>` is documented in `.trellis/spec/backend/session/execution-context-frames.md`.
- The code field appears in `crates/agentdash-spi/src/connector/mod.rs:74`.

### 5. Turn preparation reads capability MCP for tool assembly

`TurnPreparer` calls `build_tools_for_execution_context(&session_id, &context, &capability_state.tool.mcp_servers)` at `crates/agentdash-application/src/session/launch/preparation.rs:97-105`.

This is a second read path beside `ExecutionSessionFrame.mcp_servers`. It relies on the invariant that envelope MCP and `CapabilityState.tool.mcp_servers` have already been synchronized.

### 6. Runtime gateway reads current runtime MCP through RuntimeSessionMcpAccess

`RuntimeGateway` session MCP actions require `RuntimeContext::Session` and get the raw `session_id` from the invocation context:

- `mcp.list_tools`: `crates/agentdash-application/src/runtime_gateway/session_actions.rs:130-141`
- `mcp.call_tool`: `crates/agentdash-application/src/runtime_gateway/session_actions.rs:196-210`

The access trait is implemented by `SessionCapabilityService`, which delegates to `SessionRuntimeInner::discover_runtime_mcp_tool_entries()` at `crates/agentdash-application/src/session/capability_service.rs:264-309`.

`discover_runtime_mcp_tool_entries()` reads `get_latest_capability_state(session_id)` and passes `capability_state.tool.mcp_servers.clone()` to MCP discovery at `crates/agentdash-application/src/session/hub/tool_builder.rs:311-334`.

This runtime gateway boundary is still rightly session/runtime-session oriented: it operates on the currently deliverable runtime session, not on AgentRun authoring APIs.

### 7. Direct and relay MCP consume RuntimeMcpServerDeclaration as resolved declarations

Direct MCP discovery iterates `servers: &[RuntimeMcpServerDeclaration]`, parses HTTP declarations, preserves resolved headers, and gates tools through `capability_state.is_capability_tool_enabled()` at `crates/agentdash-executor/src/mcp/direct.rs:207-253` and `crates/agentdash-executor/src/mcp/direct.rs:324-333`.

Relay MCP receives resolved `RuntimeMcpServerDeclaration` declarations and converts them to relay wire declarations:

- list tools: `crates/agentdash-api/src/relay/mcp_relay_impl.rs:20-44`
- call tool: `crates/agentdash-api/src/relay/mcp_relay_impl.rs:93-117`

Relay prompt serialization also projects `RuntimeMcpServerDeclaration` into prompt payload JSON with resolved HTTP/SSE headers and stdio cwd/env at `crates/agentdash-application/src/relay_connector.rs:389-420`.

### 8. AgentRun workspace reads frame runtime surface, while VFS surface still uses session_runtime source

Backend routes:

- `GET /projects/{project_id}/agent-runs`
- `GET /agent-runs/{run_id}/agents/{agent_id}/workspace`
- AgentRun commands under `/agent-runs/{run_id}/agents/{agent_id}/...`

These are registered in `crates/agentdash-api/src/routes/lifecycle_agents.rs:44-73`.

`build_agent_run_workspace_view()`:

- Resolves delivery runtime session from `list_by_agent`.
- Loads session meta when there is a delivery runtime session.
- Reads `RuntimeSessionExecutionAnchor` to get launch frame fallback.
- Reads `agent_frame_repo.get_current(agent.id)` or launch frame.
- Projects `AgentFrameRuntimeView` from the frame.

Evidence: `crates/agentdash-api/src/routes/lifecycle_agents.rs:401-438`.

`agent_frame_runtime_to_view()` returns raw JSON surfaces including `mcp_surface` at `crates/agentdash-api/src/routes/lifecycle_views.rs:344-359`.

Frontend `useAgentRunWorkspaceState()` fetches the AgentRun workspace view, stores `workspace.frame_runtime`, and if there is a `delivery_runtime_ref`, resolves a VFS surface using `{ source_type: "session_runtime", session_id }` at `packages/app-web/src/features/workspace-panel/model/useAgentRunWorkspaceState.ts:85-118`.

That VFS route maps `session_runtime` back to AgentFrame VFS through:

- VFS source enum: `crates/agentdash-application/src/vfs/surface.rs:20-34`
- surface ref: `crates/agentdash-application/src/vfs/surface.rs:48-63`
- API resolver branch: `crates/agentdash-api/src/routes/vfs_surfaces/resolver.rs:201-207`
- session to frame VFS lookup: `crates/agentdash-api/src/session_construction.rs:21-95`

So AgentRun workspace is AgentRun-first at route and DTO level, but runtime VFS browsing still names the source as `session_runtime` and uses runtime session id as lookup key.

## Current Synchronization Responsibilities

Current synchronization is spread across three layers.

### Pre-launch construction synchronization

Responsible code:

- `SessionRequestAssembler`
- `CapabilityResolver`
- `normalize_owner_bootstrap_mcp_projection()`
- `AgentFrameBuilder`
- `FrameConstructionService`

Responsibilities:

- Build final VFS.
- Resolve runtime-bound MCP presets with final VFS context.
- Merge request MCP, capability MCP, and agent-level MCP.
- Write the merged list to both `CapabilityState.tool.mcp_servers` and frame MCP surface.
- Build `FrameLaunchEnvelope` operational fields from frame surface plus construction extras.

Key evidence:

- `crates/agentdash-application/src/session/assembler.rs:821-827`
- `crates/agentdash-application/src/session/assembler.rs:1516-1539`
- `crates/agentdash-application/src/workflow/frame_builder.rs:120-148`
- `crates/agentdash-application/src/workflow/frame_construction/mod.rs:302-395`

### Accepted-launch synchronization

Responsible code:

- `TurnCommitter::commit_accepted_agent_frame()`

Responsibilities:

- Treat accepted capability state as the committed surface.
- Persist frame revision after connector accept.
- Move `LifecycleAgent.current_frame_id` to the accepted revision.

Key evidence:

- `crates/agentdash-application/src/session/launch/commit.rs:142-185`
- `crates/agentdash-application/src/session/launch/commit.rs:188-223`

### Live runtime synchronization

Responsible code:

- `SessionCapabilityService`
- `SessionRuntimeInner::replace_current_capability_state()`
- runtime context transition appliers

Responsibilities:

- Convert delivery runtime session to `AgentFrameRuntimeTarget` at adapter boundaries.
- Persist a new AgentFrame revision for live capability/VFS/MCP change.
- Rebuild tools.
- Update connector tool set.
- Update runtime in-memory `session_profile`, active `TurnExecution.session_frame.mcp_servers`, active VFS, and active capability state.

Key evidence:

- `crates/agentdash-application/src/session/capability_service.rs:46-74`
- `crates/agentdash-application/src/session/hub/tool_builder.rs:111-263`

### RuntimeSessionExecutionAnchor synchronization

`RuntimeSessionExecutionAnchor` is synchronized at dispatch/orchestration creation time, not during surface changes:

- Domain semantics: `crates/agentdash-domain/src/workflow/runtime_session_anchor.rs:20-27`
- Graphless dispatch creation: `crates/agentdash-application/src/workflow/dispatch_service.rs:487-493`
- Orchestration dispatch creation: `crates/agentdash-application/src/workflow/dispatch_service.rs:392-401`
- Agent node launcher creation: `crates/agentdash-application/src/workflow/orchestration/agent_node_launcher.rs:127-136`

Its `launch_frame_id` remains launch evidence. Consumers that need the latest executable surface load `LifecycleAgent.current_frame_id` first and fall back to `launch_frame_id`.

## Direct Answers

### Who writes the surface today?

The durable frame surface is written by:

- `AgentFrameBuilder` during frame construction and frame revision creation.
- `TurnCommitter::commit_accepted_agent_frame()` after connector accept.
- `SessionRuntimeInner::replace_current_capability_state()` during live runtime context transitions.

The runtime MCP declaration values are produced by:

- `resolve_preset_mcp_declaration()` for preset-backed MCP declarations.
- `CapabilityResolver` for `mcp:<preset>` and `mcp:<agent-inline>` directives.
- `normalize_owner_bootstrap_mcp_projection()` for request/capability/agent-level MCP merging.

The database persistence itself is through `AgentFrameRepository`, with PostgreSQL columns including `effective_capability_json`, `vfs_surface_json`, and `mcp_surface_json` in `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs`.

### Who reads the surface today?

The main readers are:

- `FrameConstructionService` and `build_envelope_from_frame()` for runtime launch.
- `LaunchPlan::build()` for connector `ExecutionSessionFrame`.
- `TurnPreparer` and MCP discovery for tool assembly.
- `RuntimeGateway` session MCP providers through `RuntimeSessionMcpAccess`.
- AgentRun workspace APIs through `AgentFrameRuntimeView`.
- VFS surface APIs through `session_runtime -> RuntimeSessionExecutionAnchor -> current AgentFrame -> typed_vfs`.
- Direct/relay/local MCP clients through `RuntimeMcpServerDeclaration` declarations.

### Who is responsible for synchronization today?

There is no single narrow synchronizer yet. Synchronization is maintained by a chain:

1. `normalize_owner_bootstrap_mcp_projection()` aligns request MCP, capability MCP, agent preset MCP, and `CapabilityState.tool.mcp_servers`.
2. `AgentFrameBuilder` writes capability/VFS/MCP into frame JSON surfaces.
3. `build_envelope_from_frame()` reads frame surfaces and applies construction extras to build the launch-ready runtime fields.
4. `TurnCommitter` re-serializes accepted capability state back into AgentFrame surface and updates `current_frame_id`.
5. `replace_current_capability_state()` writes live changes to a new frame revision and synchronizes active runtime caches and connector tools.

This confirms the design note: AgentFrame is already the intended durable target, but current execution still relies on multiple synchronized projections: `CapabilityState.tool.mcp_servers`, `FrameLaunchEnvelope.mcp_servers`, `ExecutionSessionFrame.mcp_servers`, active turn capability state, and `AgentFrame.mcp_surface_json`.

## Naming Boundary Assessment

### Session names that still look reasonable

`RuntimeSessionExecutionAnchor` remains reasonable. It describes runtime trace/delivery evidence and the lookup index from runtime session to lifecycle run / agent / launch frame. Its domain docs explicitly say it is launch evidence and not overwritten by later frame revisions.

`RuntimeSessionMcpAccess` remains reasonable for the runtime gateway boundary. `mcp.list_tools` and `mcp.call_tool` are session-runtime actions over the currently deliverable runtime session. The provider must resolve the active/latest runtime capability projection and call the live runtime MCP discovery layer.

`RuntimeSessionRefDto`, `delivery_runtime_session_id`, and `session_runtime` are reasonable when the API surface is explicitly about runtime trace, delivery, active turn, VFS access via a running/attached runtime session, or session detail pages.

`ExecutionSessionFrame` remains reasonable as connector-facing SPI vocabulary. It is the stable frame sent to a connector for one prompt execution and does not imply business ownership.

### Canonical MCP declaration names

`RuntimeMcpServerDeclaration` is the canonical runtime-resolved MCP server declaration. The type has no runtime-session ownership field and is written into AgentFrame MCP surface, capability state, connector frames, direct MCP, relay MCP, and local relay.

`McpRuntimeBindingContext` carries final VFS facts for MCP runtime binding. Its role is resolver context, not runtime-session state.

Helper names such as `partition_runtime_mcp_declarations`, `mcp_declaration_to_runtime_server`, `runtime_server_to_mcp_declaration`, `mcp_declaration_to_relay_prompt_value`, and `normalize_runtime_mcp_declarations` follow declaration / adapter / relay boundary semantics.

`SessionRuntimeControlPlaneView` and `SessionRuntimeActionSetView` are reasonable for `/sessions/{id}/runtime-control`, but their embedding in `AgentRunWorkspaceView` is a mixed model. The AgentRun workspace route currently uses AgentRun identity, but control/action DTO names still speak in SessionRuntime vocabulary.

`session_runtime` VFS source is reasonable as a trace/delivery lookup source, but in the AgentRun workspace it keeps the user-side VFS surface lookup keyed by runtime session id. A future AgentRun/AgentFrame source would better express the target model where frame revision is the executable surface.

`RuntimeMcpServer` is not a Session residue, but it is not the canonical declaration either. It is a useful runtime/context summary shape today. Because it drops or normalizes fields that `RuntimeMcpServerDeclaration` preserves, code should treat it as an adapter/read-model shape rather than executable declaration truth.

## Current End-to-End Surface Flow

Current launch path:

```text
LaunchCommand / AgentRun command
  -> RuntimeSessionExecutionAnchor lookup
  -> current AgentFrame or launch frame
  -> final VFS
  -> McpRuntimeBindingContext(final VFS)
  -> CapabilityResolver + resolve_preset_mcp_declaration
  -> normalize_owner_bootstrap_mcp_projection
  -> CapabilityState.tool.mcp_servers
  -> AgentFrameBuilder writes AgentFrame.mcp_surface_json
  -> FrameLaunchEnvelope.mcp_servers
  -> LaunchPlan.ExecutionSessionFrame.mcp_servers
  -> TurnPreparer builds tools from CapabilityState.tool.mcp_servers
  -> connector prompt
  -> accepted AgentFrame revision commit
```

Current AgentRun workspace read path:

```text
/agent-runs/{run_id}/agents/{agent_id}/workspace
  -> LifecycleAgent
  -> latest delivery RuntimeSessionExecutionAnchor
  -> current AgentFrame or launch frame
  -> AgentFrameRuntimeView(mcp_surface, vfs_surface, capability_surface)
  -> frontend useAgentRunWorkspaceState
  -> if delivery_runtime_ref exists:
       resolveVfsSurface(source_type=session_runtime, session_id)
       -> RuntimeSessionExecutionAnchor
       -> current AgentFrame or launch frame
       -> typed_vfs
```

Current runtime gateway MCP path:

```text
RuntimeGateway Session action
  -> RuntimeSessionMcpAccess
  -> SessionCapabilityService
  -> SessionRuntimeInner.get_latest_capability_state
  -> capability_state.tool.mcp_servers
  -> MCP discovery/call
```

## Phase 0 Conclusion

The current code already contains the target direction: AgentFrame persists capability, VFS, MCP, context, and execution profile surface; RuntimeSessionExecutionAnchor is a trace backlink; AgentRun workspace routes are the user-side entry.

The remaining convergence issue is not that RuntimeSession still exists. RuntimeSession names are correct where they describe delivery, active turns, runtime actions, trace, and connector lifecycle. The MCP declaration and binding context vocabulary now matches AgentFrame executable surface semantics; later phases can focus on reducing synchronized MCP surface projections.

For later phases, the important fact-source move is to reduce writes/reads against `CapabilityState.tool.mcp_servers` and `FrameLaunchEnvelope.mcp_servers` as independent projections, and make AgentFrame revision surface the single launch/read source with explicit construction draft handoff.
