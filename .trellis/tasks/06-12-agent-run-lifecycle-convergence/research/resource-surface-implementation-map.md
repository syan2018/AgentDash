# Research: resource surface implementation map

- Query: Why AgentRun workspace panel cannot see lifecycle mounts today, and how target resource_surface should unify connector launch and frontend panel.
- Scope: internal
- Date: 2026-06-12

## Findings

### Files found

- `.trellis/tasks/06-12-agent-run-lifecycle-convergence/prd.md` - Defines the target AgentRun conversation/workspace snapshot contract and states that resource surface must be projected directly from AgentRun workspace state.
- `.trellis/tasks/06-12-agent-run-lifecycle-convergence/design.md` - Describes the current split between `resolveVfsSurface(session_runtime)` and frame runtime VFS, then targets snapshot-owned `resource_surface`.
- `.trellis/tasks/06-12-agent-run-lifecycle-convergence/implement.md` - Plans Phase 5 Resource Surface Resolver work, frontend hook migration, consistency checks, and grep gates.
- `.trellis/tasks/06-12-agent-run-lifecycle-convergence/research/current-state.md` - Captures the pre-existing evidence that AgentRun workspace currently resolves VFS through delivery runtime session.
- `.trellis/tasks/06-12-agent-run-lifecycle-convergence/implement.jsonl` - Records relevant spec context for backend workflow/session/VFS and frontend state/type contracts.
- `.trellis/tasks/06-12-agent-run-lifecycle-convergence/check.jsonl` - Records check context for the same backend/frontend specs and quality gate.
- `.trellis/spec/backend/workflow/architecture.md` - Establishes `AgentFrame` as the runtime surface revision and `RuntimeSession` as delivery/trace evidence.
- `.trellis/spec/backend/session/runtime-execution-state.md` - Defines AgentRun workspace as run/agent identity projection; runtime session control is trace/runtime detail.
- `.trellis/spec/backend/session/session-startup-pipeline.md` - Defines launch surface, accepted turn, committed frame, and typed frame surfaces.
- `.trellis/spec/backend/vfs/architecture.md` - Defines VFS mount/provider model and the `lifecycle_vfs` provider family.
- `.trellis/spec/frontend/state-management.md` - Requires frontend stores to consume backend DTOs rather than infer protocol facts.
- `.trellis/spec/frontend/type-safety.md` - Requires generated DTOs to be the wire source; current runtime surface text needs narrowing to runtime diagnostics.
- `.trellis/spec/frontend/quality-guidelines.md` - Requires focused tests/typecheck/lint and no local type escapes.
- `.trellis/spec/guides/cross-layer-thinking-guide.md` - Requires cross-layer verification that frontend sees the effective backend runtime/resource surface.
- `crates/agentdash-contracts/src/workflow.rs` - Defines `AgentRunWorkspaceView` and `AgentFrameRuntimeView`.
- `crates/agentdash-contracts/src/vfs.rs` - Defines `ResolvedVfsSurfaceSource`, `ResolvedMountSummary`, and `ResolvedVfsSurface`.
- `crates/agentdash-api/src/session_construction.rs` - Resolves a runtime session anchor back to an agent frame and typed VFS.
- `crates/agentdash-api/src/routes/vfs_surfaces/resolver.rs` - Implements `resolveVfsSurface` source dispatch, including `session_runtime`.
- `crates/agentdash-api/src/routes/lifecycle_agents.rs` - Builds `AgentRunWorkspaceView` from run, agent, delivery runtime refs, and current/anchor frame runtime.
- `crates/agentdash-api/src/routes/lifecycle_views.rs` - Converts `AgentFrame` into `AgentFrameRuntimeView`, including raw `vfs_surface` JSON.
- `crates/agentdash-application/src/workflow/frame_builder.rs` - Builds coherent AgentFrame surface revisions from lifecycle activation drafts.
- `crates/agentdash-application/src/session/launch/commit.rs` - Commits accepted capability state back into pending AgentFrame surface JSON.
- `crates/agentdash-application/src/workflow/frame_construction/composer_project_agent.rs` - Composes ProjectAgent frames and resolves active workflow projection for owner bootstrap.
- `crates/agentdash-application/src/workflow/frame_construction/owner_bootstrap.rs` - Adds active lifecycle mount to ProjectAgent owner bootstrap VFS.
- `crates/agentdash-application/src/workflow/lifecycle/mount.rs` - Creates/replaces the `lifecycle` mount backed by provider `lifecycle_vfs`.
- `crates/agentdash-application/src/workflow/frame_construction/composer_lifecycle_node.rs` - Composes Workflow AgentCall frames from runtime-session orchestration anchors.
- `crates/agentdash-application/src/session/assembler.rs` - Applies lifecycle activation to merge lifecycle VFS overlay into frame assembly.
- `crates/agentdash-application/src/workflow/orchestration/executor_launcher.rs` - Contains coverage that AgentCall frames persist lifecycle VFS mounts.
- `crates/agentdash-application/src/workflow/projection.rs` - Resolves active workflow projection for a runtime session.
- `crates/agentdash-application/src/context/vfs_discovery.rs` - Describes discovered lifecycle VFS provider metadata.
- `crates/agentdash-application/src/vfs/provider_lifecycle.rs` - Implements the `lifecycle_vfs` provider.
- `packages/app-web/src/features/workspace-panel/model/useAgentRunWorkspaceState.ts` - Current AgentRun workspace hook that fetches workspace then resolves VFS from delivery `session_runtime`.
- `packages/app-web/src/features/workspace-panel/model/useSessionRuntimeState.ts` - RuntimeSession diagnostics/detail hook that resolves `session_runtime` VFS.
- `packages/app-web/src/pages/AgentRunWorkspacePage.tsx` - Wires AgentRun workspace state into `WorkspacePanel` runtime data.
- `packages/app-web/src/features/workspace-panel/tab-types/vfs-tab.tsx` - Renders VFS browser from workspace runtime data surface.
- `packages/app-web/src/features/workspace-runtime/model/types.ts` - Defines `WorkspaceRuntimeData.runtimeSurface`.
- `packages/app-web/src/services/vfs.ts` - Frontend service wrapper for VFS surface resolution.
- `packages/app-web/src/features/workspace-panel/model/useAgentRunWorkspaceState.test.ts` - Existing hook tests that still expect `session_runtime` VFS source.
- `packages/app-web/src/features/workspace-panel/model/useSessionRuntimeState.test.ts` - RuntimeSession hook tests that can remain scoped to diagnostics/detail behavior.

### Current call chain

`AgentFrame.vfs_surface_json` is already the backend persistence point for launch/runtime VFS surface revisions. The contract exposes it today only as raw JSON on `AgentFrameRuntimeView.vfs_surface` (`crates/agentdash-contracts/src/workflow.rs:1065`, `crates/agentdash-contracts/src/workflow.rs:1072`). `AgentRunWorkspaceView` includes `frame_runtime`, `delivery_runtime_ref`, and other workspace state, but has no typed browser-ready `resource_surface` field (`crates/agentdash-contracts/src/workflow.rs:866`).

`session_construction::resolve_session_frame_vfs` is the bridge from delivery runtime session back to frame VFS. It validates the runtime session, finds `RuntimeSessionExecutionAnchor`, checks lifecycle/project permission, then loads `agent_frame_repo.get_current(agent.id).await?.or(agent_frame_repo.get(anchor.launch_frame_id).await?)` and returns `frame.typed_vfs()` (`crates/agentdash-api/src/session_construction.rs:24`). This means the resolver is anchored by a runtime session id, but its data ultimately comes from `AgentFrame.vfs_surface_json`.

`resolveVfsSurface(session_runtime)` calls the VFS surface route. The backend source dispatch handles `ResolvedVfsSurfaceSource::SessionRuntime { session_id }` by calling `ensure_session_permission`, then `resolve_session_frame_vfs(...).await?.vfs.unwrap_or_default()`, then summarizing the VFS through `build_surface_summary` (`crates/agentdash-api/src/routes/vfs_surfaces/resolver.rs:201`). The generated/handwritten frontend service simply posts a `ResolvedVfsSurfaceSource` and returns `ResolvedVfsSurface` (`packages/app-web/src/services/vfs.ts:1`).

`useAgentRunWorkspaceState` currently fetches `fetchAgentRunWorkspace(rid, aid)`, reads `workspace.delivery_runtime_ref?.runtime_session_id`, and if present calls `resolveVfsSurface({ source_type: "session_runtime", session_id: runtimeSessionId })`; the result is stored as `runtime_surface` (`packages/app-web/src/features/workspace-panel/model/useAgentRunWorkspaceState.ts:134`, `packages/app-web/src/features/workspace-panel/model/useAgentRunWorkspaceState.ts:136`, `packages/app-web/src/features/workspace-panel/model/useAgentRunWorkspaceState.ts:138`). `AgentRunWorkspacePage` then maps that state to `workspaceRuntimeData.runtimeSurface` and passes it into `WorkspacePanel` (`packages/app-web/src/pages/AgentRunWorkspacePage.tsx:197`, `packages/app-web/src/pages/AgentRunWorkspacePage.tsx:677`, `packages/app-web/src/pages/AgentRunWorkspacePage.tsx:699`). The VFS tab renders whatever surface is in workspace data (`packages/app-web/src/features/workspace-panel/tab-types/vfs-tab.tsx:1`).

`useSessionRuntimeState` also calls `resolveVfsSurface({ source_type: "session_runtime", session_id: sid })`, but that hook represents RuntimeSession detail/diagnostic state rather than AgentRun workspace state (`packages/app-web/src/features/workspace-panel/model/useSessionRuntimeState.ts:97`). This hook can keep using the runtime-session resolver after the AgentRun panel is moved to snapshot-owned resource surface.

### Why the AgentRun workspace panel misses lifecycle mounts

The lifecycle mount may exist in the effective frame path, but the AgentRun workspace panel does not consume a snapshot-owned resource projection. Instead, it derives a VFS surface from `delivery_runtime_ref.runtime_session_id`. That makes the panel depend on a support lookup path rather than the AgentRun workspace snapshot.

There are two fragile points in the current route. First, `AgentRunWorkspaceView` exposes raw `frame_runtime.vfs_surface` but not a typed `ResolvedVfsSurface`/resource browser contract (`crates/agentdash-api/src/routes/lifecycle_agents.rs:725`, `crates/agentdash-api/src/routes/lifecycle_views.rs:344`). Second, `resolve_session_frame_vfs` currently chooses current agent frame before the anchor launch frame. That is useful for some "current surface" reads, but ambiguous as a delivery-runtime truth source when the runtime session should bind to the accepted/delivery frame surface (`crates/agentdash-api/src/session_construction.rs:24`).

`session/launch/commit` is another important convergence point. Pending frame commit rewrites `pending_frame.vfs_surface_json` from `prepared.accepted_capability_state` (`crates/agentdash-application/src/session/launch/commit.rs:157`). If the accepted capability state does not contain the lifecycle VFS overlay, the persisted accepted frame loses the mount even if an earlier draft or active workflow projection had it. The target resolver should validate active workflow projection, accepted/persisted `AgentFrame.vfs_surface_json`, and optional session-runtime support resolution instead of allowing the frontend to infer from `session_runtime`.

### Lifecycle mount entry paths into target snapshot resource_surface

ProjectAgent explicit lifecycle should enter `resource_surface` through owner/bootstrap frame surface composition. ProjectAgent frame construction resolves active workflow projection for the session in `composer_project_agent` and passes it to owner bootstrap (`crates/agentdash-application/src/workflow/frame_construction/composer_project_agent.rs:77`). Owner bootstrap then calls `ensure_active_workflow_lifecycle_mount(vfs, active_workflow)` before projecting companion/lifecycle skill assets (`crates/agentdash-application/src/workflow/frame_construction/owner_bootstrap.rs:367`). The mount helper creates or replaces mount id `lifecycle` with provider `lifecycle_vfs`, scoped by run/orchestration/node/attempt/lifecycle key (`crates/agentdash-application/src/workflow/lifecycle/mount.rs:76`). The resulting VFS is part of the frame surface draft and should be persisted as `AgentFrame.vfs_surface_json`.

Workflow AgentCall lifecycle should enter `resource_surface` through node-scoped lifecycle activation. The lifecycle node composer reads runtime-session orchestration context, plan node, lifecycle identity, base frame VFS, and inherited executor settings, then composes lifecycle-node frame assembly (`crates/agentdash-application/src/workflow/frame_construction/composer_lifecycle_node.rs:1`). `compose_lifecycle_node_with_audit` calls activity activation with run/orchestration/node/attempt/lifecycle metadata and base VFS, then `SessionAssemblyBuilder.apply_lifecycle_activation` merges lifecycle activation into the frame assembly (`crates/agentdash-application/src/session/assembler.rs:274`). `build_lifecycle_activation_surface` applies `compose_vfs_with_overlay_and_directives(base_vfs, activation.lifecycle_vfs, activation.mount_directives)` and stores the result in capability state (`crates/agentdash-application/src/workflow/frame_builder.rs:54`). Existing tests assert that the final mount ids include `workspace` and `lifecycle` (`crates/agentdash-application/src/workflow/frame_builder.rs:544`) and that AgentCall frames persist provider `lifecycle_vfs` (`crates/agentdash-application/src/workflow/orchestration/executor_launcher.rs:1346`).

The target AgentRun snapshot should project both paths in the same way: load the authoritative frame for the run/agent workspace, read its typed VFS from `AgentFrame.vfs_surface_json`, summarize it into `ResolvedVfsSurface`, and attach it as `resource_surface`. For ProjectAgent explicit lifecycle, the resolver should check active workflow projection and confirm the lifecycle mount exists when a lifecycle run/node is active. For Workflow AgentCall, it should confirm the node-scoped lifecycle mount uses provider `lifecycle_vfs` and a root ref matching run/orchestration/node scope.

### Contract and backend resolver adjustments

Add a snapshot-owned resource field to the AgentRun workspace/conversation contract. The PRD names `AgentConversationSnapshot` or equivalent; the current implementation surface can add `resource_surface: Option<ResolvedVfsSurface>` to `AgentRunWorkspaceView`, or add a narrower `ConversationResourceSurfaceView` that contains `surface: ResolvedVfsSurface`, `frame_ref`, `source_ref`, and diagnostics. The key invariant is that the AgentRun workspace panel consumes this field directly, not a runtime-session query.

Add a non-session source identity for AgentRun/frame resource surfaces. Viable shapes are `ResolvedVfsSurfaceSource::AgentRun { run_id, agent_id }`, `ResolvedVfsSurfaceSource::AgentFrame { frame_id }`, or an internal snapshot-only source ref such as `agent-run:{run_id}:{agent_id}:frame:{frame_id}`. The source must not be `session_runtime` for AgentRun workspace, because connector delivery runtime is trace evidence, not the business/resource owner.

Add an AgentRun resource surface resolver in the backend workspace path. It should live close to `build_agent_run_workspace_view` or as a dedicated application/API service called by it. Inputs should include run, agent, selected frame, optional delivery runtime anchor, and active workflow projection. Output should be a `ResolvedVfsSurface` built with the same summary code used by VFS surface routes, plus diagnostics when the active workflow projection expects lifecycle VFS but the persisted frame surface does not contain it.

Fix or narrow `resolve_session_frame_vfs` frame selection as a support resolver. The implementation plan calls out binding delivery runtime session to the delivery/accepted frame surface. That can mean preferring `anchor.launch_frame_id` for trace-bound runtime detail, or explicitly documenting/testing when "current agent frame" is desired. Either way, AgentRun workspace should not depend on this selection strategy.

Add consistency tests around three projections: active workflow projection, final persisted `AgentFrame.vfs_surface_json`, and optional `resolveVfsSurface(session_runtime)` support resolution. When an active workflow exists but `lifecycle_vfs` is absent from the snapshot resource surface, the snapshot should carry a resource diagnostic rather than silently presenting an empty/non-lifecycle panel.

### Frontend hook and panel adjustments

`useAgentRunWorkspaceState` should stop importing/calling `resolveVfsSurface` for AgentRun workspaces. It should fetch the workspace snapshot and store `workspace.resource_surface` as the panel surface. During a small migration, `WorkspaceRuntimeData.runtimeSurface` can still carry this snapshot value to avoid changing every panel component at once; the name should later converge to `resourceSurface` to remove the RuntimeSession mental model.

`AgentRunWorkspacePage` should pass the snapshot-owned surface into `WorkspacePanel`. The page should not derive a VFS surface from `delivery_runtime_ref.runtime_session_id`, and no workspace panel tab should use delivery runtime refs to infer AgentRun resources.

`useSessionRuntimeState` can keep `resolveVfsSurface({ source_type: "session_runtime" })` because it is a RuntimeSession detail/diagnostic hook. That route is still useful for trace inspection, runtime-control pages, connector support tooling, and cross-check diagnostics during the migration.

### Tests to add or adjust

- Contract/generation test: `AgentRunWorkspaceView` or the conversation snapshot DTO includes `resource_surface`, generated TypeScript exposes the same snake_case field, and no local frontend type override is used.
- Backend ProjectAgent graphless owner test: snapshot `resource_surface` includes normal owner/project mounts and no lifecycle-missing diagnostic.
- Backend ProjectAgent explicit lifecycle test: snapshot `resource_surface.mounts` includes mount id `lifecycle`, provider `lifecycle_vfs`, and owner mounts remain visible.
- Backend Workflow AgentCall test: snapshot `resource_surface.mounts` includes node-scoped `lifecycle_vfs` mount whose root ref includes run/orchestration/node identity and expected writable-port metadata when applicable.
- Backend support resolver test: `resolve_session_frame_vfs` remains valid for SessionRuntime detail and follows the selected delivery/accepted frame policy.
- Backend consistency diagnostic test: active workflow projection plus missing persisted lifecycle mount yields a resource diagnostic in the AgentRun snapshot.
- Frontend hook test: `useAgentRunWorkspaceState` consumes `workspace.resource_surface`, does not call `resolveVfsSurface`, and preserves snapshot surface across refresh/error states.
- Frontend panel test: VFS tab/resource browser renders lifecycle mount from AgentRun workspace snapshot.
- Frontend RuntimeSession test: `useSessionRuntimeState` continues to resolve `session_runtime` VFS for diagnostics/detail only.

### Grep and audit gates

Primary negative gate for AgentRun workspace:

```powershell
rg -n "source_type: \"session_runtime\"" packages/app-web/src/features/workspace-panel/model/useAgentRunWorkspaceState.ts packages/app-web/src/pages/AgentRunWorkspacePage.tsx packages/app-web/src/features/workspace-panel/tab-types
```

Expected result: no matches.

Broader negative gate for delivery-runtime-derived AgentRun surface:

```powershell
rg -n "delivery_runtime_ref.*resolveVfsSurface|runtime_session_id.*resolveVfsSurface|session_runtime.*AgentRun" packages/app-web/src/features/workspace-panel packages/app-web/src/pages
```

Expected result: no matches in AgentRun workspace code. Matches are acceptable only in files that are explicitly RuntimeSession diagnostics/detail.

Existing implementation-plan gate:

```powershell
rg -n "resolveVfsSurface\\(\\{ source_type: \"session_runtime\"" packages/app-web/src/features/workspace-panel packages/app-web/src/pages
```

Expected result after migration: `useAgentRunWorkspaceState` and `AgentRunWorkspacePage` are clean. `useSessionRuntimeState` may still match if the audit scope includes RuntimeSession diagnostics.

Positive contract/resolver gate:

```powershell
rg -n "resource_surface" crates/agentdash-contracts/src packages/app-web/src/generated crates/agentdash-api/src/routes/lifecycle_agents.rs packages/app-web/src/features/workspace-panel/model/useAgentRunWorkspaceState.ts
```

Expected result: generated backend/frontend contracts, AgentRun workspace builder, and AgentRun hook all reference `resource_surface`.

Stale test gate:

```powershell
rg -n "source: \\{ source_type: \"session_runtime\"" packages/app-web/src/features/workspace-panel/model/useAgentRunWorkspaceState.test.ts
```

Expected result: no matches; AgentRun hook tests should assert snapshot-owned `resource_surface`.

### Related specs

- `.trellis/spec/backend/workflow/architecture.md` states that `AgentFrame` is the runtime surface revision, while `RuntimeSession` is connector delivery/trace evidence. This supports making `AgentFrame.vfs_surface_json` the backend truth source for AgentRun `resource_surface`.
- `.trellis/spec/backend/session/runtime-execution-state.md` states that AgentRun workspace derives shell/action state from AgentRun workspace projection, while RuntimeSession control view is for runtime detail.
- `.trellis/spec/backend/session/session-startup-pipeline.md` states that frame construction writes launch-ready typed surfaces and committed turns persist accepted frame surfaces.
- `.trellis/spec/backend/vfs/architecture.md` defines lifecycle VFS as a provider-backed mount in the unified VFS model.
- `.trellis/spec/frontend/state-management.md` requires frontend stores to consume backend DTOs and not infer protocol facts.
- `.trellis/spec/frontend/type-safety.md` requires generated DTOs as the wire source. Its current `runtime_surface` wording should be interpreted as RuntimeSession detail behavior until the spec is updated for AgentRun `resource_surface`.
- `.trellis/spec/guides/cross-layer-thinking-guide.md` requires checking that frontend-visible state is the effective backend surface, not a derived approximation.

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` reported no active task, so the research used the user-provided active task path as the write boundary.
- No external references were required; this is an internal architecture/contract convergence topic.
- The exact final DTO name is still a design choice: adding `resource_surface: ResolvedVfsSurface` directly to `AgentRunWorkspaceView` is the narrow implementation path, while a future `AgentConversationSnapshot`/`ConversationResourceSurfaceView` can wrap the same surface with diagnostics and frame/source refs.
- Current frontend type-safety spec text still describes `runtime_surface: ResolvedVfsSurface` as session workspace panel input. That is now a spec tension for AgentRun workspace and should be updated by the spec-update workflow, not in this research task.
- Line numbers cited above are based on the inspected working tree at research time and may shift as implementation proceeds.
