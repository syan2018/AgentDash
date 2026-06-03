# Lifecycle 控制面概念一致性 Final Review Plan

## Dispatch Scope

This task is a review task. Dispatch it to a reviewer after the remaining implementation tasks are complete or explicitly marked non-blocking for merge.

## Checklist

- [ ] Read core concept documents:
  - `semantic-inventory.md`
  - `lifecycle-entity-association-map.md`
  - `agent-operation-predicates.md`
  - `agent-operation-predicate-comparison.md`
  - `refactor-plan.md`
- [ ] Verify current code against target model:
  - LifecycleRun / LifecycleAgent / AgentFrame / AgentAssignment
  - RuntimeSessionExecutionAnchor
  - WorkflowGraphInstance activity state
  - Subject associations
  - Agent / Lifecycle anchored runtime views
- [ ] Confirm remaining active tasks are correctly scoped:
  - `06-02-scoped-lifecycle-artifacts`
  - `06-02-lifecycle-run-active-projection-structure`
  - `06-03-database-business-semantic-convergence`
  - `06-02-lifecycle-control-plane-final-convergence`
- [ ] Run residual scans:
  - `rg "list_by_session|SessionBinding|lifecycle_step_key" crates packages`
  - `rg "active_node_keys|current_activity_key" crates packages .trellis/spec`
  - `rg "list_port_outputs|write_port_output|load_port_output_map|activity_outputs_from_port_map" crates`
  - `rg "WorkflowContract|step_key" crates packages .trellis/spec`
- [ ] Review `.trellis/spec/` and update only durable target invariants if drift is found.
- [ ] Write `final-review.md` with blocking findings and non-blocking follow-ups.

## Validation Commands

- [ ] `cargo check --workspace`
- [ ] `pnpm run contracts:check`
- [ ] `pnpm --filter app-web run typecheck`

## Review Gate

- [ ] The final review identifies no contradictory fact ownership across Session / Lifecycle / Agent / Frame / Assignment.
- [ ] Any remaining residual is assigned to an active task or documented as non-blocking follow-up.
- [ ] This task can be archived after `final-review.md` is accepted.
