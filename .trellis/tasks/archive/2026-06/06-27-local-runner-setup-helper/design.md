# Design

## Architecture

`agentdash-local setup` 是现有 runner primitives 上的一层 orchestration command：

```text
setup
  -> resolve defaults / embedded defaults
  -> collect missing input interactively
  -> write runner config
  -> claim registration token
  -> persist credentials
  -> optional service install
  -> optional service start
  -> status/doctor summary
```

它复用现有模块：

- `runner_config`：config schema、default paths、config precedence、atomic write。
- `runner_claim`：registration token claim。
- `runner_service`：systemd / Windows SCM install/start/status。
- `runner_status`：snapshot、rendering、redaction。
- `machine_identity`：claim 使用的 machine identity。

## CLI Surface

Primary command:

```text
agentdash-local setup
  [--config <path>]
  [--server-url <url>]
  [--registration-token <token>]
  [--runner-name <name>]
  [--workspace-root <path>...]
  [--executor-enabled | --no-executor]
  [--install-service]
  [--start]
  [--dry-run]
  [--json]
  [--non-interactive]
```

Recommended cloud-generated command:

```bash
sudo agentdash-local setup \
  --server-url https://agentdash.example.com \
  --registration-token adrt_... \
  --runner-name linux-runner-01 \
  --workspace-root /srv/agentdash/workspaces \
  --install-service \
  --start
```

Diagnostics command:

```text
agentdash-local doctor [--config <path>] [--json]
```

`doctor` is a read-only diagnostics command: it reads config/status/service and performs lightweight checks without config writes, credential claim, or service lifecycle actions.

## Interactive Flow

When stdin/stdout are TTY and `--non-interactive` is not set, `setup` prompts for missing values:

1. Server URL
2. Registration token
3. Runner name
4. Workspace roots
5. Executor enabled
6. Install as service
7. Start service now

Each prompt displays the default source when available:

```text
Cloud server URL [embedded: https://agentdash.example.com]:
Runner name [default: agentdash-runner]:
Workspace root [empty allowed]:
Install as service? [Y/n]:
Start service now? [Y/n]:
```

Token input should avoid echo when feasible. If cross-platform no-echo support is too expensive for the first implementation, the prompt must label it clearly and immediately redact the value from output/logs.

## Embedded Defaults

Embedded defaults are non-secret packaging hints compiled into the runner binary. They solve the “what server URL do I type?” problem for environment-specific runner downloads.

Proposed build-time inputs:

```text
AGENTDASH_RUNNER_DEFAULT_SERVER_URL
AGENTDASH_RUNNER_DEFAULT_NAME_PREFIX
AGENTDASH_RUNNER_DEFAULT_WORKSPACE_ROOT
```

Rust access pattern:

```rust
option_env!("AGENTDASH_RUNNER_DEFAULT_SERVER_URL")
```

Precedence:

```text
CLI > environment > config file > embedded default > platform default
```

Security boundary:

- Allowed: server URL, name prefix, workspace root suggestion.
- Secret-bearing runtime facts stay outside embedded defaults: registration token, relay auth token, backend id, user access token.
- Embedded defaults must appear in `setup --dry-run` and `doctor` summaries as source `embedded`.

Official generic runner binary can ship with no embedded server URL. Cloud-hosted or customer-specific download artifacts can embed the production origin so setup becomes shorter:

```bash
agentdash-local setup --registration-token adrt_... --install-service --start
```

## Packaging Policy

- Generic runner release artifact ships without an embedded `server_url`, because generic binaries must work across development, private deployment and cloud-hosted environments.
- Cloud download pages and customer/environment-specific artifacts may compile `AGENTDASH_RUNNER_DEFAULT_SERVER_URL` into the binary, because their target server origin is already known by the packaging flow.
- Embedded values are convenience defaults only. CLI, environment and config file values keep higher priority so copied commands and operator-managed config remain authoritative.

## Config Write Strategy

`setup` should write the config before claim with:

```toml
[runner]
name = "linux-runner-01"
server_url = "https://agentdash.example.com"
workspace_roots = ["/srv/agentdash/workspaces"]
executor_enabled = true
log_path = "/var/log/agentdash/runner.log"
state_dir = "/var/lib/agentdash/runner"

[registration]
token = "adrt_..."
```

After claim, `persist_credentials` writes:

```toml
[credentials]
backend_id = "..."
relay_ws_url = "..."
auth_token = "..."
claimed_at = "..."
token_source = "runner_registration_token"
```

Registration token retention after successful claim remains an implementation decision inside this task. The first implementation should keep operator expectations explicit in `doctor`: show whether an enrollment token is present, redacted, and whether complete server-issued relay credentials are available.

## Summary Output

Human summary:

```text
AgentDash Local Runner setup complete

server:     https://agentdash.example.com
runner:     linux-runner-01
backend_id: be_xxx
config:     /etc/agentdash/runner.toml
log:        /var/log/agentdash/runner.log
service:    installed, running
relay:      connecting
```

JSON summary:

```json
{
  "ok": true,
  "config_path": "/etc/agentdash/runner.toml",
  "server_url": "https://agentdash.example.com",
  "runner_name": "linux-runner-01",
  "backend_id": "be_xxx",
  "service": { "state": "running" },
  "claim": { "state": "success" },
  "relay": { "state": "connecting" }
}
```

All token-bearing fields must be omitted or redacted.

## Error Handling

- Missing server URL in non-interactive setup returns configuration error with remediation.
- Missing registration token in non-interactive setup returns configuration error with remediation.
- Claim 401/403 reports token invalid/expired/revoked without printing the token.
- Service install/start failures include OS command context with token redaction.
- Existing config with complete credentials can skip claim unless `--force-claim` is later introduced.

## Sub-Agent Handoff

- `trellis-implement` owns Rust implementation under `crates/agentdash-local/src` and related runner tests.
- Main session owns Trellis specs/task docs, release checklist updates, final integration review and commits.
- `trellis-check` runs after implementation for focused verification against `check.jsonl`, then may self-fix scoped documentation/test drift.

## Tests

- CLI parser tests for `setup` and `doctor`.
- Prompt planner tests using fake input/output.
- Embedded default precedence tests.
- Dry-run tests asserting no config write, no claim, no service mutation.
- Summary redaction tests.
- Service orchestration tests with fake service executor.
