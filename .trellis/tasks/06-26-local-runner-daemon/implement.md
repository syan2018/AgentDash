# Local Runner 服务器守护进程交付 - Implement

## Step 1 - Runner Config Model And Paths

- Add `RunnerConfigFile`、`ResolvedRunnerConfig`、`ConfigSource`。
- Add runner-specific default path helpers; do not overload desktop `runtime_paths` silently。
- Parse env vars and config file。
- Merge fields by `CLI > env > file > default`。
- Parse workspace roots and canonicalize with existing helpers。

Tests：

- config merge priority。
- missing required fields。
- workspace root parsing。
- platform default path selection。

## Step 2 - CLI Command Groups

- Add explicit commands:
  - `run`
  - `status`
  - `service install|uninstall|start|stop|status`
  - keep `machine-identity`
- Preserve existing direct run behavior if needed, but map it into resolved runner config。
- Ensure `backend_id` direct input is documented as server-issued。

Tests：

- clap parse matrix。
- required field errors。
- `machine-identity` does not require runtime config。

## Step 3 - Credential Persistence And Redaction

- Implement TOML config/credential read/write。
- Atomic credential write after successful claim。
- Add file permission / ACL abstraction。
- Extend redaction to registration token、auth token、Bearer、query token。
- Ensure status/log output uses redacted values。

Tests：

- read/write roundtrip。
- incomplete credential detection。
- redaction cases。
- no token in status output。

## Step 4 - Runner Claim Client

- Implement claim client aligned with `runner-enrollment-token` DTO。
- Convert claim response into persisted credentials and `LocalRuntimeConfig`。
- Categorize errors fatal vs retryable。
- Backoff retry for retryable claim failures according to service policy。

Tests：

- claim success。
- invalid/expired/revoked token fatal。
- cloud unavailable retryable。
- malformed response fatal。
- credential write after claim。

## Step 5 - Resolution State Machine

- Implement config resolution:
  - unconfigured。
  - needs_claim。
  - claimed。
  - direct_credentials。
  - connecting。
  - fatal_config_error。
  - fatal_claim_error。
- Write status snapshot for each transition。
- Build existing `LocalRuntimeConfig` only after credentials are complete。

Tests：

- unconfigured exits non-zero。
- direct credentials enter connecting。
- claim path enters connecting after persistence。
- fatal errors write status。

## Step 6 - Durable Status And `status --json`

- Define `runner-status.json` schema。
- Write snapshot under state dir。
- Merge platform service state with snapshot in `status` command。
- Implement stale detection。
- Provide human and JSON output。

Tests：

- stale status。
- service-state merge。
- redacted JSON output。

## Step 7 - File Logging

- Add runner-mode subscriber/file appender。
- Ensure `diag!` output reaches log file。
- Keep stdout/stderr service-friendly。
- Document log rotation stance: systemd/journald plus file log first version, rotation delegated to host logrotate or future enhancement unless product requires internal rotation。

Tests：

- log path selection。
- redaction helper。

## Step 8 - Linux systemd Manager

- Generate systemd unit from install options。
- Create config/state/log directories if missing。
- Execute `systemctl daemon-reload` after install/uninstall。
- Implement start/stop/status/uninstall command wrappers。
- Uninstall preserves config/credentials/logs/state/workspaces。

Tests：

- unit file content。
- command sequence via dry-run abstraction。
- uninstall boundary。

Manual validation：

- install/start/status/stop/uninstall on Linux host。
- cloud online after start。
- reconnect after network interruption。

## Step 9 - Windows Service Manager

- Decide and implement native Windows service entrypoint or bundled wrapper。
- Do not rely on registering a normal console process without service dispatcher handling。
- Implement install/start/stop/status/uninstall。
- SCM stop triggers graceful shutdown。
- Document service account and workspace permission requirements。

Tests：

- command generation。
- service entrypoint compile-gated tests where practical。
- stop signal maps to runtime shutdown。

Manual validation：

- install/start/status/stop/uninstall from admin PowerShell。
- cloud online after start。
- reconnect after network interruption。

## Step 10 - Relay Loop Integration

- Reuse existing `run_standalone` / `ws_client` for outbound WebSocket。
- Add callbacks or wrapper updates for relay status:
  - connecting。
  - registered。
  - disconnected/retrying。
  - last error。
- Preserve existing reconnect behavior。
- Prove runner path starts no Dashboard API/business HTTP server。

Tests：

- existing ws_client tests。
- status update tests around relay events if exposed。

## Step 11 - Validation And Handoff

- Run `cargo test -p agentdash-local`。
- Run broader `pnpm run backend:check` if shared crates touched。
- Write final handoff:
  - config format。
  - service commands。
  - status schema。
  - log paths。
  - service names。
  - version command。
  - uninstall boundary。
  - validation evidence。

## Blockers Before Start

- `runner-enrollment-token` must freeze `/api/local-runtime/runner/claim` DTO and error categories。
- Windows Service approach must be chosen: native service entrypoint or owned wrapper。
- Service user default must be chosen and documented。
