# Execution Plan

## Steps

- [x] Inspect current RuntimeGateway provider/dynamic provider traits.
- [x] Add dynamic action discovery contract.
- [x] Implement concrete extension action discovery in extension dynamic provider.
- [x] Refactor extension provider to share resolver between discovery and invoke.
- [x] Update `RuntimeGateway::surface_for_actor` to merge static descriptors and dynamic descriptors.
- [x] Remove actor-visible marker descriptor from public surface.
- [x] Add focused tests.
- [x] Run targeted checks.

## Validation

Suggested commands:

- `cargo fmt --check`
- `cargo test -p agentdash-application-runtime-gateway --lib`
- Targeted tests by name once added, e.g. `dynamic` / `extension_action` / `surface_for_actor`.
- `rg -n "extension\\.runtime_action" crates/agentdash-application-runtime-gateway crates/agentdash-api crates/agentdash-workspace-module packages/app-web/src/features/extension-runtime`

Avoid full workspace compile unless a targeted package check requires it.

Completed validation:

- `python ./.trellis/scripts/task.py validate .trellis/tasks/06-30-runtime-gateway-dynamic-action-catalog`
- `git diff --check`
- `cargo fmt --check --package agentdash-application-runtime-gateway --package agentdash-api`
- `cargo test -p agentdash-application-runtime-gateway --lib`
- `cargo check -p agentdash-api`
- `rg -n "extension\\.runtime_action" crates/agentdash-application-runtime-gateway crates/agentdash-api crates/agentdash-workspace-module packages/app-web/src/features/extension-runtime`
- `rg -n "surface_for_actor\\(" crates packages`

## Subagent Instructions

Use a `trellis-implement` worker for implementation and a `trellis-check` worker for verification.

Dispatch prompts must start with:

```text
Active task: .trellis/tasks/06-30-runtime-gateway-dynamic-action-catalog
```

Workers must prioritize deleting the marker/catalog split over adding a parallel extension catalog.
