# Design

## Boundary

`AgentRuntimeDelegate` is currently one broad trait. That shape forces adapters with narrow ownership to forward lifecycle methods they do not own. D6 converges the model by making each lifecycle concern a facet and composing facets into one runtime delegate set passed to the agent loop.

Owners:

- Hook runtime owns all hook-derived facets.
- AgentRun admission owns only tool policy.
- AgentRun mailbox owns only turn boundary.
- Agent loop owns calling the right facet at the right lifecycle point.
- Launch planning owns facet composition order for each prepared turn.

## Facet Model

Add these traits in `agentdash-agent-types/src/runtime/delegate.rs`:

- `RuntimeCompactionDelegate`: `evaluate_compaction`, `after_compaction`, `after_compaction_failed`.
- `RuntimeContextTransformDelegate`: `transform_context`.
- `RuntimeToolPolicyDelegate`: `before_tool_call`, `after_tool_call`.
- `RuntimeTurnBoundaryDelegate`: `after_turn`, `before_stop`.
- `RuntimeProviderObserverDelegate`: `on_before_provider_request`.

Add `AgentRuntimeDelegateSet` with optional facet fields and helper methods that preserve current defaults:

- absent compaction facet means no compaction decision and no-op notifications;
- absent context transform facet preserves provider-visible messages;
- absent tool policy facet allows and performs no after-tool mutation;
- absent turn boundary facet emits default turn/stop decisions;
- absent provider observer facet is no-op.

The old `AgentRuntimeDelegate` broad trait should be removed from production composition or reduced to a test-only/compat helper only if required by compiler migration. The desired final production shape is no broad forwarding wrapper.

## Composition

Composition should be explicit instead of nested broad wrappers:

1. Launch planner resolves hook runtime and builds hook delegate facets.
2. RuntimeSession admission wraps only the tool policy facet, preserving D1 order:
   - admission checks tool metadata and `AgentRunEffectiveCapabilityPort::admit_tool`;
   - deny returns before inner hook policy;
   - allow delegates to inner hook policy when present.
3. AgentRun mailbox wraps only the turn boundary facet:
   - after-turn hook steering can route through mailbox;
   - before-stop drains AgentRun mailbox boundary;
   - no compaction/context/tool/provider observer forwarding exists in mailbox.
4. Agent loop receives one `AgentRuntimeDelegateSet`.

## Data Flow

```text
LaunchPlanner
  -> HookRuntimeDelegate facets
  -> AgentRunAdmissionToolPolicyFacet(inner hook tool policy)
  -> AgentRunMailboxTurnBoundaryFacet(inner hook turn boundary)
  -> AgentRuntimeDelegateSet
  -> AgentLoopConfig.runtime_delegates
  -> streaming/tool_call/run_loop facet calls
```

## Non-Goals

- Do not redesign hook script semantics.
- Do not change command availability, mailbox policy, or admission business rules.
- Do not introduce new runtime lifecycle features.
- Do not keep a broad compatibility layer as the main production path.

## Risk

This is a shared Rust API change. Keep edits mechanical and scoped:

- first change the type/facet definitions and agent loop call sites;
- then convert hook/admission/mailbox implementers;
- then update launch composition;
- then fix tests/imports.

Avoid broad Rust builds in implement workers. The check phase can run targeted tests and one focused package check if needed.
