# WI-00 Baseline Import And Contract Inventory

Status: done

Assigned Worker: Codex

## Tracking

- Files changed: `.trellis/tasks/06-24-agentrun-runtime-session-decoupling/work-items/WI-00-baseline-import-contract-inventory.md`.
- Tests/commands run:
  - `rg -n "crate::session::|agentdash_application::session::" crates/agentdash-application/src/agent_run crates/agentdash-application/src/lifecycle crates/agentdash-api/src`
  - `rg -n "AgentFrameRuntimeTarget|AgentFrameBuilder|AgentFrameSurfaceExt|resolve_current_frame_from_delivery_trace_ref" crates/agentdash-application/src crates/agentdash-api/src`
  - `rg -n "^(pub mod|pub use|pub\\(crate\\) use)" crates/agentdash-application/src/session/mod.rs crates/agentdash-application/src/agent_run/mod.rs crates/agentdash-application/src/agent_run/frame/mod.rs crates/agentdash-application/src/vfs/mod.rs crates/agentdash-application/src/lib.rs`
- Blockers: None.
- Handoff summary: Baseline inventory captured. WI-09/WI-10 should use the forbidden import baseline below to remove production session facade leaks, current-frame resolver consumers, and public AgentFrame write primitive exports, while keeping session substrate and test-only/internal adapter exceptions explicit.

## Purpose

Capture the pre-change dependency graph so later cleanup can prove real convergence rather than local refactors.

## Dependencies

- None.

## Scope

- Run import hotspot searches for `session`, `agent_run`, `lifecycle`, `runtime_gateway`, `vfs`, and API current-surface paths.
- Record existing exceptions and test-only imports separately from production imports.
- Confirm current public exports from `session/mod.rs`, `agent_run/mod.rs`, `agent_run/frame/mod.rs`, `vfs/mod.rs`, and `agentdash-application/src/lib.rs`.

## Deliverables

- `work-items/WI-00-baseline-import-contract-inventory.md` updated with baseline command output summary.
- Baseline forbidden import list used by `WI-09` and `WI-10`.

## Suggested Commands

```powershell
rg -n "crate::session::|agentdash_application::session::" crates/agentdash-application/src/agent_run crates/agentdash-application/src/lifecycle crates/agentdash-api/src
rg -n "AgentFrameRuntimeTarget|AgentFrameBuilder|AgentFrameSurfaceExt|resolve_current_frame_from_delivery_trace_ref" crates/agentdash-application/src crates/agentdash-api/src
rg -n "^(pub mod|pub use|pub\\(crate\\) use)" crates/agentdash-application/src/session/mod.rs crates/agentdash-application/src/agent_run/mod.rs crates/agentdash-application/src/agent_run/frame/mod.rs crates/agentdash-application/src/vfs/mod.rs crates/agentdash-application/src/lib.rs
```

## Baseline Summary

### Command 1: Session Import Hotspots

Production imports:

- `crates/agentdash-application/src/agent_run/**` imports `crate::session::*` heavily for launch/delivery substrate, runtime capability projection, frame construction, workspace query/projection, effective capability, runtime surface update, and ProjectAgent start. Key patterns include `SessionCoreService`, `SessionControlService`, `SessionExecutionState`, `SessionMeta`, `LaunchCommand`, `UserPromptInput`, `SessionLaunchService`, `SessionRuntimeTransitionService`, `construction_planner`, `plan`, `runtime_commands`, `types::CapabilityState`, and `types::AgentFrameRuntimeTarget`.
- `crates/agentdash-application/src/lifecycle/**` still imports session delivery/status helpers and construction helpers. Key production paths are `orchestrator.rs`, `dispatch_service.rs`, `subject_context_assignment.rs`, and `surface/journey/**`.
- `crates/agentdash-api/src/**` imports `agentdash_application::session::*` from composition/bootstrap and route layers. Composition/bootstrap paths include `app_state.rs`, `bootstrap/session.rs`, `bootstrap/vfs.rs`, `bootstrap/repositories.rs`, `bootstrap/relay.rs`, and `bootstrap/background_workers.rs`; route/helper paths include `agent_run_mailbox.rs`, `routes/sessions.rs`, `routes/project_agents.rs`, `routes/terminals.rs`, `routes/workflows.rs`, and `routes/vfs_surfaces/resolver.rs`.

Test-only imports:

- No dedicated `tests/` or `test_support/` path matched this command. Some matches may sit inside inline `#[cfg(test)]` modules in production files, so WI-09/WI-10 should verify individual line context before deleting a use that appears near local test fixtures.

### Command 2: AgentFrame / Current-Frame Contract Hotspots

Production imports and references:

- `AgentFrameRuntimeTarget` is still owned/exported through `session` and consumed by production AgentRun, companion, workspace module, permission, and session live-adoption code. Important paths include `agent_run/effective_capability.rs`, `agent_run/runtime_surface_update.rs`, `agent_run/frame/surface_service.rs`, `companion/tools.rs`, `workspace_module/visibility.rs`, `permission/runtime_surface_update.rs`, `session/hooks_service.rs`, `session/hub/tool_builder.rs`, and `session/launch/commit.rs`.
- `AgentFrameBuilder` is publicly re-exported through `agent_run` / `agent_run::frame` and used in AgentRun construction, Lifecycle dispatch, workflow orchestration launch, session launch commit, permission runtime update, and workspace module test/update flows. Key production paths include `agent_run/frame/construction/**`, `lifecycle/dispatch_service.rs`, `workflow/orchestration/**`, `session/launch/commit.rs`, and `permission/runtime_surface_update.rs`.
- `AgentFrameSurfaceExt` is exported through `agent_run::frame` and consumed by AgentRun surface/query/construction, API lifecycle/session read-model routes, session hub tool builder, lifecycle association, workspace command policy/query, companion, and permission tests/helpers.
- `resolve_current_frame_from_delivery_trace_ref` remains a broad current-frame resolver dependency. Production consumers include `agent_run/delivery_runtime_selection.rs`, `companion/**`, `task/context_builder.rs`, `session/hub/facade.rs`, `session/hub/tool_builder.rs`, `session/launch/commit.rs`, `session/launch/orchestrator.rs`, and API routes `routes/sessions.rs` / `routes/lifecycle_views.rs`.

Test-only imports:

- Dedicated test/helper paths include `crates/agentdash-application/src/test_support/agent_run_steering.rs` and `crates/agentdash-application/src/session/hub/tests.rs`.
- Inline test modules reference the same primitives in files such as `agent_run/frame/builder.rs`, `lifecycle/session_association.rs`, `workspace_module/tools.rs`, `permission/service.rs`, and workflow orchestration files. These should either remain behind test-only boundaries or be rewritten to consume public AgentRun facades once WI-09/WI-10 tightens exports.

### Command 3: Public Export Baseline

Production public exports:

- `crates/agentdash-application/src/lib.rs` currently exposes every application module as `pub mod`, including `agent_run`, `lifecycle`, `session`, `vfs`, `runtime_gateway`, `permission`, `workspace_module`, `workflow`, and other implementation-heavy modules. It also re-exports `ApplicationError`, `task_lock`, and `task_view_projector`.
- `crates/agentdash-application/src/session/mod.rs` publicly exposes many substrate and internal modules: `baseline_capabilities`, `bootstrap`, `construction_planner`, `context`, `runtime_transition_service`, `launch`, `plan`, `runtime_builder`, `runtime_commands`, `runtime_control`, `runtime_services`, `stall_detector`, `terminal_cache`, `turn_processor`, and `types`. It also re-exports non-session ownership types such as `AgentFrameHookRuntime`, `WorkflowApplicationError`, and `types::{AgentFrameRuntimeTarget, AgentFrameHookRuntimeTarget, ...}`.
- `crates/agentdash-application/src/agent_run/mod.rs` publicly exposes AgentRun internals including `frame`, `mailbox`, `runtime_capability`, `runtime_capability_projection`, `runtime_surface`, `workspace`, and re-exports frame write/surface primitives.
- `crates/agentdash-application/src/agent_run/frame/mod.rs` publicly exposes all frame submodules and re-exports `AgentFrameBuilder`, `AgentFrameHookRuntime`, launch envelope types, `AgentFrameSurfaceExt`, and surface service types.
- `crates/agentdash-application/src/vfs/mod.rs` publicly exposes most provider, mount, mutation, materialization, surface, tool, and type modules. `mount_lifecycle` helpers are already narrowed to `pub(crate) use`, but provider internals such as `CanvasFsMountProvider`, `InlineFsMountProvider`, `LifecycleMountProvider`, `RoutineMountProvider`, and `SkillAssetFsMountProvider` are still public.

Test-only exports:

- The command targets module facades only; no test-only facade file was part of this export baseline.

## Forbidden Import Baseline For WI-09/WI-10

WI-09/WI-10 must clean up or explicitly prove the following are internal adapter/test-only exceptions:

- Production `agentdash-api/src/**` imports of `agentdash_application::session::construction_planner`, `agentdash_application::session::plan`, `agentdash_application::session::types::AgentFrameRuntimeTarget`, or direct current-frame resolver helpers.
- Production API route usage of `resolve_current_frame_from_delivery_trace_ref` in `routes/sessions.rs` and `routes/lifecycle_views.rs`; current-surface/read-model access should move behind application facades.
- Production VFS/API helper usage of `session::construction_planner::resolve_project_workspace` in `routes/vfs_surfaces/resolver.rs`; resource surface resolution should move behind AgentRun/VFS application facade.
- Production AgentRun imports of `crate::session::AgentFrameRuntimeTarget`, `crate::session::types::AgentFrameRuntimeTarget`, `crate::session::construction_planner`, and `crate::session::plan` outside the final RuntimeSession adapter seam.
- Production business modules outside AgentRun frame/surface boundary owning `AgentFrameBuilder` or `AgentFrameSurfaceExt`, especially `permission`, `workspace_module`, `lifecycle`, `workflow/orchestration`, and API read-model routes.
- Production use of `resolve_current_frame_from_delivery_trace_ref` outside lifecycle/session internal adapter code; companion, task context, API routes, and AgentRun consumers should depend on AgentRun query/read-model facades.
- Public re-export of `AgentFrameBuilder`, `AgentFrameSurfaceExt`, `AgentFrameHookRuntime`, `AgentFrameRuntimeTarget`, `AgentFrameHookRuntimeTarget`, and `WorkflowApplicationError` from `session/mod.rs`, `agent_run/mod.rs`, or `agent_run/frame/mod.rs` unless the export is the final intended AgentRun facade contract.
- Broad root `pub mod` exports in `agentdash-application/src/lib.rs` and broad provider/internal exports in `vfs/mod.rs`; remaining public modules should be justified as application facade or stable adapter contracts.
- Test-only exceptions must live in dedicated test/support paths or inline `#[cfg(test)]` scopes and should not force production facade visibility.

## Acceptance

- Baseline distinguishes production and test-only imports.
- All later work items can reference the same forbidden import list.
