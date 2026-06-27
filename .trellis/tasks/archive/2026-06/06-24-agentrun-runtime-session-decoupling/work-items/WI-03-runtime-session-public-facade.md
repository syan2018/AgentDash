# WI-03 RuntimeSession Public Facade

Status: done

Assigned Worker: Codex

## Tracking

- Files changed:
  - `crates/agentdash-application/src/session/mod.rs`
  - `.trellis/tasks/06-24-agentrun-runtime-session-decoupling/work-items/WI-03-runtime-session-public-facade.md`
- Tests run:
  - `cargo check -p agentdash-application`：通过；保留当前工作区 unused/dead_code warning。
  - `cargo check -p agentdash-api`：主集成后通过；保留当前工作区 unused/dead_code warning。
  - `rg -n "pub use crate::(agent_run|lifecycle)|pub use .*AgentRun|pub use .*Lifecycle|AgentFrameHookRuntime|WorkflowApplicationError" crates/agentdash-application/src/session/mod.rs`：无命中。
  - `rg -n "agentdash_application::session::(AgentFrameHookRuntime|WorkflowApplicationError|baseline_capabilities|bootstrap|plan::|runtime_transition_service|effects_service|hook_delegate|hook_events|hooks_service|post_turn_handler|runtime_builder::|runtime_commands::|runtime_control::|runtime_services::|terminal_effects::|title_generator|title_service::|tool_result_cache::|turn_processor::)" crates --glob '!crates/agentdash-application/**'`：无命中。
- Blockers:
  - 无。
- Handoff summary:
  - `session/mod.rs` 已移除 `AgentFrameHookRuntime` 与 `WorkflowApplicationError` public re-export。
  - `baseline_capabilities`、`bootstrap`、`runtime_transition_service`、`effects_service`、`hook_delegate`、`hook_events`、`hooks_service`、`plan`、`post_turn_handler`、`runtime_builder`、`runtime_commands`、`runtime_control`、`runtime_services`、`terminal_effects`、`title_generator`、`title_service`、`tool_result_cache`、`turn_processor` 已降为 `pub(crate)`，保留 root facade re-export 的 RuntimeSession substrate 类型供外部使用。
  - 仍保持 public module 的生产入口包括 `construction_planner`、`context`、`continuation`、`control`、`core`、`eventing`、`launch`、`persistence`、`stall_detector`、`terminal_cache`、`types`；其中 `construction_planner` / `context` / `terminal_cache` / `stall_detector` 仍有 API/local 直接依赖，后续 WI-05/WI-09 再迁移。

## Purpose

Reduce `session` to RuntimeSession delivery/trace substrate at the public facade level.

## Dependencies

- `WI-01`

## Scope

- Tighten `session/mod.rs` exports.
- Remove re-exports of AgentRun/Lifecycle ownership types.
- Make hub/tool/surface helper modules private or crate-private where possible.
- Keep public only RuntimeSession substrate use cases: core/eventing/control/runtime delivery/launch substrate/persistence/projection/terminal/tool result where needed.

## Out Of Scope

- Do not move launch/commit write ownership in this item; `WI-07` owns that behavior.

## Deliverables

- Updated `session` public facade.
- Import fixes for downstream modules.
- Documented allowed RuntimeSession public API list.

## Allowed RuntimeSession Public API

- Public use case modules: `continuation`、`control`、`core`、`eventing`、`launch`、`persistence`、`stall_detector`、`terminal_cache`、`types`。
- Current production-import public surface: `construction_planner`、`context`。
- Public root re-exports are limited to RuntimeSession delivery/trace/runtime substrate services, persistence records, launch command/result DTOs, context projection read models, terminal effects, tool result cache, and session state/value types.
- AgentRun / Lifecycle ownership types must be imported from their owning modules, not through `session`.

## Acceptance

- `session/mod.rs` does not public re-export AgentRun/Lifecycle ownership types.
- `cargo check -p agentdash-application` passes.
