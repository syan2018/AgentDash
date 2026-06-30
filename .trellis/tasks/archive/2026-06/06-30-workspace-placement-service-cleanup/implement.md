# Implementation Plan

## Operating Rules

- Follow Trellis workflow and start this task before code edits.
- Every subagent prompt must start with `Active task: .trellis/tasks/06-30-workspace-placement-service-cleanup`.
- Cleanup-first constraint: this review exists to converge architecture from first principles. Removing
  route-local detect/fact transaction forks is more important than adding feature surface.
- Do not keep route-local `invoke_workspace_detect` helpers as compatibility paths.
- Implementation workers must not run broad Rust builds or full suites. Use scoped `rg`, format and
  targeted tests.

## Research Split

1. Route transaction map
   - Map backend inventory register, workspace create/update, bind-discovered and sync routes.
   - Identify current DTO parsing vs business transaction boundaries.

2. Application helper and repository map
   - Map `backend_sync.rs`, `detection.rs`, `RepositorySet`, backend access repository methods and
     workspace repository methods.
   - Identify fake-test patterns for application services.

3. Runtime Gateway setup adapter map
   - Map existing runtime gateway invocation adapters and ports.
   - Decide whether to introduce a narrow `WorkspacePlacementRuntime` port or reuse an existing one.

## Ordered Implementation

1. [x] Add application placement module and method types.
2. [x] Move detect invocation behind an application-facing runtime adapter.
3. [x] Convert manual backend inventory register route.
4. [x] Convert workspace create/update binding hydration.
5. [x] Convert bind-discovered transaction.
6. [x] Align sync candidate inventory with the application fact owner.
7. [x] Remove route-local detect helpers and duplicated identity/fact helpers from routes.
8. [x] Add focused service tests and update specs.
9. [x] Run targeted checks and commit as one D10 slice.

## Implementation Notes

- `crates/agentdash-api/src/workspace_placement_runtime.rs` adapts Runtime Gateway
  `workspace.detect` to the application `WorkspacePlacementRuntime` port.
- `backend_access.rs::register_project_backend_inventory` now delegates detect + inventory upsert to
  `WorkspacePlacementService`.
- `workspaces.rs::create_workspace`, `update_workspace` and `bind_discovered` now delegate placement
  writes to `WorkspacePlacementService`; route-local placement helpers were removed.
- `detect_workspace` / `detect_git` keep `invoke_workspace_setup_detect*` helpers because they are
  setup query endpoints without inventory/binding write transactions.
- `backend_sync` fact helpers are application workspace internals rather than public workspace
  re-exports.

## Validation Results

Final targeted pass:

- `python ./.trellis/scripts/task.py validate .trellis/tasks/06-30-workspace-placement-service-cleanup`
  passed.
- `git diff --check` passed.
- `cargo fmt --check --package agentdash-application --package agentdash-api` passed.
- `cargo test -p agentdash-application workspace_placement --lib` passed: 8 tests.
- `cargo test -p agentdash-api backend_inventory --lib` passed as an API targeted compile filter:
  0 matched tests.
- Static route search for old placement helpers returned no matches.
- `trellis-check` review agent confirmed the diff removes route-local placement write transactions and
  leaves setup detect endpoints as non-writing queries.

## Suggested Subagent Split

- Research A: route transaction map and adapter boundary.
- Research B: application helper/repo/test pattern map.
- Implement A: application placement service and manual register/create-update conversion.
- Implement B: bind-discovered/sync conversion after service API lands.
- Check: verify old route-local detect/fact paths are gone and route/application boundary matches specs.

## Validation Commands

Adjust exact filters after implementation:

```powershell
python ./.trellis/scripts/task.py validate .trellis/tasks/06-30-workspace-placement-service-cleanup
git diff --check
cargo fmt --check --package agentdash-application --package agentdash-api
cargo test -p agentdash-application workspace_placement --lib
cargo test -p agentdash-api backend_inventory --lib
cargo test -p agentdash-api bind_discovered --lib
rg -n "invoke_workspace_detect|workspace_directory_fact_from_detection|workspace_inventory_from_detection|apply_workspace_directory_fact" crates/agentdash-api/src/routes
```
