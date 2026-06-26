# Windows 桌面安装包与后台运行 - Implement

## Step 1 - Tauri Tray And Autostart Dependency Boundary

- Add required Tauri/Rust dependencies/features for tray and autostart path。
- Update capabilities minimally。
- Compile-only wiring first; do not change user behavior in this step。

Validation:

- `cargo check -p agentdash-local-tauri`

## Step 2 - Desktop App Settings Model

- Add `DesktopAppSettings`:
  - `launch_at_login`
  - `start_minimized_to_tray`
  - `auto_connect_local_runtime`
- Add load/save Tauri commands。
- Persist settings separately from LocalRuntimeProfile or explicitly split current profile semantics。
- Add TS bridge types in `app-tauri`。

Validation:

- `pnpm --filter app-tauri typecheck`
- `cargo check -p agentdash-local-tauri`
- settings normalize tests where practical。

## Step 3 - System Tray Menu And Window Restore

- Create tray icon/menu in Tauri setup。
- Implement menu item `打开 AgentDash`。
- Implement tray click restore behavior。
- Add status item(s) for Desktop API/runtime state if supported by Tauri menu model。

Validation:

- Windows dev shell manual: hide/show/focus via tray。

## Step 4 - Close-To-Tray And Explicit Quit

- Intercept close request。
- If explicit quit flag is false, hide main window and prevent process exit。
- Add explicit quit path from tray menu / command。
- Frontend titlebar close can keep calling window close; Rust lifecycle owns final behavior。

Validation:

- Close window -> process remains。
- Tray restore works。
- Explicit quit -> process exits。

## Step 5 - Tray Runtime Start/Stop

- Tray menu starts/stops runtime using existing profile/runtime manager。
- Menu handles missing profile with explicit error/status。
- Runtime state refresh updates menu enabled/disabled state。

Validation:

- Start/stop runtime from tray。
- Settings/runtime panel reflects state。

## Step 6 - Windows Login Autostart And Start-To-Tray

- Implement `launch_at_login` enable/disable/is_enabled。
- Implement `start_minimized_to_tray` in setup as early as possible。
- Ensure autostart entry launches installed app exe, not setup exe。
- Ensure uninstall cleanup path is known and documented。

Validation:

- Enable setting, sign out/reboot, app launches。
- With start-to-tray true, main window stays hidden and tray is available。
- Disable setting removes startup entry。

## Step 7 - Runtime Auto-Connect Convergence

- Pick single owner: Web AuthGate after user + Desktop API ready。
- Remove/disable Rust setup auto-start and `LocalRuntimeView` mount auto-start duplication, or make them no-op when the owner already ran。
- Ensure `runtime_start` is idempotent for starting/running。
- Update local runtime profile field naming if needed to avoid OS autostart ambiguity。

Validation:

- Launch app with auto-connect enabled。
- Logs show one ensure/claim/start attempt。
- Reopening settings does not auto-start again。

## Step 8 - Desktop API Loopback Contract

- Verify builtin release path binds `127.0.0.1`。
- Add guard for release sidecar/external origin if those modes remain exposed。
- Add test/helper for loopback origin validation if implementation adds validation helper。

Validation:

- `/api/health` available on loopback。
- Non-loopback origin cannot be used in release path。

## Step 9 - NSIS Bundle Metadata And Product Paths

- Confirm productName、identifier、version、icon、bundle target。
- Ensure build script prints or records setup exe output path。
- Clarify app exe process name and installed path for release validation。

Validation:

- `pnpm run desktop:bundle`

## Step 10 - Desktop Lifecycle Tests And Handoff

- Add tests for settings normalize/origin guard/runtime auto-connect idempotency where practical。
- Run:
  - `pnpm run desktop:check`
  - `pnpm run desktop:bundle`
- Write handoff:
  - setup exe output path。
  - app exe path/process name。
  - tray behavior。
  - autostart registry/startup mechanism。
  - uninstall cleanup boundary。
  - runtime auto-connect owner。

## Blockers / Review Points

- Explicit exit behavior with active tasks depends on `runtime-diagnostics-settings` active execution summary。
- Autostart implementation path must be chosen before code: Rust command wrapper vs Tauri plugin exposed to JS。
- Any release Desktop API mode other than builtin localhost needs security review。
