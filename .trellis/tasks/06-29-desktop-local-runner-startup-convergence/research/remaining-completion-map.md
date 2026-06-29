# Research: remaining-completion-map

- Query: settings/profile UI、desktop/native supervisor state、auto_connect_local_runtime vs auto_start、desktop:bundle 与 Windows 验收入口如何落地
- Scope: mixed
- Date: 2026-06-29

## Findings

### Task Context

- Active task was not registered in `task.py current --source` (`Current task: (none)`), but the dispatch prompt explicitly supplied `.trellis/tasks/06-29-desktop-local-runner-startup-convergence` and this exact research output path.
- `.trellis/tasks/06-29-desktop-local-runner-startup-convergence/prd.md` requires native/Tauri lifecycle ownership for desktop local runner startup, single-instance protection, retryable auto-start, and settings/diagnostics UI that can explain unconfigured, waiting login, claiming, relay connecting, running, failed, and stopping states.
- `.trellis/tasks/06-29-desktop-local-runner-startup-convergence/design.md` selects embedded local-owned runner host for the first implementation, not sidecar or Windows Service.
- `.trellis/tasks/06-29-desktop-local-runner-startup-convergence/implement.md` shows Phase 1 single-instance and Phase 2/3 basic host/bridge work mostly complete, with remaining unchecked items in Phase 4 settings/profile semantics, Phase 5 diagnostics/UI, and Phase 6 release validation.

### Related Specs

- `.trellis/workflow.md`: research output must be persisted under task `research/`, and implementation/check sub-agents read `prd.md`, `design.md`, `implement.md`, and context manifests.
- `.trellis/spec/guides/cross-layer-thinking-guide.md`: frontend must not infer state from indirect facts; cross-layer state must come from the authoritative runtime surface.
- `.trellis/spec/cross-layer/desktop-local-runtime.md`: release desktop bundle defaults to `external` API mode because business/login/enrollment authority belongs to cloud server; `external` only controls Dashboard API origin and must not imply the desktop package lacks embedded local execution.
- `.trellis/spec/frontend/index.md`, `.trellis/spec/frontend/component-guidelines.md`, `.trellis/spec/frontend/state-management.md`, `.trellis/spec/frontend/quality-guidelines.md`: UI changes should keep typed DTO shape, avoid duplicate state facts, and add focused tests around feature model/mapper behavior.
- `.trellis/tasks/06-26-local-runtime-distribution/implement.md`: desktop validation includes `pnpm run desktop:check`, `pnpm run desktop:bundle`, Windows install/uninstall, background/tray, login autostart, service/runtime lifecycle, and token redaction.
- `.trellis/tasks/06-27-local-backend-enrollment-convergence/design.md`: Desktop enrollment uses access token through `/api/local-runtime/ensure`; standalone runner uses registration token through `/api/local-runtime/runner/claim`; shared core fields include `registration_source` and `claimed_at`.

### Files Found

- `packages/app-web/src/features/settings/ui/SettingsPageContent.tsx`: settings route shell; conditionally includes the desktop-only local runtime panel and passes cloud/backend diagnostics plus desktop API snapshot into `LocalRuntimeView`.
- `packages/views/src/local-runtime/LocalRuntimeView.tsx`: actual settings/profile/local runtime UI; renders runtime diagnostics, desktop settings toggles, profile form, runtime start/stop/restart controls, MCP config, and logs.
- `packages/core/src/local-runtime/index.ts`: shared frontend types for `LocalRuntimeStatus`, `DesktopRuntimeSettings`, `LocalRuntimeProfile`, diagnostics snapshot, relay state, and local runtime client port.
- `packages/app-web/src/desktop/localRuntimeBridge.ts`: web-side desktop auto-connect bridge; currently still owns token-triggered auto-connect/retry and calls `runtimeStart`.
- `packages/app-tauri/src/runtimeApi.ts`: Tauri adapter for `LocalRuntimeClient`; maps `runtimeStart`, `runtimeSnapshot`, profile, logs, MCP calls to Tauri `invoke()` commands.
- `packages/app-tauri/src/desktopSettings.ts`: Tauri adapter for desktop settings/autostart/API snapshot bridge.
- `crates/agentdash-local-tauri/src/main.rs`: Tauri desktop lifecycle root, settings/profile commands, `runtime_start`, tray, single-instance plugin, Desktop API mode, profile normalization, claim call, and tests.
- `crates/agentdash-local/src/desktop_runner_host.rs`: embedded desktop runner host wrapper around `LocalRuntimeManager`; currently prevents duplicate start and exposes start/stop/restart/snapshot/log methods.
- `crates/agentdash-local/src/runtime.rs`: local runtime manager, current `LocalRuntimeState`, runtime snapshot, relay status projection, logging redaction, and runtime lifecycle tests.
- `crates/agentdash-local/src/ws_client.rs`: relay connection status includes `last_error`, `retry_count`, and `next_retry_at` for relay reconnect diagnostics.
- `packages/app-web/src/features/settings/model/runtimeDiagnostics.ts`: maps backend/runtime summary store facts into runtime diagnostics facts.
- `packages/app-web/src/features/settings/model/runtimeDiagnostics.test.ts`: existing diagnostics tests for registration source, runner read-only layer, no relay inference from online, and redaction.
- `package.json`: scripts for `desktop:check`, `desktop:build`, and `desktop:bundle`.
- `scripts/desktop-build.js`: desktop build entrypoint; default API mode is `external`.
- `scripts/lib/desktop-build.js`: real desktop build implementation; parses args/env/defaults, sets Tauri build env, runs `pnpm exec tauri build`, and prints setup/app exe artifact boundaries.
- `docs/desktop-dev.md`: current desktop dev/build docs, including `pnpm dev:desktop`, `desktop:check`, `desktop:bundle`, and expected NSIS path.

### Current UI Placement

- Settings route imports `LocalRuntimeView` from `@agentdash/views/local-runtime` at `packages/app-web/src/features/settings/ui/SettingsPageContent.tsx:10`.
- `DesktopLocalRuntimePanel` is defined in `packages/app-web/src/features/settings/ui/SettingsPageContent.tsx:65`; it obtains `getDesktopLocalRuntimeClient()`, `getDesktopBrowseDirectory()`, and `getDesktopAppBridge()` at lines 78-80.
- The settings panel polls `desktopApp.getDesktopApiSnapshot()` every 1500ms at `packages/app-web/src/features/settings/ui/SettingsPageContent.tsx:84-97`.
- `LocalRuntimeView` is rendered at `packages/app-web/src/features/settings/ui/SettingsPageContent.tsx:119-136`; it receives cloud API diagnostics, desktop API snapshot, backend facts, runtime summaries, default access token, and default server URL.
- Local runtime tab inclusion is gated by `!!getDesktopLocalRuntimeClient()` at `packages/app-web/src/features/settings/ui/SettingsPageContent.tsx:197`; the tab renders at lines 307-315.
- The actual view state for runtime/profile/settings lives in `packages/views/src/local-runtime/LocalRuntimeView.tsx:67-105`.
- Runtime snapshot and logs are polled every 1500ms through `client.runtimeSnapshot()` and `client.logsTail()` in `packages/views/src/local-runtime/LocalRuntimeView.tsx:106-135`.
- Diagnostics overview is computed with `createRuntimeDiagnosticsSnapshot(...)` in `packages/views/src/local-runtime/LocalRuntimeView.tsx:175-189` and rendered first at lines 391-392.
- Current runtime status card displays only `snapshot.state` with `stateText()` labels in `packages/views/src/local-runtime/LocalRuntimeView.tsx:387-425`.
- Desktop settings card shows three toggles, including `auto_connect_local_runtime`, in `packages/views/src/local-runtime/LocalRuntimeView.tsx:427-474`; the toggle itself is at lines 460-464.
- Profile form applies and saves `LocalRuntimeProfile.auto_start` at `packages/views/src/local-runtime/LocalRuntimeView.tsx:226-250`; the state field is declared at line 86.
- Runtime manual start currently uses `client.runtimeStart(request)` in `packages/views/src/local-runtime/LocalRuntimeView.tsx:192-207`; this is the natural UI entry for manual retry if kept as the shared native ensure command.
- Diagnostics overview currently distinguishes Cloud API, Desktop API, Local Runtime, Runner, and Relay at `packages/views/src/local-runtime/LocalRuntimeView.tsx:756-860`, but it has no first-class desktop supervisor layer.

### Current Native/Runtime State Surface

- Frontend type `LocalRuntimeState` is only `'starting' | 'running' | 'stopping' | 'stopped' | 'error'` at `packages/core/src/local-runtime/index.ts:1`.
- Frontend `LocalRuntimeStatus` has `message`, optional `relay_connection`, and optional `registration` fields at `packages/core/src/local-runtime/index.ts:3-13`; `registration` is not currently emitted by the Rust `LocalRuntimeStatus` struct.
- Frontend `RelayConnectionStatus` already models `last_error`, `retry_count`, and `next_retry_at` at `packages/core/src/local-runtime/index.ts:70-79`.
- Rust `LocalRuntimeState` has only `Starting`, `Running`, `Stopping`, `Stopped`, and `Error` at `crates/agentdash-local/src/runtime.rs:60-69`.
- Rust `LocalRuntimeStatus` includes `state`, `backend_id`, `name`, `workspace_roots`, `executor_enabled`, `mcp_server_count`, `message`, and `relay_connection` at `crates/agentdash-local/src/runtime.rs:71-83`; it does not carry supervisor owner, auth wait, API wait, last attempt, next retry, or registration.
- `LocalRuntimeManager::start` initializes relay status as `not_configured`, publishes `Starting`, then immediately publishes `Running` after building ws config before relay registration is complete at `crates/agentdash-local/src/runtime.rs:167-207`.
- Relay reconnect state is projected separately through `status_with_relay()` at `crates/agentdash-local/src/runtime.rs:185-199` and `crates/agentdash-local/src/runtime.rs:720-747`.
- `DesktopRunnerHost` currently wraps `LocalRuntimeManager` and deduplicates start calls through `ensure_lock`; it returns existing `Starting`/`Running` snapshots and stops stale `Stopped`/`Error` snapshots before restarting at `crates/agentdash-local/src/desktop_runner_host.rs:33-76`.
- `DesktopRunnerHost` has no separate supervisor state struct, no `last_error`, no `last_attempt_at`, no `next_retry_at`, and no auth/API wait state; snapshot simply delegates to `LocalRuntimeManager::snapshot()` at `crates/agentdash-local/src/desktop_runner_host.rs:86-88`.

### Current Tauri Bridge And Startup Behavior

- `DesktopState` holds `runtime: DesktopRunnerHost`, `api: DesktopApiManager`, and lifecycle state at `crates/agentdash-local-tauri/src/main.rs:40-55`.
- `runtime_start` Tauri command calls `start_runtime_from_request(..., false)` at `crates/agentdash-local-tauri/src/main.rs:276-284`.
- `runtime_snapshot` returns `state.runtime.snapshot().await` at `crates/agentdash-local-tauri/src/main.rs:304-309`.
- `start_runtime_from_request` normalizes request, calls `state.runtime.ensure_started_with`, claims `/api/local-runtime/ensure`, and builds `LocalRuntimeConfig` at `crates/agentdash-local-tauri/src/main.rs:634-654`.
- `claim_local_runtime` has an optional `retry_until_server_ready` loop, but all current call sites pass `false` except future callers could opt in; retry loop is fixed 30 attempts at one second each at `crates/agentdash-local-tauri/src/main.rs:656-697`.
- `/api/local-runtime/ensure` is called in `post_local_runtime_claim` with bearer auth when access token is present at `crates/agentdash-local-tauri/src/main.rs:729-757`.
- Single-instance plugin is already installed at `crates/agentdash-local-tauri/src/main.rs:858-861`, with `tauri-plugin-single-instance = "2.4.2"` in `crates/agentdash-local-tauri/Cargo.toml:25-26`.
- Tauri setup initializes tray and Desktop API mode, then applies startup window visibility at `crates/agentdash-local-tauri/src/main.rs:880-909`; it does not start a native auto-connect supervisor.
- Tray “启动本机 runtime” calls `start_runtime_from_profile(state)` at `crates/agentdash-local-tauri/src/main.rs:987-993`.
- `start_runtime_from_profile` loads profile and calls `start_runtime_from_request(..., false)` at `crates/agentdash-local-tauri/src/main.rs:1024-1051`; it does not check `auto_connect_local_runtime`, and it uses profile access token already persisted in profile.
- Explicit quit stops runtime and sidecar before `app.exit(0)` at `crates/agentdash-local-tauri/src/main.rs:1099-1107`.
- `apply_startup_window_visibility` only reads `start_minimized_to_tray` at `crates/agentdash-local-tauri/src/main.rs:1110-1134`.
- Desktop API `external` mode sets desktop API snapshot to `Running` with remote origin at `crates/agentdash-local-tauri/src/main.rs:1443-1452`; default mode resolution falls back to `External` at lines 1542-1544.

### Current Web Auto-Connect Behavior

- `packages/app-web/src/App.tsx:217-218` calls `ensureDesktopLocalRuntimeStarted(getStoredToken() ?? "")` when the desktop client exists and current user is ready.
- `packages/app-web/src/desktop/localRuntimeBridge.ts` still owns auto-connect attempts, retry timer, and max attempts at lines 33-40.
- `ensureDesktopLocalRuntimeStarted` resets on missing or changed token and deduplicates in-flight attempts at `packages/app-web/src/desktop/localRuntimeBridge.ts:57-92`.
- `runDesktopLocalRuntimeAutoConnect` loads desktop settings, checks `settings.auto_connect_local_runtime`, loads/creates profile, then calls `client.runtimeStart(...)` at `packages/app-web/src/desktop/localRuntimeBridge.ts:94-119`.
- Retry is web-page-local only: `scheduleDesktopRuntimeAutoConnectRetry()` retries every 2s up to 8 attempts at `packages/app-web/src/desktop/localRuntimeBridge.ts:121-131`.
- This has improved from the PRD’s original “one failure permanently skipped” fact, but it remains a Web app lifecycle retry rather than native supervisor retry and cannot represent waiting-for-auth/API as native state.

### `auto_connect_local_runtime` vs `auto_start` Distribution And Conflict Points

- `DesktopRuntimeSettings.auto_connect_local_runtime` is defined in shared frontend type at `packages/core/src/local-runtime/index.ts:95-99`.
- Tauri `DesktopAppSettings` carries `auto_connect_local_runtime` with default `true` at `crates/agentdash-local-tauri/src/main.rs:150-159`; default is implemented at lines 161-189.
- Desktop settings are loaded/saved to `desktop-app-settings.json` via `desktop_settings_load_internal` and `desktop_settings_write_internal` at `crates/agentdash-local-tauri/src/main.rs:459-477`; normalization preserves explicit `auto_connect_local_runtime` at lines 479-485.
- `LocalRuntimeProfile.auto_start` is defined in shared frontend type at `packages/core/src/local-runtime/index.ts:185-189`.
- Tauri `LocalRuntimeProfile` carries `auto_start` at `crates/agentdash-local-tauri/src/main.rs:124-148`; `normalize_profile` preserves it at `crates/agentdash-local-tauri/src/main.rs:760-780`.
- `LocalRuntimeView` loads profile `auto_start` into UI state at `packages/views/src/local-runtime/LocalRuntimeView.tsx:226-235` and writes it back at lines 238-250.
- Web auto-connect preserves existing profile `auto_start` but creates new profiles with `auto_start: false` at `packages/app-web/src/desktop/localRuntimeBridge.ts:139-170`.
- No current native call site reads `profile.auto_start` to decide startup. `rg` found it only in type/profile load/save/UI code and in the profile conversion path.
- Semantic conflict: `auto_connect_local_runtime` currently means “web page should auto-call runtimeStart after login,” while `profile.auto_start` is user-editable but inert in native startup. This violates PRD R5 because two user-visible toggles imply overlapping startup decisions but only one has behavior.
- Recommended semantic convergence:
  - Treat `DesktopAppSettings.auto_connect_local_runtime` as the desktop-wide gate for native supervisor auto-ensure after login/API readiness.
  - Treat `LocalRuntimeProfile.auto_start` as a legacy/profile-level startup preference that should either be removed from the desktop settings UI or mapped into the same gate during profile save/load. Because this project is pre-release and asks for correct state over compatibility, the cleanest implementation is to stop exposing a separate `auto_start` decision in the desktop UI and keep profile fields limited to launch configuration facts (`server_url`, roots, executor, labels, backend claim cache).
  - If the field must remain in the serialized profile for current code shape, make native `should_auto_connect(settings, profile)` explicit and tested. Suggested rule: auto-start only when `settings.auto_connect_local_runtime && profile.auto_start` if profile exists; when no profile exists, `auto_connect_local_runtime` allows creating/using default profile after auth. This rule is less clean because it gives two toggles veto power and needs very clear UI wording.
  - Avoid maintaining both as independent positive triggers. `settings=false, profile=true` must not auto-start; `settings=true, profile=false` must have a documented, tested behavior.

### Suggested Modification Range

- `crates/agentdash-local/src/desktop_runner_host.rs`
  - Add a first-class desktop supervisor snapshot, separate from `LocalRuntimeStatus`, or extend the host to maintain supervisor metadata around `ensure_started_with`.
  - Track state values required by design: `idle`, `disabled`, `waiting_for_auth`, `waiting_for_api`, `claiming`, `starting`, `running`, `retrying`, `error`, `stopping`, `stopped`.
  - Track `owner = desktop_embedded_runner`, `registration_source = desktop_access_token`, `last_error`, `last_attempt_at`, `next_retry_at`, and retry attempt count.
  - Keep `LocalRuntimeManager` as the relay/runtime owner; do not duplicate relay connection logic.
- `crates/agentdash-local/src/runtime.rs`
  - If choosing to extend existing `LocalRuntimeStatus`, update `LocalRuntimeState` and `LocalRuntimeStatus` here. Risk: this type is also the runtime manager’s state, so supervisor states like `waiting_for_auth` may not belong here.
  - Safer shape: keep runtime manager states narrow and add `DesktopRunnerSupervisorSnapshot` next to `DesktopRunnerHost`, with optional nested `local_runtime: LocalRuntimeStatus`.
- `crates/agentdash-local-tauri/src/main.rs`
  - Add Tauri commands such as `desktop_runner_snapshot`, `desktop_runner_auth_update`/`desktop_runner_record_auth_state`, and `desktop_runner_ensure` or adapt `runtime_snapshot`/`runtime_start` if the team wants one port.
  - In `.setup(...)`, initialize/schedule native supervisor auto-ensure after Desktop API snapshot is ready enough for configured mode. For `external`, the API snapshot is already `Running`, so it must not block embedded runner startup.
  - Move web auto-connect responsibility into native: Web should pass token availability/current auth state to Tauri; native decides waiting/retry/claim/start based on settings/profile/API snapshot.
  - Make tray start, settings manual start/retry, and automatic startup call the same host method.
  - Add a tested helper for `auto_connect_local_runtime` / `profile.auto_start` convergence.
- `packages/core/src/local-runtime/index.ts`
  - Add typed `DesktopRunnerSupervisorState` and `DesktopRunnerSupervisorSnapshot` if using a new snapshot.
  - Extend `DesktopRuntimeSettingsClient` or add `DesktopRunnerClient` methods for supervisor snapshot/auth/ensure/manual retry.
  - Include `last_error`, `last_attempt_at`, `next_retry_at`, retry count, owner, registration source, and nested relay/runtime facts.
- `packages/app-tauri/src/runtimeApi.ts` and/or `packages/app-tauri/src/desktopSettings.ts`
  - Wire new Tauri commands into a frontend client. The current `DesktopAppBridge` only exposes settings, autostart, API snapshot, and quit.
- `packages/app-web/src/desktop/localRuntimeBridge.ts`
  - Remove or demote Web-owned retry state after native supervisor exists.
  - Keep it as auth/defaults notifier only: when token/current user changes, call native auth update/ensure command; do not independently own max attempts/backoff.
- `packages/app-web/src/App.tsx`
  - Replace `ensureDesktopLocalRuntimeStarted(getStoredToken() ?? "")` with the new native auth/ensure bridge call.
- `packages/views/src/local-runtime/LocalRuntimeView.tsx`
  - Display supervisor state near the top of the diagnostics/status area: state label, last error, last attempt, next retry, retry count, and owner/source.
  - Add manual retry button that calls the same native ensure path as tray/auto startup. Existing “刷新状态” and “重启” are not equivalent to retrying claim/auth/API wait.
  - Decide how to present `auto_start`. Preferred: remove the separate profile auto-start control from user-facing UI or label it as profile-backed part of the same desktop auto-connect preference after native semantics are explicit.
  - Continue showing relay `last_error`, `retry_count`, and `next_retry_at` from `relay_connection`; distinguish supervisor retry (claim/API/auth) from relay reconnect retry.
- `packages/app-web/src/features/settings/model/runtimeDiagnostics.ts` and `packages/core/src/local-runtime/index.ts`
  - Update diagnostics projection so Desktop Runner Supervisor is its own layer or enriches Local Runtime layer without inferring from cloud backend `online`.
- Tests
  - Rust: add `cargo test -p agentdash-local desktop_runner_host` coverage for supervisor state transitions, duplicate ensure, disabled/waiting/auth/API/claim error/retry/manual retry, and token redaction. Add `cargo test -p agentdash-local-tauri` helper tests for settings/profile semantic convergence if helper is testable in `main.rs`.
  - Frontend: extend `packages/app-web/src/features/settings/model/runtimeDiagnostics.test.ts` or add a core local-runtime diagnostics test for supervisor state mapping, last error/retry projection, and no token leakage.
  - UI: if testing infra supports it for `packages/views`, add a focused test around `LocalRuntimeView` rendering manual retry/last error for a supplied supervisor snapshot. No current `LocalRuntimeView` test exists.

### Verification Commands

- Fast local checks for this task’s likely affected surfaces:
  - `pnpm run desktop:check`
  - `cargo test -p agentdash-local`
  - `cargo test -p agentdash-local-tauri`
  - `pnpm --filter app-web test packages/app-web/src/features/settings/model/runtimeDiagnostics.test.ts`
  - `pnpm run shared:check`
  - `pnpm --filter app-tauri typecheck`
- Existing `desktop:check` script is `pnpm run icons:generate && pnpm run shared:check && pnpm --filter app-tauri typecheck && cargo check -p agentdash-local-tauri` at `package.json:32`.
- Full package build/installer:
  - `pnpm run desktop:bundle`
  - For a configured external server: `$env:AGENTDASH_DEFAULT_CLOUD_ORIGIN = "https://agentdash.example.com"; pnpm run desktop:bundle`
  - Equivalent CLI override: `pnpm run desktop:bundle -- --default-cloud-origin https://agentdash.example.com`
- `desktop:bundle` script is `node ./scripts/desktop-build.js --bundles nsis --no-sign --ci` at `package.json:25`.
- `scripts/desktop-build.js` calls `runDesktopBuild({ tauriConfigPath: 'crates/agentdash-local-tauri/tauri.conf.json', defaultApiMode: 'external' })` at `scripts/desktop-build.js:11-15`.
- The real build runner sets env `AGENTDASH_DESKTOP_DEFAULT_API_MODE`, `AGENTDASH_DESKTOP_DEFAULT_API_ORIGIN`, `AGENTDASH_DESKTOP_DEFAULTS_JSON`, and `VITE_API_ORIGIN` before running `pnpm exec tauri build --config crates/agentdash-local-tauri/tauri.conf.json ...` at `scripts/lib/desktop-build.js:38-58`.
- `desktop-build` validates `external` mode requires origin/default cloud origin at `scripts/lib/desktop-build.js:421-425`.
- Successful build prints artifact boundaries: NSIS setup exe under `target/release/bundle/nsis` and app exe candidates under `target/release` at `scripts/lib/desktop-build.js:540-565`.
- Docs currently list expected NSIS path `target/release/bundle/nsis/AgentDash_0.1.0_x64-setup.exe` at `docs/desktop-dev.md:171-175`.

### Windows Manual Acceptance: Automatable / Semi-Automatable Entry Points

- Build/install package:
  - Build can be automated with `pnpm run desktop:bundle` after setting `AGENTDASH_DEFAULT_CLOUD_ORIGIN`.
  - Installing NSIS is semi-automatable by launching the printed setup exe; silent flags depend on Tauri/NSIS config and were not confirmed in code during this research.
- Process single-instance:
  - Automatable after install with PowerShell: start installed `AgentDash.exe` twice, then assert a single `AgentDash`/`agentdash-local-tauri` process and observe/focus main window. Avoid destructive process cleanup unless explicitly part of the test harness.
  - Current code uses official single-instance plugin; second instance callback calls `restore_main_window(app)` at `crates/agentdash-local-tauri/src/main.rs:858-861`.
- Login/auth auto-start:
  - Semi-automatable because user login and token availability happen through the UI/browser session. After login, verify native supervisor snapshot transitions through waiting/claiming/starting/running and cloud backend becomes online.
  - After implementation, a Tauri command or local status JSON for supervisor snapshot would make this more automatable.
- Tray close/restore/quit:
  - Semi-automatable through UI automation: close main window and assert process remains; click tray open or launch second instance and assert restore; explicit quit via tray or `desktop_quit_request` should stop runtime and exit.
  - Current close-to-tray is in `on_window_event` at `crates/agentdash-local-tauri/src/main.rs:863-878`; explicit quit path stops runtime at `crates/agentdash-local-tauri/src/main.rs:1099-1107`.
- Runtime online/offline:
  - Automatable at cloud API layer after login: poll backend list/runtime summary for backend with `registration_source = desktop_access_token`, expected machine id/slot, and online state.
  - Local relay retry can be observed through `runtime_snapshot().relay_connection` once runtime starts; supervisor claim/API/auth retry needs the new snapshot.
- External API mode:
  - Build with `--api-mode external --api-origin <server>` or default cloud origin, install/run, and verify Desktop API snapshot reports external origin running while embedded runner still auto-starts. This specifically covers the requirement that external mode must not disable desktop embedded runner.
- Failure/retry:
  - Semi-automatable by pointing package/dev shell at unreachable server or temporarily blocking network, then observing supervisor state `retrying`/`last_error` and manual retry. Current code cannot fully expose this before supervisor snapshot is implemented.

## Caveats / Not Found

- `task.py current --source` reported no active task. This research used the task path explicitly supplied in the user prompt.
- I did not find a native `DesktopRunnerSupervisorSnapshot` or equivalent richer desktop runner state. The existing host snapshot is still `LocalRuntimeStatus`, which cannot represent `waiting_for_auth`, `waiting_for_api`, `claiming`, supervisor retry, `last_attempt_at`, or `next_retry_at`.
- I did not find any native startup call in Tauri `.setup(...)` that starts embedded runner based on `DesktopAppSettings.auto_connect_local_runtime` and current auth. Auto-connect remains web-driven in `packages/app-web/src/desktop/localRuntimeBridge.ts`.
- I did not find any code path where `LocalRuntimeProfile.auto_start` affects native startup. It is currently persisted and displayed but behaviorally inert.
- I did not find `LocalRuntimeView` component tests. Existing coverage is model-level diagnostics tests plus Rust runtime/Tauri helper tests.
- I did not run validation commands or the bundle build; this was a read-only research pass.
- I did not confirm silent NSIS install/uninstall flags from the generated installer because that requires build artifact inspection or Tauri bundle config beyond this research scope.

## Post-Implementation Note

This file is the pre-implementation map for the remaining completion pass. The subsequent implementation added native supervisor state, settings/profile semantics, settings-page diagnostics, manual retry, bundle validation, and updated task/spec/docs. Treat the "not found" items above as the research baseline that drove the follow-up work, not as the final task state.
