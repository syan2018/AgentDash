# WI-3 Assignment 与 ProcedureContract 投影收束

## Status

completed

## Goal

让 `assignment_context` 只表达任务语义，并将 `AgentProcedureContract` 的不同字段投影到对应事实域。

## Scope

- `WorkflowInjectionSpec.guidance` 和 `context_bindings` 进入 assignment。
- `capability_config` 进入 capability resolver。
- `hook_rules` 进入 hook runtime / pending action / trace。
- `input_ports` / `output_ports` 形成明确 workflow/task delivery 表达。
- 清理 `project_guidelines` assignment slot 与 `system_guidelines` 的重叠。
- 收束 `runtime_policy` 中混入 capability facts 的内容和 scope。

## Primary Files

- `crates/agentdash-domain/src/workflow/value_objects/contract.rs`
- `crates/agentdash-domain/src/workflow/value_objects/injection.rs`
- `crates/agentdash-application/src/context/workflow_bindings.rs`
- `crates/agentdash-application/src/context/rendering/workflow_injection.rs`
- `crates/agentdash-application/src/session/assignment_context_frame.rs`
- `crates/agentdash-application/src/session/assembler.rs`
- `crates/agentdash-application/src/hooks/active_workflow_contribution.rs`

## Acceptance

- [x] assignment frame 不承载能力事实或系统指引事实。
- [x] ProcedureContract 字段有明确投影目标。
- [x] workflow guidance/context bindings 仍模型可见。
- [x] ports 的 Agent 可见表达有稳定 section 或 assignment fragment 规则。

## Implementation Notes

- `ASSIGNMENT_CONTEXT_SLOTS` 保留 task/story/project/workflow/instruction 等任务语义 slot，项目指引由 `system_guidelines` frame 的 `project_guidelines` section 承载。
- lifecycle activation 的节点说明、input/output port 交付要求投影为 `workflow_context` assignment fragment，继续通过 `assignment_context` 进入模型。
- lifecycle activation 不再把 capability keys 拼进 RuntimeAgent 可见的 `runtime_policy` fragment；能力事实继续由 `CapabilityState` 派生的 CAP sections 表达。

## Validation

- `cargo test -p agentdash-application assignment_context_frame --lib`
- `cargo test -p agentdash-application build_session_plan_fragments --lib`
- `cargo test -p agentdash-application capability_surface_fragments_are_audit_only --lib`
- `cargo test -p agentdash-application lifecycle_context_contribution_contains_workflow_assignment_fragments --lib`

## Remaining Follow-up

- `bootstrap_fragments` 命名仍属于更大范围的 bundle 语义清理，未在本切片内改动。
