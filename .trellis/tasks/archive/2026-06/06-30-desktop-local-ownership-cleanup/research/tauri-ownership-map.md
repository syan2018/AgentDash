# Research: Tauri desktop ownership map

- Query: 审查 `crates/agentdash-local-tauri/src/main.rs` 的 desktop local runtime ownership，标出应下沉到 `agentdash-local` 的 DTO/helper/IO/claim/validation/normalization，保留 Tauri shell 职责，并确认前端 Tauri command payload 是否可保持不变。
- Scope: mixed
- Date: 2026-06-30

## Findings

### Files Found

- `.trellis/tasks/06-30-desktop-local-ownership-cleanup/prd.md` - 本任务需求，明确 durable facts 归 `agentdash-local`，Tauri 只保留 shell adapter。
- `.trellis/tasks/06-30-desktop-local-ownership-cleanup/design.md` - 技术边界，列出 profile/settings/claim/start-config 下沉方向。
- `.trellis/tasks/06-30-desktop-local-ownership-cleanup/implement.md` - 实施顺序与禁止保留旧 Tauri 分叉的约束。
- `.trellis/tasks/06-30-desktop-local-ownership-cleanup/implement.jsonl` - 已列入本任务后续 implement context 的相关代码/spec。
- `.trellis/spec/cross-layer/desktop-local-runtime.md` - 桌面壳、runtime host、settings/profile/auto-connect 跨层契约。
- `crates/agentdash-local-tauri/src/main.rs` - 当前混合了 Tauri shell、profile/settings 文件 IO、desktop claim 和 start config 投影。
- `crates/agentdash-local/src/lib.rs` - local crate 模块/export 入口。
- `crates/agentdash-local/src/runtime.rs` - `LocalRuntimeConfig` 与 runtime lifecycle owner。
- `crates/agentdash-local/src/runtime_paths.rs` - local runtime data/config/profile 路径事实源。
- `crates/agentdash-local/src/machine_identity.rs` - machine identity load/create/normalize owner。
- `crates/agentdash-local/src/runner_claim.rs` - runner claim HTTP/error/test 模式参考，但 endpoint/auth 与 desktop ensure 不同。
- `crates/agentdash-local/src/runner_config.rs` - local crate 配置 DTO、read/write、runtime config 投影和测试模式参考。
- `crates/agentdash-api/src/dto/backend.rs` - cloud `/api/local-runtime/ensure` request/response DTO 形态。
- `crates/agentdash-api/src/routes/backends.rs` - cloud ensure route 映射 response。
- `crates/agentdash-application/src/backend/management.rs` - cloud enrollment use case 与 desktop access-token source。
- `packages/core/src/local-runtime/index.ts` - TS local runtime/profile/settings payload 类型。
- `packages/app-tauri/src/runtimeApi.ts` - Tauri invoke adapter for profile/runtime commands。
- `packages/app-tauri/src/desktopSettings.ts` - Tauri invoke adapter for settings/autostart/quit/API snapshot。
- `packages/app-web/src/desktop/localRuntimeBridge.ts` - Web auto-connect bridge。
- `packages/app-web/src/App.tsx` - current user 后触发 auto-connect。

### Related Specs

- `.trellis/spec/cross-layer/desktop-local-runtime.md:269-270` 把 scope 定义为 `agentdash-local-tauri` tray/menu/window lifecycle、Tauri commands、`desktop-app-settings.json`、desktop bridge 和 Desktop API host。
- `.trellis/spec/cross-layer/desktop-local-runtime.md:341-350` 规定 close-to-tray、explicit quit、tray runtime actions、settings persistence 和 autostart command status。
- `.trellis/spec/cross-layer/desktop-local-runtime.md:353-359` 当前 spec 仍说 Tauri commands normalize desktop profile/request data，但同段也说明 `DesktopRunnerHost`/`agentdash-local` 拥有 runtime reuse、serialized ensure/start、stop/restart/snapshot/logs；本任务应把 profile/request normalization 从 Tauri 收束到 local。
- `.trellis/spec/cross-layer/desktop-local-runtime.md:360-361` 规定 desktop embedded runner enrollment origin 跟随当前 Desktop Dashboard API origin，auto-connect failure 走 native snapshot/logs。
- `.trellis/spec/cross-layer/desktop-local-runtime.md:370-383` 是 settings/profile/auto-connect 关键验收矩阵，包括 settings 缺失默认、malformed 报错、runtime ensure 串行化、old profile server_url 归一到当前 Dashboard API origin。
- `.trellis/spec/cross-layer/desktop-local-runtime.md:416-417` 给出 canonical flow：authenticated web intent -> `runtime_start` -> `DesktopRunnerHost.ensure_started_with` -> snapshot；Desktop Dashboard API origin -> ensure origin -> relay credentials。
- `.trellis/spec/cross-layer/desktop-local-runtime.md:545-550` 明确 `agentdash-local::runtime_paths` 是本机 runtime 路径事实源，profile 持久化到 `local-runtime/config/local-runtime-profile.json`，profile load/save/start 必须由 `agentdash-local` machine identity 覆盖 canonical machine id。

### Current Tauri Main: Move To `agentdash-local`

DTOs currently owned by Tauri and should move:

- `RuntimeStartRequest` at `crates/agentdash-local-tauri/src/main.rs:109` - command payload shape should stay, but Rust type owner should become local, e.g. `DesktopRuntimeStartRequest`.
- `LocalRuntimeProfile` at `crates/agentdash-local-tauri/src/main.rs:126` - durable profile DTO and serde defaults belong to local.
- `DesktopAppSettings` at `crates/agentdash-local-tauri/src/main.rs:152` plus `Default` at `crates/agentdash-local-tauri/src/main.rs:161` - durable settings DTO/defaults belong to local.
- `EnsureLocalRuntimePayload` at `crates/agentdash-local-tauri/src/main.rs:425` - desktop ensure request body belongs to local desktop claim client.
- `LocalRuntimeScopePayload` at `crates/agentdash-local-tauri/src/main.rs:440` - if retained as a client payload helper, it belongs with desktop ensure in local.
- `EnsureLocalRuntimeResponse` at `crates/agentdash-local-tauri/src/main.rs:447` - desktop client response DTO and validation target belong to local. Cloud server DTO remains `crates/agentdash-api/src/dto/backend.rs:48`.

DTO-bound defaults/conversions currently in Tauri and should move with their DTOs:

- `default_profile_id` at `crates/agentdash-local-tauri/src/main.rs:179`.
- `default_executor_enabled` at `crates/agentdash-local-tauri/src/main.rs:183`.
- `default_auto_connect_local_runtime` at `crates/agentdash-local-tauri/src/main.rs:187`.
- `impl From<LocalRuntimeProfile> for RuntimeStartRequest` at `crates/agentdash-local-tauri/src/main.rs:191`; keep the semantic guarantee that persisted `access_token` is not reused (`access_token: String::new()` at `crates/agentdash-local-tauri/src/main.rs:195`).

Profile/settings file IO currently in Tauri and should move:

- `profile_load` reads `local-runtime-profile.json` directly and normalizes the parsed DTO at `crates/agentdash-local-tauri/src/main.rs:244-252`; the Tauri command should delegate to `agentdash-local`.
- `profile_save` resolves path, normalizes, creates dirs, serializes pretty JSON, and writes at `crates/agentdash-local-tauri/src/main.rs:256-264`; this is local durable IO and should move.
- `profile_delete` deletes the file at `crates/agentdash-local-tauri/src/main.rs:268-273`; should become local API.
- `desktop_settings_load_internal` reads `desktop-app-settings.json`, defaults when missing, parses JSON, normalizes at `crates/agentdash-local-tauri/src/main.rs:463-471`; move to local.
- `desktop_settings_write_internal` creates dirs and writes settings JSON at `crates/agentdash-local-tauri/src/main.rs:474-480`; move to local.
- `desktop_app_settings_path` currently derives settings path in Tauri from `local_runtime_config_dir()` at `crates/agentdash-local-tauri/src/main.rs:1730-1733`; path constant/name should move to local alongside `runtime_paths`.
- `profile_path` at `crates/agentdash-local-tauri/src/main.rs:1726` is a thin wrapper around `local_runtime_profile_path`; Tauri should not need it after delegation.

Normalization helpers currently in Tauri and should move:

- `normalize_desktop_app_settings` at `crates/agentdash-local-tauri/src/main.rs:483`.
- `normalize_profile` at `crates/agentdash-local-tauri/src/main.rs:788`; it calls `load_or_create_machine_identity`, trims optional text, clears `access_token`, normalizes `profile_id`, and fills machine fields.
- `normalize_start_request` at `crates/agentdash-local-tauri/src/main.rs:811`; it should become local start request normalization.
- `normalize_profile_id` at `crates/agentdash-local-tauri/src/main.rs:845`.
- `normalize_optional_text` at `crates/agentdash-local-tauri/src/main.rs:854`; if still needed by Tauri API origin config, either duplicate a shell-local origin helper name with narrower scope or move only desktop profile/settings uses to local. Avoid leaving a profile/settings normalization fork.
- `local_device_payload` at `crates/agentdash-local-tauri/src/main.rs:863` and `local_hostname` at `crates/agentdash-local-tauri/src/main.rs:872`; desktop claim request construction belongs to local. `machine_identity.rs` already has its own local hostname helper at `crates/agentdash-local/src/machine_identity.rs:67`.
- `normalize_server_origin` at `crates/agentdash-local-tauri/src/main.rs:831` and `normalize_server_origin_with_default` at `crates/agentdash-local-tauri/src/main.rs:835` should not remain a profile/start normalization fork in Tauri. Because the default origin currently comes from Tauri desktop API config at `crates/agentdash-local-tauri/src/main.rs:839-843`, implement local normalization as a local API that accepts the shell-provided current Desktop Dashboard API origin/default.

HTTP claim, response validation, and config projection currently in Tauri and should move:

- `start_runtime_from_request` currently normalizes request, calls `DesktopRunnerHost::ensure_started_with`, performs claim in the closure, and constructs `LocalRuntimeConfig` at `crates/agentdash-local-tauri/src/main.rs:638-659`. The Tauri orchestration shell can keep the command wrapper and host call, but the closure should call a local API that returns `LocalRuntimeConfig`.
- `claim_local_runtime` builds desktop ensure payload, retries, marks waiting-for-api via host callback, and calls `post_local_runtime_claim` at `crates/agentdash-local-tauri/src/main.rs:662-714`. Payload construction and HTTP ensure should move; retry status notification can be exposed as a callback/hook so Tauri/host can keep UI status wording.
- `validate_claim_response` checks machine id, non-empty label, user scope, default capability slot, and `registration_source == desktop_access_token` at `crates/agentdash-local-tauri/src/main.rs:716-749`; this is local desktop claim validation.
- `post_local_runtime_claim` POSTs `{server_url}/api/local-runtime/ensure`, applies bearer auth, maps 401/403, reads error text, and deserializes response at `crates/agentdash-local-tauri/src/main.rs:752-785`; this is the desktop access-token ensure client and must move.
- `LocalRuntimeConfig::new` projection from claim response and normalized request currently lives at `crates/agentdash-local-tauri/src/main.rs:650-657`; local should own this projection so future claim fields and config contracts change in one crate.

Tests currently in Tauri tied to moved behavior and should move or be rewritten in local:

- `desktop_app_settings_default_keeps_runtime_auto_connect_enabled` at `crates/agentdash-local-tauri/src/main.rs:1856`.
- `normalize_desktop_app_settings_preserves_explicit_choices` at `crates/agentdash-local-tauri/src/main.rs:1865`.
- `runtime_start_request_from_profile_does_not_reuse_persisted_access_token` at `crates/agentdash-local-tauri/src/main.rs:1878`.
- `normalize_server_origin_uses_desktop_dashboard_origin_for_development_default` at `crates/agentdash-local-tauri/src/main.rs:1897`.
- `normalize_server_origin_uses_desktop_dashboard_origin_for_old_profile_origin` at `crates/agentdash-local-tauri/src/main.rs:1908`.

### Current Tauri Main: Keep In Tauri Shell

State/lifecycle/window/tray responsibilities that should stay in `agentdash-local-tauri`:

- `DesktopState` holds `DesktopRunnerHost`, `DesktopApiManager`, and lifecycle flag at `crates/agentdash-local-tauri/src/main.rs:41-45`.
- `DesktopLifecycleState` and explicit quit flag helpers at `crates/agentdash-local-tauri/src/main.rs:57-69`.
- `DesktopApiManager`, `DesktopApiSnapshot`, `DesktopApiState` at `crates/agentdash-local-tauri/src/main.rs:72-105` and manager methods at `crates/agentdash-local-tauri/src/main.rs:1413-1527`; this is Desktop API host/sidecar shell state.
- `DesktopAutostartStatus` at `crates/agentdash-local-tauri/src/main.rs:173` can stay as OS adapter command response. It is not a durable local runtime fact.
- `desktop_autostart_is_enabled` and `desktop_autostart_set_enabled` command wrappers at `crates/agentdash-local-tauri/src/main.rs:221-231`, but their settings load/write calls should use local APIs after OS mutation.
- `desktop_quit_request` at `crates/agentdash-local-tauri/src/main.rs:235-240`.
- `runtime_stop`, `runtime_restart`, `runtime_snapshot` at `crates/agentdash-local-tauri/src/main.rs:287-308`; these are host lifecycle command adapters over `DesktopRunnerHost`.
- Tauri command registration in `.invoke_handler(...)` at `crates/agentdash-local-tauri/src/main.rs:1001-1021`.
- Single-instance, close-to-tray, setup, exit guard in `main` at `crates/agentdash-local-tauri/src/main.rs:945-1035`.
- `configure_tray` and `handle_tray_menu_event` at `crates/agentdash-local-tauri/src/main.rs:1038-1112`.
- `start_runtime_from_profile` at `crates/agentdash-local-tauri/src/main.rs:1114-1163` can stay as tray shell orchestration, but profile load and profile->start request conversion should call local APIs/types.
- `record_tray_status`, `request_desktop_quit`, `apply_startup_window_visibility`, and `restore_main_window` at `crates/agentdash-local-tauri/src/main.rs:1165-1248`; settings reads in window visibility should delegate to local.
- Desktop API builtin/sidecar lifecycle: `start_desktop_api`, `run_desktop_api`, `start_desktop_api_sidecar`, `spawn_desktop_api_sidecar`, `wait_for_sidecar_api_ready` at `crates/agentdash-local-tauri/src/main.rs:1250-1411`.
- Desktop API config/env/origin validation at `crates/agentdash-local-tauri/src/main.rs:1529-1724` should stay Tauri shell, because it binds packaged Dashboard API origin and sidecar process behavior. Its current origin may be passed into local normalization as an explicit value.
- Windows autostart internals stay shell/OS adapter: `desktop_autostart_is_enabled_internal` at `crates/agentdash-local-tauri/src/main.rs:491`, `desktop_autostart_set_enabled_internal` at `crates/agentdash-local-tauri/src/main.rs:523`, `current_app_exe_path` at `crates/agentdash-local-tauri/src/main.rs:556`, `build_windows_autostart_command` at `crates/agentdash-local-tauri/src/main.rs:560`, `is_setup_exe_name` at `crates/agentdash-local-tauri/src/main.rs:580`, and registry helpers at `crates/agentdash-local-tauri/src/main.rs:587-635`.
- `open_external_url`, `desktop_browse_directory`, `logs_tail`, `logs_clear`, MCP command wrappers are Tauri command adapters; they are outside this D11 cleanup unless static search shows they are entangled with moved profile/settings/claim facts.

### Runtime Start And Auto-Connect Current Calling Chain

Manual / command path:

1. Frontend `packages/app-tauri/src/runtimeApi.ts:51-53` calls `invoke('runtime_start', { request })`.
2. Tauri `runtime_start` receives `RuntimeStartRequest` at `crates/agentdash-local-tauri/src/main.rs:277-283`.
3. `runtime_start` calls `start_runtime_from_request(state.inner(), request, false)` at `crates/agentdash-local-tauri/src/main.rs:281`.
4. `start_runtime_from_request` normalizes request at `crates/agentdash-local-tauri/src/main.rs:643`.
5. It enters `state.runtime.ensure_started_with(...)` at `crates/agentdash-local-tauri/src/main.rs:645-648`.
6. The closure calls `claim_local_runtime(...)` at `crates/agentdash-local-tauri/src/main.rs:648-649`.
7. `claim_local_runtime` builds desktop ensure payload at `crates/agentdash-local-tauri/src/main.rs:667-682`, loops/retries at `crates/agentdash-local-tauri/src/main.rs:684-714`, and calls HTTP client at `crates/agentdash-local-tauri/src/main.rs:687`.
8. `post_local_runtime_claim` POSTs `/api/local-runtime/ensure` at `crates/agentdash-local-tauri/src/main.rs:757`, applies bearer auth at `crates/agentdash-local-tauri/src/main.rs:760-763`, maps errors at `crates/agentdash-local-tauri/src/main.rs:771-779`, and deserializes at `crates/agentdash-local-tauri/src/main.rs:782-785`.
9. `validate_claim_response` validates returned facts at `crates/agentdash-local-tauri/src/main.rs:716-749`.
10. `start_runtime_from_request` projects `EnsureLocalRuntimeResponse + RuntimeStartRequest` into `LocalRuntimeConfig::new(...)` at `crates/agentdash-local-tauri/src/main.rs:650-657`.
11. `DesktopRunnerHost::ensure_started_with` serializes and starts local runtime in local crate at `crates/agentdash-local/src/desktop_runner_host.rs:43-109`.

Native startup path:

1. Tauri setup calls `initialize_desktop_runner_host(state)` at `crates/agentdash-local-tauri/src/main.rs:997`.
2. `initialize_desktop_runner_host` loads settings through Tauri internal IO at `crates/agentdash-local-tauri/src/main.rs:880-891`.
3. If `auto_connect_local_runtime` is false, it marks disabled at `crates/agentdash-local-tauri/src/main.rs:893-899`.
4. It calls `profile_load().await` at `crates/agentdash-local-tauri/src/main.rs:901`; missing profile marks idle at `crates/agentdash-local-tauri/src/main.rs:903-908`.
5. If profile `auto_start` is false, it marks idle at `crates/agentdash-local-tauri/src/main.rs:919-925`.
6. If profile auto-start is true, native side marks waiting for Web bridge auth at `crates/agentdash-local-tauri/src/main.rs:927-930`. It does not claim without Web/auth intent.

Web auto-connect path:

1. `packages/app-tauri/src/App.tsx:29-33` installs `window.__AGENTDASH_DESKTOP_LOCAL_RUNTIME__` and `window.__AGENTDASH_DESKTOP_APP__`.
2. `packages/app-web/src/App.tsx:215-223` calls `ensureDesktopLocalRuntimeStarted(getStoredToken() ?? "", { currentUserAvailable: true })` after current user exists.
3. `ensureDesktopLocalRuntimeStarted` gates current-user availability, in-flight reuse, and retry state at `packages/app-web/src/desktop/localRuntimeBridge.ts:61-104`.
4. `runDesktopLocalRuntimeAutoConnect` loads settings at `packages/app-web/src/desktop/localRuntimeBridge.ts:107-115`, skips if auto-connect disabled, checks runtime snapshot at `packages/app-web/src/desktop/localRuntimeBridge.ts:117-118`, and loads/creates profile at `packages/app-web/src/desktop/localRuntimeBridge.ts:120-122`.
5. It calls `client.runtimeStart({ ...profile, access_token: token, server_url: resolveDesktopServerUrl() })` at `packages/app-web/src/desktop/localRuntimeBridge.ts:124-128`.
6. `loadOrCreateAutoConnectProfile` loads current profile and saves normalized profile with empty `access_token` and current desktop server URL at `packages/app-web/src/desktop/localRuntimeBridge.ts:156-168`; if missing, it creates a default profile with empty machine fields, `executor_enabled: true`, `auto_start: true`, and null backend/relay fields at `packages/app-web/src/desktop/localRuntimeBridge.ts:171-183`.
7. That flows back into the same Tauri `runtime_start` chain above.

Tray path:

1. Tray menu `TRAY_MENU_RUNTIME_START` calls `start_runtime_from_profile(state)` at `crates/agentdash-local-tauri/src/main.rs:1080-1083`.
2. `start_runtime_from_profile` loads saved profile at `crates/agentdash-local-tauri/src/main.rs:1115`, converts it into start request at `crates/agentdash-local-tauri/src/main.rs:1141`, and calls the same `start_runtime_from_request`.
3. Current conversion clears persisted access token at `crates/agentdash-local-tauri/src/main.rs:191-202`.

### TS/Tauri Command Payload Stability

Payload shape can be kept unchanged. Evidence:

- TS canonical interfaces are snake_case: `DesktopRuntimeSettings` has `launch_at_login`, `start_minimized_to_tray`, `auto_connect_local_runtime` at `packages/core/src/local-runtime/index.ts:118-122`.
- `RuntimeStartRequest` has `server_url`, `access_token`, `profile_id`, `machine_id`, `machine_label`, `name`, `workspace_roots`, `executor_enabled` at `packages/core/src/local-runtime/index.ts:197-206`.
- `LocalRuntimeProfile` extends `RuntimeStartRequest` with `auto_start`, `backend_id`, `relay_ws_url` at `packages/core/src/local-runtime/index.ts:208-212`.
- `LocalRuntimeClient` command port is stable at `packages/core/src/local-runtime/index.ts:240-253`.
- `packages/app-tauri/src/runtimeApi.ts:31-54` only invokes command names with `{ profile }` or `{ request }`; it does not depend on Rust type names.
- `packages/app-tauri/src/desktopSettings.ts:31-39` invokes `desktop_settings_load` and `desktop_settings_save` with `{ settings }`; it does not depend on Rust type names.
- `packages/app-web/src/desktop/localRuntimeBridge.ts:124-128` starts runtime by spreading profile and overriding `access_token`/`server_url`; same fields can deserialize into moved local Rust DTOs if serde `rename_all = "snake_case"` remains.
- Tests already assert token/server_url payload semantics in `packages/app-web/src/desktop/localRuntimeBridge.test.ts:126-142` and current-origin override at `packages/app-web/src/desktop/localRuntimeBridge.test.ts:153-169`.

Conclusion: Move/rename Rust DTOs inside crates without changing Tauri command names or JS object fields. If implementers choose local names like `DesktopRuntimeStartRequest`, the command can still accept that type as `request` because Tauri payload binding keys are command parameter names, not Rust struct names.

### Cloud/API Boundary Notes

- Cloud server request/response DTOs live at `crates/agentdash-api/src/dto/backend.rs:24-65`; do not move those into `agentdash-local`.
- Cloud ensure route maps `CurrentUser + EnsureLocalRuntimeRequest` into application input and returns server-owned fields at `crates/agentdash-api/src/routes/backends.rs:348-393`.
- Application layer records desktop enrollment source via `EnrollmentSource::DesktopAccessToken` and unified local backend enrollment at `crates/agentdash-application/src/backend/management.rs:195-207` and `crates/agentdash-application/src/backend/management.rs:316-342`.
- Local desktop claim module should consume the server DTO shape or define a matching client DTO in `agentdash-local`, but it should not introduce a second Tauri implementation.

### Existing Local Patterns To Reuse

- `agentdash-local/src/lib.rs:1-3` states CLI and future Tauri desktop should use local library as本机能力入口; binary/shell only parses args and starts host.
- `agentdash-local/src/lib.rs:16-17` exposes `runner_claim` and `runner_config` as public modules; desktop modules can follow this export style.
- `agentdash-local/src/runtime.rs:29-57` owns `LocalRuntimeConfig::new` and canonicalizes workspace roots.
- `agentdash-local/src/runtime_paths.rs:45-58` already owns `local_runtime_config_dir`, `local_runtime_profile_path`, `local_mcp_servers_path`, and `machine_identity_path`.
- `agentdash-local/src/machine_identity.rs:14-34` owns machine identity load/create, and `agentdash-local/src/machine_identity.rs:48-65` owns identity normalization.
- `agentdash-local/src/runner_claim.rs:51-120` is a useful HTTP/error mapping pattern; keep desktop ensure distinct because it uses `/api/local-runtime/ensure` and user access-token auth, not `/api/local-runtime/runner/claim` and registration token.
- `agentdash-local/src/runner_config.rs:184-205` shows a local config-to-`LocalRuntimeConfig` projection pattern.
- `agentdash-local/src/runner_config.rs:382-390` and `agentdash-local/src/runner_config.rs:417-440` show local file read and atomic write patterns. Desktop JSON can be simpler or use atomic write, but owner should be local.
- `agentdash-local/src/desktop_runner_host.rs:1-3` already documents the desired boundary: Tauri handles desktop lifecycle and command bridge; runner start/reuse/stop/logs are local.

### Suggested Implementation Order

1. Add `crates/agentdash-local/src/desktop_profile.rs` with moved `LocalRuntimeProfile`, `DesktopRuntimeStartRequest` or equivalent, profile load/save/delete, profile path usage, `From<Profile> for StartRequest`, and normalization. Export it from `crates/agentdash-local/src/lib.rs`.
2. Add `crates/agentdash-local/src/desktop_settings.rs` with moved `DesktopAppSettings`, default/normalization, settings path, load/save. Export it.
3. Thin Tauri `profile_*`, `desktop_settings_*`, `desktop_autostart_set_enabled`, `initialize_desktop_runner_host`, and `apply_startup_window_visibility` to use local profile/settings APIs. Keep autostart OS mutation in Tauri.
4. Add `crates/agentdash-local/src/desktop_claim.rs` with `DesktopEnsureLocalRuntimePayload`, response DTO, HTTP POST to `/api/local-runtime/ensure`, validation, and error mapping. Keep endpoint/auth separate from `runner_claim`.
5. Add a local API such as `build_desktop_runtime_config(request, current_server_origin, retry/status hook)` that normalizes the start request, runs desktop ensure, validates response, and returns `LocalRuntimeConfig`.
6. Replace Tauri `start_runtime_from_request` closure with a call to the local start-config/claim API, passing shell-provided current Desktop Dashboard API origin and retry notification hook. Remove Tauri-local claim DTOs/helpers afterward.
7. Move or rewrite moved behavior tests into `agentdash-local`; leave Tauri tests only for shell behavior such as autostart command formation, Desktop API config, tray/window/lifecycle.
8. Run static search after implementation for removed Tauri forks: `RuntimeStartRequest`, `LocalRuntimeProfile`, `DesktopAppSettings`, `EnsureLocalRuntimePayload`, `EnsureLocalRuntimeResponse`, `post_local_runtime_claim`, `validate_claim_response`, `desktop_settings_write_internal`, `profile_path(` in `crates/agentdash-local-tauri/src/main.rs`.

### Minimum Non-Overlapping File Range

Primary implement scope:

- `crates/agentdash-local/src/lib.rs` - add exports only.
- `crates/agentdash-local/src/desktop_profile.rs` - new owner for profile DTO/IO/normalization/start-request conversion.
- `crates/agentdash-local/src/desktop_settings.rs` - new owner for settings DTO/defaults/IO/normalization.
- `crates/agentdash-local/src/desktop_claim.rs` - new owner for desktop ensure request/response/client/validation/config projection.
- `crates/agentdash-local-tauri/src/main.rs` - remove local DTO/IO/claim forks and keep shell adapter.

Likely test-only scope:

- Unit tests colocated in the three new `agentdash-local` modules.
- Existing Tauri main tests should be reduced to shell/API/autostart/origin behavior; moved profile/settings/claim tests should not remain duplicated in Tauri.

Avoid touching unless compile errors force it:

- `packages/core/src/local-runtime/index.ts`
- `packages/app-tauri/src/runtimeApi.ts`
- `packages/app-tauri/src/desktopSettings.ts`
- `packages/app-web/src/desktop/localRuntimeBridge.ts`
- `crates/agentdash-api/**`
- `crates/agentdash-application/**`

### External References / Versions

- No external web lookup was needed; this is an internal ownership review.
- Existing crate dependency context: `agentdash-local` already has `reqwest = 0.13.2` with rustls at `crates/agentdash-local/Cargo.toml:38`, plus workspace `serde`, `serde_json`, `chrono`, `anyhow`, and `thiserror` at `crates/agentdash-local/Cargo.toml:40-45`.
- `agentdash-local-tauri` currently also depends on `reqwest = 0.13.2` with rustls/json at `crates/agentdash-local-tauri/Cargo.toml:23` and Tauri 2 at `crates/agentdash-local-tauri/Cargo.toml:26`; after moving desktop ensure, reevaluate whether Tauri still needs `reqwest` for Desktop API health polling/sidecar readiness before removing it.

## Caveats / Not Found

- Trellis active task was not set (`task.py current --source` returned none). This research used the explicit task path from the user request and wrote only under that task's `research/` directory.
- I did not run Rust compilation, `cargo check`, broad tests, or git commands per subagent constraints.
- I did not inspect every settings UI consumer. Static search found command payload ownership through `packages/core`, `packages/app-tauri`, `packages/app-web/src/desktop/localRuntimeBridge.ts`, and tests; no evidence requires TS payload churn.
- `normalize_optional_text` and `normalize_origin` are shared-looking helpers in Tauri. Implementation should avoid deleting shell API-origin normalization accidentally while removing profile/settings/start-request normalization forks.
- If any Tauri-local profile/settings/claim implementation remains active after adding local APIs, this task should be treated as having a significant residual, not complete.
