# LifecycleRun Active Projection Structure Implement Plan

## Context Recovery

- Start from `research/audit-context-index.md` after context compaction.
- Use that index as the required audit manifest for parent task context, sibling dependencies, specs, research files, code paths, migrations, frontend consumers, and validation commands.
- Treat this task as the parent convergence tail item: implement after runtime session anchors, scoped lifecycle artifacts, frontend session runtime query, and frame launch envelope have established their target facts.

## Checklist

- [ ] Add `ActiveActivityRef` domain/contract DTO.
- [ ] Decide whether refs are persisted or read-builder derived.
- [ ] Update `sync_graph_instance_activity_projections` or remove run-level active string persistence.
- [ ] Update lifecycle run view builder.
- [ ] Update infrastructure migration for field rename/removal if persisted.
- [ ] Update generated TS and frontend types/store usage.
- [ ] Audit session-indexed DTO/service exposure and keep active runtime state on Agent / Lifecycle anchored generated contracts.
- [ ] Remove business use of `current_activity_key()`.
- [ ] Add multi graph instance same key tests.

## Validation Commands

- [ ] `cargo test -p agentdash-domain workflow`
- [ ] `cargo test -p agentdash-application workflow::lifecycle_run_view_builder`
- [ ] `pnpm run contracts:check`
- [ ] `pnpm --filter app-web test`
- [ ] Focused frontend check for session runtime query consuming Agent / Lifecycle anchored generated types.

## Risk Points

- Some old tests may assert string active keys; update them to assert structured identity.
- Avoid adding a second persisted active projection unless it has a clear synchronization owner.
- Keep session-indexed endpoints as adapters that return Agent / Lifecycle anchored views; otherwise the project gains another public runtime model to keep synchronized.
