# WI-09 Public Visibility And Import Cleanup

Status: done

Assigned Worker: codex-main

## Tracking

- Files changed:
  - `crates/agentdash-application/src/agent_run/mod.rs`
  - `crates/agentdash-application/src/agent_run/frame/mod.rs`
  - `crates/agentdash-application/src/agent_run/project_agent_context.rs`
  - `crates/agentdash-application/src/agent_run/frame/construction/request_assembler.rs`
  - `crates/agentdash-application/src/agent_run/frame/construction/composer_project_agent.rs`
  - `crates/agentdash-application/src/agent_run/workspace/query.rs`
  - `crates/agentdash-application/src/lifecycle/subject_context_assignment.rs`
  - `crates/agentdash-application/src/task/context_builder.rs`
  - `crates/agentdash-application/src/session/mod.rs`
  - `crates/agentdash-application/src/session/construction_planner.rs`
  - `crates/agentdash-application/src/vfs/mod.rs`
  - `crates/agentdash-api/src/bootstrap/frame_launch_envelope_provider.rs`
  - `crates/agentdash-api/src/routes/project_agents.rs`
  - `crates/agentdash-api/src/routes/vfs_surfaces/resolver.rs`
- Tests run:
  - `cargo fmt -p agentdash-application -p agentdash-api`
  - `cargo check -p agentdash-application`
  - `cargo check -p agentdash-api`
  - `cargo check -p agentdash-local`
  - `cargo check -p agentdash-mcp`
  - `rg -n "agentdash_application::session::construction_planner|agentdash_application::session::plan|agentdash_application::session::AgentFrameRuntimeTarget" crates/agentdash-api/src crates/agentdash-mcp/src crates/agentdash-local/src`
  - `rg -n "SessionRuntimeInner|session::hub|resolve_current_frame_from_delivery_trace_ref" crates/agentdash-api/src crates/agentdash-application/src/runtime_gateway`
  - `rg -n "AgentFrameBuilder" crates/agentdash-application/src/canvas crates/agentdash-application/src/workspace_module crates/agentdash-application/src/permission crates/agentdash-api/src`
  - `rg -n "agentdash_application::agent_run::frame::(builder|construction|hook_runtime|launch_commit|runtime_launch|surface|surface_service)|agentdash_application::vfs::(provider_canvas|provider_inline|provider_lifecycle|provider_routine|provider_skill_asset|mutation_queue)" crates/agentdash-api/src crates/agentdash-mcp/src crates/agentdash-local/src`
- Blockers: None.
- Handoff summary: `session::construction_planner` was removed from the session public facade and moved under AgentRun as `project_agent_context`. External API consumers now import project-agent context and workspace resolution through the AgentRun facade. AgentRun frame implementation modules and VFS provider modules are crate-private while stable facade re-exports remain available for outer crates. Import gates show no production forbidden imports; remaining `AgentFrameBuilder` hits are confined to `#[cfg(test)]` fixtures in application tests.

## Purpose

Remove broad exports and old import paths after all consumer migrations complete.

## Dependencies

- `WI-03`
- `WI-04`
- `WI-05`
- `WI-06`
- `WI-07`
- `WI-08`

## Scope

- Tighten `agentdash-application/src/lib.rs` exports.
- Tighten `session/mod.rs`.
- Tighten `agent_run/frame/mod.rs`.
- Tighten `vfs/mod.rs`.
- Remove old compatibility shims or make them private.
- Run import hotspot checks and document remaining intentional exceptions.

## Deliverables

- Import graph suitable for future physical crate split.
- Updated `work-items/WI-09...` with before/after import summary.

## Import Summary

Before this item, project-agent context and workspace resolution were exposed as `session::construction_planner`, and API/VFS routes consumed that path directly. Frame and VFS implementation modules also remained publicly reachable from outer crates, leaving import choices broader than the intended future crate boundary.

After this item, project-agent context lives under AgentRun ownership and is re-exported from `agent_run`. The session facade no longer exports the construction planner, frame internals are `pub(crate)` behind the AgentRun facade, and VFS provider modules are `pub(crate)` behind VFS facade exports. Outer crates consume stable AgentRun/VFS surfaces only.

## Acceptance

- Production forbidden imports are gone or documented as internal adapters.
- `cargo check -p agentdash-application`
- `cargo check -p agentdash-api`
- `cargo check -p agentdash-local`
- `cargo check -p agentdash-mcp`
