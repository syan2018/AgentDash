# Design

## Boundary

D10 gives one owner to workspace placement facts:

- Runtime Gateway setup action owns physical detect execution.
- `WorkspacePlacementService` owns the application transaction that turns detect output into
  `WorkspaceDirectoryFact`, inventory rows and workspace bindings.
- API routes own auth, DTO parsing and response mapping only.
- Domain still owns `Workspace`, `WorkspaceBinding`, `BackendWorkspaceInventory` entities and
  repository traits.

This keeps cloud routes from owning local runtime invocation details while still preserving the rule
that cloud never directly reads the local filesystem.

## Current Evidence

Existing application helpers:

- `agentdash_application::workspace::detect_workspace_from_backend`
- `workspace_directory_fact_from_detection`
- `workspace_inventory_from_detection`
- `apply_workspace_directory_fact`
- `sync_project_backend_workspace_bindings`

Current route-local orchestration:

- `backend_access.rs::register_project_backend_inventory` calls route-local `invoke_workspace_detect`,
  creates inventory from detection and upserts it.
- `workspaces.rs::create_workspace` and `update_workspace` call `derive_workspace_shape` /
  `hydrate_workspace_bindings`, which route-locally detect and collect inventory rows.
- `workspaces.rs::bind_discovered` loops over commands, calls route-local detect, validates identity,
  upserts inventory and mutates workspace bindings.
- `workspaces.rs` has another route-local `invoke_workspace_detect`.

## Proposed Service

Place the service under `crates/agentdash-application/src/workspace/placement.rs` and export from
`workspace/mod.rs`.

Core shape:

```rust
pub struct WorkspacePlacementService {
    repos: RepositorySet,
    runtime_gateway: Arc<dyn WorkspacePlacementRuntime>,
}

pub enum WorkspacePlacementIntent {
    ManualRegisterInventory,
    BindDiscovered,
    CreateOrUpdateWorkspace,
    SyncCandidateInventory,
    AdvancedBindingOnly,
}
```

The exact type names can change to match existing crate style. The important contract is that each
public method describes one placement use case and returns application structs that route code can
map to existing contract DTOs.

## Implemented Shape

The implemented owner is `crates/agentdash-application/src/workspace/placement.rs`.

Public methods:

- `register_backend_inventory(RegisterBackendInventoryInput)` loads and validates the
  `ProjectBackendAccess`, invokes the runtime detect port and upserts `manual_register` inventory.
- `create_workspace(CreateWorkspacePlacementInput)` derives identity from explicit identity or the
  first binding, hydrates bindings through detect, upserts inventory and creates the Workspace.
- `update_workspace(UpdateWorkspacePlacementInput)` updates scalar Workspace fields and, when
  bindings are provided, hydrates them through detect before replacing the Workspace binding set.
- `bind_discovered(BindDiscoveredWorkspaceBindingsInput)` validates one active local backend,
  redetects each selected root, verifies identity including relaxed P4 discovery matching, upserts
  `identity_discovery` inventory and applies directory facts to touched Workspaces.

The low-level directory fact helpers remain in `workspace/backend_sync.rs` but are no longer
re-exported from `agentdash_application::workspace`; this keeps fact projection as application
workspace internals rather than a route-level API.

## Runtime Detect Adapter

Do not make application depend on `agentdash-application-runtime-gateway` concrete types. Introduce
or reuse a narrow port for:

```rust
async fn detect_workspace(project_id, backend_id, root_ref, user_id) -> WorkspaceDetectionResult
```

API can implement the port by calling `state.services.runtime_gateway.invoke(...)`. This moves route
helpers into an application-facing adapter while keeping setup action invocation outside route code.

If an existing application runtime gateway port already fits, reuse it instead of adding another
adapter.

## Placement Methods

Recommended methods:

- `register_backend_inventory(input)`:
  - validates active ProjectBackendAccess provided by caller or loads it through repos;
  - detects root;
  - upserts `BackendWorkspaceInventory` with `ManualRegister`;
  - returns inventory.

- `derive_workspace_shape(input)`:
  - parses initial bindings supplied by API as already-domain `WorkspaceBinding` values;
  - detects and hydrates bindings;
  - returns identity kind/payload, hydrated bindings, inventory rows.

- `update_workspace_bindings(input)`:
  - detects and hydrates provided bindings against current identity;
  - returns hydrated bindings and inventory rows.

- `bind_discovered(input)`:
  - enforces single local backend where current API does;
  - re-detects each root;
  - validates identity with strict or relaxed P4 discovery match;
  - upserts inventory and applies directory facts to workspaces.

- `sync_project_backend_workspace_bindings(input)`:
  - may remain as current application function if route already calls it, but should share fact apply
    helpers and service naming so it is part of the same owner surface.

Implemented note: sync remains `sync_project_backend_workspace_bindings(&RepositorySet, project_id)`
because it consumes existing inventory snapshots rather than invoking detect. It shares
`WorkspaceDirectoryFact` and `apply_workspace_directory_fact` with the placement service inside the
application workspace module.

## Non-Goals

- Do not redesign ProjectBackendAccess grant policy.
- Do not change Runtime Gateway setup action schema.
- Do not change frontend API routes unless route DTOs become provably wrong.
- Do not turn browser directory browse into placement; browse remains setup UI capability without DB
  transaction.

## Validation

- Service unit tests with fake repos/runtime detect adapter.
- API adapter tests for route auth/DTO behavior where existing route tests exist.
- Static search proving route-local `invoke_workspace_detect` is removed.
- Existing workspace/backend access tests remain green.
