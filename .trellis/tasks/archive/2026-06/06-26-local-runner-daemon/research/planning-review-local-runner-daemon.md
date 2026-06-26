# Research: Local Runner Daemon Planning Review

- Query: Review the `Local Runner 服务器守护进程交付` planning artifacts and recommend fuller design/implement content without modifying product code.
- Scope: internal
- Date: 2026-06-26

## Findings

### Files Found

| Path | Description |
| --- | --- |
| `.trellis/tasks/06-26-local-runner-daemon/prd.md` | Defines headless Local Runner requirements: config sources, service commands, file logs, status output, systemd and Windows Service support. |
| `.trellis/tasks/06-26-local-runner-daemon/design.md` | Current initial design covering high-level architecture, config priority, service model, diagnostics and tradeoffs. |
| `.trellis/tasks/06-26-local-runner-daemon/implement.md` | Current implementation checklist covering CLI, config model, token claim, service install, WS loop and validation. |
| `.trellis/tasks/06-26-local-runner-daemon/implement.jsonl` | Context manifest already includes local runtime boundary, logging and diagnostics specs. |
| `.trellis/tasks/06-26-local-runner-daemon/check.jsonl` | Check manifest includes backend quality guidelines and reuse thinking guide. |
| `.trellis/spec/cross-layer/desktop-local-runtime.md` | Key cross-layer contract for local runtime identity, server claim results, local paths, relay protocol and runtime boundaries. |
| `.trellis/spec/backend/logging-guidelines.md` | Logging levels, structured fields and sensitive-value constraints. |
| `.trellis/spec/backend/diagnostics-guidelines.md` | Requires `diag!` for platform diagnostics and clarifies `agentdash-local` currently does not provide API diagnostic buffer/file layer. |
| `crates/agentdash-local/src/main.rs` | Current CLI accepts direct `--cloud-url`, `--token`, `--backend-id`, `--workspace-roots`, `--name`, `--no-executor`, plus `machine-identity`. |
| `crates/agentdash-local/src/runtime.rs` | Runtime construction, in-memory status/log buffer, token redaction helper, `run_standalone`, embedded session DB setup and relay config construction. |
| `crates/agentdash-local/src/runtime_paths.rs` | Current desktop/local-runtime data path source; server daemon paths need separate system-level defaults. |
| `crates/agentdash-local/src/ws_client.rs` | WebSocket connect/register/message-loop/reconnect implementation. |
| `crates/agentdash-local/src/machine_identity.rs` | Machine identity load/create implementation persisted through `runtime_paths`. |
| `crates/agentdash-local/src/local_backend_config.rs` | Workspace-local config loading and saving pattern; not the daemon config file. |
| `scripts/dev-runtime.js` | Development process supervisor, stale-process cleanup, local runtime ensure flow and startup argument assembly. |
| `.trellis/tasks/06-26-runner-enrollment-token/{prd,design,implement}.md` | Handoff source for runner registration token and `/api/local-runtime/runner/claim`. |
| `.trellis/tasks/06-26-runtime-diagnostics-settings/{design,implement}.md` | Handoff source for structured status and settings UI boundaries. |
| `.trellis/tasks/06-26-distribution-release-validation/{design,implement}.md` | Handoff source for release artifacts and validation checklist expectations. |
| `crates/agentdash-api/src/routes/backends.rs` | Existing desktop `/api/local-runtime/ensure` endpoint and runtime health/summary routes. |
| `crates/agentdash-application/src/backend/management.rs` | Existing ensure-local-runtime application service, stable backend id derivation and token issuance for desktop path. |

### Code Patterns

- Current standalone CLI is direct-credential based. `Cli` exposes `--cloud-url`, `--token`, `--workspace-roots`, `--name`, `--backend-id` and `--no-executor`; the only subcommand is `machine-identity` (`crates/agentdash-local/src/main.rs:8`, `crates/agentdash-local/src/main.rs:39`). `cli_action` requires `--cloud-url`, `--token` and a claimed `--backend-id` before entering `run_standalone` (`crates/agentdash-local/src/main.rs:59`, `crates/agentdash-local/src/main.rs:75`, `crates/agentdash-local/src/main.rs:79`, `crates/agentdash-local/src/main.rs:93`).
- The spec requires `backend_id`, `relay_ws_url` and relay token to come from server ensure/claim, and standalone `agentdash-local` must explicitly consume claimed backend identity rather than locally creating it (`.trellis/spec/cross-layer/desktop-local-runtime.md`, section "机器身份").
- `LocalRuntimeConfig` currently carries the already-claimed runtime tuple: `cloud_url`, `token`, `backend_id`, `name`, `workspace_roots`, `executor_enabled` (`crates/agentdash-local/src/runtime.rs:29`). This is a good boundary for "resolved runtime config"; daemon-level config should resolve into this rather than expanding `ws_client::Config` construction everywhere.
- `run_standalone` keeps the current infinite reconnect behavior by building `ws_config` once and delegating to `ws_client::run` (`crates/agentdash-local/src/runtime.rs:374`). The plan should preserve this path after config/claim resolution.
- The relay loop already expresses connect -> register -> message loop -> disconnect -> reconnect. It builds the WS URL as `cloud_url?token=...`, logs connect failures, and waits with exponential delay (`crates/agentdash-local/src/ws_client.rs:52`, `crates/agentdash-local/src/ws_client.rs:57`, `crates/agentdash-local/src/ws_client.rs:64`, `crates/agentdash-local/src/ws_client.rs:79`, `crates/agentdash-local/src/ws_client.rs:85`, `crates/agentdash-local/src/ws_client.rs:322`). Design should add durable "last connection state" projection around this loop, not replace the loop.
- The relay session sends `RegisterPayload` with backend id, name, version and capabilities, then waits up to 10 seconds for `register_ack`; relay-level token-invalid failures are currently surfaced as register/connection errors (`crates/agentdash-local/src/ws_client.rs:139`, `crates/agentdash-local/src/ws_client.rs:148`, `crates/agentdash-local/src/ws_client.rs:160`).
- Runtime manager already has an in-memory `LocalRuntimeStatus` with `state`, `backend_id`, `name`, workspace roots, executor flag, MCP server count and message (`crates/agentdash-local/src/runtime.rs:51`). Daemon status needs a separate on-disk status snapshot because `service status` runs in a different process.
- Runtime logs are currently an in-memory ring buffer with redaction for `token=`, `access_token=` and `refresh_token=` (`crates/agentdash-local/src/runtime.rs:76`, `crates/agentdash-local/src/runtime.rs:275`, `crates/agentdash-local/src/runtime.rs:511`). `auth_token` and `registration_token` must be added to the daemon redaction rules because the current helper does not cover them.
- `agentdash-local` main initializes only a plain tracing fmt subscriber with env filter (`crates/agentdash-local/src/main.rs:47`). Diagnostics spec says `agentdash-local` uses `diag!` but does not currently attach the API process file layer or diagnostic buffer (`.trellis/spec/backend/diagnostics-guidelines.md`, section "日志落地"). Daemon file logging therefore needs an explicit local subscriber/file appender design.
- `runtime_paths` currently targets per-user desktop local-runtime directories such as `%APPDATA%/AgentDash/local-runtime` or `$XDG_DATA_HOME/agentdash/local-runtime` (`crates/agentdash-local/src/runtime_paths.rs:8`). Server daemon config/log/status defaults should not silently reuse those per-user paths; the design already proposes `/etc/agentdash/runner.toml` and `%PROGRAMDATA%\AgentDash\runner\config.toml`, but should also define state and credential paths.
- Machine identity is persisted through `machine_identity_path()` and normalizes empty id/label values (`crates/agentdash-local/src/machine_identity.rs:14`, `crates/agentdash-local/src/machine_identity.rs:24`, `crates/agentdash-local/src/machine_identity.rs:47`). For daemon installs, design should decide whether machine identity remains in local-runtime data root or moves under daemon state dir, because service user identity changes path resolution on Windows and Linux.
- Dev runtime claim uses the current desktop ensure endpoint, then starts `agentdash-local` with `--cloud-url`, `--token`, `--name`, `--backend-id`, optional workspace roots and `--no-executor` (`scripts/dev-runtime.js:135`, `scripts/dev-runtime.js:747`). The runner daemon can borrow validation shape and startup args but must switch to registration-token claim contract, not reuse the desktop access-token path.
- Dev cleanup detects stale `agentdash-local` and kills it to avoid duplicate registration (`scripts/dev-runtime.js`, `runCleanup`, local conflict branch). Service install/uninstall design should similarly define duplicate-runner boundaries: service manager owns daemon lifecycle; dev scripts may kill debug binaries, but runner service commands should not kill arbitrary user processes beyond the named service they installed.
- Existing cloud desktop ensure endpoint is `/api/local-runtime/ensure` (`crates/agentdash-api/src/routes/backends.rs:55`); it derives relay URL from request headers and calls `ensure_local_runtime_record` (`crates/agentdash-api/src/routes/backends.rs:428`, `crates/agentdash-api/src/routes/backends.rs:434`, `crates/agentdash-api/src/routes/backends.rs:435`). The runner token task explicitly introduces separate `/api/local-runtime/runner/claim`, so this task's design should not overfit to the current ensure DTO.
- Existing application ensure service creates stable backend id from `machine_id + share_scope_kind + share_scope_id + capability_slot` and generates auth token server-side (`crates/agentdash-application/src/backend/management.rs:135`, `crates/agentdash-application/src/backend/management.rs:188`, `crates/agentdash-application/src/backend/management.rs:298`). The runner claim handoff should specify whether this same stable id algorithm is reused for project-scoped runner tokens.

### Current Plan Gaps / Risks

1. **P0: Windows Service entrypoint is underspecified.** `service install` alone is not enough if `agentdash-local.exe` is registered directly with SCM. A Windows service process must enter the service control dispatcher and handle stop/status control, or use a bundled service wrapper. The design currently says "register Windows Service" but not whether the binary gains a Windows service runtime mode or a wrapper is shipped. Without this, Windows acceptance will fail at service start.
2. **P0: Claim/credential state machine is too implicit.** Current design says "if credentials exist connect, else registration token claim", but implementation needs exact states and failure semantics: unconfigured, needs_claim, claiming, claimed, connecting, registered, disconnected_retrying, fatal_config_error, fatal_claim_error, stopping. Token invalid/revoked/expired should be fatal until config changes; cloud unreachable during claim can retry or exit depending service policy, but must be explicit.
3. **P0: Credential persistence boundary is missing.** The design lists `registration_token` and `auth_token` in one config file but does not specify whether registration token remains after first successful claim, whether runtime credentials are written back to the same file, and which file permissions protect them. Recommended split: operator config plus `credentials.toml` or same TOML sections with atomic writes and owner-only permissions; define Linux `root:agentdash 0640` or `0600`, Windows ACL restricted to LocalSystem/Administrators/service account.
4. **P0: Service user and workspace permissions are not designed.** Windows Service under LocalSystem will not inherit the desktop user's mapped drives, PATH, SSH keys, Git credentials or workspace permissions. Linux systemd under root vs dedicated `agentdash` user has different workspace access. The current risk note mentions this but design should decide the first-version default and document required override knobs.
5. **P1: Config source priority needs field-level behavior.** `CLI > env > file` is stated, but install-time vs run-time semantics are unclear. `service install --config X` should install a stable config path; the service runtime should not bake transient CLI secrets into the service definition unless intentionally required. Environment variable names and list parsing for workspace roots also need to be fixed before implementation.
6. **P1: On-disk status projection is missing.** `service status` runs out-of-process and cannot read `LocalRuntimeManager` memory. It needs a status file path, schema, update cadence, stale threshold, and lock/write strategy. The output should merge system service state with last daemon status: configured, last claim, relay target, last connected/disconnected, last error, pid/version, log path.
7. **P1: File logging architecture is missing.** Existing `agentdash-local` uses `diag!` and stdout subscriber only. Daemon mode needs deterministic log path, rotation policy or logrotate/systemd journal stance, redaction rules covering `registration_token` and `auth_token`, and service stdout/stderr behavior.
8. **P1: systemd unit details are absent.** Need unit user/group, `ExecStart`, `WorkingDirectory`, environment file policy, restart policy, restart delay, `KillSignal`/timeout, `StateDirectory`/`LogsDirectory`/`ConfigurationDirectory` if used, and whether install also reloads daemon. Without this, "service install" tests cannot be deterministic.
9. **P1: Uninstall boundary is absent.** `service uninstall` should define what it removes: service registration/unit and optional generated service file. It should not delete operator config, credentials, logs, machine identity or workspace data unless a separate explicit purge exists. This matters for release validation.
10. **P1: Runner path model conflicts with desktop `runtime_paths`.** Current library data root is per-user. Server daemon defaults are system-level. Design should introduce runner-specific path helpers instead of overloading desktop local-runtime paths.
11. **P1: API/claim contract dependency is not pinned enough.** Handoff says runner consumes `/api/local-runtime/runner/claim`, but the daemon design should list request fields, response fields, error categories and retryability expected from the token task so implementers do not guess.
12. **P2: Service command implementation strategy is not chosen.** For testability, plan should choose pure command assembly helpers for systemd/sc.exe plus thin execution layer, or service-manager crate wrappers. This affects unit tests and release instructions.
13. **P2: Status/output formats are not fixed.** Human text is useful, but diagnostics/settings/release validation will benefit from `status --json`. Design should define both redacted human output and JSON schema.
14. **P2: Validation misses service-mode unit tests.** Current acceptance asks `cargo test -p agentdash-local`, but implement plan should include command generation tests, config merge tests, claim response persistence tests, redaction tests, status stale tests and platform-specific `#[cfg]` tests where possible.

### Recommended Fuller Design Structure

The following outline can be pasted into `design.md` and expanded.

#### 1. Product Boundary

- `agentdash-local` gains a headless Runner mode for server hosting. Runner starts no Dashboard API, no desktop shell and no business HTTP listener; it only performs outbound cloud claim and WebSocket relay connections.
- Existing desktop local-runtime manager remains a separate lifecycle surface. Shared code should be config resolution -> `LocalRuntimeConfig` -> existing `run_standalone`/`ws_client` loop.
- Runner service lifecycle is owned by OS service managers: systemd on Linux and Windows Service Control Manager on Windows.

#### 2. Runtime Modes And CLI Shape

- `agentdash-local run` or default run path starts foreground runner/runtime after resolving config.
- `agentdash-local status [--config PATH] [--json]` reports resolved config source, credential state, relay state, system service state when available, log path and status path.
- `agentdash-local service install|uninstall|start|stop|status [--config PATH]` manages the OS service.
- Existing direct runtime flags remain only as inputs into resolved config, not as a second identity model: `backend_id`, `relay_ws_url` and `auth_token` still must be server-issued.
- `machine-identity` remains available for diagnostics, but daemon mode must use runner path rules for machine identity if service user path differs from desktop path.

#### 3. Configuration Model And Priority

- Field priority is `CLI > environment > config file > platform defaults`.
- Config file defaults:
  - Linux config: `/etc/agentdash/runner.toml`
  - Linux state: `/var/lib/agentdash/runner/`
  - Linux log: `/var/log/agentdash/runner.log`
  - Windows config: `%PROGRAMDATA%\AgentDash\runner\config.toml`
  - Windows state: `%PROGRAMDATA%\AgentDash\runner\`
  - Windows log: `%PROGRAMDATA%\AgentDash\runner\runner.log`
- Environment variable names should be fixed, for example:
  - `AGENTDASH_RUNNER_CONFIG`
  - `AGENTDASH_RUNNER_SERVER_URL`
  - `AGENTDASH_RUNNER_REGISTRATION_TOKEN`
  - `AGENTDASH_RUNNER_BACKEND_ID`
  - `AGENTDASH_RUNNER_RELAY_WS_URL`
  - `AGENTDASH_RUNNER_AUTH_TOKEN`
  - `AGENTDASH_RUNNER_NAME`
  - `AGENTDASH_RUNNER_WORKSPACE_ROOTS`
  - `AGENTDASH_RUNNER_EXECUTOR_ENABLED`
  - `AGENTDASH_RUNNER_LOG_PATH`
  - `AGENTDASH_RUNNER_STATE_DIR`
- `server_url` is the HTTP(S) origin used for claim. `relay_ws_url` is the server-issued WS/WSS endpoint used for relay. The runner must not derive `backend_id` locally.
- `workspace_roots` delimiter rules should match current CLI behavior where practical: comma and platform path delimiter accepted, then normalized/canonicalized through existing helper.

#### 4. Credential Persistence

- Recommended persisted sections:

```toml
[runner]
name = "build-runner-01"
server_url = "https://agentdash.example.com"
workspace_roots = ["/srv/workspaces"]
executor_enabled = true

[registration]
token = "..." # optional after claim depending operator policy

[credentials]
backend_id = "local_..."
relay_ws_url = "wss://agentdash.example.com/ws/backend"
auth_token = "..."
claimed_at = "2026-06-26T00:00:00Z"
token_source = "runner_registration_token"
```

- Successful claim writes `backend_id`, `relay_ws_url`, `auth_token`, `claimed_at`, server/project metadata if returned. Write must be atomic: temp file in same directory, fsync if helper exists, then rename.
- File permissions:
  - Linux config/credentials created by service install should be owner-readable only or root/service-group-readable: `0600` or `0640` with a dedicated group.
  - Windows config/credentials ACL should be limited to Administrators and the configured service identity.
- Redaction must cover `registration_token`, `auth_token`, `token`, `access_token`, `refresh_token`, bearer headers and URL query token values.
- Open design decision: after successful claim, either retain registration token for rotation/reclaim or remove it from disk. If retained, status must report only "present/redacted". If removed, re-claim requires operator to provide a token again.

#### 5. Claim / Direct Credential State Machine

Suggested states:

| State | Entry condition | Behavior | Exit |
| --- | --- | --- | --- |
| `unconfigured` | Missing both complete runtime credentials and registration token | Log fatal config error, exit non-zero in foreground; service remains failed | Operator updates config |
| `needs_claim` | Missing one or more of `backend_id`, `relay_ws_url`, `auth_token`, registration token present | Enter claim flow | `claiming` |
| `claiming` | Registration token available | POST `/api/local-runtime/runner/claim` with machine identity, name, executor flag, workspace roots, client version, device | `claimed` or `fatal_claim_error` or retryable claim error |
| `claimed` | Claim response contains complete runtime credentials | Atomically persist credentials, build `LocalRuntimeConfig` | `connecting` |
| `direct_credentials` | Config/env/CLI provides complete server-issued runtime credentials | Build `LocalRuntimeConfig`; mark source | `connecting` |
| `connecting` | Runtime config complete | Enter existing `ws_client` loop | `registered` or `disconnected_retrying` |
| `registered` | register_ack received | Update status snapshot with connected time | `disconnected_retrying` or `stopping` |
| `disconnected_retrying` | WS read/connect/register failure after a valid runtime config | Keep existing exponential reconnect; update last error | `connecting` |
| `fatal_config_error` | Invalid/missing config, invalid path, incomplete credentials without token | Log, write status, exit non-zero | Operator fix |
| `fatal_claim_error` | Token invalid/expired/revoked/scope denied | Log, write status, exit non-zero | Operator rotate token |
| `stopping` | Service stop or Ctrl+C | Signal relay loop shutdown; write final status | `stopped` |

- Direct credentials are allowed only when all of `backend_id`, `relay_ws_url`, `auth_token` are present and documented as server-issued. This supports emergency/manual operation without introducing local identity generation.
- Claim errors should be categorized by the token task contract: invalid/revoked/expired/scope denied are fatal; server unreachable/timeouts are retryable according to service policy; malformed response is fatal until server/version mismatch is resolved.

#### 6. systemd Service

- Service name: `agentdash-local-runner`.
- Recommended unit:

```ini
[Unit]
Description=AgentDash Local Runner
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=agentdash
Group=agentdash
ExecStart=/usr/bin/agentdash-local run --config /etc/agentdash/runner.toml
Restart=always
RestartSec=5s
WorkingDirectory=/var/lib/agentdash/runner
Environment=RUST_LOG=info
StateDirectory=agentdash/runner
LogsDirectory=agentdash

[Install]
WantedBy=multi-user.target
```

- `service install` should write or update the unit file, run daemon-reload, and optionally create config/state/log directories with the selected user/group.
- The first version should avoid deleting config/credentials/logs on uninstall. `uninstall` stops/disables/removes the unit and runs daemon-reload.
- Workspace access must be explicit: service user needs read/write permissions for configured `workspace_roots`; root default is powerful but less desirable for hosted runner isolation.

#### 7. Windows Service

- Service name: `AgentDashLocalRunner`.
- Design must choose one:
  - Implement a native Windows service entrypoint in `agentdash-local` using a service runtime crate/API, where SCM stop maps to the runner shutdown signal; or
  - Ship and own a wrapper binary specifically for the service process.
- Registering a normal console process with `sc.exe create` is not sufficient unless that process handles the Windows service control dispatcher.
- Recommended first-version shape:
  - `agentdash-local service install --config "%PROGRAMDATA%\AgentDash\runner\config.toml"` registers the service with bin path pointing to `agentdash-local service run --config ...` or another dedicated service entrypoint.
  - Service runs as LocalSystem by default only if accepted; otherwise document `--user` / service account support. LocalSystem has different filesystem/network identity than the desktop user.
  - Stop control must trigger graceful shutdown and wait for relay loop to exit.
  - Uninstall removes the service registration only, not config/credentials/logs.

#### 8. Logging

- File logs are enabled in runner/daemon mode independent from `agentdash-api` diagnostics file layer.
- Default file path:
  - Linux: `/var/log/agentdash/runner.log`
  - Windows: `%PROGRAMDATA%\AgentDash\runner\runner.log`
- Service stdout/stderr may also flow to journald/Event Log, but status and release validation should point to the runner log path.
- Redaction rules apply before file output and status output for token-like fields and URL query secrets.
- Use `diag!` for platform process diagnostics, with `Subsystem::Relay` for relay lifecycle and `Subsystem::Infra` for config/service/claim filesystem operations.

#### 9. Status

- Runner writes a status snapshot file under state dir, for example `runner-status.json`.
- Snapshot fields:
  - `version`
  - `pid`
  - `service_name`
  - `service_state`
  - `config_path`
  - `config_sources`
  - `credential_state`
  - `registration_source`
  - `backend_id`
  - `runner_name`
  - `server_url`
  - `relay_ws_url`
  - `workspace_roots`
  - `executor_enabled`
  - `last_claim_attempt_at`
  - `last_claim_success_at`
  - `last_connected_at`
  - `last_disconnected_at`
  - `last_error_code`
  - `last_error_message`
  - `log_path`
  - `status_path`
- `status` merges:
  - Platform service manager state from systemd/SCM.
  - Last runner snapshot from file.
  - Staleness: if snapshot mtime is older than threshold and service is running, report `status_stale`.
- Add `--json` for diagnostics/settings and release validation; human output can be concise and redacted.

#### 10. Permissions And Security

- Runner opens only outbound HTTP(S)/WS(S); no business HTTP port.
- Claim request includes machine id/label, runner name, client version, device payload, executor flag and workspace roots; it never sends user access token.
- Config and credential files are treated as secrets. Logs and status never include token material.
- Workspace execution occurs under the OS service identity; install docs must tell operators to grant that identity access to configured roots.

#### 11. Uninstall / Purge Boundary

- `service uninstall`:
  - Stop service if running.
  - Disable service.
  - Remove systemd unit or Windows Service registration.
  - Leave config, credentials, logs, state and workspace files intact.
- Optional future `service purge` can remove config/credentials/logs after explicit confirmation. This should not be part of the first acceptance unless required by release validation.

### Recommended Implement Small-Step Commit Checklist

1. **Introduce runner config model and path helpers**
   - Add `RunnerConfigFile`, `ResolvedRunnerConfig`, `ConfigSource`, runner path defaults and env parsing.
   - Tests: file/env/CLI merge priority, workspace root parsing, platform default path selection.

2. **Refactor CLI into explicit command groups**
   - Add `run`, `status`, `service` command enum while preserving existing direct run behavior if needed.
   - Keep `machine-identity` independent from runtime credentials.
   - Tests: parse matrix and required field errors.

3. **Add credential persistence and redaction helpers**
   - Atomic TOML read/write for credentials/config update.
   - File permission/ACL helper abstraction with platform-specific implementation.
   - Redaction covers `registration_token`, `auth_token`, `token`, access/refresh token and URL query token.
   - Tests: atomic write roundtrip, incomplete credential detection, redaction.

4. **Add runner claim client abstraction**
   - Implement request/response DTOs aligned with `runner-enrollment-token` output.
   - Convert successful claim into persisted credentials and `LocalRuntimeConfig`.
   - Categorize errors as fatal vs retryable.
   - Tests with mocked HTTP client/service trait for success, invalid token, revoked/expired, unreachable, malformed response.

5. **Add runner resolution state machine**
   - Resolve config -> either direct credentials or claim -> `LocalRuntimeConfig`.
   - Write status snapshots throughout state transitions.
   - Tests: unconfigured, needs claim, claimed, direct credentials, fatal claim, retryable claim.

6. **Add durable status snapshot and `status --json`**
   - Define status schema and writer.
   - Merge service state + snapshot + stale detection.
   - Tests: stale status, redacted output, service-state merge.

7. **Add runner file logging setup**
   - Configure runner-mode subscriber/file appender.
   - Ensure `diag!` output reaches file; stdout/stderr remains service-friendly.
   - Tests: redaction helper and path selection; manual validation for file creation.

8. **Implement systemd service manager**
   - Generate unit content from resolved install options.
   - Execute `systemctl` operations through an abstraction.
   - Tests: unit content, command sequence, uninstall boundary.

9. **Implement Windows service runtime and manager**
   - Add native service entrypoint or bundled wrapper decision.
   - Implement install/start/stop/status/uninstall command assembly and SCM stop -> graceful shutdown.
   - Tests: service command generation; compile-gated service handler tests where possible.

10. **Wire existing relay loop with runner lifecycle**
    - Start existing `run_standalone`/`ws_client` after config/claim resolution.
    - Add callbacks or wrapper updates for last connected/disconnected/register error status.
    - Tests: keep existing `ws_client` reconnect tests; add status update tests around events if exposed.

11. **Validation and docs handoff**
    - Run `cargo test -p agentdash-local`.
    - Add manual Linux and Windows service validation notes for release task.
    - Confirm no Dashboard API or business HTTP server is started by runner path.

### Handoff Contract: `runner-enrollment-token`

The daemon task needs the token task to deliver or freeze these contracts:

- Endpoint: `POST /api/local-runtime/runner/claim`.
- Auth model: request carries runner registration token; endpoint must not accept user access token as equivalent.
- Request fields:
  - `registration_token`
  - `machine_id`
  - `machine_label`
  - `runner_name`
  - `client_version`
  - `device`
  - `executor_enabled`
  - `workspace_roots`
  - `capability_slot` if not fully server-derived
- Response fields:
  - `backend_id`
  - `name`
  - `relay_ws_url`
  - `auth_token`
  - `backend_enabled`
  - `project_id`
  - `machine_id`
  - `machine_label`
  - `capability_slot`
  - `claimed_at` or server time
- Error categories:
  - `invalid_token`
  - `token_expired`
  - `token_revoked`
  - `scope_denied`
  - `project_not_found`
  - `backend_disabled`
  - `rate_limited`
  - `server_unavailable`
- Retryability:
  - invalid/expired/revoked/scope denied are fatal until operator changes config.
  - unavailable/timeouts/rate-limited can retry with backoff.
- Backend identity:
  - Confirm whether project-scoped runner reuses stable backend id derivation from machine id + project id + capability slot, or returns a separate token-bound backend id.
- Access side effect:
  - Claim must create/update backend and grant `ProjectBackendAccess` consistent with token project scope.

### Handoff Contract: `runtime-diagnostics-settings`

The daemon task should provide these fields/surfaces for the diagnostics/settings task:

- `status --json` schema with distinct source labels:
  - `registration_source`: `runner_registration_token` or `direct_server_credentials`
  - `credential_state`: `unconfigured`, `needs_claim`, `claimed`, `direct_credentials`, `claim_failed`
  - `relay_connection`: `not_started`, `connecting`, `registered`, `disconnected_retrying`, `stopped`, `error`
  - `service_state`: OS service manager state
- Redacted display values:
  - server origin
  - relay WS URL without query token
  - backend id
  - config/log/status paths
- Log tail source:
  - Local runner log path and a bounded tail command/output format, or at minimum the path for external tailing.
- Boundary:
  - Desktop settings may show independent runner status but should not manage system service lifecycle unless explicitly designed; current diagnostics task says desktop settings manage desktop lifecycle, not independent runner service lifecycle.

### Handoff Contract: `distribution-release-validation`

The daemon task should hand release validation:

- Linux artifact expectations:
  - runner binary path/name
  - example config
  - `service install/start/status/stop/uninstall` commands
  - default config/state/log/status paths
  - systemd unit name and expected service states
  - online verification via cloud backend list/runtime summary
  -断网重连 expected status/log messages
- Windows artifact expectations:
  - runner binary path/name
  - service install/start/status/stop/uninstall commands
  - service name `AgentDashLocalRunner`
  - service account guidance
  - default `%PROGRAMDATA%` config/state/log/status paths
  - online and reconnect validation steps
- Version contract:
  - `agentdash-local --version` or equivalent version output must be available.
  - Runner binary version must match release source used by desktop/API/protocol artifacts.
- Uninstall validation:
  - Service registration is removed.
  - Config/credentials/logs/state are preserved unless explicit purge exists.
  - User workspace data is never removed by uninstall.

### External References

- systemd service unit reference: `systemd.service(5)` and `systemd.exec(5)` are the relevant upstream contracts for `Type=simple`, `ExecStart`, restart behavior, `User`, `Group`, `StateDirectory`, `LogsDirectory` and environment handling.
- Windows Services reference: Microsoft Windows Service applications must register with the Service Control Manager and handle service control events; this is the reason the design must choose a native service entrypoint or wrapper rather than only `sc.exe create` for a console binary.
- Current crate versions relevant to implementation:
  - `clap = "4"` with derive is already used by `agentdash-local` CLI.
  - `tracing-subscriber` and `agentdash-diagnostics` are already present.
  - `reqwest = "0.13.2"` with rustls is already present and can back a claim client.
  - No Windows service crate is currently declared in `crates/agentdash-local/Cargo.toml`; adding native Windows service support likely needs a new dependency or a project-owned wrapper.

### Related Specs

- `.trellis/spec/cross-layer/desktop-local-runtime.md`
- `.trellis/spec/backend/logging-guidelines.md`
- `.trellis/spec/backend/diagnostics-guidelines.md`

## Caveats / Not Found

- I did not modify `design.md`, `implement.md`, product code, specs or JSONL manifests; this file is the persisted review output for the main planning session to copy from.
- The active Trellis task command returned no current task; the user explicitly supplied `.trellis/tasks/06-26-local-runner-daemon`, so this research was written under that task's `research/` directory.
- I did not find an existing native Windows Service implementation or dependency in `agentdash-local`; this is a critical design gap, not merely an implementation detail.
- I did not find an existing runner registration token implementation in code, only the planning artifacts. The daemon design should treat `/api/local-runtime/runner/claim` as a dependency contract until that task lands.
- I did not verify external documentation live; external references above are stable platform documentation categories rather than fetched snapshots.
