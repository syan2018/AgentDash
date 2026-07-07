# Research: AgentRun list invalidation for SubAgent lineage/status changes

- Query: AgentRun list invalidation for SubAgent lineage/status/title/activity changes; locate backend list read model, lineage/status write points, current frontend refresh path, and proposed project-scoped invalidation.
- Scope: internal
- Date: 2026-07-07

## Findings

### 1. Summary recommendation

AgentRun list should refresh from a project-scoped backend projection invalidation, not from manual refresh and not from an AgentRun workspace page side effect. The production contract should be `ControlPlaneProjectionChanged { projection: AgentRunList, ... }` or a project-stream envelope that carries the same typed payload plus `project_id`.

Recommended backend production points:

- Same-run SubAgent/companion child lineage: emit after `LifecycleRelationWriter::write_for_dispatch` creates `AgentLineage` for `AgentPolicy::SpawnChild`.
- Cross-run AgentRun fork/new root: emit after fork materialization commits child run/agent/frame/anchor/delivery binding/lineage.
- Delivery shell state: emit after current `AgentRunDeliveryBinding` is upserted to `Running` and after terminal transition upserts `Terminal`.
- Workspace title shell state: emit after `WorkspaceTitlePort` updates `LifecycleAgent.workspace_title`.
- Root list ordering/activity: emit when `LifecycleRun.last_activity_at` changes, because root rows use run-level activity as the keyset/shell activity source.

Recommended frontend consumption:

- Extend the project event stream/store path so `agent-run-list-state-store.ts` can consume a project-scoped projection invalidation even when no `/agent-runs/:runId/:agentId` workspace page is mounted.
- Keep `controlPlaneModel.ts` workspace-local handling as a local optimization for open workspaces, but do not make it the correctness path for the global AgentRun list.

If `ControlPlaneProjectionChangeReason` remains unchanged, creation/lineage has no precise reason. Because the project is pre-release, add typed reasons such as `agent_run_lineage_changed`, `agent_run_shell_changed`, and possibly `agent_run_activity_changed`, then regenerate Backbone TS bindings. Reusing `mailbox_state_changed` or `companion_result` for lineage creation would blur the fact boundary.

### 2. Backend list read model and lineage/status write points with file:line anchors

Files found:

- `crates/agentdash-agent-protocol/src/backbone/platform.rs` - defines `PlatformEvent::ControlPlaneProjectionChanged`, `ControlPlaneProjection::AgentRunList`, and current change reasons.
- `crates/agentdash-api/src/routes/lifecycle_agents.rs` - owns `GET /projects/{project_id}/agent-runs` and maps application list items to contract DTOs.
- `crates/agentdash-application-agentrun/src/agent_run/workspace/query.rs` - resolves lightweight list shell/read model for one run+agent.
- `crates/agentdash-application-lifecycle/src/lifecycle/dispatch/lifecycle_relation_writer.rs` - creates same-run `AgentLineage` for spawned child agents and opens optional companion gate.
- `crates/agentdash-application/src/companion/dispatch.rs` - companion `target=sub` child dispatch path that calls lifecycle dispatch with `AgentPolicy::SpawnChild`.
- `crates/agentdash-application/src/companion/tools.rs` - `companion_request` calls child dispatch, then writes the child launch prompt through child mailbox intake.
- `crates/agentdash-application-agentrun/src/agent_run/delivery_state.rs` - persists delivery terminal state onto current `AgentRunDeliveryBinding`.
- `crates/agentdash-application-agentrun/src/agent_run/frame/launch_commit.rs` - persists delivery running state when a launch is accepted.
- `crates/agentdash-application-agentrun/src/agent_run/workspace_title.rs` - maps runtime session title updates back to `LifecycleAgent.workspace_title`.
- `crates/agentdash-application-runtime-session/src/session/eventing.rs` - handles source session title updates and currently emits `SessionMetaUpdate`, not AgentRun list invalidation.
- `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_lineage_repository.rs` - materializes cross-run AgentRun fork child run/agent/frame/anchor/delivery binding/lineage in one transaction.

Protocol and current backend event production:

- `PlatformEvent::ControlPlaneProjectionChanged` exists as a typed platform event at `crates/agentdash-agent-protocol/src/backbone/platform.rs:36`.
- The payload carries `projection`, `reason`, `run_id`, `agent_id`, optional `frame_id`, `gate_id`, `mailbox_message_id`, and `delivery_runtime_session_id` at `crates/agentdash-agent-protocol/src/backbone/platform.rs:55`.
- `ControlPlaneProjection::AgentRunList` already exists at `crates/agentdash-agent-protocol/src/backbone/platform.rs:76`.
- Current reasons include mailbox, wait, delivery terminal, companion result, title changed, etc., but not lineage/list shell/activity changed at `crates/agentdash-agent-protocol/src/backbone/platform.rs:87`.
- The only production-like control-plane emission found in AgentRun runtime mailbox code emits `projection: Mailbox`, not `AgentRunList`, at `crates/agentdash-application-agentrun/src/agent_run/mailbox_runtime_adapter.rs:221` and `crates/agentdash-application-agentrun/src/agent_run/mailbox_runtime_adapter.rs:237`.

AgentRun list read model:

- `get_project_agent_runs` is the list endpoint at `crates/agentdash-api/src/routes/lifecycle_agents.rs:190`.
- It loads all project lifecycle runs and sorts by `LifecycleRun.last_activity_at desc` with `run.id` tie-break at `crates/agentdash-api/src/routes/lifecycle_agents.rs:214` and `crates/agentdash-api/src/routes/lifecycle_agents.rs:221`.
- It pages by run-level cursor at `crates/agentdash-api/src/routes/lifecycle_agents.rs:227`.
- For each page run, it loads all agents at `crates/agentdash-api/src/routes/lifecycle_agents.rs:236`.
- It loads the same-run `AgentLineage` control tree at `crates/agentdash-api/src/routes/lifecycle_agents.rs:248`.
- It builds `children_map`/`child_ids` from `AgentLineage` at `crates/agentdash-api/src/routes/lifecycle_agents.rs:1671`.
- Root entries are agents that are not lineage children, and their `subagent_count` comes from descendants at `crates/agentdash-api/src/routes/lifecycle_agents.rs:256`.
- Inline child rows are recursively resolved through `build_inline_children`; each child calls the same lightweight list item resolver and is sorted by child shell activity at `crates/agentdash-api/src/routes/lifecycle_agents.rs:337` and `crates/agentdash-api/src/routes/lifecycle_agents.rs:375`.
- The API calls application `resolve_list_item` at `crates/agentdash-api/src/routes/lifecycle_agents.rs:1659`.
- `resolve_list_item` reads current delivery selection, optional session meta, execution state, project agent label, and subject association at `crates/agentdash-application-agentrun/src/agent_run/workspace/query.rs:284`.
- The list shell is built at `crates/agentdash-application-agentrun/src/agent_run/workspace/query.rs:305`.
- The shell title reads `LifecycleAgent.workspace_title/workspace_title_source`, falling back to project-agent name or agent id at `crates/agentdash-application-agentrun/src/agent_run/workspace/query.rs:520`.
- The shell delivery status comes from workspace state at `crates/agentdash-application-agentrun/src/agent_run/workspace/query.rs:545`.
- Child shell activity uses `LifecycleAgent.updated_at` at `crates/agentdash-application-agentrun/src/agent_run/workspace/query.rs:547`.
- Root list shell activity is overwritten to `LifecycleRun.last_activity_at`, matching server pagination, at `crates/agentdash-api/src/routes/lifecycle_agents.rs:1698` and `crates/agentdash-api/src/routes/lifecycle_agents.rs:1704`.

Same-run SubAgent / companion child lineage creation:

- Companion `target=sub` dispatch calls `CompanionChildDispatchService::dispatch_child` at `crates/agentdash-application/src/companion/tools.rs:979`.
- For `wait=true`, dispatch opens an interaction gate and uses `parent_run_id`, `parent_agent_id`, and `RuntimePolicy::CreateRuntimeSession` at `crates/agentdash-application/src/companion/dispatch.rs:64`.
- For async dispatch, it launches an agent with `parent_run_id`, `parent_agent_id`, `RunPolicy::AppendGraph`, and `AgentPolicy::SpawnChild` at `crates/agentdash-application/src/companion/dispatch.rs:110`.
- Dispatch common/plain paths call `LifecycleRelationWriter::write_for_dispatch` after materializing runtime refs at `crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs:399` and `crates/agentdash-application-lifecycle/src/lifecycle/dispatch_service.rs:436`.
- `LifecycleRelationWriter::write_for_dispatch` creates `AgentLineage` whenever `plan.parent_agent_id` exists at `crates/agentdash-application-lifecycle/src/lifecycle/dispatch/lifecycle_relation_writer.rs:40`.
- `AgentLineage::new` records `run_id`, optional parent agent id, child agent id, relation kind, source frame id, and metadata at `crates/agentdash-domain/src/workflow/agent_lineage.rs:23`.
- The Postgres `AgentLineageRepository::create` inserts into `agent_lineages` at `crates/agentdash-infrastructure/src/persistence/postgres/lifecycle_anchor_repository.rs:787`.
- The list read model consumes `AgentLineage`, not `AgentRunLineage`, for inline SubAgent children at `crates/agentdash-api/src/routes/lifecycle_agents.rs:248`.

Cross-run fork/new-root materialization:

- `AgentRunForkService` delegates materialization at `crates/agentdash-application-agentrun/src/agent_run/fork.rs:271`.
- Postgres fork materialization creates a child `LifecycleRun`, child root `LifecycleAgent`, child frame, execution anchor, delivery binding, and `AgentRunLineage` at `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_lineage_repository.rs:110`.
- The delivery binding is initialized from the new child anchor at `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_lineage_repository.rs:154`.
- The cross-run `AgentRunLineage::new_fork` is created at `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_lineage_repository.rs:157`.
- The transaction inserts child run, child agent, frame, anchor, delivery binding, and lineage at `crates/agentdash-infrastructure/src/persistence/postgres/agent_run_lineage_repository.rs:190`.
- This affects the AgentRun list because a new `LifecycleRun` becomes a new root list row sorted by `last_activity_at`, even though inline same-run children use `AgentLineage`.

Delivery terminal/title/activity write points:

- `AgentRunDeliveryBinding` owns `status`, `active_turn_id`, `last_turn_id`, `terminal_state`, `terminal_message`, `observed_at`, and `updated_at` at `crates/agentdash-domain/src/workflow/agent_run_delivery_binding.rs:67`.
- `mark_running` sets status `Running`, active turn, last turn, clears terminal fields, and updates timestamp at `crates/agentdash-domain/src/workflow/agent_run_delivery_binding.rs:115`.
- Launch commit persists a running current delivery binding after runtime turn acceptance at `crates/agentdash-application-agentrun/src/agent_run/frame/launch_commit.rs:349` and `crates/agentdash-application-agentrun/src/agent_run/frame/launch_commit.rs:355`.
- `mark_terminal_from_runtime_session` resolves session -> anchor, verifies current binding, marks terminal, and upserts the binding at `crates/agentdash-application-agentrun/src/agent_run/delivery_state.rs:23` and `crates/agentdash-application-agentrun/src/agent_run/delivery_state.rs:56`.
- The API terminal convergence adapter maps terminal facts into gate producer terminal convergence at `crates/agentdash-api/src/agent_run_terminal_control.rs:49` and `crates/agentdash-api/src/agent_run_terminal_control.rs:110`; this is a good adjacent place for terminal-related `AgentRunList` invalidation if the app-level delivery state service stays persistence-only.
- `AgentRunWorkspaceTitleAdapter` resolves runtime session -> AgentRun anchor -> LifecycleAgent and writes workspace title at `crates/agentdash-application-agentrun/src/agent_run/workspace_title.rs:28`.
- `LifecycleAgent.update_workspace_title` updates `workspace_title`, `workspace_title_source`, and `updated_at` at `crates/agentdash-domain/src/workflow/lifecycle_agent.rs:159`.
- Runtime session eventing handles `SourceSessionTitleUpdated`, writes the workspace title via the port at `crates/agentdash-application-runtime-session/src/session/eventing.rs:370`, then emits `SessionMetaUpdate { key: "session_meta_updated" }` at `crates/agentdash-application-runtime-session/src/session/eventing.rs:378`; it does not currently emit `ControlPlaneProjectionChanged(projection=Title)` or `AgentRunList`.

### 3. Current frontend invalidation path and gap with file:line anchors

Files found:

- `packages/app-web/src/features/agent/agent-run-list-state-store.ts` - AgentRun list cache/store and project-event invalidation subscription.
- `packages/app-web/src/features/agent-run-workspace/model/controlPlaneModel.ts` - maps workspace stream `control_plane_projection_changed` events into refresh plans.
- `packages/app-web/src/features/agent-run-workspace/model/useAgentRunWorkspaceControlPlane.ts` - executes workspace refresh plans, including list refresh callback.
- `packages/app-web/src/pages/AgentRunWorkspacePage.tsx` - wires workspace-local `refreshAgentRunList` callback into control-plane hook.
- `packages/app-web/src/stores/eventStore.ts` - project event stream store and global project event listener registry.
- `crates/agentdash-contracts/src/project/contract.rs` - project event stream envelope variants.
- `crates/agentdash-api/src/stream.rs` - backend project event stream route converts only existing stream events into project envelopes.
- `crates/agentdash-application-agentrun/src/agent_run/journal.rs` - AgentRun journal stream live source maps RuntimeSession events for an opened AgentRun workspace.

Current list store consumption:

- AgentRun list store imports `fetchProjectAgentRuns` and `subscribeProjectEvents` at `packages/app-web/src/features/agent/agent-run-list-state-store.ts:4`.
- Store refreshes first page with `fetchProjectAgentRuns` at `packages/app-web/src/features/agent/agent-run-list-state-store.ts:103` and via `refreshProject` at `packages/app-web/src/features/agent/agent-run-list-state-store.ts:183`.
- `fetchProjectAgentRuns` calls `GET /projects/{projectId}/agent-runs` at `packages/app-web/src/services/lifecycle.ts:49`.
- The project-event predicate only returns true for `event.type === "StateChanged"` with matching `project_id` at `packages/app-web/src/features/agent/agent-run-list-state-store.ts:283`.
- The hook subscribes to global project events and calls `invalidateAgentRunListStateForProjectEvent` at `packages/app-web/src/features/agent/agent-run-list-state-store.ts:322`.
- Therefore, list refresh without an open workspace depends on Project `StateChanged` events only.

Current workspace-local projection path:

- `controlPlaneModel.ts` extracts `control_plane_projection_changed` from Backbone platform events at `packages/app-web/src/features/agent-run-workspace/model/controlPlaneModel.ts:92`.
- It maps `workspace`, `mailbox`, `waiting`, `delivery`, and `title` changes to both workspace refresh and list refresh at `packages/app-web/src/features/agent-run-workspace/model/controlPlaneModel.ts:129`.
- It maps `agent_run_list` to list refresh only at `packages/app-web/src/features/agent-run-workspace/model/controlPlaneModel.ts:138`.
- The hook executes `plan.refreshAgentRunListReason` by calling `refreshAgentRunList` at `packages/app-web/src/features/agent-run-workspace/model/useAgentRunWorkspaceControlPlane.ts:358`.
- `AgentRunWorkspacePage` wires that callback to `refreshAgentRunListState(ownerProjectId ?? draftProjectIdValue, reason)` at `packages/app-web/src/pages/AgentRunWorkspacePage.tsx:262` and passes it into the workspace control-plane hook at `packages/app-web/src/pages/AgentRunWorkspacePage.tsx:449`.

Project event stream limitation:

- Project event stream currently has only `Connected`, `StateChanged`, `BackendRuntimeChanged`, and `Heartbeat` variants at `crates/agentdash-contracts/src/project/contract.rs:73`.
- Backend stream conversion only maps state changes, backend runtime changes, and heartbeats at `crates/agentdash-api/src/stream.rs:245`.
- Frontend `eventStore` publishes `StateChanged` and `BackendRuntimeChanged` project events to listeners at `packages/app-web/src/stores/eventStore.ts:71`.
- AgentRun journal live source subscribes to a delivery runtime session when an AgentRun workspace stream is open at `crates/agentdash-application-agentrun/src/agent_run/journal.rs:443`, and maps current delivery runtime events into the AgentRun journal stream at `crates/agentdash-application-agentrun/src/agent_run/journal.rs:584`.

Why refresh misses SubAgent changes:

- Same-run SubAgent creation writes `AgentLineage`; terminal writes `AgentRunDeliveryBinding`; title writes `LifecycleAgent.workspace_title`. These are not Project `StateChanged` facts.
- The list store only listens for Project `StateChanged`, so it does not see those backend writes.
- Backend protocol already has `ControlPlaneProjection::AgentRunList`, but there is no located production emitter for `projection: AgentRunList`.
- Workspace-local `controlPlaneModel.ts` can refresh the list when a workspace stream carries a suitable platform event, but this only runs while the workspace page/hook is mounted.
- If the AgentRun list is visible without the relevant workspace stream, or if only the parent/child not currently streamed changes, the list remains stale until manual refresh, route action refresh, or an unrelated open workspace stream event calls `refreshAgentRunListState`.

### 4. Proposed event/invalidation path

Backend path:

1. Introduce a project-scoped control-plane invalidation publisher.
   - Shape: either add `ProjectEventStreamEnvelope::ControlPlaneProjectionChanged { project_id, data: ControlPlaneProjectionChanged }`, or add a sibling project projection event with equivalent generated DTO.
   - Reason: `ControlPlaneProjectionChanged` currently travels inside Backbone session/journal streams; AgentRun list correctness needs an event source keyed by project, not by an opened delivery runtime session.

2. Emit `projection=AgentRunList` from read-model write boundaries.
   - Same-run child lineage: after `LifecycleRelationWriter::write_for_dispatch` creates `AgentLineage`, publish for the run's project with parent and child refs. This covers companion/SubAgent child count and inline child appearance.
   - Cross-run fork/new root: after fork materialization transaction commits, publish for the parent project/child run so the new root row appears.
   - Delivery running: after launch commit upserts `AgentRunDeliveryBinding::mark_running`, publish for the affected run/agent/frame/runtime.
   - Delivery terminal: after `AgentRunDeliveryStateService::mark_terminal_from_runtime_session` returns updated, publish for the affected run/agent/frame/runtime; adjacent terminal convergence can also publish `projection=Delivery` for workspace, but list should get `projection=AgentRunList` directly.
   - Title: after `AgentRunWorkspaceTitleAdapter::update_workspace_title` returns `updated=true`, publish `projection=AgentRunList` with reason `title_changed` and optionally also `projection=Title` for workspace shell.
   - Activity/root ordering: when root `LifecycleRun.last_activity_at` changes through run repository updates, publish `projection=AgentRunList` because root row ordering and root shell activity use this field.

3. Keep emitters after durable writes.
   - For transaction-backed materialization, publish only after commit.
   - For terminal/title writes, publish only when a write actually changed state.
   - For stale terminal ignored by current binding mismatch, do not publish.

4. Prefer typed reasons over free strings.
   - Existing reason enum can represent `delivery_terminal` and `title_changed`, but not lineage or generic list shell changes.
   - Add reasons such as `agent_run_lineage_changed`, `agent_run_shell_changed`, and `agent_run_activity_changed`, then regenerate TS bindings with the Backbone generator.

Frontend path:

1. Extend project event contracts and validators so `eventStore` can publish the new project-scoped projection event.
2. Update `agent-run-list-state-store.ts`:
   - `shouldRefreshAgentRunListStateForProjectEvent` should accept matching project-scoped projection events where `data.projection === "agent_run_list"`.
   - It may also accept shell-affecting projections if backend chooses to emit `workspace`/`delivery`/`title` instead of a dedicated list projection, but the cleaner contract is one backend `agent_run_list` invalidation per list read-model change.
3. Preserve existing `StateChanged` refresh for story/project facts until all relevant projections are classified.
4. Keep `controlPlaneModel.ts` behavior for open workspace streams, but treat it as local responsiveness, not the only invalidation path.

Expected end-to-end flow:

```text
companion_request target=sub
  -> LifecycleDispatchService launch/open gate
  -> LifecycleRelationWriter creates AgentLineage
  -> ProjectEventStreamEnvelope::ControlPlaneProjectionChanged(project_id, projection=agent_run_list, reason=agent_run_lineage_changed)
  -> eventStore publishes project event
  -> agent-run-list-state-store refreshes /projects/{project_id}/agent-runs
  -> ActiveAgentRunList and AgentRunShortcutList show child/count/status without manual refresh
```

Terminal/title flow:

```text
RuntimeSession terminal/title
  -> AgentRunDeliveryBinding or LifecycleAgent workspace title durable update
  -> project-scoped agent_run_list invalidation
  -> list store refetches authoritative read model
```

### 5. Tests needed

Backend tests:

- Lifecycle dispatch test: `AgentPolicy::SpawnChild` / companion child dispatch creates `AgentLineage` and emits one project-scoped `AgentRunList` invalidation with parent/child refs.
- Gate/open interaction test: `wait=true` companion child path also emits list invalidation after child lineage is written.
- Fork materialization test: cross-run fork commits child run/agent/frame/anchor/delivery binding/lineage and emits list invalidation only after successful commit.
- Delivery running test: launch accepted -> `AgentRunDeliveryBinding::mark_running` upsert -> list invalidation emitted with runtime session id.
- Delivery terminal test: terminal transition updates current binding and emits list invalidation; stale runtime terminal ignored by current binding mismatch emits none.
- Title update test: `SourceSessionTitleUpdated` / `WorkspaceTitlePort` updates `LifecycleAgent` and emits `AgentRunList` invalidation; lower-priority title ignored emits none.
- Contract tests: generated Backbone / project-event TS includes any new reason or project stream envelope.

Frontend tests:

- `agent-run-list-state-store.test.ts`: matching project-scoped `control_plane_projection_changed` with `projection="agent_run_list"` calls `refreshProject`; other project id does not refresh.
- Store test: `StateChanged` still refreshes as before.
- Workspace model test: existing open-workspace `control_plane_projection_changed(agent_run_list)` still plans list refresh.
- Integration-style component test: `ActiveAgentRunList`/`AgentRunShortcutList` consume refreshed shared store without local child/status inference.
- Event stream parser test: new project event envelope parses, validates project id and typed projection payload, and rejects malformed payloads.

## Caveats / Not Found

- No production emitter for `ControlPlaneProjection::AgentRunList` was found. The codebase currently emits at least mailbox/resource-surface control-plane projection changes, but AgentRun list invalidation is not produced from lineage, delivery, or title write points.
- The current `ProjectEventStreamEnvelope` does not carry Backbone `ControlPlaneProjectionChanged`; it only carries project state/backend-runtime/heartbeat envelopes. A project-scoped projection event requires contract work.
- Same-run SubAgent list hierarchy uses `AgentLineage` (`agent_lineages`), while cross-run fork uses `AgentRunLineage` (`agent_run_lineages`). Both can affect the visible list, but companion/SubAgent inline children primarily depend on `AgentLineage`.
- Root list ordering uses `LifecycleRun.last_activity_at`, while child row ordering uses child shell `LifecycleAgent.updated_at`; invalidation should cover both facts, but the actual run activity writer paths should be audited during implementation if this slice touches root ordering semantics.
- External references: none. This research is repo-internal and based on task artifacts, Trellis specs, and source inspection.

## Implementation slice note: AgentRun list invalidation only

This slice implemented the project-scoped `AgentRunList` invalidation path for the facts that currently explain the companion/SubAgent stale-list bug: same-run `AgentLineage` creation, accepted delivery running binding, terminal delivery convergence, and workspace title shell updates. Those writes already have application-layer durable boundaries and can publish after the fact is committed without changing mailbox/waiter semantics.

Cross-run fork/new-root invalidation remains a follow-up because the materialization lives in the Postgres `AgentRunLineage` transaction path rather than the same application write boundary used by companion child lineage. Root activity invalidation also remains a follow-up because `LifecycleRun.last_activity_at` can be changed through broad run repository updates, and wiring it correctly should be done at the run-activity ownership boundary rather than by guessing from unrelated activity projections.

## Related specs

- `.trellis/spec/cross-layer/backbone-protocol.md` - Backbone `PlatformEvent` and typed control-plane projection events.
- `.trellis/spec/frontend/state-management.md` - AgentRun workspace/list projection ownership and frontend refresh rules.
- `.trellis/spec/backend/session/agentrun-mailbox.md` - mailbox/waiting boundaries and control-plane projection responsibilities.
- `.trellis/spec/backend/workflow/activity-lifecycle.md` - lifecycle gate terminal convergence and wait producer ownership.
