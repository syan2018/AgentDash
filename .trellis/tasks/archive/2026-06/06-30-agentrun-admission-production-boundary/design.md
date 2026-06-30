# AgentRun Admission Production Boundary Design

## Boundary

AgentRun owns final visible runtime capability view and execution admission. RuntimeSession may assemble and run tools, but it must ask AgentRun for the effective view/admission facts tied to the current runtime session anchor. Providers and concrete tools keep local invariants, but they are not the Grant authorization authority.

## Current Split To Converge

- `AgentRunEffectiveCapabilityPort::admit_tool` exists in `agentdash-application-ports`, and `AgentRunEffectiveCapabilityService::admit_tool` already evaluates visible `CapabilityState` plus frame-scoped grant projection.
- Product code has no non-test call to the port `admit_tool`; the real execution entry is the agent loop `prepare_tool_call` path through `AgentRuntimeDelegate::before_tool_call`.
- `RuntimeSessionEffectiveCapabilityPort::execution_capability_state_for_runtime_session` still returns `CapabilityState`. Quick convergence made it return the base state unchanged so grants do not pollute visible state, but the name still suggests execution admission.
- RuntimeSession launch/tool assembly currently consumes capability state for schema exposure. That usage can remain only as visible surface construction, not as authorization.

## Target Shape

1. Product adapter in `agentdash-application-agentrun`
   - Implement `AgentRunEffectiveCapabilityPort` around existing AgentRun frame/runtime anchor repositories and `AgentRunEffectiveCapabilityService`.
   - `effective_capability` resolves the runtime session anchor, anchored AgentFrame, and frame-scoped grant projection, then returns `AgentRunEffectiveCapabilityView`.
   - `admit_tool` resolves the runtime session anchor/frame target and delegates to `AgentRunEffectiveCapabilityService::admit_tool`.

2. Runtime admission bridge
   - Add a narrow adapter that implements the current wide `AgentRuntimeDelegate` only for tool policy where possible.
   - In `before_tool_call`, map `ToolCallInfo` / tool metadata into `AgentRunAdmissionRequest` and call `AgentRunEffectiveCapabilityPort::admit_tool`.
   - Return `ToolCallDecision::Deny` when admission denies. The existing agent loop already converts deny into immediate error result before `tool.execute`.
   - Keep hook-runtime delegate behavior and mailbox turn-boundary behavior composed without hiding AgentRun admission in provider-specific code.

3. Capability-state-only path
   - Rename/narrow the old RuntimeSession effective capability port if the blast radius is acceptable in this slice.
   - If deletion is too broad for D1, update names/comments/spec so it is explicitly schema-facing visible state. Do not reintroduce grant projection into returned `CapabilityState`.

4. Error and result semantics
   - Deny should be a normal tool-call denial with a typed/string reason surfaced through existing tool result semantics.
   - Repository failures may produce a runtime delegate error, which the agent loop already converts to a tool error result. Do not panic in providers/tools.

## Data Flow

```text
RuntimeSession launch
  -> constructs runtime_delegate with AgentRun admission adapter
Agent loop prepare_tool_call
  -> delegate.before_tool_call
  -> AgentRunEffectiveCapabilityPort::admit_tool
  -> AgentRunEffectiveCapabilityService::admit_tool(view, request)
  -> ToolCallDecision::Allow | Deny
  -> only Allow reaches tool.execute
```

## Non-Goals

- Do not implement the full D6 delegate-set split here.
- Do not add VFS mount/path authorization; that belongs to D9.
- Do not make provider-level `CapabilityState` checks the Grant owner.

## Decision Record

Decision state: self-decided.

AgentRun is the only boundary with run id, agent id, frame id, runtime session anchor, visible surface, and grant projection. Therefore admission belongs there. RuntimeSession is execution substrate and may carry the delegate adapter, but not the permission concept owner.
