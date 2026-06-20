# WI-3 Assignment 与 ProcedureContract 投影收束

## Status

planned

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

- [ ] assignment frame 不承载能力事实或系统指引事实。
- [ ] ProcedureContract 字段有明确投影目标。
- [ ] workflow guidance/context bindings 仍模型可见。
- [ ] ports 的 Agent 可见表达有稳定 section 或 assignment fragment 规则。

