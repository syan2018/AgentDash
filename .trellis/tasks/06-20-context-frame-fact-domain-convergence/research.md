# Initial Research Notes

## Subagent Findings

Three read-only subagents audited the module from separate angles:

- Capability / companion facts.
- Assignment / HookInjection / ProcedureContract.
- ContextFrame protocol / frontend / usage statistics.

## Capability And Companion

- `CapabilityState.companion.agents` is the runtime companion roster fact source.
- `CapabilityResolver::resolve_checked` gates roster by `collaboration` capability and authority.
- `AgentFrame.effective_capability_json` persists the full capability state.
- Runtime tools consume `ExecutionContext.turn.capability_state.companion.agents`.
- Frontend now has parser/renderer support for `companion_agent_roster_delta`.
- Residual protocol artifacts still imply `companion_agents` can be an assignment slot.

## Assignment And ProcedureContract

- `WorkflowInjectionSpec` contains `guidance` and `context_bindings`.
- Active workflow projection produces `HookInjection { slot: "workflow" }` for step summary and guidance.
- Workflow bindings produce `ContextFragment { slot: "workflow_context" }`.
- Assignment frame is currently the standard model-visible surface for these task semantics.
- `AgentProcedureContract.capability_config` belongs to capability resolution, not assignment.
- `hook_rules` belong to hook runtime / pending / trace.
- `input_ports` and `output_ports` need a clear task delivery projection.

## ContextFrame Protocol

- `ContextFrame` combines `rendered_text` and structured `sections`.
- `rendered_text` can be model-visible through turn-start notice or connector rendering.
- Frontend parser/renderers currently cover known backend section kinds after local frontend edits, but manual parser drift remains a risk.
- `context_usage_items_from_section` does not cover several model-visible CAP dimensions.
- CAP frame split into `capability_state_snapshot` for initial/bootstrap and `capability_state_delta` for live transition semantics.

## Candidate Cleanup Items

- `companion_agents` assignment slot and hook order.
- `project_guidelines` assignment slot.
- direct `HookInjection` ContextFrame section.
- `ToolSchema` full section if no full snapshot producer is defined.
- `RUNTIME_AGENT_CONTEXT_SLOTS` alias.
- `bootstrap_context` comments and `bootstrap_fragments` naming.
- `runtime_policy` fragments that carry capability facts while scoped to RuntimeAgent.
