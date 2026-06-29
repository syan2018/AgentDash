# Execution Plan

## Steps

- [x] Inspect current WorkspaceModule descriptor builder and list/describe call path.
- [x] Add typed `WorkspaceModuleOperationReadiness` contract and regenerate/check generated TS.
- [x] Introduce Gateway-backed runtime action operation projection for extension modules.
- [x] Keep Project extension projection only for extension/module ownership, tabs, channel methods, permissions and action-key ownership join.
- [x] Wire WorkspaceModule list/describe runtime context to call `RuntimeGateway::surface_for_actor`.
- [x] Preserve AgentRun effective capability filtering as module visibility owner.
- [x] Convert missing gateway/channel/backend/artifact/action cases into typed readiness diagnostics.
- [x] Remove frontend `projection.runtime_actions.find(...)` execution gate.
- [x] Add/update focused backend and frontend tests.
- [x] Run targeted checks.

## Validation

Suggested commands:

- `python ./.trellis/scripts/task.py validate .trellis/tasks/06-30-runtime-action-availability-split`
- `git diff --check`
- `cargo fmt --check --package agentdash-workspace-module --package agentdash-contracts`
- `cargo test -p agentdash-workspace-module workspace_module --lib`
- `pnpm run contracts:check` if contract/generated TS changes.
- Focused frontend bridge test command for `packages/app-web/src/features/extension-runtime/model/bridge.test.ts`.
- `rg -n "projection\\.runtime_actions\\.find" packages/app-web/src/features/extension-runtime`
- `rg -n "ExtensionRuntimeProjection.*runtime_actions|\\.runtime_actions" crates/agentdash-workspace-module/src/workspace_module`

Avoid full workspace Rust compile in implement workers. Check workers may run the targeted crate/package commands above.

Completed validation:

- `python ./.trellis/scripts/task.py validate .trellis/tasks/06-30-runtime-action-availability-split`
- `git diff --check`
- `cargo fmt --check --package agentdash-workspace-module --package agentdash-contracts`
- `cargo test -p agentdash-workspace-module workspace_module --lib`
- `pnpm run contracts:check`
- `pnpm --filter app-web exec vitest run src/features/extension-runtime/model/bridge.test.ts`
- `pnpm --filter app-web run typecheck`
- `rg -n "projection\\.runtime_actions\\.find|Extension action 不可用" packages/app-web/src/features/extension-runtime`
- `rg -n "ExtensionRuntimeProjection.*runtime_actions|\\.runtime_actions" crates/agentdash-workspace-module/src/workspace_module`

Check worker additionally ran `pnpm --filter app-web run lint`.

## Subagent Instructions

Use a `trellis-implement` worker for implementation and a `trellis-check` worker for verification.

Dispatch prompts must start with:

```text
Active task: .trellis/tasks/06-30-runtime-action-availability-split
```

All workers must repeat and follow the cleanup-first constraint: the goal is to remove the old Project-level action availability path, not to add another compatibility gate.

Implementation worker must not run broad Rust builds or full suites. It may run targeted searches, formatting for touched crates, contract generation/check when it changes contracts, and narrow tests if cheap.

## Risk Points

- `RuntimeActionDescriptor` does not carry `extension_key`. Use Project extension projection only as an ownership index, not as the schema/permission source.
- Existing tests may expect `build_workspace_modules(&projection, ...)` to include raw runtime actions. Update tests to reflect the new split: baseline builder without Gateway catalog should not claim manifest actions are ready executable operations.
- Contract changes require generated TS drift handling.
