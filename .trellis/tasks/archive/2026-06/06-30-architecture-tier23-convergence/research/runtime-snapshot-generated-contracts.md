# Research: Runtime Snapshot Generated Contracts

- Query: Work Group B mapping for backend runtime summary generated contracts, frontend mirrors, and desktop local runtime snapshot inclusion.
- Scope: internal
- Date: 2026-06-30

## Findings

### Task And Spec Context

- This task's B group is explicitly "Runtime Snapshot Generated Contracts"; the design says the first phase focuses on backend runtime summary and that desktop local runtime snapshot should not move raw Rust supervisor structs directly, but should first define a stable diagnostics DTO boundary (`.trellis/tasks/06-30-architecture-tier23-convergence/design.md:21`, `.trellis/tasks/06-30-architecture-tier23-convergence/design.md:25`).
- The implementation plan for B names the relevant files and asks to move backend runtime summary response types into generated contract source, replace frontend hand-written mirrors, keep route adapter as the application-to-wire mapper, and decide desktop inclusion before touching Tauri callers (`.trellis/tasks/06-30-architecture-tier23-convergence/implement.md:25`, `.trellis/tasks/06-30-architecture-tier23-convergence/implement.md:38`, `.trellis/tasks/06-30-architecture-tier23-convergence/implement.md:39`, `.trellis/tasks/06-30-architecture-tier23-convergence/implement.md:40`, `.trellis/tasks/06-30-architecture-tier23-convergence/implement.md:41`).
- Cross-layer contract spec says business HTTP DTOs belong in `agentdash-contracts`, API routes use those contract DTOs, and frontend consumes generated files only (`.trellis/spec/cross-layer/frontend-backend-contracts.md:19`).
- Route-local DTOs are allowed only for tiny transport wrappers; any DTO consumed by the frontend or reused across features must move into the contract crate (`.trellis/spec/cross-layer/frontend-backend-contracts.md:28`, `.trellis/spec/cross-layer/frontend-backend-contracts.md:29`).
- Frontend type-safety spec says internal API responses use `src/generated/*` as the wire source, and frontend must not re-declare backend unions or identity-rebuild generated DTOs (`.trellis/spec/frontend/type-safety.md:11`, `.trellis/spec/frontend/type-safety.md:22`, `.trellis/spec/frontend/type-safety.md:36`, `.trellis/spec/frontend/type-safety.md:37`, `.trellis/spec/frontend/type-safety.md:41`).
- Desktop local runtime spec already fixes `desktop_api_snapshot.state` to `starting | running | error | stopped` and says Local Runtime UI depends on the `@agentdash/core` `LocalRuntimeClient` port rather than direct Tauri imports (`.trellis/spec/cross-layer/desktop-local-runtime.md:23`, `.trellis/spec/cross-layer/desktop-local-runtime.md:826`).
- Desktop spec also states `/backends/runtime-summary` is the cloud/frontend projection for execution busy/allocatable status and is produced by merging runtime health, registry executor snapshots, and active execution leases (`.trellis/spec/cross-layer/desktop-local-runtime.md:563`).

### Files Found

- `crates/agentdash-application/src/backend/runtime_summary.rs` - application read model that computes backend runtime summary from backend config, online registry snapshots, runtime health, and active execution leases.
- `crates/agentdash-api/src/dto/backend.rs` - current API-local DTO definitions for runtime summary, executor summary, and active session leases.
- `crates/agentdash-api/src/routes/backends.rs` - `/backends/runtime-summary` route and mapper from application read model into API-local DTO.
- `crates/agentdash-contracts/src/backend/contract.rs` - existing generated backend DTO source; has backend health/config/access DTOs but not runtime summary DTOs.
- `crates/agentdash-contracts/src/generate_ts.rs` - generation registration for `backend-contracts.ts`; currently exports backend health/config/access DTOs but not runtime summary.
- `packages/app-web/src/generated/backend-contracts.ts` - generated frontend backend DTO output; currently lacks runtime summary, active session, and execution lease enum types.
- `packages/app-web/src/types/acp.ts` - frontend hand-written mirror for runtime summary, active sessions, executor summary, and execution lease unions.
- `packages/app-web/src/stores/coordinatorStore.ts` - frontend store fetching `/backends/runtime-summary` as the hand-written `BackendRuntimeSummary[]`.
- `packages/app-web/src/features/settings/ui/SettingsSystemSections.tsx` - settings UI consumes summary fields for active session count and allocatable status.
- `packages/app-web/src/features/settings/model/runtimeDiagnostics.ts` - maps generated/backend config plus runtime summary into `@agentdash/core` diagnostics facts.
- `packages/core/src/local-runtime/index.ts` - local runtime/desktop diagnostics TypeScript port and view-model types.
- `crates/agentdash-local/src/runtime.rs` - Rust local runtime supervisor status struct returned through Tauri commands.
- `crates/agentdash-local-tauri/src/main.rs` - Tauri command-local `DesktopApiSnapshot` and command handlers returning local runtime snapshots.
- `packages/app-tauri/src/runtimeApi.ts` and `packages/app-tauri/src/desktopSettings.ts` - Tauri `invoke()` adapters that type command results using `@agentdash/core/local-runtime`.

### Current Backend Runtime Summary Mapping

Application layer:

- `BackendRuntimeOnlineSnapshot` and `BackendRuntimeExecutorSnapshot` are registry/input snapshots, not browser wire DTOs (`crates/agentdash-application/src/backend/runtime_summary.rs:12`, `crates/agentdash-application/src/backend/runtime_summary.rs:20`).
- `BackendRuntimeSummary` is the application read model with backend config, id/name/enabled/online, optional capabilities and runtime health, executor summaries, active leases, and allocatable flag (`crates/agentdash-application/src/backend/runtime_summary.rs:28`).
- `BackendRuntimeExecutorSummary` is the application executor projection with `executor_id`, `name`, `variants`, `available`, active lease count, and `allocatable` (`crates/agentdash-application/src/backend/runtime_summary.rs:43`).
- `project_backend_runtime_summaries()` computes `online`, runtime health, active leases, executor summaries, and `allocatable = backend.enabled && online && any executor.allocatable` (`crates/agentdash-application/src/backend/runtime_summary.rs:95`, `crates/agentdash-application/src/backend/runtime_summary.rs:100`, `crates/agentdash-application/src/backend/runtime_summary.rs:113`, `crates/agentdash-application/src/backend/runtime_summary.rs:114`, `crates/agentdash-application/src/backend/runtime_summary.rs:119`).
- Executor allocatable is currently equal to executor availability, with active lease count counted case-insensitively by executor id (`crates/agentdash-application/src/backend/runtime_summary.rs:136`, `crates/agentdash-application/src/backend/runtime_summary.rs:155`).

API route and route-local DTO:

- The route is registered as `GET /backends/runtime-summary` and handled by `list_runtime_summary` (`crates/agentdash-api/src/routes/backends.rs:67`, `crates/agentdash-api/src/routes/backends.rs:71`, `crates/agentdash-api/src/routes/backends.rs:175`).
- `list_runtime_summary` filters visible backends by current user, reads online registry snapshots, calls `list_backend_runtime_summaries`, maps to `backend_runtime_summary_response`, and returns `Json<Vec<BackendRuntimeSummaryResponse>>` (`crates/agentdash-api/src/routes/backends.rs:178`, `crates/agentdash-api/src/routes/backends.rs:180`, `crates/agentdash-api/src/routes/backends.rs:186`, `crates/agentdash-api/src/routes/backends.rs:187`, `crates/agentdash-api/src/routes/backends.rs:199`, `crates/agentdash-api/src/routes/backends.rs:201`).
- Current route-local DTOs are `BackendRuntimeSummaryResponse`, `BackendRuntimeExecutorResponse`, and `BackendActiveSessionResponse` (`crates/agentdash-api/src/dto/backend.rs:71`, `crates/agentdash-api/src/dto/backend.rs:84`, `crates/agentdash-api/src/dto/backend.rs:94`).
- `BackendRuntimeSummaryResponse` fields are `backend_id`, `name`, `enabled`, `online`, `runtime_health`, `executors`, `active_session_count`, `active_sessions`, and `allocatable` (`crates/agentdash-api/src/dto/backend.rs:71`).
- `BackendActiveSessionResponse` includes serialized domain enums `BackendExecutionSelectionMode` and `BackendExecutionLeaseState` (`crates/agentdash-api/src/dto/backend.rs:94`, `crates/agentdash-api/src/dto/backend.rs:101`, `crates/agentdash-api/src/dto/backend.rs:102`).
- The mapper `backend_runtime_summary_response` is already the single application projection to wire DTO adapter and should remain the mapping point after the DTO types move (`crates/agentdash-api/src/routes/backends.rs:283`, `crates/agentdash-api/src/routes/backends.rs:286`, `crates/agentdash-api/src/routes/backends.rs:300`, `crates/agentdash-api/src/routes/backends.rs:303`, `crates/agentdash-api/src/routes/backends.rs:312`).
- `active_session_response` maps `BackendExecutionLease` into `BackendActiveSessionResponse` (`crates/agentdash-api/src/routes/backends.rs:237`, `crates/agentdash-api/src/routes/backends.rs:238`).

Current generated backend contract coverage:

- `agentdash-contracts::backend` already contains `BackendRuntimeHealthResponse`, `BackendCapabilitiesResponse`, `BackendResponse`, and `BackendWithStatusResponse` (`crates/agentdash-contracts/src/backend/contract.rs:83`, `crates/agentdash-contracts/src/backend/contract.rs:115`, `crates/agentdash-contracts/src/backend/contract.rs:123`, `crates/agentdash-contracts/src/backend/contract.rs:180`).
- `generate_ts.rs` emits these existing backend types into `backend-contracts.ts`, but does not export runtime summary/active-session DTOs (`crates/agentdash-contracts/src/generate_ts.rs:385`, `crates/agentdash-contracts/src/generate_ts.rs:392`, `crates/agentdash-contracts/src/generate_ts.rs:396`, `crates/agentdash-contracts/src/generate_ts.rs:400`, `crates/agentdash-contracts/src/generate_ts.rs:401`).
- `rg` found no `BackendRuntimeSummary`, `BackendRuntimeExecutorSummary`, `BackendActiveSession`, `BackendExecutionSelectionMode`, or `BackendExecutionLeaseState` in `packages/app-web/src/generated/backend-contracts.ts`; the generated file currently only has backend health/config/access style types.

Frontend mirror and consumers:

- `packages/app-web/src/types/index.ts` already aliases backend config from generated `BackendWithStatusResponse`, so the runtime summary mirror is the remaining obvious backend runtime gap (`packages/app-web/src/types/index.ts:1`, `packages/app-web/src/types/index.ts:31`).
- `packages/app-web/src/types/acp.ts` hand-writes `BackendExecutionSelectionMode`, `BackendExecutionLeaseState`, `BackendActiveSession`, `BackendRuntimeExecutorSummary`, and `BackendRuntimeSummary` (`packages/app-web/src/types/acp.ts:65`, `packages/app-web/src/types/acp.ts:66`, `packages/app-web/src/types/acp.ts:68`, `packages/app-web/src/types/acp.ts:82`, `packages/app-web/src/types/acp.ts:91`).
- The same file already aliases `RuntimeHealthStatus` and `RuntimeHealth` from generated backend contracts, which is the intended migration pattern for the summary types (`packages/app-web/src/types/acp.ts:62`, `packages/app-web/src/types/acp.ts:63`).
- `coordinatorStore` fetches `/backends/runtime-summary` as `BackendRuntimeSummary[]` from `../types`, so the store can switch to the generated alias without service-level identity mapping (`packages/app-web/src/stores/coordinatorStore.ts:2`, `packages/app-web/src/stores/coordinatorStore.ts:35`, `packages/app-web/src/stores/coordinatorStore.ts:37`).
- Settings UI reads only presentation fields from the summary, especially `active_session_count` and `allocatable` (`packages/app-web/src/features/settings/ui/SettingsSystemSections.tsx:371`, `packages/app-web/src/features/settings/ui/SettingsSystemSections.tsx:374`, `packages/app-web/src/features/settings/ui/SettingsSystemSections.tsx:499`).
- Runtime diagnostics reduces full `BackendRuntimeSummary` into a narrow `RuntimeDiagnosticsRuntimeSummaryFact` with only `backend_id`, `online`, `allocatable`, and `active_session_count` (`packages/app-web/src/features/settings/model/runtimeDiagnostics.ts:49`, `packages/app-web/src/features/settings/model/runtimeDiagnostics.ts:51`, `packages/app-web/src/features/settings/model/runtimeDiagnostics.ts:53`, `packages/app-web/src/features/settings/model/runtimeDiagnostics.ts:56`).

### What Should Move Into `agentdash-contracts`

Move these current route-local/browser-facing wire DTOs to `crates/agentdash-contracts/src/backend/contract.rs` and export them from `backend-contracts.ts`:

- `BackendRuntimeSummaryResponse`: primary `GET /backends/runtime-summary` response item. It is directly consumed by frontend store and settings UI, so it satisfies the spec rule for generated contract ownership.
- `BackendRuntimeExecutorResponse`: nested wire DTO under `BackendRuntimeSummaryResponse.executors`.
- `BackendActiveSessionResponse`: nested wire DTO under `BackendRuntimeSummaryResponse.active_sessions`.
- `BackendExecutionSelectionMode` wire enum: currently hand-written as `"explicit" | "auto_idle" | "workspace_binding"` in the frontend and serialized from a domain enum. The contract crate should own/export the browser-facing enum shape, either with the same name or a `Dto` suffix plus frontend alias.
- `BackendExecutionLeaseState` wire enum: currently hand-written as `"claimed" | "running" | "released" | "lost" | "failed"` in the frontend and serialized from a domain enum. It should be generated with the active-session DTO.

Keep these as application/domain internals, not generated DTOs:

- `BackendRuntimeOnlineSnapshot` and `BackendRuntimeExecutorSnapshot`: these are registry/application input snapshots, not browser responses.
- `BackendRuntimeSummary` and `BackendRuntimeExecutorSummary` from `agentdash-application`: these are application read models. They can keep non-wire fields such as `backend`, `capabilities`, and active `BackendExecutionLease` domain values.
- `BackendExecutionLease`: domain/runtime lease entity; only the projected active-session wire subset should be generated.
- `OnlineBackendInfo` and registry executor snapshots in `agentdash-api`/relay registry: these are route input facts for projection, not stable browser contracts.

Frontend types should become aliases or view models:

- In `packages/app-web/src/types/acp.ts`, replace the hand-written wire mirror with aliases from `../generated/backend-contracts`:
  - `BackendRuntimeSummary = import("../generated/backend-contracts").BackendRuntimeSummaryResponse`
  - `BackendRuntimeExecutorSummary = import("../generated/backend-contracts").BackendRuntimeExecutorResponse`
  - `BackendActiveSession = import("../generated/backend-contracts").BackendActiveSessionResponse`
  - `BackendExecutionSelectionMode = import("../generated/backend-contracts").BackendExecutionSelectionMode`
  - `BackendExecutionLeaseState = import("../generated/backend-contracts").BackendExecutionLeaseState`
- Keep `RuntimeDiagnosticsRuntimeSummaryFact`, `RuntimeDiagnosticsSnapshot`, `RunnerLayerStatus`, `DesktopApiLayerStatus`, and other `@agentdash/core/local-runtime` types as frontend/core view models. They intentionally project a smaller diagnostics surface from backend summary and desktop/local facts.
- Keep settings UI derived display strings and badge states as UI view model logic. They read generated fields but are not wire DTOs.

### Desktop Local Runtime Snapshot Inclusion Decision

Recommendation: do not include desktop local runtime snapshot in this implementation round. Complete backend runtime summary generated contracts first.

Reasons:

- The current B group has a tight HTTP contract target: `GET /backends/runtime-summary` route-local DTO plus frontend mirror. This can be migrated inside `agentdash-contracts`, `agentdash-api`, generated TS, and app-web type aliases.
- Desktop local runtime is a different wire boundary: Tauri commands return `LocalRuntimeSnapshot`/`DesktopApiSnapshot` to `packages/app-tauri`, and `@agentdash/core/local-runtime` defines the TypeScript port. This is not currently emitted through `agentdash-contracts`.
- Desktop raw snapshot structs expose supervisor/application state. Rust local runtime `LocalRuntimeStatus` includes owner, registration source, backend id, workspace roots, timestamps, retry, and relay connection fields (`crates/agentdash-local/src/runtime.rs:80`, `crates/agentdash-local/src/runtime.rs:94`). Tauri has a command-local `DesktopApiSnapshot` struct (`crates/agentdash-local-tauri/src/main.rs:82`) and returns it via `desktop_api_snapshot` (`crates/agentdash-local-tauri/src/main.rs:301`, `crates/agentdash-local-tauri/src/main.rs:303`).
- The frontend/core side already separates stable diagnostics view models from raw local runtime/Tauri facts: `LocalRuntimeStatus`, `DesktopApiSnapshot`, `RuntimeDiagnosticsSnapshot`, `RuntimeDiagnosticsBackendFact`, `RuntimeDiagnosticsRuntimeSummaryFact`, and `RuntimeDiagnosticsInput` live in `packages/core/src/local-runtime/index.ts` (`packages/core/src/local-runtime/index.ts:14`, `packages/core/src/local-runtime/index.ts:59`, `packages/core/src/local-runtime/index.ts:137`, `packages/core/src/local-runtime/index.ts:149`, `packages/core/src/local-runtime/index.ts:172`, `packages/core/src/local-runtime/index.ts:186`).
- Including desktop now would add dependencies across `crates/agentdash-local`, `crates/agentdash-local-tauri`, `packages/core`, `packages/app-tauri`, `packages/views`, and `packages/app-web`. That broadens the work beyond the backend runtime summary route and raises conflict risk with parallel agents.

If a later implementation includes desktop diagnostics, define a stable diagnostics DTO rather than generating raw supervisor structs. Draft shape:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum DesktopApiRuntimeStateDto {
    Starting,
    Running,
    Error,
    Stopped,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct DesktopApiDiagnosticsDto {
    pub state: DesktopApiRuntimeStateDto,
    pub origin: String,
    pub message: Option<String>,
    pub database_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum LocalRuntimeLayerStateDto {
    Idle,
    Disabled,
    WaitingForAuth,
    WaitingForApi,
    Claiming,
    Starting,
    Running,
    Retrying,
    Stopping,
    Stopped,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct LocalRuntimeDiagnosticsDto {
    pub state: LocalRuntimeLayerStateDto,
    pub backend_id: Option<String>,
    pub name: Option<String>,
    pub registration_source: Option<String>,
    pub workspace_roots: Vec<String>,
    pub message: Option<String>,
    pub last_error: Option<String>,
    pub last_attempt_at: Option<String>,
    pub next_retry_at: Option<String>,
    pub retry_count: Option<u32>,
    pub relay: Option<RelayConnectionDiagnosticsDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct BackendRuntimeSummaryDiagnosticsDto {
    pub backend_id: String,
    pub online: bool,
    pub allocatable: bool,
    pub active_session_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct RuntimeDiagnosticsSnapshotDto {
    pub generated_at: String,
    pub cloud_api: CloudApiDiagnosticsDto,
    pub desktop_api: Option<DesktopApiDiagnosticsDto>,
    pub local_runtime: Option<LocalRuntimeDiagnosticsDto>,
    pub backends: Vec<RuntimeDiagnosticsBackendDto>,
    pub runtime_summaries: Vec<BackendRuntimeSummaryDiagnosticsDto>,
}
```

Dependencies and risks if desktop is included now:

- Need to decide whether `agentdash-contracts` is allowed to own Tauri/local-runtime command DTOs, or whether a separate local/desktop contract package should generate `@agentdash/core` types.
- Need to map Rust `agentdash-local::runtime::LocalRuntimeStatus` and `agentdash-local-tauri::DesktopApiSnapshot` into stable diagnostics DTOs before exposing generated types to `packages/core`.
- Need to avoid freezing implementation-only supervisor details like retry internals, relay raw state, and private local config fields unless they are truly diagnostics contract.
- Need app-tauri and views type updates in addition to app-web, raising parallel-change conflict risk.
- Need new validation beyond backend contract check: app-tauri typecheck, core/view tests, and possibly Tauri command serialization checks.

### Recommended Implementation Order

1. In `agentdash-contracts::backend`, add generated wire DTOs for `BackendRuntimeSummaryResponse`, `BackendRuntimeExecutorResponse`, `BackendActiveSessionResponse`, `BackendExecutionSelectionMode`, and `BackendExecutionLeaseState`.
2. Add `From`/mapping helpers from domain/application values where appropriate, or keep mapping in `crates/agentdash-api/src/routes/backends.rs` but construct contract DTOs there. Preserve the route adapter as the single application projection to wire mapper.
3. Update `crates/agentdash-contracts/src/generate_ts.rs` to export the new backend runtime summary DTOs into `backend-contracts.ts`.
4. Update `crates/agentdash-api/src/dto/backend.rs` imports/aliases so `/backends/runtime-summary` returns the contract DTOs rather than API-local structs. Remove the old route-local summary/executor/active-session structs after generated DTOs compile.
5. Run contract generation/check and update generated TS intentionally.
6. Replace frontend hand-written runtime summary/active-session/lease enum definitions in `packages/app-web/src/types/acp.ts` with generated aliases. Keep `RuntimeDiagnosticsRuntimeSummaryFact` and `runtimeSummaryDiagnosticsFacts()` as the narrow diagnostics view-model projection.
7. Run focused verification, then only consider a separate desktop diagnostics DTO task if the backend HTTP summary path is green.

### Validation Commands

Recommended minimum for backend runtime summary generated contracts:

```powershell
pnpm run contracts:check
cargo test -p agentdash-contracts
pnpm --filter app-web typecheck
```

Useful targeted checks if implementation touches mappers or tests:

```powershell
cargo test -p agentdash-application runtime_summary
cargo test -p agentdash-api backends
pnpm --filter app-web test -- runtimeDiagnostics
```

If a future desktop diagnostics DTO is included, add:

```powershell
pnpm --filter @agentdash/core typecheck
pnpm --filter @agentdash/views typecheck
pnpm --filter app-tauri typecheck
```

## Caveats / Not Found

- `task.py current --source` returned no active task in this shell, but the user supplied the explicit task path. Research output is written under the supplied task's `research/` directory.
- No external docs were needed; this mapping is based on repository code and Trellis specs only.
- `packages/app-web/src/generated/backend-contracts.ts` currently does not contain runtime summary, executor summary, active session, or execution lease enum types.
- Desktop local runtime diagnostics are already modeled as `@agentdash/core` view models, but not as generated Rust contracts. Pulling them into this B implementation would require a separate contract ownership decision, not just moving the backend runtime summary DTOs.
- This research intentionally did not inspect or modify Work Group A generator internals beyond identifying the existing `backend-contracts.ts` export registration point.

