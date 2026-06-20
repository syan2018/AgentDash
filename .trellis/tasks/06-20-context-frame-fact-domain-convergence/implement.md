# ContextFrame 事实域收束重构实施计划

## Work Item Tracking

本任务不拆 Trellis child task。实施切片统一由 [work-items.md](work-items.md) 和 `work-items/` 下的独立工作项文档跟踪。

## Phase 0: Freeze Contract

- [x] Review current local uncommitted changes and decide which belong to this task.
- [x] Finalize frame taxonomy: capability snapshot/delta, assignment, system guidelines, runtime control.
- [x] Decide whether `capability_state_update` is renamed or kept with explicit mode metadata.
- [x] Decide whether `ToolSchema` full section is revived as snapshot or removed.
- [x] Decide whether `ContextFrameSection::HookInjection` is removed or given a concrete producer.
- [x] Start with [WI-1](work-items/WI-1-context-frame-contract.md) after planning review.

## Phase 1: Backend Protocol Cleanup

- [x] Update `ContextFrameSection` protocol to match final taxonomy.
- [x] Remove or redefine residual section kinds and aliases.
- [x] Update contract samples and generated bindings if applicable.
- [x] Add a typed fallback or diagnostic strategy for unknown frontend sections if protocol generation remains manual.
- [x] Complete [WI-1](work-items/WI-1-context-frame-contract.md).

## Phase 2: Capability Domain

- [x] Make companion roster projection exclusively derive from `CapabilityState.companion.agents`.
- [x] Remove `companion_agents` assignment/hook slot remnants.
- [x] Split or annotate capability snapshot vs delta.
- [x] Ensure initial owner bootstrap exposes complete capability surface when required by UI.
- [x] Ensure runtime transitions emit only delta frame semantics.
- [x] Complete [WI-2](work-items/WI-2-capability-domain.md).

## Phase 3: Assignment / ProcedureContract Domain

- [x] Keep `WorkflowInjectionSpec.guidance` and `context_bindings` in assignment.
- [x] Route `capability_config` only through capability resolver.
- [x] Route hook rules through hook runtime / pending / trace.
- [x] Define workflow port visibility as a typed task delivery surface.
- [x] Remove `project_guidelines` and capability/runtime facts from assignment slots.
- [x] Decide `bootstrap_fragments` naming: keep as Bundle physical field; assignment semantics are expressed by `assignment_context`.
- [x] Complete [WI-3](work-items/WI-3-assignment-procedure-domain.md).

## Phase 4: Model Delivery And Usage

- [x] Audit every ContextFrame kind that can produce non-empty `rendered_text`.
- [x] Update `context_usage_items_from_context_frame` to cover all model-visible sections.
- [x] Ensure system prompt assembly and turn-start notice delivery agree with frame domain.
- [x] Convert audit-only fragments to audit scope.
- [x] Complete [WI-4](work-items/WI-4-delivery-usage.md).

## Phase 5: Frontend

- [x] Align parser and renderers with final backend section list.
- [x] Show CAP snapshot as current state and CAP delta as change log.
- [x] Keep assignment UI focused on task/workflow/instruction content.
- [x] Add visible fallback for unknown sections.
- [x] Complete [WI-5](work-items/WI-5-frontend-context-frame.md).

## Phase 6: Specs And Validation

- [x] Update backend capability spec with companion roster and CAP snapshot/delta contract.
- [x] Update cross-layer ContextFrame spec with frame domain taxonomy.
- [x] Run targeted backend tests for owner bootstrap, runtime transition, ProcedureContract projection and context usage.
- [x] Run frontend context frame tests and app-web check.
- [x] Run broader backend lib tests or document unrelated known failures.
- [x] Complete [WI-6](work-items/WI-6-spec-validation.md).

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
