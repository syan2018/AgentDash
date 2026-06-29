# Research: application service boundary for WorkspacePlacementService

- Query: application service/repository/test boundary for D10 WorkspacePlacementService; converge `workspace.detect -> WorkspaceDirectoryFact -> inventory/binding` under an application owner.
- Scope: internal
- Date: 2026-06-30

## Findings

### Files Found

- `AGENTS.md` - project working rules: Chinese communication, cleanup-first pre-release posture, no broad wasteful testing, do not touch parallel work.
- `.trellis/workflow.md` - Trellis phase and research persistence rules; research belongs under task `research/`.
- `.trellis/tasks/06-30-workspace-placement-service-cleanup/prd.md` - D10 requirements and acceptance criteria for moving route-local detect/fact transactions into application.
- `.trellis/tasks/06-30-workspace-placement-service-cleanup/design.md` - proposed service owner, runtime detect adapter boundary, method list and non-goals.
- `.trellis/tasks/06-30-workspace-placement-service-cleanup/implement.md` - implementation ordering, research split and validation command constraints.
- `.trellis/tasks/06-30-workspace-placement-service-cleanup/implement.jsonl` - curated context already includes D10 source review, cross-layer workspace routing spec, backend architecture and current route/application files.
- `.trellis/spec/backend/architecture.md` - API owns auth/DTO/error mapping; business orchestration enters application.
- `.trellis/spec/backend/repository-pattern.md` - repositories are aggregate ports; cross-aggregate consistency should be explicit command/use-case boundaries.
- `.trellis/spec/backend/error-handling.md` - application service boundary should return `ApplicationError` or structured local error convertible to it.
- `.trellis/spec/backend/quality-guidelines.md` - async/error/DTO conventions; no broad panic patterns.
- `.trellis/spec/cross-layer/project-backend-workspace-routing.md` - authoritative contract for backend inventory registration, workspace detect, identity discovery and binding confirmation.
- `crates/agentdash-application/src/workspace/backend_sync.rs` - current directory fact, inventory projection, fact apply, candidate listing and sync helpers.
- `crates/agentdash-application/src/workspace/detection.rs` - current detect result model and `BackendTransport` based workspace detector.
- `crates/agentdash-application/src/workspace/mod.rs` - currently re-exports fact/inventory/apply helpers broadly.
- `crates/agentdash-application/src/repository_set.rs` - application-level `RepositorySet` holds workspace, backend, access and inventory repository ports.
- `crates/agentdash-application/src/error.rs` - shared `ApplicationError` already maps domain/database/runtime-adjacent semantics for API.
- `crates/agentdash-application-ports/src/runtime_gateway_setup.rs` - setup action DTOs and `WorkspaceDetectSetupPort`; useful type reference, but not ideal as placement service's full dependency.
- `crates/agentdash-api/src/routes/backend_access.rs` - route-local manual inventory register transaction and route-local `invoke_workspace_detect`.
- `crates/agentdash-api/src/routes/workspaces.rs` - route-local create/update binding hydration, bind-discovered transaction, relaxed P4 matching, and detect invocation helpers.
- `crates/agentdash-domain/src/backend/repository.rs` - backend access and inventory repository port methods needed by placement service.
- `crates/agentdash-domain/src/workspace/repository.rs` - `WorkspaceRepository` aggregate port over `Workspace + bindings`.
- `crates/agentdash-domain/src/backend/entity.rs` - backend type, project access status, inventory source/status domain types.
- `crates/agentdash-domain/src/workspace/entity.rs` - `Workspace` aggregate owns binding list and `set_bindings`/default binding refresh.

### Current Boundary Evidence

- API route ownership is currently too wide. `backend_access.rs::register_project_backend_inventory` loads access, checks active status, invokes `workspace.detect`, projects inventory and upserts it in one route handler (`crates/agentdash-api/src/routes/backend_access.rs:246`, `crates/agentdash-api/src/routes/backend_access.rs:267`, `crates/agentdash-api/src/routes/backend_access.rs:275`, `crates/agentdash-api/src/routes/backend_access.rs:282`).
- `backend_access.rs` has its own private `invoke_workspace_detect` that constructs `RuntimeInvocationRequest` and deserializes `WorkspaceDetectionResult` (`crates/agentdash-api/src/routes/backend_access.rs:426`, `crates/agentdash-api/src/routes/backend_access.rs:438`, `crates/agentdash-api/src/routes/backend_access.rs:453`).
- `workspaces.rs::create_workspace` calls route-local `derive_workspace_shape`, then route-upserts inventory and creates the workspace (`crates/agentdash-api/src/routes/workspaces.rs:108`, `crates/agentdash-api/src/routes/workspaces.rs:124`, `crates/agentdash-api/src/routes/workspaces.rs:150`, `crates/agentdash-api/src/routes/workspaces.rs:157`).
- `workspaces.rs::update_workspace` maps DTOs into domain bindings, calls route-local hydration, route-upserts inventory and updates workspace (`crates/agentdash-api/src/routes/workspaces.rs:184`, `crates/agentdash-api/src/routes/workspaces.rs:215`, `crates/agentdash-api/src/routes/workspaces.rs:221`, `crates/agentdash-api/src/routes/workspaces.rs:242`, `crates/agentdash-api/src/routes/workspaces.rs:249`).
- `workspaces.rs::bind_discovered` parses and deduplicates binding commands, enforces single backend/local access, redetects, identity-checks, upserts inventory and applies facts to workspaces in route code (`crates/agentdash-api/src/routes/workspaces.rs:473`, `crates/agentdash-api/src/routes/workspaces.rs:491`, `crates/agentdash-api/src/routes/workspaces.rs:519`, `crates/agentdash-api/src/routes/workspaces.rs:539`, `crates/agentdash-api/src/routes/workspaces.rs:555`, `crates/agentdash-api/src/routes/workspaces.rs:573`, `crates/agentdash-api/src/routes/workspaces.rs:579`, `crates/agentdash-api/src/routes/workspaces.rs:594`).
- `workspaces.rs` duplicates route-local detect invocation (`crates/agentdash-api/src/routes/workspaces.rs:941`, `crates/agentdash-api/src/routes/workspaces.rs:953`, `crates/agentdash-api/src/routes/workspaces.rs:968`). `detect_git` and `discover_by_identity` are setup/query capabilities and can remain route/runtime setup paths if they do not perform DB placement transactions (`crates/agentdash-api/src/routes/workspaces.rs:351`, `crates/agentdash-api/src/routes/workspaces.rs:655`).
- Current application helper functions already express the core fact transforms: inventory projection (`crates/agentdash-application/src/workspace/backend_sync.rs:50`), directory fact projection (`crates/agentdash-application/src/workspace/backend_sync.rs:70`), identity match (`crates/agentdash-application/src/workspace/backend_sync.rs:104`), apply fact (`crates/agentdash-application/src/workspace/backend_sync.rs:118`), candidate listing (`crates/agentdash-application/src/workspace/backend_sync.rs:164`) and sync candidate bindings (`crates/agentdash-application/src/workspace/backend_sync.rs:177`).
- Those helpers are currently re-exported broadly from `workspace/mod.rs`, including low-level projection/apply helpers (`crates/agentdash-application/src/workspace/mod.rs:5`, `crates/agentdash-application/src/workspace/mod.rs:10`). This is the main cleanup target after service lands.
- `RepositorySet` already contains every repository the service needs: backend repo, project backend access repo, inventory repo, workspace repo (`crates/agentdash-application/src/repository_set.rs:59`, `crates/agentdash-application/src/repository_set.rs:62`, `crates/agentdash-application/src/repository_set.rs:65`, `crates/agentdash-application/src/repository_set.rs:66`).
- Domain repository boundaries are narrow aggregate ports: `WorkspaceRepository` writes workspace aggregate plus bindings (`crates/agentdash-domain/src/workspace/repository.rs:6`, `crates/agentdash-domain/src/workspace/repository.rs:9`, `crates/agentdash-domain/src/workspace/repository.rs:12`); inventory repository only upserts/lists inventory facts (`crates/agentdash-domain/src/backend/repository.rs:126`, `crates/agentdash-domain/src/backend/repository.rs:128`, `crates/agentdash-domain/src/backend/repository.rs:129`).
- `ProjectBackendAccessRepository` has both exact access lookup and active project/backend lookup (`crates/agentdash-domain/src/backend/repository.rs:93`, `crates/agentdash-domain/src/backend/repository.rs:102`), so service can support both access-id and backend-id intents.
- API already maps `agentdash_application::ApplicationError` to `ApiError` (`crates/agentdash-api/src/rpc.rs:150`), so placement service should use `ApplicationError` rather than adding route-specific errors.

### Recommended Service API Shape

Create `crates/agentdash-application/src/workspace/placement.rs` and export only the service and public command/result DTOs from `workspace/mod.rs`.

Recommended service dependency shape:

```rust
pub struct WorkspacePlacementService {
    repos: RepositorySet,
    runtime: Arc<dyn WorkspacePlacementRuntime>,
}

#[async_trait::async_trait]
pub trait WorkspacePlacementRuntime: Send + Sync {
    async fn detect_workspace(
        &self,
        input: WorkspacePlacementDetectInput,
    ) -> Result<WorkspaceDetectionResult, WorkspacePlacementRuntimeError>;
}
```

The service should own current placement intents as public methods instead of one generic enum dispatcher. A method-per-use-case keeps route call sites clear and avoids the additive-wrapper smell.

Recommended public methods:

- `register_backend_inventory(input: RegisterBackendInventoryInput) -> Result<RegisterBackendInventoryResult, ApplicationError>`
  - Inputs: `project_id`, `access_id`, `user_id`, `root_ref`.
  - Service responsibilities: trim/validate `root_ref`, load `ProjectBackendAccess` by id, require project match, require active, call runtime detect, build inventory with `BackendWorkspaceInventorySource::ManualRegister`, upsert, return inventory.
  - Route responsibility after migration: project edit auth, UUID parsing, request/response DTO mapping.

- `create_workspace_shape(input: CreateWorkspaceShapeInput) -> Result<CreateWorkspaceShapeResult, ApplicationError>`
  - Inputs: `project_id`, `user_id`, optional `identity_kind`, optional raw `identity_payload`, domain-level seed bindings already parsed from API DTO, optional shortcut binding already merged by route or represented as `Vec<WorkspaceBinding>`.
  - Output: `identity_kind`, normalized `identity_payload`, hydrated `bindings`, `inventory_items`, `warnings`.
  - Service responsibilities: infer identity from first binding when no explicit identity is supplied, hydrate all binding facts via detect, validate identity match, collect inventory facts. Workspace entity construction can either remain in route or move into a larger `create_workspace` method. Cleanup-first preferred shape is a full `create_workspace(input) -> Workspace` method if it avoids leaving route as inventory transaction owner.

- `update_workspace_bindings(input: UpdateWorkspaceBindingsInput) -> Result<UpdateWorkspaceBindingsResult, ApplicationError>`
  - Inputs: existing `Workspace`, `user_id`, next domain `WorkspaceBinding` list.
  - Output: updated `Workspace` or hydrated bindings plus inventory items. To keep route thin, prefer returning the updated `Workspace` after service sets bindings/status/default binding and persists both inventory and workspace.
  - Service responsibilities: ensure each backend is authorized for project, detect/hydrate facts, validate identity, upsert inventory, update workspace.

- `bind_discovered(input: BindDiscoveredWorkspaceBindingsInput) -> Result<BindDiscoveredWorkspaceBindingsResult, ApplicationError>`
  - Inputs: `project_id`, `user_id`, command list `{ workspace_id, backend_id, root_ref }`.
  - Service responsibilities: reject empty commands, dedupe, require single backend, require active project backend access, require backend type local, load project workspaces, redetect each root, perform strict-or-relaxed discovery identity match, upsert identity-discovery inventory, apply facts, persist touched workspaces, return updated workspaces, bound ids, created/updated counts, inventory items and warnings.
  - Route responsibility: auth/DTO parsing/response mapping only.

- `sync_candidate_inventory(input: SyncCandidateInventoryInput) -> Result<WorkspaceBindingSyncResult, ApplicationError>`
  - Wrap or move `sync_project_backend_workspace_bindings`; keep it on the same service owner surface because it applies inventory facts into workspace bindings.

- `hydrate_binding_only(input: HydrateBindingOnlyInput) -> Result<HydrateBindingOnlyResult, ApplicationError>`
  - This is the "advanced binding-only" use case. It should accept already-domain bindings and avoid detect side effects only when caller explicitly edits workspace bindings as advanced maintenance. It should still use the same status/default-binding helper and avoid inventory upsert.

Recommended structs should reuse domain types rather than mirrored DTOs:

```rust
pub struct WorkspacePlacementDetectInput {
    pub project_id: Option<Uuid>,
    pub workspace_id: Option<Uuid>,
    pub user_id: Option<String>,
    pub backend_id: String,
    pub root_ref: String,
}

pub struct RegisterBackendInventoryInput {
    pub project_id: Uuid,
    pub access_id: Uuid,
    pub user_id: Option<String>,
    pub root_ref: String,
}

pub struct BindDiscoveredWorkspaceBindingCommand {
    pub workspace_id: Uuid,
    pub backend_id: String,
    pub root_ref: String,
}

pub struct BindDiscoveredWorkspaceBindingsResult {
    pub backend_id: String,
    pub workspaces: Vec<Workspace>,
    pub bound_workspace_ids: Vec<Uuid>,
    pub created_bindings: usize,
    pub updated_bindings: usize,
    pub inventory_items: Vec<BackendWorkspaceInventory>,
    pub warnings: Vec<String>,
}
```

The service can have smaller private helper structs such as `DetectedDirectoryFact { fact, detected }` but public result types should be route-mappable without exposing route DTOs.

### Runtime Detect Port Decision

Add a narrow runtime detect port in `crates/agentdash-application/src/workspace/placement.rs` or a nearby `workspace/runtime.rs`; implement it in API composition code using `state.services.runtime_gateway.invoke`.

Reasoning:

- Existing `WorkspaceDetectSetupPort` lives in `crates/agentdash-application-ports/src/runtime_gateway_setup.rs:157` and only accepts `WorkspaceDetectInput { backend_id, root_ref }` (`crates/agentdash-application-ports/src/runtime_gateway_setup.rs:42`). It does not express platform actor user id or setup context project/workspace/root, which route-local invocation currently sets (`crates/agentdash-api/src/routes/workspaces.rs:957`, `crates/agentdash-api/src/routes/workspaces.rs:960`).
- Existing setup port is implemented by `ApplicationWorkspaceSetupPort` over `BackendTransport` in API bootstrap (`crates/agentdash-api/src/bootstrap/runtime_gateway.rs:114`, `crates/agentdash-api/src/bootstrap/runtime_gateway.rs:125`). Reusing that port directly would bypass Runtime Gateway actor/context invocation semantics, while the D10 design wants runtime gateway setup action to still own physical detect execution.
- `WorkspaceDetectionResult` already exists in `agentdash-application::workspace` (`crates/agentdash-application/src/workspace/detection.rs:17`) and is shape-compatible with `WorkspaceDetectOutput` (`crates/agentdash-application-ports/src/runtime_gateway_setup.rs:48`). The new port can return `WorkspaceDetectionResult` and the API adapter can deserialize gateway output into it.
- Although `agentdash-application` currently depends on `agentdash-application-runtime-gateway` in `Cargo.toml`, the lower-risk convergence is still to keep `WorkspacePlacementService` depending on a tiny trait, not on `RuntimeGateway`, `RuntimeInvocationRequest`, `RuntimeActor` or `RuntimeContext`. This preserves testability and lets route-local detect helpers disappear without moving gateway request construction into the service crate.

Exact files:

- Add trait and service: `crates/agentdash-application/src/workspace/placement.rs`.
- Export service surface: `crates/agentdash-application/src/workspace/mod.rs`.
- Add API adapter near the current route adapter layer, preferably `crates/agentdash-api/src/workspace_placement_runtime.rs` or inside `crates/agentdash-api/src/routes/workspaces.rs` only as a small adapter type, not helper functions. It should be shared by `backend_access.rs` and `workspaces.rs`.
- Adapter uses `WORKSPACE_DETECT_ACTION`, `WorkspaceDetectInput`, `RuntimeInvocationRequest`, `RuntimeActor::PlatformUser`, `RuntimeContext::Setup`, and `state.services.runtime_gateway.invoke`, which are the same concrete calls now duplicated in routes.

### Helper Function Move / Visibility Recommendation

- Move into private service helpers:
  - `workspace_inventory_from_detection` currently public at `crates/agentdash-application/src/workspace/backend_sync.rs:50`. It should become `fn` private to `placement.rs` or `backend_sync.rs` and only be reached through service methods.
  - `workspace_directory_fact_from_detection` currently public at `crates/agentdash-application/src/workspace/backend_sync.rs:70`. It should become private service/fact projection helper.
  - `apply_workspace_directory_fact` currently public at `crates/agentdash-application/src/workspace/backend_sync.rs:118`. It should become `pub(crate)` or private to placement/backend_sync. Route code should never call it after D10.
  - `directory_fact_matches_identity` currently public at `crates/agentdash-application/src/workspace/backend_sync.rs:104`. It should become private placement validation helper unless other application modules use it after route cleanup.
  - Route-local `derive_workspace_shape`, `hydrate_workspace_bindings`, `detect_workspace_binding_fact` should move from `crates/agentdash-api/src/routes/workspaces.rs:735`, `crates/agentdash-api/src/routes/workspaces.rs:838`, `crates/agentdash-api/src/routes/workspaces.rs:863` into placement service.
  - Route-local `discovery_identity_payload_matches` and `relaxed_p4_discovery_payload` should move from `crates/agentdash-api/src/routes/workspaces.rs:694` and `crates/agentdash-api/src/routes/workspaces.rs:723` into service policy helpers because they define write-time binding acceptance semantics.
  - Route-local `ensure_local_project_backend_access` should move from `crates/agentdash-api/src/routes/workspaces.rs:625` into service because it combines access and backend type policy used by bind-discovered.

- Remain public application APIs:
  - `WorkspacePlacementService` methods and their input/result structs.
  - `list_project_workspace_candidates` can remain public if routes still list candidates without mutation (`crates/agentdash-application/src/workspace/backend_sync.rs:164`).
  - `sync_project_backend_workspace_bindings` should either become `WorkspacePlacementService::sync_candidate_inventory` or remain as a thin deprecated-in-practice application function that delegates to the service. Cleanup-first preference is to move it under the service API and stop re-exporting it as a free function.
  - `derive_workspace_status_from_bindings` can remain public only if non-placement application modules need it; otherwise make it `pub(crate)` and let placement service own status recomputation.
  - `detect_workspace_from_backend` in `detection.rs` should remain public because `ApplicationWorkspaceSetupPort` uses it to implement the physical setup action (`crates/agentdash-api/src/bootstrap/runtime_gateway.rs:130`).

- Remove from `workspace/mod.rs` public re-export after migration:
  - `workspace_inventory_from_detection`, `workspace_directory_fact_from_detection`, `apply_workspace_directory_fact`, `directory_fact_matches_identity`, `workspace_matches_directory_fact` (`crates/agentdash-application/src/workspace/mod.rs:5`, `crates/agentdash-application/src/workspace/mod.rs:10`).

### Focused Tests and Feasible Fakes

Recommended service tests should live in `crates/agentdash-application/src/workspace/placement.rs` under `#[cfg(test)]`. Use scoped `cargo test -p agentdash-application workspace_placement --lib` later; do not run broad builds in this research step.

Fake infrastructure pattern:

- Use module-local `InMemory*Repository` structs with `tokio::sync::Mutex` for async repository fakes, matching `backend/project_access.rs` (`crates/agentdash-application/src/backend/project_access.rs:158`, `crates/agentdash-application/src/backend/project_access.rs:186`) and avoiding shared test framework churn.
- For small non-async ports, simple `MockAvailability` style structs are common (`crates/agentdash-application/src/workspace/resolution.rs:184`, `crates/agentdash-application/src/workspace/resolution.rs:188`).
- For repository traits with unused methods, existing tests sometimes use `unreachable!("测试未使用")` in methods not exercised (`crates/agentdash-application/src/backend/authorization.rs:339`). Use this sparingly; for placement service, implement the methods the public method under test can actually call.
- A full `RepositorySet` requires many unrelated ports (`crates/agentdash-application/src/repository_set.rs:55` through `crates/agentdash-application/src/repository_set.rs:100`). To reduce fake burden, prefer either:
  - a small `WorkspacePlacementRepositories` struct in placement module containing only `workspace_repo`, `backend_repo`, `project_backend_access_repo`, `backend_workspace_inventory_repo`; or
  - `WorkspacePlacementService::new(repos: RepositorySet, runtime: Arc<dyn WorkspacePlacementRuntime>)` for production plus an internal generic constructor over a narrow repository bundle for tests.

Focused tests:

- `register_backend_inventory_detects_and_upserts_manual_inventory`
  - Seed active `ProjectBackendAccess`, fake runtime returns `WorkspaceDetectionResult`; assert one detect call with project/user/backend/root context and inventory source `ManualRegister`, status `Available`, matching root/backend.

- `register_backend_inventory_rejects_inactive_access_without_detect`
  - Seed paused/revoked access; expect `ApplicationError::Conflict`; assert runtime detect call count is zero and inventory not mutated.

- `register_backend_inventory_rejects_wrong_project_access`
  - Seed access for a different project; expect `ApplicationError::NotFound`, preserving current route semantics from `load_access_for_project` (`crates/agentdash-api/src/routes/backend_access.rs:409`, `crates/agentdash-api/src/routes/backend_access.rs:420`).

- `create_workspace_shape_infers_identity_from_first_binding_and_hydrates_remaining`
  - Two bindings, fake detect returns same identity; assert returned identity payload from first detect, both bindings ready, inventory item count equals binding count, duplicate detection warnings are preserved if result shape includes warnings.

- `create_workspace_shape_rejects_identity_mismatch`
  - Explicit identity payload differs from detected fact; expect `ApplicationError::BadRequest`, equivalent to route errors at `crates/agentdash-api/src/routes/workspaces.rs:805` and `crates/agentdash-api/src/routes/workspaces.rs:851`.

- `update_workspace_bindings_hydrates_and_persists_workspace_and_inventory`
  - Existing workspace plus next bindings; assert inventory upsert and workspace update happen once and workspace status/default binding are refreshed.

- `bind_discovered_redetects_applies_identity_discovery_inventory_and_counts_create_update`
  - Seed local backend, active access, target workspace; fake runtime returns matching identity; assert inventory source `IdentityDiscovery`, touched workspace persisted, created/updated counts are correct.

- `bind_discovered_rejects_mixed_backends_without_detect`
  - Commands contain multiple backend ids; expect `ApplicationError::BadRequest`; runtime call count zero.

- `bind_discovered_rejects_non_local_backend`
  - Seed active access and remote backend; expect `ApplicationError::BadRequest`, equivalent to current route check (`crates/agentdash-api/src/routes/workspaces.rs:634`, `crates/agentdash-api/src/routes/workspaces.rs:639`).

- `bind_discovered_accepts_relaxed_p4_server_stream_client_match`
  - Expected workspace identity is `server_stream_client`; detected identity is `server_stream` with same server/stream; assert success. This protects current relaxed P4 discovery behavior (`crates/agentdash-api/src/routes/workspaces.rs:712`, `crates/agentdash-api/src/routes/workspaces.rs:727`, `crates/agentdash-api/src/routes/workspaces.rs:730`).

- `bind_discovered_rejects_identity_mismatch`
  - Matching backend/root but different identity; expect `ApplicationError::BadRequest` and no workspace update.

- `sync_candidate_inventory_applies_unique_match_only`
  - Existing current logic skips zero/multiple workspace matches and only applies available inventory with exactly one match (`crates/agentdash-application/src/workspace/backend_sync.rs:197`, `crates/agentdash-application/src/workspace/backend_sync.rs:207`, `crates/agentdash-application/src/workspace/backend_sync.rs:224`). Keep one test around this behavior if the function moves into service.

### Constraints to Avoid Broad Refactor / Compile Risk

- Do not introduce a generic `WorkspacePlacementIntent` enum as the primary API if it causes large match-based dispatcher churn. Public method-per-intent is clearer and keeps route conversions incremental.
- Do not move `detect_git`, browse directory or discover-by-identity candidate search into placement service unless they start writing inventory/bindings. They are setup/query capabilities and not the D10 transaction owner.
- Do not change domain repository traits unless a real missing persistence operation appears. Existing ports cover required access/load/update/upsert paths.
- Do not create a cross-aggregate repository to hide inventory + workspace writes. Specs prefer an explicit application command/use-case boundary for cross-aggregate consistency.
- Avoid passing API DTOs into application. Convert API request structs to domain/application input structs at the route edge.
- Avoid depending on `RuntimeGateway` concrete request types in `agentdash-application::workspace::placement`; use a narrow trait and an API adapter to contain runtime invocation construction.
- Be careful with `RepositorySet` in service tests: using full `RepositorySet` forces many unrelated fake ports. A narrow repository dependency struct materially reduces compile and test burden.
- Preserve current error semantics: inactive access is conflict (`crates/agentdash-api/src/routes/backend_access.rs:262`), access from another project is not found (`crates/agentdash-api/src/routes/backend_access.rs:420`), ungranted backend is forbidden (`crates/agentdash-api/src/routes/backend_access.rs:401`), non-local bind-discovered backend is bad request (`crates/agentdash-api/src/routes/workspaces.rs:639`), runtime gateway errors should still map through the existing API/runtime mapping (`crates/agentdash-api/src/rpc.rs:277`).
- Keep changes cleanup-first: after route conversion, static search under `crates/agentdash-api/src/routes` should no longer find active `invoke_workspace_detect`, `workspace_inventory_from_detection`, `workspace_directory_fact_from_detection`, or `apply_workspace_directory_fact` transaction usage.

### Related Specs

- `.trellis/spec/backend/architecture.md` - API vs application/domain dependency direction and route responsibility.
- `.trellis/spec/backend/repository-pattern.md` - repository aggregate boundary and explicit cross-aggregate command/use-case rule.
- `.trellis/spec/backend/error-handling.md` - `ApplicationError` as application-to-API boundary.
- `.trellis/spec/backend/quality-guidelines.md` - async/error/test quality expectations.
- `.trellis/spec/cross-layer/project-backend-workspace-routing.md` - backend inventory registration and bind-discovered contracts.

### External References

- No external references used. This research is based on current repository code and Trellis specs only.

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` returned no active task for this session; the report path is based on the explicit task path and output path provided in the user request, not a runtime active-task pointer.
- No broad Rust build or test command was run, per request.
- `agentdash-application` currently depends on `agentdash-application-runtime-gateway` in `Cargo.toml`; the recommendation to add a narrow runtime detect port is architectural cleanup, not a dependency removal requirement for this slice.
- I did not find a shared in-memory fake for the full `RepositorySet`. Existing application tests mostly define module-local fakes, so D10 should either use a narrow repository bundle or accept local fakes for the four repositories it needs.
