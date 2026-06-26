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

## Risk Checks

- UI never parses logs to infer state。
- Registration source is explicit。
- Desktop API and Local Runner terminology remains distinct。
- Token-like values are absent from copied/exported logs。
