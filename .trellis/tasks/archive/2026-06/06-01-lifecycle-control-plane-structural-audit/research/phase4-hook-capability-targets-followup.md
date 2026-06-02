# Research: phase4-hook-capability-targets-followup

- Query: Phase 4 follow-up for "Hook/capability command primary target 改为 agent/frame/assignment，session_id 仅作为 runtime adapter provenance"
- Scope: internal
- Date: 2026-06-01

## Findings

### Task / spec baseline

- `.trellis/tasks/06-01-lifecycle-control-plane-structural-audit/structural-analysis.md:213` describes P1-08: hook runtime is already `AgentFrameHookRuntime`, but API entry still goes through `session_id -> find frame`; `.trellis/tasks/06-01-lifecycle-control-plane-structural-audit/structural-analysis.md:227` says control command primary target must be `agent_id` / `frame_id` / `assignment_id`, with optional `runtime_session_id` / `turn_id` provenance.
- `.trellis/tasks/06-01-lifecycle-control-plane-structural-audit/implement.md:55` lists Phase 4. The unchecked gates at `.trellis/tasks/06-01-lifecycle-control-plane-structural-audit/implement.md:59` and `.trellis/tasks/06-01-lifecycle-control-plane-structural-audit/implement.md:68` are exactly this follow-up.
- `.trellis/spec/backend/workflow/activity-lifecycle.md:102` says hook advance/resolution should use assignment / graph instance refs; session-indexed lookup is only a trace adapter and must immediately resolve to frame/agent/assignment.
- `.trellis/spec/backend/session/runtime-execution-state.md:121` and `.trellis/spec/backend/session/session-startup-pipeline.md:132` say runtime context / capability transition truth belongs to `AgentFrameTransitionRecord` / `agent_frame_transitions`; `SessionRuntimeCommandStore` is only a delivery outbox.
- `.trellis/spec/backend/hooks/execution-hook-runtime.md:101` says live / pending / applied-on-next-turn paths should share the runtime context transition structure; `.trellis/spec/backend/hooks/execution-hook-runtime.md:104` explicitly calls `replace_current_capability_state` an internal primitive.

### Files found

- `crates/agentdash-spi/src/hooks/mod.rs` - SPI hook traits and query DTOs still expose session-indexed hook command shapes.
- `crates/agentdash-application/src/workflow/frame_hook_runtime.rs` - `AgentFrameHookRuntime` stores agent/frame identity, but trait implementation still exposes `session_id()` and consumes session-shaped query DTOs.
- `crates/agentdash-application/src/hooks/provider.rs` and `crates/agentdash-application/src/hooks/workflow_snapshot.rs` - application hook provider loads snapshots by `session_id` and resolves active workflow from that lookup.
- `crates/agentdash-application/src/session/hooks_service.rs` and `crates/agentdash-application/src/session/hub/hook_dispatch.rs` - hook runtime creation, reload, refresh, and trigger dispatch are still session-indexed.
- `crates/agentdash-application/src/session/capability_service.rs`, `crates/agentdash-application/src/session/types.rs`, `crates/agentdash-application/src/session/hub/tool_builder.rs`, `crates/agentdash-application/src/session/hub/runtime_context_transition.rs` - capability update internals now use `AgentFrameRuntimeTarget`, but service facades and callers still frequently begin from runtime session id.
- `crates/agentdash-application/src/workflow/step_activation.rs`, `crates/agentdash-application/src/workflow/agent_executor.rs`, `crates/agentdash-application/src/workflow/orchestrator.rs` - workflow control paths still derive hook/capability target from root/current runtime session.
- `crates/agentdash-application/src/canvas/tools.rs` and `crates/agentdash-application/src/companion/tools.rs` - canvas capability sync and companion result hook notification still target a session before resolving frame/hook runtime.
- `crates/agentdash-application/src/workflow/session_association.rs` and `crates/agentdash-application/src/workflow/projection.rs` - runtime-session-to-assignment adapter exists and is structurally useful when kept at adapter boundaries.
- `crates/agentdash-spi/src/session_persistence.rs` and `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs` - runtime delivery outbox already stores delivery under session while linking to frame transition fact.

### Still session-primary control APIs / functions / call points

| Location | Current shape | Classification |
|---|---|---|
| `crates/agentdash-spi/src/hooks/mod.rs:857` | `ExecutionHookProvider::load_session_snapshot`, `refresh_session_snapshot`, `evaluate_hook` are the SPI hook provider surface. | Control coupling. The provider trait is still named and shaped around session snapshot load/refresh; callers cannot express frame/assignment as primary target. |
| `crates/agentdash-spi/src/hooks/mod.rs:541`, `crates/agentdash-spi/src/hooks/mod.rs:549`, `crates/agentdash-spi/src/hooks/mod.rs:647` | `SessionHookSnapshotQuery`, `SessionHookRefreshQuery`, `HookEvaluationQuery` all carry top-level `session_id`. | Control coupling for hook command/query DTO shape. `runtime_session_id` should move into provenance on a frame/assignment query. |
| `crates/agentdash-application/src/hooks/provider.rs:103` and `crates/agentdash-application/src/hooks/provider.rs:129` | `AppExecutionHookProvider::load_session_snapshot` creates `SessionHookSnapshot { session_id }` and calls `resolve_active_workflow(&query.session_id)`. | Control coupling. Hook snapshot authority is active workflow/assignment/frame, but API target is runtime session. |
| `crates/agentdash-application/src/hooks/workflow_snapshot.rs:60` | `WorkflowSnapshotBuilder::resolve_active_workflow(session_id)` exposes only a session entry. | Boundary coupling. Internally it immediately resolves through the adapter, but hook provider has no frame/assignment entry point. |
| `crates/agentdash-application/src/session/hooks_service.rs:23`, `crates/agentdash-application/src/session/hooks_service.rs:31`, `crates/agentdash-application/src/session/hooks_service.rs:35`, `crates/agentdash-application/src/session/hooks_service.rs:99` | `ensure_hook_runtime`, `get_hook_runtime`, `reload_hook_runtime`, `resolve_hook_runtime` all take `session_id`. | Control coupling when used outside runtime adapter / active connector paths. |
| `crates/agentdash-application/src/session/hooks_service.rs:55` and `crates/agentdash-application/src/session/hooks_service.rs:73` | Reload loads `SessionHookSnapshotQuery { session_id }`, then calls `build_frame_hook_runtime(..., session_id, ...)`. | Adapter implementation is embedded in hook service instead of an explicit trace-to-frame adapter. |
| `crates/agentdash-application/src/session/hub/hook_dispatch.rs:178` and `crates/agentdash-application/src/session/hub/hook_dispatch.rs:201` | Lazy rebuild of hook runtime uses `provider.load_session_snapshot(SessionHookSnapshotQuery { session_id, ... })`. | Control coupling for rebuild path. |
| `crates/agentdash-application/src/session/hub/hook_dispatch.rs:69` and `crates/agentdash-application/src/session/hub/hook_dispatch.rs:85` | Hook trigger dispatch evaluates and refreshes by `session_id` from `HookTriggerInput`. | Mixed, but still session-primary command shape. It is acceptable only when invoked by connector/runtime adapter; workflow/companion callers should pass frame/assignment target. |
| `crates/agentdash-application/src/session/hook_delegate.rs:186` and `crates/agentdash-application/src/session/hook_delegate.rs:202` | Runtime delegate evaluates and refreshes using `self.hook_runtime.session_id()`. | Mostly runtime adapter/provenance. It is connector-facing, but it reinforces the SPI query shape. |
| `crates/agentdash-application/src/workflow/orchestrator.rs:391` | After activity advancement, `refresh_hook_snapshot_for_turn` refreshes with `session_id: hook_runtime.session_id()`. | Control coupling. The advance path already has lifecycle association context; refresh target should be frame/assignment with runtime provenance. |
| `crates/agentdash-application/src/workflow/step_activation.rs:298` | `apply_to_running_session` starts from `hook_runtime.session_id()`, resolves frame via `resolve_runtime_session_frame_id`, reads base capability state by session, then applies transition. | Control coupling. PhaseNode capability command target should be the current frame/assignment known by workflow activation, with runtime session only delivery provenance. |
| `crates/agentdash-application/src/canvas/tools.rs:579` | `sync_canvas_mount_capability_state(..., session_id, ...)` reads state, hook runtime, and target frame by `session_id`. | Control coupling. Canvas VFS capability effect should target frame; runtime session is only the live delivery/provenance channel. |
| `crates/agentdash-application/src/workflow/agent_executor.rs:353` and `crates/agentdash-application/src/workflow/agent_executor.rs:391` | ContinueRoot path ensures hook runtime by `root_runtime_session_id` and applies activation to running session. | Control coupling, also overlaps Phase 4 `ContinueRoot` policy work. The owner should be root agent/current frame/assignment, not root session. |
| `crates/agentdash-application/src/workflow/agent_executor.rs:471` | Pending root transition resolves `target_frame_id` from `root_runtime_session_id` and enqueues delivery to that session. | Control coupling at caller boundary; the outbox write itself is delivery-only. |
| `crates/agentdash-application/src/companion/tools.rs:687` and `crates/agentdash-application/src/companion/tools.rs:1570` | Parent companion result loads parent hook runtime by `parent_session_id`, then evaluates `HookEvaluationQuery { session_id: hook_runtime.session_id(), ... }`. | Control coupling. Parent notification should target parent agent/frame/assignment; session id is trace provenance for the parent connector. |
| `crates/agentdash-application/src/session/capability_service.rs:36`, `crates/agentdash-application/src/session/capability_service.rs:40`, `crates/agentdash-application/src/session/capability_service.rs:44` | Service facade exposes current/latest capability state and frame resolution by `session_id`. | Mixed. These are acceptable as adapter helpers, but they are currently consumed directly by workflow/canvas control commands. |

### Runtime adapter / provenance-only hits

- `crates/agentdash-domain/src/workflow/repository.rs:131` defines `AgentFrameRepository::find_by_runtime_session` as RuntimeSession -> AgentFrame trace lookup. This is acceptable when the caller is explicitly a runtime adapter.
- `crates/agentdash-application/src/workflow/session_association.rs:83` resolves `session_id -> current_frame -> agent -> active assignment -> run`; `crates/agentdash-application/src/workflow/session_association.rs:138` first prefers exact launch frame assignment and then scoped graph/activity fallback. This is the right adapter pattern for terminal/runtime callbacks.
- `crates/agentdash-application/src/workflow/projection.rs:70` wraps that adapter for active workflow projection. The internal behavior is acceptable; the problem is that hook provider exposes only this session entry instead of frame/assignment entries.
- `crates/agentdash-application/src/workflow/frame_hook_runtime.rs:34` stores `run_id`, `agent_id`, `frame_id`, and `runtime_session_id`; `crates/agentdash-application/src/workflow/frame_hook_runtime.rs:39` labels runtime session as provider query / trace compatibility. This is provenance, not owner, as long as command APIs stop targeting it.
- `crates/agentdash-spi/src/session_persistence.rs:432` defines `RuntimeDeliveryCommand` with `frame_transition_id` and `target_frame_id`, not full capability state. This is delivery-only and aligned with Phase 4.
- `crates/agentdash-spi/src/session_persistence.rs:858` keeps `delivery_runtime_session_id` in `SessionRuntimeCommandStore::upsert_runtime_delivery_command`; acceptable because the table is the delivery outbox.
- `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:139` validates delivery `frame_transition_id` and `target_frame_id` against the frame transition fact; `crates/agentdash-infrastructure/src/persistence/postgres/session_repository.rs:710` writes `agent_frame_transitions` and `session_runtime_commands` together. This is the strongest completed slice.
- `crates/agentdash-api/src/routes/sessions.rs:36`, `crates/agentdash-api/src/session_construction.rs:15`, and `crates/agentdash-api/src/bootstrap/session_construction_provider.rs:75` are runtime trace / launch adapter entry points. They start from session because the external surface is a session trace or connector launch, then resolve frame/agent for permission or construction.
- `crates/agentdash-executor/src/connectors/pi_agent/connector.rs:916` uses session id only for hook trace envelope delivery. This is trace provenance.

### Current strongest completed pieces

- `AgentFrameRuntimeTarget` already exists at `crates/agentdash-application/src/session/types.rs:58`; `frame_id` is the effective runtime surface target and `delivery_runtime_session_id` is explicitly delivery runtime.
- `replace_current_capability_state` already takes `AgentFrameRuntimeTarget` at `crates/agentdash-application/src/session/hub/tool_builder.rs:106`, validates the frame contains the delivery runtime session at `crates/agentdash-application/src/session/hub/tool_builder.rs:133`, and only then updates runtime registry / connector at `crates/agentdash-application/src/session/hub/tool_builder.rs:179`.
- Tests already cover part of the target split: `crates/agentdash-application/src/session/hub/tests.rs:1077` asserts mismatched frame/session delivery target is rejected, and `crates/agentdash-application/src/session/hub/tests.rs:1249` asserts pending delivery payload does not carry transition/state truth.
- `PendingRuntimeContextTransitionInput` and `LiveRuntimeContextTransitionInput` now use `delivery_runtime_session_id` at `crates/agentdash-application/src/session/hub/runtime_context_transition.rs:29` and `crates/agentdash-application/src/session/hub/runtime_context_transition.rs:52`; this naming is aligned with provenance/delivery, but some callers still discover the target by session first.

## Suggested Minimal Encapsulation

### 1. Introduce target/provenance wrappers at application boundary

Add small wrappers instead of another large abstraction:

- `HookControlTarget`: `{ frame_id, agent_id, run_id, assignment_id: Option<Uuid> }`
- `CapabilityControlTarget`: reuse `AgentFrameRuntimeTarget` for writes, but require the caller to supply `frame_id`; add optional `{ assignment_id, agent_id }` when the caller has them.
- `RuntimeAdapterProvenance`: `{ runtime_session_id: Option<String>, turn_id: Option<String>, source: RuntimeEventSource }`

Only runtime adapter code should construct these from raw session id through a named resolver, for example `RuntimeSessionTraceResolver::resolve_hook_target(runtime_session_id) -> HookControlTarget + RuntimeAdapterProvenance`.

### 2. Split hook service into frame-first and adapter entry points

Minimum hook API shape:

- `ensure_hook_runtime_for_target(target: HookControlTarget, provenance: RuntimeAdapterProvenance)`
- `refresh_hook_snapshot_for_target(target: HookControlTarget, provenance: RuntimeAdapterProvenance, reason)`
- `evaluate_hook_for_target(target: HookControlTarget, provenance: RuntimeAdapterProvenance, trigger, payload)`
- Keep `ensure_hook_runtime_for_runtime_session(session_id, ...)` only in session launch / connector adapter modules, and have it immediately call the target API.

The fact source owner is `AgentFrame` for hook runtime surface and `AgentAssignment + WorkflowGraphInstance` for activity projection. `RuntimeSession` supplies provenance and live connector delivery only.

### 3. Split capability service into frame-first command and runtime-sync adapter

Minimum capability API shape:

- `load_capability_state_for_frame(frame_id)` derives from current `AgentFrame` projection, not runtime registry cache.
- `apply_live_runtime_context_transition(target: AgentFrameRuntimeTarget, provenance: RuntimeAdapterProvenance, transition input)` keeps `frame_id` mandatory and `delivery_runtime_session_id` explicitly delivery-only.
- `resolve_runtime_session_frame_id(session_id)` remains private to runtime adapter modules and should not be called from `workflow/step_activation.rs`, `canvas/tools.rs`, or companion control code.

The fact source owner is `AgentFrame` / `AgentFrameTransitionRecord`; runtime registry and connector are post-commit delivery surfaces.

### 4. Transaction boundary

- Pending path: keep current transaction boundary in `SessionRuntimeCommandStore::upsert_runtime_delivery_command`: validate `RuntimeDeliveryCommand` against `AgentFrameTransitionRecord`, then write `agent_frame_transitions` and `session_runtime_commands` together.
- Live path: durable boundary should first persist the new `AgentFrame` revision, plus any context-frame/session-event audit record that documents the frame transition. After that commit, update runtime registry and call `connector.update_session_tools`. Connector failure should be reported as delivery failure, not used to roll back frame truth.
- Hook evaluation side effects should not write through the hook provider directly; they should dispatch typed effects to assignment/gate/capability services, each with its own owner-specific transaction.

### 5. Validation gates

- Static gate: after migration, `rg -n "SessionHookSnapshotQuery|SessionHookRefreshQuery|HookEvaluationQuery \\{|ensure_hook_runtime\\(|get_hook_runtime\\(|resolve_runtime_session_frame_id\\(" crates/agentdash-application/src` should show only runtime adapter modules, tests, or explicit provenance/trace sinks.
- Hook gate: add a test proving hook snapshot load / refresh / evaluate can run from `frame_id + assignment_id` without raw runtime session id.
- Capability gate: add tests for PhaseNode and canvas live update that pass `AgentFrameRuntimeTarget` directly and never call `resolve_runtime_session_frame_id` inside workflow/canvas control logic.
- Companion gate: add a test for companion result notification targeting parent frame/assignment while preserving parent runtime session only in trace payload.
- Delivery gate: keep existing mismatched target rejection and pending payload tests; extend with a failing case where delivery runtime session belongs to another frame.

## Caveats / Not Found

- No external references were needed; this is a codebase-internal structural audit.
- No direct HTTP route was found that exposes a hook/capability control command with raw session id. The remaining issues are primarily application/SPI boundaries and internal call paths.
- Several session hits are intentionally allowed runtime trace, launch, connector, or delivery paths. The incomplete work is not "remove every session_id", but "stop accepting session_id as hook/capability command owner".
- Current code appears to have already renamed runtime context inputs from `session_id` to `delivery_runtime_session_id`. This improves naming, but workflow/canvas/companion callers still often derive the frame target from session first.
- I did not run tests and did not modify code.
