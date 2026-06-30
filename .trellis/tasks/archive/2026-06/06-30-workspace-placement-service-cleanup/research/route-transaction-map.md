# Research: route transaction map for D10 WorkspacePlacementService

- Query: Map current route-local workspace placement transactions for manual inventory register, workspace create/update, bind-discovered and sync candidates; identify route helpers to delete or move, response data the application service must return, preserved user-visible errors, and risky implementation overlap.
- Scope: internal
- Date: 2026-06-30

## Findings

### Files found

- `AGENTS.md` - project working rules: Chinese communication, cleanup-first during pre-research, no compatibility fallbacks, avoid touching parallel work.
- `.trellis/workflow.md` - Trellis workflow requires persisted research artifacts and sub-agent context discipline.
- `.trellis/tasks/06-30-workspace-placement-service-cleanup/prd.md` - D10 goal: move `workspace.detect -> WorkspaceDirectoryFact -> BackendWorkspaceInventory / WorkspaceBinding` transactions from routes to application `WorkspacePlacementService`.
- `.trellis/tasks/06-30-workspace-placement-service-cleanup/design.md` - target boundary: API routes own auth, DTO parsing and response mapping only; application service owns placement facts and transactions.
- `.trellis/tasks/06-30-workspace-placement-service-cleanup/implement.md` - cleanup-first implementation order and validation hints.
- `.trellis/tasks/06-30-workspace-placement-service-cleanup/implement.jsonl` - relevant context manifest includes route files and workspace application helpers.
- `.trellis/spec/cross-layer/project-backend-workspace-routing.md` - cross-layer contract for inventory registration, discovery, bind-discovered validation and error semantics.
- `.trellis/spec/backend/architecture.md` - backend invariant: API layer handles auth/DTO/error mapping, business orchestration belongs in application.
- `crates/agentdash-api/src/routes/backend_access.rs` - current manual inventory register and sync candidates route implementation.
- `crates/agentdash-api/src/routes/workspaces.rs` - current create/update/bind-discovered route-local detect/fact/write implementation.
- `crates/agentdash-application/src/workspace/backend_sync.rs` - existing fact projection, inventory projection, candidate sync and binding apply helpers.
- `crates/agentdash-application/src/workspace/detection.rs` - existing `WorkspaceDetectionResult` shape produced by workspace detection.
- `crates/agentdash-api/src/dto/workspace.rs` and `crates/agentdash-contracts/src/workspace/contract.rs` - route request/response DTO shapes to preserve.
- `crates/agentdash-contracts/src/backend/contract.rs` - backend inventory register response DTO shape.

### Related specs

- `.trellis/spec/cross-layer/project-backend-workspace-routing.md` says manual inventory registration must trim and reject empty `root_ref`, invoke Runtime Gateway `workspace.detect`, upsert `BackendWorkspaceInventory` with `manual_register`, and return existing Runtime Gateway errors / detect deserialization errors.
- `.trellis/spec/cross-layer/project-backend-workspace-routing.md` says bind-discovered must re-detect stale roots, reject identity mismatch with `400 BadRequest`, require authorized active local backend, and upsert `identity_discovery` inventory.
- `.trellis/spec/backend/architecture.md` says API routes own authorization, request/response DTOs and error mapping, while application owns business orchestration.

### Route path: manual inventory register

Path: `POST /projects/{project_id}/backend-access/{access_id}/inventory/register`, wired in `backend_access.rs` at lines 94-96; handler starts at `backend_access.rs:246`.

- Auth: parses Project ID and ProjectBackendAccess ID, then requires project edit permission via `load_project_with_permission` (`backend_access.rs:252-260`). Loads the access row and verifies it belongs to the project via `load_access_for_project` (`backend_access.rs:261`, helper at `backend_access.rs:409-423`). Rejects inactive access with `409 Conflict` (`backend_access.rs:262-264`).
- DTO: consumes `RegisterBackendWorkspaceInventoryRequest { root_ref }` (`backend_access.rs:250`; contract field at `backend/contract.rs:384-387`). Trims and rejects empty `root_ref` through `normalize_required` (`backend_access.rs:266`, helper at `backend_access.rs:532-538`).
- Business transaction currently in route: invokes Runtime Gateway `workspace.detect` via route-local `invoke_workspace_detect` (`backend_access.rs:267-274`, helper at `backend_access.rs:426-456`); projects detect output into `BackendWorkspaceInventory` with source `ManualRegister` via `workspace_inventory_from_detection` (`backend_access.rs:275-281`).
- Repo writes: upserts `backend_workspace_inventory_repo.upsert(&item)` in the route (`backend_access.rs:282-286`).
- Current response: `BackendWorkspaceInventoryResponse::from(item)` (`backend_access.rs:287`) with fields `id`, `backend_id`, `root_ref`, `identity_kind`, `identity_payload`, `detected_facts`, `status`, `source`, `last_seen_at`, `last_error`, `created_at`, `updated_at` (`backend/contract.rs:345-381`).
- Service must return: the persisted or just-upserted `BackendWorkspaceInventory` domain item, preserving detect-derived `identity_kind`, `identity_payload`, `detected_facts`, `status`, source `ManualRegister`, timestamps and `last_error`, so the route can continue mapping to `BackendWorkspaceInventoryResponse`.

### Route path: create workspace

Path: `POST /projects/{project_id}/workspaces`, wired in `workspaces.rs` at line 81; handler starts at `workspaces.rs:108`.

- Auth: parses Project ID and requires project edit permission (`workspaces.rs:114-121`).
- DTO: consumes `CreateWorkspaceRequest` with `name`, optional `identity_kind`, optional `identity_payload`, optional `resolution_policy`, optional `default_binding_id`, optional `bindings`, optional `shortcut_binding`, optional `mount_capabilities` (`dto/workspace.rs:22-32`). Route trims and validates name via `normalize_workspace_name` (`workspaces.rs:123`, helper at `workspaces.rs:973-979`).
- Business transaction currently in route: calls route-local `derive_workspace_shape` (`workspaces.rs:124-134`, helper at `workspaces.rs:735-836`). That helper chooses `bindings` over `shortcut_binding` (`workspaces.rs:752-758`), parses `WorkspaceBindingInput` to domain bindings with nil workspace id (`workspaces.rs:760-764`), requires `identity_payload` when `identity_kind` is explicit (`workspaces.rs:766-771`), normalizes identity payload (`workspaces.rs:770-771`), hydrates all bindings via detect/fact (`workspaces.rs:772-780`), or derives identity from the first binding when no explicit identity is supplied (`workspaces.rs:789-804`). It validates every detected fact against the target identity (`workspaces.rs:805-814`, `workspaces.rs:838-861`) and accumulates hydrated bindings plus inventory items (`workspaces.rs:816-835`).
- Business transaction currently in route, continued: route constructs `Workspace::new`, applies bindings/default binding/mount capabilities/status/default refresh (`workspaces.rs:136-148`).
- Repo writes: route upserts initial inventory rows with `backend_workspace_inventory_repo.upsert_many` (`workspaces.rs:150-155`), creates the workspace (`workspaces.rs:157`), reloads it (`workspaces.rs:158-163`).
- Current response: `WorkspaceResponse::from(stored)` (`workspaces.rs:164`) with `WorkspaceResponse` fields `id`, `project_id`, `name`, `identity_kind`, `identity_payload`, `resolution_policy`, `default_binding_id`, `status`, `bindings`, `mount_capabilities`, `created_at`, `updated_at` (`workspace/contract.rs:119-160`).
- Service must return: the stored created `Workspace` domain object after binding hydration, inventory upsert, workspace create, status derivation and default binding refresh. If implementation chooses a lower-level method for shape derivation, the application use case still needs to own the transaction and return enough for the route to map the stored workspace response.

### Route path: update workspace

Path: `PUT /workspaces/{id}`, wired in `workspaces.rs` at lines 96-99; handler starts at `workspaces.rs:184`.

- Auth: parses Workspace ID and loads workspace plus project with edit permission through `load_workspace_and_project_with_permission` (`workspaces.rs:190-197`).
- DTO: consumes `UpdateWorkspaceRequest` with optional `name`, `identity_kind`, `identity_payload`, `resolution_policy`, `default_binding_id`, `bindings`, `mount_capabilities` (`dto/workspace.rs:34-43`). Route validates name if present (`workspaces.rs:199-201`) and normalizes identity payload if present (`workspaces.rs:202-210`).
- Business transaction currently in route: route mutates workspace metadata (`workspaces.rs:199-213`), parses replacement bindings with `binding_input_to_binding` (`workspaces.rs:215-219`), rejects duplicate backend/root (`workspaces.rs:220`), hydrates bindings via detect/fact against current identity (`workspaces.rs:221-229`), replaces workspace bindings (`workspaces.rs:230-231`), applies default binding and mount capabilities (`workspaces.rs:233-238`), derives status and refreshes default binding (`workspaces.rs:239-240`).
- Repo writes: route upserts any inventory rows (`workspaces.rs:242-247`), updates workspace (`workspaces.rs:249`), reloads it (`workspaces.rs:250-255`).
- Current response: `WorkspaceResponse::from(stored)` (`workspaces.rs:256`) with the same fields as create.
- Service must return: the stored updated `Workspace` after metadata updates, optional binding hydration, inventory upsert, workspace update, status derivation and default binding refresh.

### Route path: bind-discovered

Path: `POST /projects/{project_id}/workspaces/bind-discovered`, wired in `workspaces.rs` at lines 89-93; handler starts at `workspaces.rs:473`.

- Auth: parses Project ID and requires project edit permission (`workspaces.rs:479-486`). Requires a single local active backend through `ensure_local_project_backend_access` (`workspaces.rs:519`, helper at `workspaces.rs:625-645`), which delegates Project backend authorization to `ensure_project_backend_access`, rejects inactive access with `409 Conflict`, loads backend row, and rejects non-local backend with `400 BadRequest`.
- DTO: consumes `BindDiscoveredWorkspaceBindingsRequest { bindings }` (`workspace/contract.rs:225-235`). Route rejects empty bindings (`workspaces.rs:488-490`), parses each `workspace_id`, trims/requires `binding.backend_id` and `binding.root_ref`, dedupes by `(workspace_id, normalized backend/root)` (`workspaces.rs:491-505`), and rejects mixed backend IDs (`workspaces.rs:507-518`).
- Business transaction currently in route: loads all project workspaces into a map (`workspaces.rs:521-528`), then for each command finds the target workspace or returns not found (`workspaces.rs:535-538`), re-detects `root_ref` (`workspaces.rs:539-546`), accumulates detect warnings (`workspaces.rs:548`), creates a seed binding (`workspaces.rs:549-554`), projects `WorkspaceDirectoryFact` with source `IdentityDiscovery` (`workspaces.rs:555-559`), validates detected identity kind and relaxed discovery identity payload match (`workspaces.rs:560-572`), applies directory fact to the workspace with access priority and counts created/updated binding results (`workspaces.rs:579-584`).
- Repo writes: route upserts each inventory item inside the loop (`workspaces.rs:573-577`), then updates each touched workspace and reloads it (`workspaces.rs:590-601`).
- Current response: `BindDiscoveredWorkspaceBindingsResponse` constructed in the route (`workspaces.rs:604-615`) with fields `backend_id`, updated `workspaces`, `bound_workspace_ids`, `created_bindings`, `updated_bindings`, `inventory_items`, `warnings` (`workspace/contract.rs:237-246`).
- Service must return: `backend_id`, stored updated `Workspace` domain objects sorted consistently with current `bound_workspace_ids`, touched workspace IDs as `Uuid`, `created_bindings`, `updated_bindings`, `BackendWorkspaceInventory` items created from facts, and concatenated detect warnings. Route should only map these to `WorkspaceResponse`, string IDs and `BackendWorkspaceInventoryResponse`.

### Route path: sync candidates

Path: `POST /projects/{project_id}/workspaces/sync-backend-bindings`, wired in `backend_access.rs` at lines 77-80; handler starts at `backend_access.rs:312`.

- Auth: parses Project ID and requires project edit permission (`backend_access.rs:317-324`).
- DTO: no request body.
- Business transaction currently in application, not route: route calls `sync_project_backend_workspace_bindings(&state.repos, project_id)` (`backend_access.rs:325`). The helper lists active accesses (`backend_sync.rs:181-184`), lists inventory for those backends (`backend_sync.rs:185`, helper at `backend_sync.rs:244-256`), loads workspaces (`backend_sync.rs:186`), builds candidates/conflicts (`backend_sync.rs:187`, `backend_sync.rs:258-298`), applies exactly-one-match available inventory as workspace binding facts (`backend_sync.rs:197-229`), updates workspaces (`backend_sync.rs:228`), sorts/dedupes updated IDs (`backend_sync.rs:232-233`) and returns sync counters/candidates/conflicts (`backend_sync.rs:235-241`).
- Repo writes: no route-local writes; application helper writes `workspace_repo.update(workspace)` (`backend_sync.rs:228`). It does not call detect and does not upsert inventory.
- Current response: route maps application `WorkspaceBindingSyncResult` to contract `WorkspaceBindingSyncResult` (`backend_access.rs:326`, mapper at `backend_access.rs:458-480`). Candidate mapping preserves `backend_id`, `root_ref`, `identity_kind`, `identity_payload`, `detected_facts`, `status`, `matched_workspace_ids`, `reason` (`backend_access.rs:482-500`; contract fields at `workspace/contract.rs:162-183`).
- Service must return: the same application sync result shape or a service-owned equivalent containing `updated_workspace_ids`, `created_bindings`, `updated_bindings`, `candidates`, `conflicts`. The implementation can keep the existing application function internally, but it should be exposed/named as part of the placement owner surface.

### Route-local helpers/functions to delete or move

- Delete `backend_access.rs::invoke_workspace_detect` as an active route helper (`backend_access.rs:426-456`). Its Runtime Gateway action construction and output deserialization should move behind an application-facing runtime adapter/port consumed by `WorkspacePlacementService`.
- Remove direct route import/use of `workspace_inventory_from_detection` from `backend_access.rs` (`backend_access.rs:12-18`, call at `backend_access.rs:275-281`). Projection stays in application service/helper.
- Move or delete `workspaces.rs::invoke_workspace_detect` (`workspaces.rs:941-971`) for placement paths. Do not leave it active for create/update/bind-discovered. `detect_workspace` endpoint may need a non-placement detect use case/adapter, but route-local `workspace.detect` construction should not remain as a compatibility path for placement.
- Move `workspaces.rs::derive_workspace_shape` (`workspaces.rs:735-836`) into the service or replace it with service methods such as create workspace with initial binding and derive shape. It currently owns binding selection, identity derivation, detect/fact, and inventory collection.
- Move `workspaces.rs::hydrate_workspace_bindings` (`workspaces.rs:838-861`) into service because it runs detect/fact validation and returns inventory rows.
- Move `workspaces.rs::detect_workspace_binding_fact` (`workspaces.rs:863-886`) into service because it combines access validation, runtime detect and `WorkspaceDirectoryFact` projection.
- Move `workspaces.rs::discovery_identity_payload_matches` and `relaxed_p4_discovery_payload` (`workspaces.rs:694-733`) into application service/policy because bind-discovered identity validation is part of the placement transaction.
- Move `workspaces.rs::BindDiscoveredCommand` (`workspaces.rs:618-622`) if command normalization moves into the service. If route continues DTO parsing and dedupe, the command type can disappear from the route entirely.
- Move or duplicate carefully `workspaces.rs::ensure_unique_bindings` and `binding_unique_key` (`workspaces.rs:915-932`) into application/domain-level placement validation if service owns binding replacement semantics. Route-local duplicate detection is currently business validation, not just DTO shape.
- Keep route-local simple scalar parsing/mapping helpers where they are pure DTO/error mapping: `parse_project_id`, `parse_workspace_id`, `normalize_workspace_name` can remain in route if service accepts normalized values. However, identity payload normalization (`workspaces.rs:934-939`) is domain/business validation and should be consumed by the service boundary rather than route-local placement code.
- `backend_access.rs::workspace_binding_sync_response` and `workspace_inventory_candidate_response` (`backend_access.rs:458-500`) can remain as response mappers unless service returns contract DTOs directly. Keeping them in the route matches API response mapping ownership.

### Response DTO data the service must preserve

- Manual register: return a `BackendWorkspaceInventory` suitable for `BackendWorkspaceInventoryResponse` fields `id`, `backend_id`, `root_ref`, `identity_kind`, `identity_payload`, `detected_facts`, `status`, `source`, `last_seen_at`, `last_error`, `created_at`, `updated_at` (`backend/contract.rs:345-381`).
- Create/update workspace: return stored `Workspace` domain objects suitable for `WorkspaceResponse` fields `id`, `project_id`, `name`, `identity_kind`, `identity_payload`, `resolution_policy`, `default_binding_id`, `status`, `bindings`, `mount_capabilities`, `created_at`, `updated_at` (`workspace/contract.rs:119-160`). Binding response requires `id`, `workspace_id`, `backend_id`, `root_ref`, `status`, `detected_facts`, `last_verified_at`, `priority`, `created_at`, `updated_at` (`workspace/contract.rs:88-117`).
- Bind-discovered: return `backend_id`, updated stored workspaces, bound workspace IDs, created/updated binding counts, inventory items, and warnings (`workspace/contract.rs:237-246`). The service result should keep `Uuid`/domain items internally and let the route map strings/DTOs.
- Sync candidates: return updated workspace IDs, created/updated binding counts, candidates and conflicts (`workspace/contract.rs:176-183`). Each candidate must preserve `backend_id`, `root_ref`, `identity_kind`, `identity_payload`, `detected_facts`, `status`, `matched_workspace_ids`, `reason` (`workspace/contract.rs:162-174`).

### User-visible error semantics to preserve

- Invalid Project ID / Workspace ID / ProjectBackendAccess ID stays `400 BadRequest` through parse helpers (`backend_access.rs:252-253`, `workspaces.rs:114`, `workspaces.rs:190`, `workspaces.rs:479`; helper messages at `backend_access.rs:527-530`, `workspaces.rs:981-987`).
- Project edit/view permission failures continue to come from `load_project_with_permission` or `load_workspace_and_project_with_permission` (`backend_access.rs:254-260`, `backend_access.rs:318-324`, `workspaces.rs:115-121`, `workspaces.rs:191-197`, `workspaces.rs:480-486`).
- Access not found or not belonging to project stays `404 NotFound` with "ProjectBackendAccess 不存在" (`backend_access.rs:409-423`).
- Project/backend authorization missing stays `403 Forbidden` with "Project 尚未授权访问 backend ..." from `ensure_project_backend_access` (`backend_access.rs:391-405`), including usage in workspace binding fact detection (`workspaces.rs:869-870`) and local discovery auth (`workspaces.rs:630`).
- Inactive access stays `409 Conflict` with "ProjectBackendAccess 当前未启用" (`backend_access.rs:262-264`, `workspaces.rs:631-633`).
- Empty `root_ref` / `backend_id` fields stay `400 BadRequest`: manual `root_ref 不能为空` (`backend_access.rs:266`), binding backend/root empty (`workspaces.rs:647-652`, `workspaces.rs:892-899`), empty `bindings` (`workspaces.rs:488-490`, `workspaces.rs:507-510`).
- Runtime Gateway detect invocation failures must continue to propagate through `state.services.runtime_gateway.invoke(...) ?` (`backend_access.rs:453`, `workspaces.rs:968`), preserving backend offline/directory inaccessible behavior.
- Invalid detect input serialization stays `400 BadRequest` with "workspace.detect 输入非法: ..." (`backend_access.rs:433-437`, `workspaces.rs:948-952`).
- Invalid built-in action key stays `500 Internal` with "内置 Runtime Action Key 非法: ..." (`backend_access.rs:438-441`, `workspaces.rs:953-956`).
- Detect response deserialization failure stays `500 Internal` with "workspace.detect 返回值解析失败: ..." (`backend_access.rs:454-455`, `workspaces.rs:969-970`).
- Create workspace with explicit `identity_kind` but no `identity_payload` stays `400 BadRequest` with "显式提供 identity_kind 时，identity_payload 不能为空" (`workspaces.rs:766-769`).
- Create workspace without identity and without any binding stays `400 BadRequest` with "创建 Workspace 时，必须提供 identity 或至少一个 binding" (`workspaces.rs:789-792`).
- Duplicate backend/root bindings in one workspace stay `400 BadRequest` with "同一个 Workspace 中不能重复绑定相同 backend/root" (`workspaces.rs:915-923`).
- Directory identity mismatch in create/update stays `400 BadRequest` with "目录 `...` 与 Workspace identity 不匹配" (`workspaces.rs:805-814`, `workspaces.rs:851-855`).
- Bind-discovered mixed backend IDs stays `400 BadRequest` with "bind-discovered 单次请求只能绑定同一个 backend" (`workspaces.rs:511-518`).
- Bind-discovered target workspace missing from project stays `404 NotFound` with "Workspace 不存在或不属于当前 Project" (`workspaces.rs:535-538`).
- Bind-discovered non-local backend stays `400 BadRequest` with "本机 Workspace discovery 仅支持 local backend" (`workspaces.rs:639-642`).
- Bind-discovered identity mismatch stays `400 BadRequest` with "目录 `...` 与 Workspace `...` 的 identity 不匹配" (`workspaces.rs:560-572`), including relaxed P4 server-stream match behavior for discovery (`workspaces.rs:694-733`).
- Stored workspace read-after-write failures stay `500 Internal`: create "Workspace 创建后读取失败" (`workspaces.rs:158-163`), update/bind "Workspace 更新后读取失败" (`workspaces.rs:250-255`, `workspaces.rs:595-600`), bind cache miss "Workspace 更新缓存缺失" (`workspaces.rs:590-593`).

### Risky file overlap for implementation workers

- `crates/agentdash-api/src/routes/workspaces.rs` is the highest-overlap file. Create/update/bind-discovered, detect endpoint helpers, discovery helpers, DTO parsing helpers and imports are interleaved. Two workers editing this file concurrently will likely conflict around imports, helper removal and `ensure_local_project_backend_access`.
- `crates/agentdash-api/src/routes/backend_access.rs` overlaps manual register and sync candidates. Manual-register conversion removes Runtime Gateway detect imports and `workspace_inventory_from_detection`; sync conversion may touch response mappers and application imports in the same import block.
- `crates/agentdash-application/src/workspace/backend_sync.rs` contains helper functions that service implementation may move/reuse/rename while sync candidate alignment also touches it. Coordinate before moving `apply_workspace_directory_fact`, `workspace_directory_fact_from_detection`, `workspace_inventory_from_detection` or `sync_project_backend_workspace_bindings`.
- `crates/agentdash-application/src/workspace/mod.rs` will need export changes for a new placement module and may conflict with any worker adding application workspace APIs.
- Any runtime adapter likely touches API bootstrap/AppState service wiring and application runtime port surfaces. That should be sequenced before route conversion so implement workers do not invent parallel adapters.
- `crates/agentdash-api/src/dto/workspace.rs` and `crates/agentdash-contracts/src/workspace/contract.rs` were read for DTO shape only. No DTO change appears required for D10; editing them during this cleanup would create unnecessary overlap.

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` returned no active task for this session, so this research uses the explicit task path supplied in the prompt.
- This was read-only source research. No Rust builds, tests or git commands were run.
- I did not inspect all repository trait implementations or fake-test patterns; that is Research B territory per `implement.md`.
- I did not inspect Runtime Gateway bootstrap/adapter files; that is Research C territory per `implement.md`.
- `detect_workspace` and `detect_git` endpoints remain outside this route-transaction map except where they share route-local detect helpers. They may need follow-up adapter cleanup, but the D10 cleanup-first constraint specifically forbids leaving route-local `invoke_workspace_detect` active for placement routes.
