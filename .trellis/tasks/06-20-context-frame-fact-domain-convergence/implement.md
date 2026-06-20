# ContextFrame 事实域收束重构实施计划

## Work Item Tracking

本任务不拆 Trellis child task。实施切片统一由 [work-items.md](work-items.md) 和 `work-items/` 下的独立工作项文档跟踪。

## Phase 0: Freeze Contract

- [ ] Review current local uncommitted changes and decide which belong to this task.
- [ ] Finalize frame taxonomy: capability snapshot/delta, assignment, system guidelines, runtime control.
- [ ] Decide whether `capability_state_update` is renamed or kept with explicit mode metadata.
- [ ] Decide whether `ToolSchema` full section is revived as snapshot or removed.
- [ ] Decide whether `ContextFrameSection::HookInjection` is removed or given a concrete producer.
- [ ] Start with [WI-1](work-items/WI-1-context-frame-contract.md) after planning review.

## Phase 1: Backend Protocol Cleanup

- [ ] Update `ContextFrameSection` protocol to match final taxonomy.
- [ ] Remove or redefine residual section kinds and aliases.
- [ ] Update contract samples and generated bindings if applicable.
- [ ] Add a typed fallback or diagnostic strategy for unknown frontend sections if protocol generation remains manual.
- [ ] Complete [WI-1](work-items/WI-1-context-frame-contract.md).

## Phase 2: Capability Domain

- [ ] Make companion roster projection exclusively derive from `CapabilityState.companion.agents`.
- [ ] Remove `companion_agents` assignment/hook slot remnants.
- [ ] Split or annotate capability snapshot vs delta.
- [ ] Ensure initial owner bootstrap exposes complete capability surface when required by UI.
- [ ] Ensure runtime transitions emit only delta frame semantics.
- [ ] Complete [WI-2](work-items/WI-2-capability-domain.md).

## Phase 3: Assignment / ProcedureContract Domain

- [ ] Keep `WorkflowInjectionSpec.guidance` and `context_bindings` in assignment.
- [ ] Route `capability_config` only through capability resolver.
- [ ] Route hook rules through hook runtime / pending / trace.
- [ ] Define workflow port visibility as a typed task delivery surface.
- [ ] Remove `project_guidelines` and capability/runtime facts from assignment slots.
- [ ] Rename `bootstrap_fragments` if it continues to mean assignment fragments.
- [ ] Complete [WI-3](work-items/WI-3-assignment-procedure-domain.md).

## Phase 4: Model Delivery And Usage

- [ ] Audit every ContextFrame kind that can produce non-empty `rendered_text`.
- [ ] Update `context_usage_items_from_context_frame` to cover all model-visible sections.
- [ ] Ensure system prompt assembly and turn-start notice delivery agree with frame domain.
- [ ] Convert audit-only fragments to audit scope.
- [ ] Complete [WI-4](work-items/WI-4-delivery-usage.md).

## Phase 5: Frontend

- [ ] Align parser and renderers with final backend section list.
- [ ] Show CAP snapshot as current state and CAP delta as change log.
- [ ] Keep assignment UI focused on task/workflow/instruction content.
- [ ] Add visible fallback for unknown sections.
- [ ] Complete [WI-5](work-items/WI-5-frontend-context-frame.md).

## Phase 6: Specs And Validation

- [ ] Update backend capability spec with companion roster and CAP snapshot/delta contract.
- [ ] Update cross-layer ContextFrame spec with frame domain taxonomy.
- [ ] Run targeted backend tests for owner bootstrap, runtime transition, ProcedureContract projection and context usage.
- [ ] Run frontend context frame tests and app-web check.
- [ ] Run broader backend lib tests or document unrelated known failures.
- [ ] Complete [WI-6](work-items/WI-6-spec-validation.md).

## Suggested Validation Commands

```bash
pnpm --filter app-web run check
cargo fmt
cargo test -p agentdash-application owner_bootstrap --lib
cargo test -p agentdash-application runtime_context_transition --lib
cargo test -p agentdash-application assignment_context_frame --lib
cargo test -p agentdash-contracts context_usage --lib
```

## Risk Points

- ContextFrame payload changes touch backend contracts and frontend parser together.
- `rendered_text` is both model delivery and debug text; changing it can affect agent behavior.
- ProcedureContract projection crosses lifecycle, hook runtime, capability resolver and session launch.
- Existing dirty worktree contains related local edits; integration should separate already completed local fixes from broader protocol restructuring.
