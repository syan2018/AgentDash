# LifecycleRun Active Projection Structure Implement Plan

## Checklist

- [ ] Add `ActiveActivityRef` domain/contract DTO.
- [ ] Decide whether refs are persisted or read-builder derived.
- [ ] Update `sync_graph_instance_activity_projections` or remove run-level active string persistence.
- [ ] Update lifecycle run view builder.
- [ ] Update infrastructure migration for field rename/removal if persisted.
- [ ] Update generated TS and frontend types/store usage.
- [ ] Remove business use of `current_activity_key()`.
- [ ] Add multi graph instance same key tests.

## Validation Commands

- [ ] `cargo test -p agentdash-domain workflow`
- [ ] `cargo test -p agentdash-application workflow::lifecycle_run_view_builder`
- [ ] `pnpm run contracts:check`
- [ ] `pnpm --filter app-web test`

## Risk Points

- Some old tests may assert string active keys; update them to assert structured identity.
- Avoid adding a second persisted active projection unless it has a clear synchronization owner.
