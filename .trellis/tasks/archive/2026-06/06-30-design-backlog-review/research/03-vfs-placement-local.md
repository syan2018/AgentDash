# Research: VFS / placement / local runtime design

- Query: D9 / D10 / D11 design research for VFS per-mount/path authorization, WorkspacePlacementService directory fact transaction, and desktop profile/claim/settings ownership after quick convergence.
- Scope: internal
- Date: 2026-06-30

## Findings

### Files Found

- `.trellis/tasks/06-30-design-backlog-review/prd.md` - Canonical D1-D12 review requirements and required decision template.
- `.trellis/tasks/06-30-design-backlog-review/design.md` - Review shape and ordering; D9-D11 are the VFS / placement / local runtime group.
- `.trellis/tasks/06-30-design-backlog-review/implement.md` - Research dispatch and synthesis plan.
- `.trellis/tasks/06-30-design-backlog-review/implement.jsonl` - Context manifest pointing to VFS, desktop local runtime, workspace routing, and prior research.
- `.trellis/tasks/06-30-module-adversarial-review/followups/design-backlog.md` - Canonical backlog source for D9 / D10 / D11.
- `.trellis/spec/backend/vfs/architecture.md` - VFS address, mount, provider, and runtime tool composition invariants.
- `.trellis/spec/backend/vfs/vfs-access.md` - VFS access, mount capability, Project VFS Mount, runtime tool, and error semantics.
- `.trellis/spec/backend/vfs/vfs-materialization.md` - VFS URI materialization scope and local path constraints.
- `.trellis/spec/cross-layer/project-backend-workspace-routing.md` - ProjectBackendAccess, workspace detect, inventory, binding, and runtime summary contract.
- `.trellis/spec/cross-layer/desktop-local-runtime.md` - Tauri thin shell, `agentdash-local` machine identity/runtime owner, runner claim, local relay, and desktop settings contract.
- `.trellis/tasks/06-30-module-adversarial-review/research/09-vfs-runtime-tool-surface.md` - Prior D9 evidence: project-only VFS grant, mount capability/tool capability split, discovery policy residuals.
- `.trellis/tasks/06-30-module-adversarial-review/research/10-local-runtime-relay-surface.md` - Prior D11 evidence: Tauri still owns profile/claim/settings while local command hub and MCP transport were improved.
- `.trellis/tasks/06-30-module-adversarial-review/research/11-project-workspace-backend-placement.md` - Prior D10/D11 evidence: directory fact helpers exist but write paths remain distributed; desktop lifecycle partly moved down.
- `.trellis/tasks/06-30-architecture-quick-convergence/prd.md` - Quick convergence scope and explicit out-of-scope design residuals.
- `.trellis/tasks/06-30-architecture-quick-convergence/implement.md` - Completion notes for VFS/local guard rails and remaining residuals.
- `.trellis/tasks/06-30-architecture-quick-convergence/work-items/03-vfs-local-guard-rails.md` - Guard rail work item; explicitly excludes per-mount/path authorization.
- `crates/agentdash-application-vfs/src/mount_project.rs` - Project VFS mount construction and project-only agent grant pruning.
- `crates/agentdash-application-vfs/src/mount.rs` - Provider constants and raw metadata checks used to identify project VFS mounts.
- `crates/agentdash-domain/src/common/agent_config.rs` - `AgentVfsAccessGrant` shape.
- `crates/agentdash-application-vfs/src/tools/mounts.rs` - Agent-facing mount discovery exposes mount capabilities.
- `crates/agentdash-spi/src/connector/mod.rs` - Tool capability gate checks tool-level capability keys, not mount/path authorization.
- `crates/agentdash-application/src/frame_construction/owner_bootstrap.rs` - Project owner frame construction applies agent VFS grants.
- `crates/agentdash-application/src/workspace/backend_sync.rs` - Existing `WorkspaceDirectoryFact` helpers and inventory/binding sync.
- `crates/agentdash-api/src/routes/backend_access.rs` - Manual backend inventory registration route still invokes detect and upserts inventory directly.
- `crates/agentdash-api/src/routes/workspaces.rs` - bind-discovered and workspace create/update routes still own detect/fact/apply orchestration.
- `crates/agentdash-local-tauri/src/main.rs` - Tauri command adapter still defines desktop runtime/profile/settings/ensure DTOs and IO/HTTP claim logic.
- `crates/agentdash-local/src/desktop_runner_host.rs` - Desktop embedded runner lifecycle is already owned by `agentdash-local`.
- `crates/agentdash-local/src/machine_identity.rs` - Machine identity load/create/persist is already owned by `agentdash-local`.
- `crates/agentdash-local/src/runner_claim.rs` - Standalone runner claim client and credential projection are already owned by `agentdash-local`.
- `packages/core/src/local-runtime/index.ts` - TS local runtime port and DTOs consumed by Web/Tauri adapters.
- `packages/app-web/src/desktop/localRuntimeBridge.ts` - Frontend auto-connect bridge still normalizes/creates profile defaults.

### Quick Convergence Facts To Preserve

- Runtime callable tool duplicate guard is complete: `SessionRuntimeToolComposer` now diagnoses duplicate callable tool names (`crates/agentdash-application/src/runtime_tools/provider.rs:72`) and session tool assembly also rejects duplicate callable names (`crates/agentdash-application-runtime-session/src/session/tool_assembly.rs:92`).
- Local workspace root guard is complete: `ToolExecutor` and `ProcessExecutor` consume shared `WorkspaceRootGuard` (`crates/agentdash-local/src/tool_executor.rs:19`, `crates/agentdash-local/src/process_executor.rs:7`, `crates/agentdash-local/src/workspace_root_guard.rs:6`).
- Local relay scheduling guard is complete: domain handlers declare `CommandDispatchPlan` and `ws_client` consumes the plan (`crates/agentdash-local/src/handlers/mod.rs:48`, `crates/agentdash-local/src/handlers/mod.rs:137`, `crates/agentdash-local/src/ws_client.rs:374`).
- Builtin skill VFS discovery identity guard is complete: `load_skills_from_vfs` accepts `AuthIdentity` and tests cover identity forwarding (`crates/agentdash-application-skill/src/skill/loader.rs:120`, `crates/agentdash-application-skill/src/skill/loader.rs:509`).
- Therefore D9 should not redesign runtime tool uniqueness, local root guard, relay scheduling, or builtin skill identity. The remaining VFS/local design residual is per-mount/path authorization, not these guard rails.

## D9. VFS per-mount/path authorization model

### Code Evidence

- `AgentVfsAccessGrant` only carries `mount_id` and mount-level `capabilities` (`crates/agentdash-domain/src/common/agent_config.rs:263`).
- `apply_agent_vfs_access_grants` builds a grant map by mount id, skips all non-project VFS mounts, clears missing project mount capabilities, intersects granted capabilities, and drops empty project mounts (`crates/agentdash-application-vfs/src/mount_project.rs:136`, `crates/agentdash-application-vfs/src/mount_project.rs:145`, `crates/agentdash-application-vfs/src/mount_project.rs:148`, `crates/agentdash-application-vfs/src/mount_project.rs:152`, `crates/agentdash-application-vfs/src/mount_project.rs:161`).
- Project VFS mount identity is inferred through raw metadata key `agentdash_project_vfs_mount` (`crates/agentdash-application-vfs/src/mount.rs:18`, `crates/agentdash-application-vfs/src/mount.rs:121`).
- Frame construction applies agent VFS grants only for `OwnerScope::Project` after appending agent knowledge mount (`crates/agentdash-application/src/frame_construction/owner_bootstrap.rs:363`, `crates/agentdash-application/src/frame_construction/owner_bootstrap.rs:369`).
- `mounts_list` exposes current runtime mount capabilities to the Agent as accessible mount facts (`crates/agentdash-application-vfs/src/tools/mounts.rs:33`, `crates/agentdash-application-vfs/src/tools/mounts.rs:51`, `crates/agentdash-application-vfs/src/tools/mounts.rs:62`).
- `CapabilityState.is_capability_tool_enabled` gates tool-level capability keys and does not know mount id or path (`crates/agentdash-spi/src/connector/mod.rs:422`, `crates/agentdash-spi/src/connector/mod.rs:429`).

### Wrong Path / Concept Split

- `AgentVfsAccessGrant` currently sounds generic but is operationally a Project VFS Mount grant. Treating it as generic VFS authorization would silently exclude `agent`, `lifecycle`, `routine`, `canvas`, `skill_asset`, and workspace mounts because the pruning function skips non-project mounts.
- Mount `capabilities` are provider operation affordances and visible mount surface, not a complete admission decision. Using them as the only authorization language would mix provider support, role/project defaults, and runtime permission grants into one mutable field.
- Tool-level `CapabilityState` answers whether `fs.read`, `fs.write`, `shell.exec`, etc. are visible/callable at all; it cannot answer whether `main://src/foo.rs` or `lifecycle://artifacts/x` is allowed.
- PermissionGrant path-level admission has no single landing point today: it could be incorrectly modeled as tool capability, project VFS grant, mount capability mutation, provider metadata, or ad hoc tool checks.

### Recommended Owner / Contract

Decision state: `self-decided`.

Owner:

- Keep Project VFS mount preset grants as a narrow ProjectAgent configuration concern, preferably renamed/typed as `ProjectVfsMountGrant` or equivalent.
- Add a separate application-layer VFS access projection owner for generic per-mount/path authorization. The natural owner is the frame/session runtime surface assembly boundary that already has AgentRun identity, frame identity, PermissionGrant projection, and closed VFS facts.
- VFS service/tools enforce the projection at operation entry after URI normalization and mount resolution; providers remain unaware of AgentRun permission policy.

Contract shape:

```rust
pub struct RuntimeVfsAccessPolicy {
    pub rules: Vec<RuntimeVfsAccessRule>,
}

pub struct RuntimeVfsAccessRule {
    pub surface_ref: RuntimeVfsSurfaceRef,
    pub mount_id: String,
    pub path_pattern: VfsPathPattern,
    pub operations: BTreeSet<MountCapability>,
    pub source: RuntimeVfsAccessSource,
}
```

- `Mount.capabilities` remains provider/max operation support.
- Effective access for a tool call is `tool capability enabled` AND `mount supports operation` AND `RuntimeVfsAccessPolicy admits normalized mount-relative path`.
- `PermissionGrant` should produce admission facts that compile into `RuntimeVfsAccessPolicy`; it should not mutate visible `CapabilityState` or raw mount metadata.
- Project-only preset grant can be compiled into the same policy, but the legacy type should not pretend to authorize non-project mounts.

### Implementation Slices

1. Rename or introduce a new typed Project VFS mount grant path so current project-only behavior is explicit. Keep semantics unchanged while deleting the generic naming ambiguity.
2. Define `RuntimeVfsAccessPolicy` and path pattern primitives in application/VFS boundary types; support exact mount, subtree prefix, and operation set first.
3. Compile current ProjectAgent project mount grants into policy during frame construction; do not change non-project mounts yet.
4. Wire policy into shared runtime VFS/tool resolution (`fs.read`, `fs.write/apply_patch`, `fs.list/search`, `shell.exec`) after path normalization and before provider dispatch.
5. Extend PermissionGrant projection/admission to emit VFS path rules, then consume those rules in the same policy compiler.
6. Update `mounts_list` output semantics if needed: expose effective allowed operations per mount/path summary, or clearly keep provider capabilities separate from authorization summary.

### Verification Strategy

- Unit tests: policy compiler keeps current Project VFS grant behavior, including missing project mount removal and granted capability intersection.
- Unit tests: non-project mounts are not accidentally pruned by Project VFS grant migration.
- Tool tests: `fs.read`, `fs.apply_patch`, `fs.glob/grep`, and `shell.exec` deny normalized paths outside policy even when mount capability supports the operation.
- Permission tests: tool-level grants do not expand mount/path access; path-level grants admit only declared mount/path/operation.
- Regression tests: `mounts_list` reflects the intended provider/effective access language and does not expose an operation that will be denied as "available" without explanation.

## D10. WorkspacePlacementService unified directory fact transaction

### Code Evidence

- Application layer already has a combined `WorkspaceDirectoryFact { binding, inventory }` (`crates/agentdash-application/src/workspace/backend_sync.rs:38`).
- Helpers can build inventory-only facts and combined directory facts from detect results (`crates/agentdash-application/src/workspace/backend_sync.rs:50`, `crates/agentdash-application/src/workspace/backend_sync.rs:70`).
- `apply_workspace_directory_fact` mutates Workspace bindings/status/default binding (`crates/agentdash-application/src/workspace/backend_sync.rs:118`, `crates/agentdash-application/src/workspace/backend_sync.rs:123`, `crates/agentdash-application/src/workspace/backend_sync.rs:137`, `crates/agentdash-application/src/workspace/backend_sync.rs:143`).
- Sync path applies inventory to unique matching workspace in application layer (`crates/agentdash-application/src/workspace/backend_sync.rs:180`, `crates/agentdash-application/src/workspace/backend_sync.rs:197`, `crates/agentdash-application/src/workspace/backend_sync.rs:223`, `crates/agentdash-application/src/workspace/backend_sync.rs:228`).
- Manual backend inventory registration route invokes `workspace.detect`, creates only inventory, and upserts directly from API route (`crates/agentdash-api/src/routes/backend_access.rs:246`, `crates/agentdash-api/src/routes/backend_access.rs:267`, `crates/agentdash-api/src/routes/backend_access.rs:275`, `crates/agentdash-api/src/routes/backend_access.rs:282`).
- Backend access route has its own `invoke_workspace_detect` helper (`crates/agentdash-api/src/routes/backend_access.rs:426`).
- bind-discovered route independently parses commands, ensures access, invokes detect, validates identity, upserts inventory, applies binding, and persists workspace (`crates/agentdash-api/src/routes/workspaces.rs:473`, `crates/agentdash-api/src/routes/workspaces.rs:519`, `crates/agentdash-api/src/routes/workspaces.rs:539`, `crates/agentdash-api/src/routes/workspaces.rs:555`, `crates/agentdash-api/src/routes/workspaces.rs:573`, `crates/agentdash-api/src/routes/workspaces.rs:579`, `crates/agentdash-api/src/routes/workspaces.rs:594`).
- Workspace create/update shape derivation also owns detect/fact/hydration in API route (`crates/agentdash-api/src/routes/workspaces.rs:735`, `crates/agentdash-api/src/routes/workspaces.rs:795`, `crates/agentdash-api/src/routes/workspaces.rs:838`, `crates/agentdash-api/src/routes/workspaces.rs:863`).
- Workspaces route has another `invoke_workspace_detect` helper (`crates/agentdash-api/src/routes/workspaces.rs:941`).

### Wrong Path / Concept Split

- `workspace.detect -> directory fact -> inventory/binding` is conceptually one placement transaction, but today the orchestration is split across application helpers and two API route modules.
- Manual register writes only inventory, bind-discovered writes inventory and binding, sync conditionally writes binding from existing inventory, and create/update hydrates bindings before workspace persistence. Each path can diverge in source, priority, identity matching, status, and repository write order.
- Duplicate `invoke_workspace_detect` helpers in route modules make RuntimeGateway setup invocation a route concern instead of placement use case concern.
- "Advanced Maintenance only changes binding" is currently an implicit route/UI behavior, not an explicit placement intent.

### Recommended Owner / Contract

Decision state: `self-decided`.

Owner:

- Create/finish an application-layer `WorkspacePlacementService` as the single owner of workspace directory facts, detect invocation, inventory writes, binding writes, matching policy, and intent-specific transaction shape.
- API routes own auth/permission, DTO parsing, and response mapping only.
- RuntimeGateway remains the transport/action executor for local `workspace.detect`; it does not own placement semantics.

Contract shape:

```rust
pub enum WorkspacePlacementIntent {
    ManualRegisterInventory,
    BindDiscovered,
    CreateOrUpdateWorkspace,
    SyncCandidateInventory,
    AdvancedBindingOnly,
}

pub struct WorkspacePlacementCommand {
    pub project_id: Uuid,
    pub actor_user_id: Option<String>,
    pub workspace_id: Option<Uuid>,
    pub backend_id: String,
    pub root_ref: String,
    pub expected_identity: Option<WorkspaceIdentityExpectation>,
    pub priority: Option<i32>,
    pub intent: WorkspacePlacementIntent,
}

pub struct WorkspacePlacementResult {
    pub detected: WorkspaceDetectionResult,
    pub fact: WorkspaceDirectoryFact,
    pub inventory_written: bool,
    pub binding_result: Option<WorkspaceDirectoryFactApplyResult>,
    pub workspace: Option<Workspace>,
    pub warnings: Vec<String>,
}
```

- For normal detected directory facts, the service decides whether inventory, binding, or both are written based on explicit intent.
- `AdvancedBindingOnly` should be a first-class intent so it can intentionally skip inventory registration without bypassing normalization, identity validation, and binding status rules.
- Manual register should no longer be "route writes inventory"; it should call the placement service and return inventory plus any candidate/applied-binding information the intent allows.

### Implementation Slices

1. Move the duplicated `invoke_workspace_detect` logic from API routes into application placement service; keep route-level permission checks.
2. Wrap existing helpers (`workspace_directory_fact_from_detection`, `workspace_inventory_from_detection`, `apply_workspace_directory_fact`) behind service methods.
3. Convert manual backend inventory registration route to call `WorkspacePlacementService::register_inventory`.
4. Convert bind-discovered route to call service batch operation; batch should own same-backend validation, identity match, inventory upsert, binding apply, and workspace persistence.
5. Convert workspace create/update binding hydration to use service detect/fact methods; route should only assemble Workspace DTO and persist via service.
6. Add `AdvancedBindingOnly` path for maintenance UI so "only binding" is explicit and tested rather than a route bypass.
7. Remove route-local detect helpers and any remaining direct `backend_workspace_inventory_repo.upsert` / `apply_workspace_directory_fact` from API routes.

### Verification Strategy

- Application tests: manual register, bind-discovered, sync, create/update, and advanced binding-only all produce consistent `WorkspaceDirectoryFact` values from identical detect output.
- Application tests: identity mismatch, unavailable backend access, inactive access, empty root, and duplicate binding keys produce stable error classes.
- Repository interaction tests/mocks: inventory and binding writes happen in the expected order per intent; advanced binding-only does not write inventory.
- API tests: routes become thin adapters and preserve HTTP status/DTO behavior.
- Regression tests: candidate/sync and bind-discovered apply identical matching policy for P4 workspace identity.

## D11. Desktop profile/claim/settings moved down to agentdash-local

### Code Evidence

- Tauri main still defines `RuntimeStartRequest`, `LocalRuntimeProfile`, and `DesktopAppSettings` (`crates/agentdash-local-tauri/src/main.rs:109`, `crates/agentdash-local-tauri/src/main.rs:126`, `crates/agentdash-local-tauri/src/main.rs:152`).
- Tauri commands still load/save desktop settings and profile directly using file IO (`crates/agentdash-local-tauri/src/main.rs:207`, `crates/agentdash-local-tauri/src/main.rs:216`, `crates/agentdash-local-tauri/src/main.rs:244`, `crates/agentdash-local-tauri/src/main.rs:256`, `crates/agentdash-local-tauri/src/main.rs:262`).
- Runtime start in Tauri normalizes the request, calls local claim logic, then constructs `LocalRuntimeConfig` for `DesktopRunnerHost` (`crates/agentdash-local-tauri/src/main.rs:640`, `crates/agentdash-local-tauri/src/main.rs:643`, `crates/agentdash-local-tauri/src/main.rs:647`, `crates/agentdash-local-tauri/src/main.rs:650`).
- Tauri main owns desktop ensure payload construction, `/api/local-runtime/ensure` POST, and response validation (`crates/agentdash-local-tauri/src/main.rs:662`, `crates/agentdash-local-tauri/src/main.rs:668`, `crates/agentdash-local-tauri/src/main.rs:687`, `crates/agentdash-local-tauri/src/main.rs:717`, `crates/agentdash-local-tauri/src/main.rs:752`, `crates/agentdash-local-tauri/src/main.rs:757`, `crates/agentdash-local-tauri/src/main.rs:783`).
- Tauri main uses `agentdash-local` machine identity but still performs profile/start normalization itself (`crates/agentdash-local-tauri/src/main.rs:788`, `crates/agentdash-local-tauri/src/main.rs:811`).
- `agentdash-local` already owns desktop embedded runner lifecycle (`crates/agentdash-local/src/desktop_runner_host.rs:1`, `crates/agentdash-local/src/desktop_runner_host.rs:36`, `crates/agentdash-local/src/desktop_runner_host.rs:263`).
- `agentdash-local` already owns machine identity load/create/persist (`crates/agentdash-local/src/machine_identity.rs:14`, `crates/agentdash-local/src/machine_identity.rs:18`, `crates/agentdash-local/src/machine_identity.rs:28`, `crates/agentdash-local/src/machine_identity.rs:32`).
- `agentdash-local` already owns standalone runner registration claim and credential projection (`crates/agentdash-local/src/runner_claim.rs:51`, `crates/agentdash-local/src/runner_claim.rs:71`, `crates/agentdash-local/src/runner_claim.rs:119`, `crates/agentdash-local/src/runner_claim.rs:122`).
- TS core mirrors local runtime profile/start DTOs and client operations (`packages/core/src/local-runtime/index.ts:197`, `packages/core/src/local-runtime/index.ts:208`, `packages/core/src/local-runtime/index.ts:240`).
- Web desktop bridge still creates/normalizes auto-connect profiles and injects token/server URL before starting runtime (`packages/app-web/src/desktop/localRuntimeBridge.ts:107`, `packages/app-web/src/desktop/localRuntimeBridge.ts:121`, `packages/app-web/src/desktop/localRuntimeBridge.ts:124`, `packages/app-web/src/desktop/localRuntimeBridge.ts:156`, `packages/app-web/src/desktop/localRuntimeBridge.ts:171`).

### Wrong Path / Concept Split

- Tauri shell is still a partial owner of desktop runtime profile, settings, claim payload, server origin normalization, response validation, and profile file format.
- `agentdash-local` owns machine identity, standalone runner claim, and desktop runner lifecycle, but not the desktop embedded runner's profile/claim/settings contract. This creates two local runtime enrollment styles: standalone runner claim is in `agentdash-local`; desktop access-token ensure is in Tauri main.
- TS core should be a UI/client port, not a second owner of server-side claim validation or profile normalization rules. It can carry DTOs, but canonical Rust semantics should live under `agentdash-local`.

### Recommended Owner / Contract

Decision state: `self-decided`.

Owner:

- `agentdash-local` owns all local runtime durable facts and enrollment logic: machine identity, desktop profile, desktop app settings, desktop access-token ensure claim client, claim response validation, credential projection, and runtime config construction.
- `agentdash-local-tauri` remains a shell adapter for Tauri commands, tray/window/autostart OS integration, and forwarding UI requests into `agentdash-local`.
- `packages/core` remains the TypeScript port consumed by Web/Tauri; it should mirror generated/stable DTO shape but not define authoritative semantics.

Contract shape:

```rust
pub mod desktop_profile {
    pub struct DesktopRuntimeStartRequest { ... }
    pub struct DesktopLocalRuntimeProfile { ... }
    pub struct DesktopAppSettings { ... }

    pub fn load_profile() -> Result<Option<DesktopLocalRuntimeProfile>>;
    pub fn save_profile(profile: DesktopLocalRuntimeProfile) -> Result<DesktopLocalRuntimeProfile>;
    pub fn delete_profile() -> Result<()>;
    pub fn load_settings() -> Result<DesktopAppSettings>;
    pub fn save_settings(settings: DesktopAppSettings) -> Result<DesktopAppSettings>;
}

pub mod desktop_claim {
    pub async fn claim_desktop_runtime(
        request: DesktopRuntimeStartRequest,
    ) -> Result<DesktopRuntimeClaim>;

    pub fn runtime_config_from_claim(
        claim: DesktopRuntimeClaim,
        request: DesktopRuntimeStartRequest,
    ) -> LocalRuntimeConfig;
}
```

- The desktop claim path should reuse the same low-level HTTP/error/redaction patterns as `runner_claim`, while preserving distinct authentication source: desktop uses access token against `/api/local-runtime/ensure`; runner uses registration token against `/api/local-runtime/runner/claim`.
- Server-issued `backend_id`, `relay_ws_url`, and `auth_token` remain the only formal backend relay credentials.
- Desktop settings file IO belongs with profile IO because both are local durable desktop runtime facts; Tauri only invokes them and owns OS-specific autostart toggling.

### Implementation Slices

1. Move Rust DTOs for `RuntimeStartRequest`, `LocalRuntimeProfile`, and `DesktopAppSettings` from Tauri main into `agentdash-local` public modules.
2. Move profile load/save/delete and settings load/save/normalize into `agentdash-local`; Tauri commands become invoke adapters.
3. Move desktop access-token ensure payload/response DTOs, server origin normalization, POST client, retry-ready handling, and response validation into `agentdash-local::desktop_claim`.
4. Keep `DesktopRunnerHost::ensure_started_with` as lifecycle owner, but pass it a `LocalRuntimeConfig` built by `agentdash-local` desktop claim/profile code.
5. Reduce Tauri main to OS shell concerns: command registration, tray/window lifecycle, autostart bridge, browser/open-url, and log/MCP command forwarding.
6. Align TS port types with the moved Rust contract; ideally generate or keep DTO names stable at the boundary while removing duplicated normalization assumptions from frontend bridge where possible.
7. Add tests in `agentdash-local` for profile/settings roundtrip, claim payload construction, response validation, token redaction/error classes, and runtime config projection.

### Verification Strategy

- `agentdash-local` unit tests: profile load/save/delete, settings defaults/normalization, desktop claim response validation, server origin normalization, runtime config projection.
- Tauri command tests or focused compile check: commands call local module functions and no longer perform claim/profile file IO directly.
- Frontend tests: desktop auto-connect still calls `profileLoad/profileSave/runtimeStart` and handles errors without knowing claim internals.
- Regression tests: standalone runner claim behavior remains unchanged; desktop claim still uses `/api/local-runtime/ensure` and rejects non-`desktop_access_token` registration source.

## Cross-Item Convergence

- D9 deletes owner duplication by separating provider mount capabilities, tool visibility, Project VFS preset grants, and runtime mount/path authorization. The only generic admission language should be `RuntimeVfsAccessPolicy`.
- D10 deletes owner duplication by making `WorkspacePlacementService` the transaction owner for directory facts. Routes stop owning detect/fact/write orchestration.
- D11 deletes owner duplication by moving desktop local durable facts and claim protocol into `agentdash-local`. Tauri becomes a shell adapter, matching the existing local ownership of machine identity and desktop runner lifecycle.
- These designs intentionally do not add parallel compatibility paths. The project is pre-release; the correct implementation should move ownership and remove old route/shell owner code in the same slice.

## External References

- None. This research used repository code, Trellis specs, and prior task artifacts only.

## Related Specs

- `.trellis/spec/backend/vfs/architecture.md`
- `.trellis/spec/backend/vfs/vfs-access.md`
- `.trellis/spec/backend/vfs/vfs-materialization.md`
- `.trellis/spec/cross-layer/project-backend-workspace-routing.md`
- `.trellis/spec/cross-layer/desktop-local-runtime.md`
- `.trellis/spec/guides/cross-layer-thinking-guide.md`
- `.trellis/spec/guides/code-reuse-thinking-guide.md`

## Caveats / Not Found

- `python ./.trellis/scripts/task.py current --source` returned `Current task: (none)`. This file was written to the user-specified task path `.trellis/tasks/06-30-design-backlog-review/research/03-vfs-placement-local.md`.
- No business code was modified.
- No Rust build, full test, or migration command was run; this task explicitly requested targeted read/rg research only.
- Line numbers reflect the current working tree during this research and may shift with concurrent edits.
