# WI-10 Final Review Gate

Status: done

Assigned Worker: codex-main

## Tracking

- Files changed:
  - `.trellis/tasks/06-24-agentrun-runtime-session-decoupling/work-items/WI-10-final-review-gate.md`
  - `.trellis/tasks/06-24-agentrun-runtime-session-decoupling/work-items/00-index.md`
  - `.trellis/tasks/06-24-agentrun-runtime-session-decoupling/review-gate.md`
  - `.trellis/tasks/06-24-agentrun-runtime-session-decoupling/target-application-state.md`
  - `.trellis/spec/backend/session/architecture.md`
  - `.trellis/spec/backend/runtime-gateway.md`
- Tests run:
  - `cargo check -p agentdash-application`
  - `cargo check -p agentdash-api`
  - `cargo check -p agentdash-application-ports`
  - `cargo check -p agentdash-local`
  - `cargo check -p agentdash-mcp`
  - `cargo test -p agentdash-application runtime_gateway::mcp_access`
  - `cargo test -p agentdash-application runtime_gateway`
  - `cargo test -p agentdash-application agent_run::runtime_surface`
  - `cargo test -p agentdash-api current_surface_project_guard -- --nocapture`
  - `cargo test -p agentdash-application permission::service::tests`
  - `cargo test -p agentdash-application canvas::`
  - `cargo test -p agentdash-application launch_commit`
  - `cargo test -p agentdash-application runtime_command_apply_commit_failure_marks_failed_and_returns_error`
  - `cargo test -p agentdash-application invoke_canvas_bind_data_routes_to_host_canvas_use_case`
  - `cargo test -p agentdash-application invoke_canvas_bind_data_runtime_update_preserves_external_integration_skill`
  - `rg -n "agentdash_application::session::construction_planner|agentdash_application::session::plan|agentdash_application::session::AgentFrameRuntimeTarget" crates/agentdash-api/src crates/agentdash-mcp/src crates/agentdash-local/src`
  - `rg -n "SessionRuntimeInner|session::hub|resolve_current_frame_from_delivery_trace_ref" crates/agentdash-api/src crates/agentdash-application/src/runtime_gateway`
  - `rg -n "AgentFrameBuilder" crates/agentdash-application/src/canvas crates/agentdash-application/src/workspace_module crates/agentdash-application/src/permission crates/agentdash-api/src`
  - `rg -n "agentdash_application::agent_run::frame::(builder|construction|hook_runtime|launch_commit|runtime_launch|surface|surface_service)|agentdash_application::vfs::(provider_canvas|provider_inline|provider_lifecycle|provider_routine|provider_skill_asset|mutation_queue)" crates/agentdash-api/src crates/agentdash-mcp/src crates/agentdash-local/src`
- Blockers: None.
- Handoff summary: Final gate passed. All work items are done. RuntimeGateway MCP consumes the ports crate contract, API current-surface consumers use AgentRun facades, Canvas/Extension project guards reject mismatches before invocation, business surface changes route through AgentRun typed update facades, and the remaining `AgentFrameBuilder` search hits are test fixtures only.

## Purpose

Run the complete review gate after implementation and visibility cleanup.

## Dependencies

- `WI-09`

## Scope

- Execute `../review-gate.md`.
- Verify target state from `../target-application-state.md`.
- Update specs if durable contracts changed.
- Record final evidence in this item.

## Deliverables

- Final gate report.
- Tests and command output summary.
- Remaining risk list, if any.

## Final Gate Report

- Boundary evidence: pass. `session/mod.rs` no longer exports project-agent construction planner; `AgentFrameRuntimeTarget`, runtime surface query/update, launch commit, resource surface and project-agent context sit behind AgentRun facades. RuntimeSession delivery/live adoption remains an implementation substrate.
- Import evidence: pass. Forbidden session planner/plan/target imports in outer crates returned no matches; API/runtime_gateway SessionHub/current-frame resolver search returned no matches; frame/vfs internal submodule imports from outer crates returned no matches.
- Behavioral evidence: pass. RuntimeGateway MCP idle list/call, capability filtering, AgentRun runtime surface query, project mismatch guards, Permission update/adoption, Canvas runtime surface update, VFS/resource surface and launch/commit tests passed.
- Documentation evidence: pass. Work item docs, review gate, target state and durable specs were updated to reflect AgentRun ownership of current/runtime surface and route-level Project/session guard requirements.
- Remaining risk: existing compiler warnings remain in unrelated or already-tracked unused surfaces; they do not contradict this decoupling gate.

## Acceptance

- All required review-gate evidence is satisfied.
- Decoupling task can be marked ready for implementation completion review.
