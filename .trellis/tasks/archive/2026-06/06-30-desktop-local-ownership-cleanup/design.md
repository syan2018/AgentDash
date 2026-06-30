# Design

## Boundary

D11 gives one owner to desktop local durable facts:

- `agentdash-local` owns local runtime profile/settings/claim DTOs, normalization, file IO and HTTP
  ensure claim semantics.
- `agentdash-local-tauri` owns the desktop shell: Tauri command registration, tray/window lifecycle,
  autostart OS integration, error string mapping and calls into `DesktopRunnerHost`.
- `LocalRuntimeConfig` remains the runtime start contract consumed by `DesktopRunnerHost`.

This keeps product runtime facts usable by CLI/tests/future shells and prevents Tauri `main.rs` from
becoming a second local runtime implementation.

## Current Evidence

In `crates/agentdash-local-tauri/src/main.rs`:

- `RuntimeStartRequest`, `LocalRuntimeProfile`, `DesktopAppSettings`,
  `EnsureLocalRuntimePayload`, `EnsureLocalRuntimeResponse` are defined in Tauri.
- `profile_load`, `profile_save`, `profile_delete` read/write the local runtime profile path
  directly.
- `desktop_settings_load_internal` and `desktop_settings_write_internal` read/write
  `desktop-app-settings.json` directly.
- `claim_local_runtime`, `post_local_runtime_claim` and `validate_claim_response` implement desktop
  ensure semantics in Tauri.
- `normalize_profile` and `normalize_start_request` load machine identity and construct the durable
  local runtime values.

In `crates/agentdash-local`:

- `LocalRuntimeConfig`, `DesktopRunnerHost`, machine identity and runtime paths already live in the
  local crate.
- Standalone runner claim exists in `runner_claim.rs`, but it targets
  `/api/local-runtime/runner/claim` with registration token semantics; desktop access-token ensure is
  a separate API and should be added as a desktop-specific local module instead of forced into runner
  token claim.

## Proposed Modules

Add focused modules under `crates/agentdash-local/src/`:

- `desktop_profile.rs`
  - `DesktopRuntimeStartRequest`
  - `LocalRuntimeProfile`
  - `load_desktop_runtime_profile()`
  - `save_desktop_runtime_profile(profile)`
  - `delete_desktop_runtime_profile()`
  - `normalize_desktop_runtime_profile(profile)`
  - `normalize_desktop_runtime_start_request(request)`

- `desktop_settings.rs`
  - `DesktopAppSettings`
  - `load_desktop_app_settings()`
  - `save_desktop_app_settings(settings)`
  - `normalize_desktop_app_settings(settings)`

- `desktop_claim.rs`
  - `DesktopEnsureLocalRuntimePayload`
  - `DesktopEnsureLocalRuntimeResponse`
  - `ensure_desktop_local_runtime(request, retry callback)`
  - `validate_desktop_ensure_response(response, request)`
  - `desktop_runtime_config_from_ensure(request, response)`

The exact names can change to fit local style. The important contract is that Tauri does not own
the DTOs or implementation for profile/settings/claim.

## Implemented Shape

Implemented modules:

- `crates/agentdash-local/src/desktop_profile.rs`
  - owns `DesktopRuntimeStartRequest`, `LocalRuntimeProfile`, profile load/save/delete,
    machine-identity normalization and profile/start request origin normalization.
- `crates/agentdash-local/src/desktop_settings.rs`
  - owns `DesktopAppSettings`, default `auto_connect_local_runtime = true`, settings load/save and
    normalization.
- `crates/agentdash-local/src/desktop_claim.rs`
  - owns desktop ensure request/response DTOs, `/api/local-runtime/ensure` HTTP client,
    retry policy/event, response validation, HTTP status mapping and `LocalRuntimeConfig`
    projection.

Tauri command names and snake_case payload shape are unchanged. `agentdash-local-tauri` passes the
current Desktop Dashboard API origin into local profile/start request normalization, because API
origin selection remains a shell packaging concern while profile/start facts are local runtime
facts.

## Runtime Start Shape

Tauri `runtime_start` should:

1. Receive `DesktopRuntimeStartRequest`.
2. Call a local API that normalizes request, performs desktop ensure with optional retry notification
   hook, and returns `LocalRuntimeConfig`.
3. Pass that config into `DesktopRunnerHost::ensure_started_with`.
4. Map errors to `String` for Tauri.

Auto-connect should load settings/profile through local APIs and reuse the same start helper.

## Autostart Boundary

Autostart is OS shell integration, so Tauri keeps:

- `desktop_autostart_is_enabled_internal`
- `desktop_autostart_set_enabled_internal`
- platform-specific registry/path code

When autostart changes settings, Tauri updates the `launch_at_login` field and delegates settings
write to `agentdash-local`.

## Non-Goals

- Do not redesign runner registration token claim.
- Do not change Tauri command names unless impossible to keep.
- Do not move tray/window/autostart/lifecycle state into `agentdash-local`.
- Do not introduce compatibility wrappers that keep the old Tauri implementation alive.

## Validation

- `agentdash-local` unit tests:
  - profile save/load/delete roundtrip with machine identity normalization;
  - settings defaults and save/load roundtrip;
  - desktop ensure response validation for machine mismatch, non-user scope, non-default slot and
    non-desktop registration source;
  - runtime config projection from ensure response.
- Tauri targeted compile check or scoped tests if available.
- Static search proving removed Tauri-local owner paths:
  - no `struct LocalRuntimeProfile` / `struct DesktopAppSettings` in Tauri main;
  - no `post_local_runtime_claim` / `validate_claim_response` in Tauri main;
  - no profile/settings file IO helpers in Tauri main.

Validation completed:

- `cargo test -p agentdash-local desktop_profile --lib`: 7 tests.
- `cargo test -p agentdash-local desktop_settings --lib`: 4 tests.
- `cargo test -p agentdash-local desktop_claim --lib`: 8 tests.
- `cargo check -p agentdash-local-tauri` passed.
- Static search found no active old Tauri profile/settings/claim owner helpers.
