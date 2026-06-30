# Research: agentdash-local module/test pattern map

- Query: Map `agentdash-local` module/export/test patterns for moving desktop profile/settings/claim/start-config ownership out of Tauri.
- Scope: internal
- Date: 2026-06-30

## Findings

### Files found

- `.trellis/tasks/06-30-desktop-local-ownership-cleanup/prd.md` - durable facts owner must become `agentdash-local`; Tauri keeps command/error/lifecycle adapter only.
- `.trellis/tasks/06-30-desktop-local-ownership-cleanup/design.md` - proposes `desktop_profile`, `desktop_settings`, and `desktop_claim` modules; runner token claim remains separate.
- `.trellis/tasks/06-30-desktop-local-ownership-cleanup/implement.md` - orders profile/settings move before claim/start-config and forbids old Tauri compatibility paths.
- `.trellis/tasks/06-30-desktop-local-ownership-cleanup/implement.jsonl` - relevant context points to local exports, runtime paths, machine identity, runner config/claim, and desktop runtime spec.
- `.trellis/spec/cross-layer/desktop-local-runtime.md` - current contract: machine identity/profile facts belong to `agentdash-local`, desktop settings bridge stays via Tauri commands, desktop enrollment source is `desktop_access_token`, runner enrollment source is `runner_registration_token`.
- `crates/agentdash-local/src/lib.rs` - current local crate export style.
- `crates/agentdash-local/src/runtime_paths.rs` - local runtime path source.
- `crates/agentdash-local/src/machine_identity.rs` - local machine identity normalization/persistence pattern.
- `crates/agentdash-local/src/runtime.rs` - `LocalRuntimeConfig` and runtime status contracts consumed by `DesktopRunnerHost`.
- `crates/agentdash-local/src/desktop_runner_host.rs` - serialized desktop start/claim boundary.
- `crates/agentdash-local/src/runner_config.rs` - config DTO, file IO, normalization, and focused unit test pattern.
- `crates/agentdash-local/src/runner_claim.rs` - HTTP claim client, error classification, response projection, and focused unit test pattern.
- `crates/agentdash-local-tauri/src/main.rs` - current profile/settings/claim owner fork to remove from active path.
- `crates/agentdash-api/src/dto/backend.rs` and `crates/agentdash-api/src/routes/backends.rs` - server `/api/local-runtime/ensure` request/response shape.
- `crates/agentdash-contracts/src/backend/contract.rs` - existing generated runner claim response and shared backend enum types.

### Existing local code patterns to preserve

- `lib.rs` keeps many modules private and re-exports stable API at crate root; current examples are `pub use runtime::{LocalRuntimeConfig, ...}` and `pub use machine_identity::{LocalMachineIdentity, load_or_create_machine_identity}` at `crates/agentdash-local/src/lib.rs:41` and `crates/agentdash-local/src/lib.rs:48`. New desktop modules should follow this root re-export pattern so Tauri imports product APIs, not implementation files.
- `runner_claim` is an exception as a public module because the CLI imports `agentdash_local::runner_claim::{claim_runner, direct_credentials}` from `src/main.rs`; see `crates/agentdash-local/src/lib.rs:16` and `crates/agentdash-local/src/main.rs:6`. Desktop APIs do not need that exception unless Tauri intentionally wants a namespaced import.
- `runtime_paths` is already the path owner for local runtime data/config/profile/MCP/machine identity: `local_runtime_config_dir()` and `local_runtime_profile_path()` are defined at `crates/agentdash-local/src/runtime_paths.rs:45` and `crates/agentdash-local/src/runtime_paths.rs:49`. Add `desktop_app_settings_path()` there rather than leaving Tauri to join the filename.
- `machine_identity` exposes only `load_or_create_machine_identity()` publicly, with a private `load_or_create_machine_identity_at(path)` used by module tests at `crates/agentdash-local/src/machine_identity.rs:14` and `crates/agentdash-local/src/machine_identity.rs:18`. Profile/settings modules should use the same public default-path API plus private `_at` helpers for focused tests.
- `LocalRuntimeConfig::new()` canonicalizes workspace roots and is the final start contract at `crates/agentdash-local/src/runtime.rs:40`. Desktop start-config projection should call this constructor instead of assembling the struct directly.
- `DesktopRunnerHost::ensure_started_with()` serializes config construction and runtime start in one lock at `crates/agentdash-local/src/desktop_runner_host.rs:43`. The new desktop start helper should be called inside that closure; it should not move host lifecycle, tray, or retry state into profile/settings modules.
- `runner_config` uses DTO structs plus `read_config_file`, `persist_credentials`, private atomic write helpers, and `tempfile` unit tests without a broad compile path; see `crates/agentdash-local/src/runner_config.rs:97`, `crates/agentdash-local/src/runner_config.rs:382`, `crates/agentdash-local/src/runner_config.rs:392`, and tests at `crates/agentdash-local/src/runner_config.rs:666`.
- `runner_claim` separates request construction, HTTP error mapping, and response-to-credentials projection. `claim_runner()` is public at `crates/agentdash-local/src/runner_claim.rs:51`, `credentials_from_claim()` is directly unit-tested at `crates/agentdash-local/src/runner_claim.rs:122` and `crates/agentdash-local/src/runner_claim.rs:189`, and HTTP status mapping is unit-tested without a mock server at `crates/agentdash-local/src/runner_claim.rs:212`.

### Recommended modules, exports, type names, and function signatures

Recommended additions in `crates/agentdash-local/src/`:

```rust
mod desktop_profile;
mod desktop_settings;
mod desktop_claim;
```

Recommended root exports in `lib.rs`:

```rust
pub use desktop_profile::{
    DesktopRuntimeStartRequest, LocalRuntimeProfile, delete_desktop_runtime_profile,
    load_desktop_runtime_profile, normalize_desktop_runtime_profile,
    normalize_desktop_runtime_start_request, save_desktop_runtime_profile,
};
pub use desktop_settings::{
    DesktopAppSettings, load_desktop_app_settings, normalize_desktop_app_settings,
    save_desktop_app_settings,
};
pub use desktop_claim::{
    DesktopClaimError, DesktopEnsureRetryEvent, DesktopEnsureRetryPolicy,
    DesktopEnsureLocalRuntimePayload, DesktopEnsureLocalRuntimeResponse,
    desktop_runtime_config_from_ensure, ensure_desktop_local_runtime,
    ensure_desktop_runtime_config, validate_desktop_ensure_response,
};
pub use runtime_paths::{
    desktop_app_settings_path, local_mcp_servers_path, local_runtime_config_dir,
    local_runtime_data_dir, local_runtime_profile_path, machine_identity_path,
};
```

Suggested `desktop_profile.rs` public surface:

```rust
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct LocalRuntimeProfile {
    pub server_url: String,
    #[serde(default)]
    pub access_token: String,
    #[serde(default = "default_profile_id")]
    pub profile_id: String,
    #[serde(default)]
    pub machine_id: String,
    #[serde(default)]
    pub machine_label: Option<String>,
    #[serde(default)]
    pub backend_id: Option<String>,
    #[serde(default)]
    pub relay_ws_url: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub workspace_roots: Vec<PathBuf>,
    #[serde(default = "default_executor_enabled")]
    pub executor_enabled: bool,
    #[serde(default)]
    pub auto_start: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DesktopRuntimeStartRequest {
    pub server_url: String,
    #[serde(default)]
    pub access_token: String,
    pub profile_id: String,
    #[serde(default)]
    pub machine_id: String,
    #[serde(default)]
    pub machine_label: Option<String>,
    pub name: Option<String>,
    #[serde(default)]
    pub workspace_roots: Vec<PathBuf>,
    pub executor_enabled: bool,
}

pub fn load_desktop_runtime_profile() -> anyhow::Result<Option<LocalRuntimeProfile>>;
pub fn save_desktop_runtime_profile(profile: LocalRuntimeProfile) -> anyhow::Result<LocalRuntimeProfile>;
pub fn delete_desktop_runtime_profile() -> anyhow::Result<()>;
pub fn normalize_desktop_runtime_profile(profile: LocalRuntimeProfile) -> anyhow::Result<LocalRuntimeProfile>;
pub fn normalize_desktop_runtime_start_request(request: DesktopRuntimeStartRequest) -> anyhow::Result<DesktopRuntimeStartRequest>;
```

Private test helpers should mirror `machine_identity`:

```rust
fn load_desktop_runtime_profile_at(path: PathBuf) -> anyhow::Result<Option<LocalRuntimeProfile>>;
fn save_desktop_runtime_profile_at(path: PathBuf, profile: LocalRuntimeProfile) -> anyhow::Result<LocalRuntimeProfile>;
fn delete_desktop_runtime_profile_at(path: PathBuf) -> anyhow::Result<()>;
fn normalize_desktop_runtime_profile_with_identity(
    profile: LocalRuntimeProfile,
    identity: LocalMachineIdentity,
    current_server_origin: &str,
) -> LocalRuntimeProfile;
fn normalize_desktop_runtime_start_request_with_identity(
    request: DesktopRuntimeStartRequest,
    identity: LocalMachineIdentity,
    current_server_origin: &str,
) -> DesktopRuntimeStartRequest;
```

The explicit `current_server_origin` parameter is important: Tauri currently normalizes `server_url` to `desktop_api_config().origin`, not to the persisted profile value, at `crates/agentdash-local-tauri/src/main.rs:831`. Because `agentdash-local` should not own Tauri's API-origin selection, Tauri should pass the already selected Dashboard API origin into local normalization/start-config APIs.

Suggested `desktop_settings.rs` public surface:

```rust
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct DesktopAppSettings {
    #[serde(default)]
    pub launch_at_login: bool,
    #[serde(default)]
    pub start_minimized_to_tray: bool,
    #[serde(default = "default_auto_connect_local_runtime")]
    pub auto_connect_local_runtime: bool,
}

impl Default for DesktopAppSettings { ... } // auto_connect_local_runtime = true

pub fn load_desktop_app_settings() -> anyhow::Result<DesktopAppSettings>;
pub fn save_desktop_app_settings(settings: DesktopAppSettings) -> anyhow::Result<DesktopAppSettings>;
pub fn normalize_desktop_app_settings(settings: DesktopAppSettings) -> DesktopAppSettings;
```

Private test helpers:

```rust
fn load_desktop_app_settings_at(path: PathBuf) -> anyhow::Result<DesktopAppSettings>;
fn save_desktop_app_settings_at(path: PathBuf, settings: DesktopAppSettings) -> anyhow::Result<DesktopAppSettings>;
```

`runtime_paths.rs` should gain:

```rust
const DESKTOP_APP_SETTINGS_FILE: &str = "desktop-app-settings.json";

pub fn desktop_app_settings_path() -> anyhow::Result<PathBuf> {
    Ok(local_runtime_config_dir()?.join(DESKTOP_APP_SETTINGS_FILE))
}
```

This directly replaces Tauri's current `desktop_app_settings_path()` which joins `local_runtime_config_dir()` with `DESKTOP_APP_SETTINGS_FILE` at `crates/agentdash-local-tauri/src/main.rs:1730`.

Suggested `desktop_claim.rs` public surface:

```rust
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct DesktopEnsureLocalRuntimePayload {
    pub machine_id: String,
    pub machine_label: Option<String>,
    pub profile_id: String,
    pub scope: DesktopLocalRuntimeScopePayload,
    pub capability_slot: String,
    pub name: Option<String>,
    pub executor_enabled: bool,
    pub client_version: Option<String>,
    pub device: serde_json::Value,
    pub rotate_token: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct DesktopLocalRuntimeScopePayload {
    pub kind: BackendShareScopeKind,
    pub id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DesktopEnsureLocalRuntimeResponse {
    pub backend_id: String,
    pub name: String,
    pub relay_ws_url: String,
    pub auth_token: String,
    #[serde(default)]
    pub backend_enabled: bool,
    pub profile_id: String,
    pub machine_id: String,
    pub machine_label: String,
    pub visibility: Option<BackendVisibility>,
    pub share_scope_kind: BackendShareScopeKind,
    pub share_scope_id: Option<String>,
    pub capability_slot: String,
    pub registration_source: String,
    pub claimed_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone)]
pub struct DesktopEnsureRetryPolicy {
    pub retry_until_server_ready: bool,
    pub max_attempts: u32,
    pub delay: Duration,
}

#[derive(Debug, Clone)]
pub struct DesktopEnsureRetryEvent {
    pub attempt: u32,
    pub next_retry_at: String,
    pub error: String,
}

#[derive(Debug, thiserror::Error)]
pub enum DesktopClaimError {
    #[error("fatal desktop claim error {code}: {message}")]
    Fatal { code: String, message: String },
    #[error("retryable desktop claim error {code}: {message}")]
    Retryable { code: String, message: String },
}

pub async fn ensure_desktop_local_runtime<F, Fut>(
    request: &DesktopRuntimeStartRequest,
    retry_policy: DesktopEnsureRetryPolicy,
    on_retry: F,
) -> Result<DesktopEnsureLocalRuntimeResponse, DesktopClaimError>
where
    F: FnMut(DesktopEnsureRetryEvent) -> Fut,
    Fut: Future<Output = ()>;

pub async fn ensure_desktop_runtime_config<F, Fut>(
    request: DesktopRuntimeStartRequest,
    current_server_origin: &str,
    retry_policy: DesktopEnsureRetryPolicy,
    on_retry: F,
) -> anyhow::Result<LocalRuntimeConfig>
where
    F: FnMut(DesktopEnsureRetryEvent) -> Fut,
    Fut: Future<Output = ()>;

pub fn validate_desktop_ensure_response(
    response: &DesktopEnsureLocalRuntimeResponse,
    request: &DesktopRuntimeStartRequest,
) -> anyhow::Result<()>;

pub fn desktop_runtime_config_from_ensure(
    request: &DesktopRuntimeStartRequest,
    response: DesktopEnsureLocalRuntimeResponse,
) -> anyhow::Result<LocalRuntimeConfig>;
```

Use `agentdash_contracts::backend::{BackendShareScopeKind, BackendVisibility}` for local-side DTO enums. `agentdash-local` already depends on `agentdash-contracts` and `agentdash-domain` in `Cargo.toml` at `crates/agentdash-local/Cargo.toml:18`; `agentdash-api` owns server DTOs but should not become a dependency of `agentdash-local`.

`DesktopEnsureLocalRuntimeResponse` should include the full server response, not the current Tauri subset. Server DTO includes `backend_enabled`, `profile_id`, `visibility`, typed `share_scope_kind`, `registration_source`, and `claimed_at` at `crates/agentdash-api/src/dto/backend.rs:48`; route projection fills those fields at `crates/agentdash-api/src/routes/backends.rs:378`.

### Reusing existing patterns

- Reuse `runtime_paths` by adding `desktop_app_settings_path()` and keeping profile at `local_runtime_profile_path()`. This keeps profile/settings/MCP/machine identity under the same `local-runtime/config` and data root defined by `runtime_paths.rs:45`.
- Reuse `machine_identity` in profile/start normalization exactly as Tauri does now, but local-owned. Tauri currently overwrites `machine_id` from `load_or_create_machine_identity()` in both profile and start request at `crates/agentdash-local-tauri/src/main.rs:788` and `crates/agentdash-local-tauri/src/main.rs:811`.
- Reuse `LocalRuntimeConfig::new()` for start-config projection; it already owns workspace root canonicalization at `crates/agentdash-local/src/runtime.rs:40` and `crates/agentdash-local/src/runtime.rs:444`.
- Reuse `runner_config` file IO style: public default-path APIs, private `_at` helpers, `serde_json::to_string_pretty` for JSON files, parent-dir creation before write, and `tempfile` roundtrip tests. Atomic replacement is valuable but not currently used by Tauri settings/profile; if added, keep it inside local modules rather than in Tauri.
- Reuse `runner_claim` error pattern: expose `code()`, `message()`, `is_retryable()` on `DesktopClaimError`, classify 401/403 as fatal auth errors, 429/5xx/transport as retryable, and unit-test mapping functions without starting a server. Current runner mapping is at `crates/agentdash-local/src/runner_claim.rs:136`.
- Reuse `DesktopRunnerHost::ensure_started_with()` by making Tauri pass a closure that calls `ensure_desktop_runtime_config(...)`. Retry UI/snapshot updates stay in Tauri/host callback, while HTTP claim and validation live in local.

### Focused unit tests, avoiding broad compilation

No full workspace check is needed for this research-specified work. Add narrow module tests in `agentdash-local` and run them individually only after implementation:

```powershell
cargo test -p agentdash-local desktop_profile --lib
cargo test -p agentdash-local desktop_settings --lib
cargo test -p agentdash-local desktop_claim --lib
```

Profile tests:

- `profile_missing_returns_none`: `load_desktop_runtime_profile_at(temp/profile.json)` returns `Ok(None)` when absent.
- `profile_save_load_roundtrip_normalizes_identity_and_token`: save a profile with whitespace `server_url`, stale machine id, stale machine label, and non-empty `access_token`; loaded profile uses supplied/current machine identity, trims/overrides server origin, and has `access_token == ""`.
- `profile_delete_removes_file`: save, delete, then load returns `None`.
- `start_request_normalization_keeps_one_time_token`: start request normalization trims `access_token` but does not persist it; it uses canonical machine identity and current Dashboard API origin.
- `profile_id_defaults_to_default`: empty profile id becomes `"default"`, matching Tauri current `normalize_profile_id()` at `crates/agentdash-local-tauri/src/main.rs:845`.

Settings tests:

- `settings_missing_returns_default`: default has `auto_connect_local_runtime == true`, matching the PRD and Tauri default at `crates/agentdash-local-tauri/src/main.rs:157`.
- `settings_save_load_roundtrip`: writes `launch_at_login`, `start_minimized_to_tray`, and `auto_connect_local_runtime=false`, then loads the same normalized values.
- `settings_malformed_file_returns_error`: malformed JSON returns an error, matching spec validation at `.trellis/spec/cross-layer/desktop-local-runtime.md:370`.
- `settings_save_creates_parent_dir`: private `_at` helper writes into a nested temp path.

Desktop claim/start-config tests:

- `ensure_payload_uses_desktop_scope_and_slot`: payload builder sets `scope.kind=user`, `scope.id=None`, `capability_slot=default`, `rotate_token=false`, `client_version`, and `device`. This mirrors current payload construction at `crates/agentdash-local-tauri/src/main.rs:668`.
- `validate_rejects_machine_mismatch`: mismatch between request and response machine id errors, matching current validation at `crates/agentdash-local-tauri/src/main.rs:720`.
- `validate_rejects_empty_machine_label`: current validation rejects empty `machine_label` at `crates/agentdash-local-tauri/src/main.rs:727`.
- `validate_rejects_non_user_scope`: response `share_scope_kind != User` errors, matching `main.rs:730`.
- `validate_rejects_non_default_slot`: response `capability_slot != "default"` errors, matching `main.rs:736`.
- `validate_rejects_non_desktop_registration_source`: source other than `desktop_access_token` errors, matching `main.rs:742`. Unlike current Tauri `Option<String>`, the local response should treat missing source as invalid because the server DTO now requires it.
- `runtime_config_projection_uses_relay_credentials`: response `relay_ws_url`, `auth_token`, `backend_id`, `name`, request `workspace_roots`, and `executor_enabled` become `LocalRuntimeConfig::new(...)`.
- `claim_http_error_mapping_redacts_token`: unit-test status/error mapping like `runner_claim` does at `crates/agentdash-local/src/runner_claim.rs:212`. Do not add a mock HTTP server unless an end-to-end HTTP request path test becomes necessary.

If a network-path test is desired without extra dev dependencies, make `post_desktop_local_runtime_claim()` small and keep it untested directly; test URL construction, bearer decision, response parsing, validation, and HTTP status mapping as pure functions. `agentdash-local` currently has only `tempfile` as a dev dependency at `crates/agentdash-local/Cargo.toml:66`.

### Desktop ensure claim vs runner registration token claim boundary

Reusable:

- Error enum shape and helper methods (`code`, `message`, `is_retryable`) from `RunnerClaimError`.
- HTTP status classification concept: auth/client contract errors are fatal; server/transport/rate-limit failures are retryable.
- Redaction via `runner_redaction::redact_secret`, used by `runner_claim` at `crates/agentdash-local/src/runner_claim.rs:9`.
- Machine identity input from `LocalMachineIdentity`.
- Response projection core: `backend_id`, `relay_ws_url`, `auth_token`, `registration_source`, and `claimed_at` are common facts. Spec says desktop ensure is isomorphic to runner claim on this core at `.trellis/spec/cross-layer/desktop-local-runtime.md:122`.
- Server-side application core is shared by design: desktop ensure and runner claim share `enroll_local_backend(...)`, but through different auth adapters at `.trellis/spec/cross-layer/desktop-local-runtime.md:118`.

Must remain separate API:

- Endpoint: desktop uses `POST /api/local-runtime/ensure`; runner uses `POST /api/local-runtime/runner/claim`. Current Tauri endpoint is `main.rs:757`; runner constant is `crates/agentdash-local/src/runner_claim.rs:11`.
- Authentication: desktop uses a transient user access token as Bearer only when present; runner uses a registration token in body/Bearer. Current Tauri bearer behavior is at `crates/agentdash-local-tauri/src/main.rs:760`; runner request body includes `registration_token` at `crates/agentdash-local/src/runner_claim.rs:13`.
- Request shape: desktop sends `profile_id`, user scope, default capability slot, `rotate_token`, and desktop profile name; runner sends `runner_name` and optional registration token/capability slot. Server DTO for desktop request is `crates/agentdash-api/src/dto/backend.rs:24`; runner contract request is `crates/agentdash-contracts/src/backend/contract.rs:484`.
- Response validation: desktop must enforce machine id, non-empty machine label, user scope, default slot, and `desktop_access_token`; runner currently trusts the generated claim response and maps credentials. Desktop validation should not be pushed into `runner_claim`.
- Persistence: desktop profile must clear persisted `access_token` and stores desktop profile/start preferences; runner persists relay credentials into TOML via `persist_credentials()` at `crates/agentdash-local/src/runner_config.rs:392`.
- Lifecycle owner: desktop start-config returns `LocalRuntimeConfig` for `DesktopRunnerHost`; runner claim returns `RunnerCredentials` for standalone config write-back. Mixing these would blur `desktop_embedded_runner` vs service runner facts that specs keep separate at `.trellis/spec/cross-layer/desktop-local-runtime.md:356`.

### Implementation risk and suggested split

- Profile/settings and claim/start-config can be partially parallel, but not fully independent. Profile/settings can land first because they mostly depend on `runtime_paths`, `machine_identity`, and Tauri command delegation. Claim/start-config should consume `DesktopRuntimeStartRequest` from `desktop_profile`, so it should either wait for that type or coordinate on the exact struct before coding.
- The biggest risk is origin ownership. `agentdash-local` should own normalization mechanics and durable facts, but Tauri still owns the current Desktop Dashboard API origin because it comes from desktop API config / packaging / dev environment. Pass `current_server_origin` into local APIs; do not have local call Tauri-only `desktop_api_config()`.
- A second risk is importing server DTOs from `agentdash-api`. Avoid this dependency. Either add a contracts DTO in a separate task or keep a local-side `DesktopEnsureLocalRuntimeResponse` in `agentdash-local` using existing `agentdash_contracts` backend enums.
- A third risk is preserving Tauri command payload shape while moving Rust type ownership. The Rust type can move to local and still derive the same `snake_case` serde shape. Do not rename fields unless there is a correctness reason.
- Static cleanup must remove active Tauri definitions and helpers after delegation. The search target remains: `struct LocalRuntimeProfile`, `struct DesktopAppSettings`, `post_local_runtime_claim`, `validate_claim_response`, `desktop_settings_write_internal`, and `profile_path(` in `crates/agentdash-local-tauri/src/main.rs`.
- Recommended split:
  1. Implement A: `desktop_profile`, `desktop_settings`, `runtime_paths::desktop_app_settings_path`, exports, focused local tests, Tauri profile/settings command delegation.
  2. Implement B: `desktop_claim`, `ensure_desktop_runtime_config`, start-config projection, focused claim tests, Tauri `runtime_start`/auto-connect delegation.
  3. Check pass: static search proving old Tauri owner paths are gone and no broad compile unless final integrator chooses it.

## Caveats / Not Found

- `task.py current --source` returned no active task in this Codex session. The user explicitly provided `.trellis/tasks/06-30-desktop-local-ownership-cleanup`, so this research was written there rather than guessing another task path.
- No external web references were used. External/version references are local manifest facts: `reqwest = 0.13.2` and `tempfile = 3.18` in `crates/agentdash-local/Cargo.toml:38` and `crates/agentdash-local/Cargo.toml:67`.
- I did not run `cargo check`, broad Rust builds, full suites, or git commands, per the research constraints.
- `agentdash-contracts` currently has `RunnerRegistrationClaimResponse` but no desktop ensure response contract; the local module can own a local-side DTO for this slice, but adding a generated contract may be a future cross-package cleanup if frontend/native contract generation needs it.
