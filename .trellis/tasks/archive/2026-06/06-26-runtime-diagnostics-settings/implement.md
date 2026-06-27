# 运行状态诊断与设置体验 - Implement

## Step 1 - Handoff Gate

Before coding, confirm:

- Runner task provides registration source and relay connection snapshot。
- Windows desktop task provides desktop settings commands。
- Runtime restart active-session behavior is known。
- If handoff is missing, update this task to design-only until fields stabilize。

## Step 2 - DTO And Contracts

- Add or consume `registration_source` and relay connection DTO。
- For backend API fields, update `agentdash-contracts` and generated TS。
- For Tauri-only fields, update `@agentdash/core/local-runtime` types。
- Add feature view-model mappers。

Validation:

- `pnpm run contracts:check`
- mapper unit tests。

## Step 3 - Runtime Snapshot Extension

- Extend `LocalRuntimeStatus` or add `runtime_diagnostics_snapshot`。
- Add relay connection fields:
  - state。
  - redacted target。
  - last_connected_at。
  - last_disconnected_at。
  - last_error。
  - retry_count。
  - next_retry_at if available。
- Ensure ws_client writes structure, not only `diag!`。

Validation:

- Rust unit tests for snapshot transitions if practical。
- desktop smoke test of snapshot command。

## Step 4 - Log Redaction

- Extend redaction helper。
- Ensure `logs_tail` returns redacted messages。
- Ensure copy/export uses redacted content。
- Add tests for query token、Bearer、JSON field、case variants。

Validation:

- `cargo test -p agentdash-local` targeted tests。

## Step 5 - Desktop Settings Bridge

- Add settings load/save command if not already provided by desktop task。
- Wire fields:
  - `launch_at_login`
  - `start_minimized_to_tray`
  - `auto_connect_local_runtime`
- Keep desktop settings out of generic Web path。

Validation:

- `pnpm --filter app-tauri typecheck`
- desktop settings tests if available。

## Step 6 - Frontend Model Layer

- Add diagnostics query/hook。
- Combine cloud API, desktop API, runtime/runner, relay, registration, logs and settings into view model。
- Add severity reducer that picks worst layer without using logs as source。
- Add error copy mapper。

Validation:

- frontend unit tests for state combinations。
- `pnpm run frontend:check`

## Step 7 - Settings UI

- Add 本机运行诊断 section。
- Render:
  - health overview。
  - connection chain。
  - registration identity。
  - action buttons。
  - desktop settings。
  - logs panel。
- Use existing components/primitives and lucide icons。
- Hide desktop-only controls outside Tauri host。

Validation:

- `pnpm run frontend:lint`
- visual/manual check in desktop shell。

## Step 8 - Actions And Recovery

- Wire refresh。
- Wire restart/stop where supported。
- Wire logs clear/copy。
- For independent runner service, show read-only “由系统服务管理” unless service management is explicitly exposed。
- Show active-session block notice if restart fails because sessions are running/canceling。

Validation:

- restart success/failure manual。
- log copy contains no token。

## Step 9 - End-To-End Checks

- `pnpm run contracts:check`
- `pnpm run frontend:check`
- `pnpm run frontend:lint`
- Rust targeted tests if logging/snapshot changed。
- Desktop manual:
  - Desktop API starting/running/error。
  - runtime stopped/running/error。
  - relay reconnect after network interruption。
  - logs clear/copy。
  - token redaction。

## Handoff Output

Write final child summary with:

- DTO names and generated files。
- status facts and their sources。
- UI paths。
- action/recovery commands。
- logs redaction test evidence。
- release validation steps。

## 2026-06-26 Implementation Handoff

已完成：

- `/backends` contract 新增 `registration_source` 字段，server 从 backend device 中已写入的显式 `registration_source` 元数据投影到 generated DTO；前端后端管理详情与诊断 view-model 只消费该字段。
- `@agentdash/core/local-runtime` 新增 `RuntimeDiagnosticsSnapshot`、layer/relay/registration/settings 类型、`createRuntimeDiagnosticsSnapshot()` 和 `redactRuntimeDiagnosticText()`，用于集中表达 Cloud API、Desktop API、Local Runtime、Runner、relay、registration、logs、settings 的事实源。
- `app-tauri` desktop bridge 暴露已有 `desktop_api_snapshot`，`app-web` 设置页轮询该 bridge，并把 Project event stream、backend list、runtime-summary、Desktop API snapshot 传入本机 runtime UI。
- `LocalRuntimeView` 新增运行状态诊断总览、注册身份、独立 runner 只读交接、桌面设置区；桌面设置区接入已有 `desktop_settings_load/save` 与 autostart 状态命令。
- runtime start/stop/restart、logs tail/clear/copy 继续走既有 `LocalRuntimeClient` port；日志复制增加前端边界脱敏，producer 侧仍复用 `agentdash-local` 的 `runner_redaction`。
- 桌面托管 runtime 的 WebSocket lifecycle 现在通过 `LocalRuntimeStatus.relay_connection` 输出结构化 relay snapshot，状态由 `ws_client` 在 connecting、registered、reconnecting、disconnected 等生命周期点写入。
- 独立 runner 未暴露桌面 service 管理 bridge 时，UI 只展示 “由 systemd / Windows Service 或前台进程管理”，不提供假 restart/service 控制。

当前仍保留为 release validation / 后续 runner handoff 的实机项：

- 独立 runner status 文件/`status --json` 尚未接入桌面 UI bridge；本轮仅消费云端 `/backends` 与 `/backends/runtime-summary` 的 runner/backend 投影，并把独立 runner service 管理明确交给 systemd / Windows Service。
- Windows 开机自启动、启动到托盘、启动后自动连接 runtime 需要在安装包实机验证：注册表 login item、托盘隐藏/恢复、Desktop API ready gate、auto-connect profile 行为。
- runtime restart active-session 阻止文案沿用 LocalRuntimeManager 返回错误，需要实机覆盖有 running/canceling session 时的桌面提示。
- 需要 release validation 手工证明日志复制不包含 `token/access_token/refresh_token/auth_token/relay_token/registration_token`、Bearer token 和 URL query token。

## Risk Checks

- UI never parses logs to infer state。
- Registration source is explicit。
- Desktop API and Local Runner terminology remains distinct。
- Token-like values are absent from copied/exported logs。
