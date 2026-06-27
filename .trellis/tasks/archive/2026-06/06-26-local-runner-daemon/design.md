# Local Runner 服务器守护进程交付 - Design

## Product Boundary

`agentdash-local` 增加 headless Local Runner 产品形态，面向 Linux/Windows 服务器托管。Runner 不启动 Dashboard API、不承载桌面 UI、不监听业务 HTTP API；它只做两件事：

- 使用 registration token 或 server-issued runtime credentials 解析出 `LocalRuntimeConfig`。
- 通过现有 WebSocket relay 出站连接云端并执行本机任务。

桌面 Local Runtime 与服务器 Local Runner 共享底层 runtime/relay 能力，但生命周期不同：桌面由 Tauri 进程托管，runner 由 OS service manager 托管。

## Runtime Modes And CLI Shape

CLI 目标形态：

- `agentdash-local run [--config PATH]`：前台运行 runner。
- `agentdash-local status [--config PATH] [--json]`：输出平台服务状态 + 最近 runner status snapshot。
- `agentdash-local service install|uninstall|start|stop|status [--config PATH]`：管理 OS service。
- `agentdash-local machine-identity`：保留诊断命令。

现有 direct runtime flags 可继续支持，但只能表达 server-issued 凭据输入。Runner 不能本地创造 `backend_id`，不能把 registration token 当 relay token。

## Configuration Model

配置优先级固定为：

```text
CLI > environment > config file > platform defaults
```

默认路径：

- Linux config：`/etc/agentdash/runner.toml`
- Linux state：`/var/lib/agentdash/runner/`
- Linux log：`/var/log/agentdash/runner.log`
- Windows config：`%PROGRAMDATA%\AgentDash\runner\config.toml`
- Windows state：`%PROGRAMDATA%\AgentDash\runner\`
- Windows log：`%PROGRAMDATA%\AgentDash\runner\runner.log`

环境变量：

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

`server_url` 是 HTTP(S) origin，用于 claim。`relay_ws_url` 是 server-issued WS/WSS endpoint，用于 relay。workspace roots 按现有 CLI 习惯接受逗号分隔，并经过现有路径 canonicalize。

## Credential Persistence

配置文件推荐结构：

```toml
[runner]
name = "build-runner-01"
server_url = "https://agentdash.example.com"
workspace_roots = ["D:/workspaces"]
executor_enabled = true

[registration]
token = "..." # optional after first successful claim

[credentials]
backend_id = "local_..."
relay_ws_url = "wss://agentdash.example.com/ws/backend"
auth_token = "..."
claimed_at = "2026-06-26T00:00:00Z"
token_source = "runner_registration_token"
```

成功 claim 后原子写回 `credentials`。写回策略：

- same-directory temp file。
- flush/fsync where practical。
- rename 替换。
- Linux 权限 `0600` 或 `0640` with dedicated group。
- Windows ACL 限制到 Administrators 与 service identity。

Open decision for implementation review：成功 claim 后是否保留 registration token。第一版推荐保留但只显示 `present/redacted`，便于 service 重装/凭据恢复；如果产品希望降低长期 secret 暴露，可改为 claim 后移除，代价是恢复时需要重新输入 token。

## Claim And Runtime State Machine

状态：

| State | Entry | Behavior | Exit |
| --- | --- | --- | --- |
| `unconfigured` | 缺少完整 runtime credentials 且无 registration token | 写 fatal status，退出非零 | operator fixes config |
| `needs_claim` | 缺少 `backend_id/relay_ws_url/auth_token` 且有 registration token | 进入 claim | `claiming` |
| `claiming` | registration token 可用 | POST `/api/local-runtime/runner/claim` | `claimed` / fatal / retry |
| `claimed` | claim response 完整 | 原子写回 credentials，构建 `LocalRuntimeConfig` | `connecting` |
| `direct_credentials` | 完整 server-issued credentials 已配置 | 构建 `LocalRuntimeConfig` | `connecting` |
| `connecting` | runtime config 完整 | 进入现有 ws_client loop | `registered` / retry |
| `registered` | register_ack 成功 | 写 connected status | retry / stopping |
| `disconnected_retrying` | connect/read/register 失败 | 保留现有 backoff 重连，写 last error | connecting |
| `fatal_config_error` | 无效配置/路径/凭据 | 写 status，退出非零 | operator fix |
| `fatal_claim_error` | token invalid/expired/revoked/scope denied | 写 status，退出非零 | operator rotate token |
| `stopping` | Ctrl+C 或 service stop | shutdown relay loop | stopped |

Claim error retryability 来自 `runner-enrollment-token` handoff：

- invalid/expired/revoked/scope denied：fatal until config changes。
- server unavailable/timeouts/rate-limited：retry with backoff。
- malformed response/version mismatch：fatal until server/client version aligned。

## Service Model

Linux：

- service name：`agentdash-local-runner`
- unit type：`simple`
- default user：dedicated `agentdash` service user if installer creates it; otherwise documented operator-selected user。
- `ExecStart=/usr/bin/agentdash-local run --config /etc/agentdash/runner.toml`
- `Restart=always`
- `RestartSec=5s`
- `After=network-online.target`

Windows：

- service name：`AgentDashLocalRunner`
- display name：`AgentDash Local Runner`
- first-version design must choose native Windows service entrypoint or bundled service wrapper。
- 注册普通 console binary 到 SCM 不够；service process 必须进入 service control dispatcher 并处理 stop/status control。
- recommended command shape：`agentdash-local service run --config "%PROGRAMDATA%\AgentDash\runner\config.toml"` as service entrypoint。
- SCM stop maps to graceful shutdown signal。

Uninstall boundary：

- Stop service。
- Disable/remove service registration or unit file。
- Leave config、credentials、logs、machine identity、workspace data intact。
- Future `service purge` can remove secrets/logs only with explicit confirmation。

## Logging

Runner mode 配置独立 file logging，不依赖 `agentdash-api` diagnostic buffer。

默认 log：

- Linux：`/var/log/agentdash/runner.log`
- Windows：`%PROGRAMDATA%\AgentDash\runner\runner.log`

日志脱敏覆盖：

- `token`
- `access_token`
- `refresh_token`
- `auth_token`
- `registration_token`
- Bearer header
- URL query token values

Use `diag!` for process diagnostics with `Subsystem::Relay` for relay lifecycle and infra/config subsystem for claim/config/service/filesystem operations.

## Status Snapshot

Runner writes `runner-status.json` under state dir. `status --json` merges platform service manager state and last snapshot.

Fields：

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
- `relay_ws_url` redacted
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

If service is running but status file mtime is older than threshold, report `status_stale`。

## Security And Permissions

- Runner opens only outbound HTTP(S)/WS(S)。
- Config/credential files are secrets。
- Workspace execution runs as OS service identity; install docs must tell operators to grant that identity access to configured roots。
- Windows LocalSystem default is powerful and does not inherit desktop user mapped drives/SSH/Git credentials; first implementation must either document this sharply or support `--user` / service account configuration。

## Handoff Dependencies

From `runner-enrollment-token`：

- claim endpoint path。
- request/response DTO。
- fatal vs retryable error categories。
- backend identity algorithm。
- ProjectBackendAccess side effect。

To `runtime-diagnostics-settings`：

- `status --json` schema。
- log tail path/format。
- relay connection states。
- registration source field。

To `distribution-release-validation`：

- Linux/Windows service commands。
- service names。
- config/state/log/status paths。
- version command。
- uninstall/preserve boundary。
