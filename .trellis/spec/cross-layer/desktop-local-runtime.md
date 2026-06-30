# Desktop Local Runtime

Tauri 桌面端把 Web Dashboard、本机 runtime 管理面板和桌面壳能力收敛在同一个应用进程中。本文档约束跨 Rust/Tauri/React 的 command、HTTP authority、profile 和打包入口。

## Scope

- `agentdash-local-tauri` 作为薄壳持有 `LocalRuntimeManager`，通过 Tauri command 暴露 runtime/profile/MCP/log 操作。
- Dashboard 不直接访问 Rust 内存态；默认通过远端 cloud HTTP API 访问业务数据。Local Runtime 设置面板才通过 Tauri `invoke()` 访问本机 runtime manager。

## 关键类型

- **Rust**：`ApiServerOptions`（`agentdash-api/src/lib.rs`）+ `build_server()` / `build_server_with_migrations()` 可复用入口
- **Tauri commands**：profile / runtime / logs / MCP / open_external_url（定义在 `agentdash-local-tauri`）
- **TS port**：`LocalRuntimeClient`（`@agentdash/core`），Tauri 适配层实现 `invoke()` 绑定

## 核心约束

### API 与 Dashboard

- Desktop release bundle 默认使用 `external` API mode，并通过 `AGENTDASH_DEFAULT_CLOUD_ORIGIN` / `--api-origin` 指向远端 server，原因是桌面安装包的业务数据、登录、项目和 runner enrollment 权威事实属于 cloud server。
- Builtin Desktop API 是显式 opt-in 的本机 API host，origin 固定为 `http://127.0.0.1:17301`，原因是它只适合未来本地部署/实验形态，不属于默认桌面发行路径。
- Sidecar Desktop API 仍必须使用 loopback origin，原因是 sidecar 是本机 API 进程，不以 localhost 作为认证边界向 LAN 暴露。
- `desktop_api_snapshot` 的 `state` 只能是 `starting | running | error | stopped`
- DashboardHost 必须先确认 `/api/health` ready 后才渲染 Web Dashboard
- `packages/app-web` 只导出 `App`，`app-tauri` 复用该入口，不能复制组件树

### 机器身份

- 机器级身份由 `agentdash-local` runtime library 负责识别、生成和持久化
- Tauri / dev scripts 只能调用 local library 或 `agentdash-local machine-identity` 获取
- `backend_id`、`relay_ws_url` 和 relay token 必须来自 server ensure/claim 响应
- standalone `agentdash-local` 入口必须显式接收已领取的 `backend_id`，原因是本机 runtime 只能消费 server claim 结果，不能在本地创建正式 backend identity
- server ensure API 使用 `machine_id + share_scope_kind + share_scope_id + capability_slot` 定位 local backend，原因是机器级身份与共享 scope 共同决定本机执行面的唯一归属
- `machine_label` / hostname 只用于展示；profile load/save/start 都由 `agentdash-local` 持久化身份覆盖 canonical machine id
- profile 保存 profile id、workspace roots、backend claim 结果和启动偏好；desktop embedded runner 的 enrollment origin 来自当前 Desktop Dashboard API origin，机器身份事实由 `agentdash-local machine-identity` 独立持有
- `scripts/dev-joint.js` 必须复用同一条 ensure/claim 协议，通过 `agentdash-local machine-identity` 读取身份

## Scenario: Runner Registration Token Enrollment

### 1. Scope / Trigger

- Trigger: 独立 Local Runner 在无 UI 服务器场景中不能保存用户 access token，需要用云端生成的 registration token 领取正式 backend relay 凭据。
- Scope: Project-scoped runner token 管理 API、public runner claim API、`runner_registration_tokens` PostgreSQL 表、`BackendConfig` project-scope ensure、`ProjectBackendAccess` 授权、generated backend contracts。

### 2. Signatures

Management API under secured project routes:

```text
POST /api/projects/{project_id}/runner-registration-tokens
GET  /api/projects/{project_id}/runner-registration-tokens
POST /api/projects/{project_id}/runner-registration-tokens/{token_id}/revoke
POST /api/projects/{project_id}/runner-registration-tokens/{token_id}/rotate
```

Public runner claim route:

```text
POST /api/local-runtime/runner/claim
Authorization: Bearer adrt_<token_id>_<secret>   # optional when body carries registration_token
```

Claim request / response contract:

```rust
pub struct RunnerRegistrationClaimRequest {
    pub registration_token: Option<String>,
    pub machine_id: String,
    pub machine_label: Option<String>,
    pub runner_name: Option<String>,
    pub client_version: Option<String>,
    pub device: serde_json::Value,
    pub executor_enabled: bool,
    pub capability_slot: Option<String>,
}

pub struct RunnerRegistrationClaimResponse {
    pub backend_id: String,
    pub name: String,
    pub relay_ws_url: String,
    pub auth_token: String,
    pub machine_id: String,
    pub machine_label: String,
    pub share_scope_kind: BackendShareScopeKind,
    pub share_scope_id: Option<String>,
    pub capability_slot: String,
    pub registration_source: String,
    pub claimed_at: chrono::DateTime<chrono::Utc>,
}
```

Database table:

```sql
runner_registration_tokens(
  id text primary key,
  project_id text not null references projects(id) on delete cascade,
  name text not null,
  token_secret_hash text not null,
  token_prefix text not null,
  created_by_user_id text not null,
  expires_at timestamptz not null,
  revoked_at timestamptz null,
  last_used_at timestamptz null,
  last_claimed_backend_id text null,
  default_capability_slot text not null default 'default',
  machine_policy jsonb not null default '{}'::jsonb,
  created_at timestamptz not null,
  updated_at timestamptz not null
)
```

### 3. Contracts

- Registration token plaintext format is `adrt_<token_id>_<secret>`. Only create and rotate responses return the plaintext token once.
- Metadata/list/revoke responses return `token_prefix`, status, scope and usage metadata; they never return plaintext token, `token_secret_hash`, backend relay `auth_token`, or Authorization header contents.
- Claim route is public in the HTTP router and does not extract `CurrentUser`. It authenticates only with the runner registration token from body `registration_token` or `Authorization: Bearer`.
- Desktop ensure and runner claim share one application use case `enroll_local_backend(backend_repo, source, req)` with `EnrollmentSource = DesktopAccessToken{user_id} | RunnerRegistrationToken{project_id, created_by_user_id}`. The use case is the single owner of stable backend id derivation, relay `auth_token` issue/reuse (`generate_backend_auth_token`), device metadata projection, and `registration_source` write. The two HTTP routes are thin auth adapters; only their authentication source differs.
- **Runner backend identity is machine-level, NOT project-scoped.** `stable_local_backend_id` excludes `project_id` for runners; claim lands `share_scope_kind=user`, `share_scope_id=token.created_by_user_id` (owner), `visibility=shared`, `profile_id=runner-registration`, `registration_source=runner_registration_token`. The same `machine_id + capability_slot` yields ONE stable `backend_id` regardless of which project claims it. Returned `backend_id` and relay `auth_token` come from the server backend ensure path.
- **`ProjectBackendAccess` is the authoritative project→backend grant.** Claim ensures active `ProjectBackendAccess(project_id, backend_id)` through the shared application grant use case; this row — not a project-baked `share_scope` — is what makes a runner visible to a project. One machine-level runner backend can be granted to N projects via N grant rows. The same grant use case is used by Project Settings manual authorization, so create/reactivate/conflict handling and policy/note updates remain one lifecycle.
- Authorization: a `user`-scoped backend is accessible to its owner (unchanged); additionally, if it has an active `ProjectBackendAccess` grant for project P, members of P are allowed per their project permission (`user_scoped_grant_allows`, with batched grant prefetch in the list path). A user-scoped backend with no grant stays owner-only — desktop personal backends do not regress.
- Desktop ensure response is isomorphic to runner claim on the shared relay-credential + identity core: `EnsureLocalRuntimeResponse` carries `registration_source` (= `desktop_access_token`) and `claimed_at`, in addition to the desktop-only `backend_enabled` / `profile_id`.
- `/ws/backend` continues to authenticate only with backend relay `auth_token`; a registration token is not a relay token.

### 4. Validation & Error Matrix

| Condition | Required behavior |
| --- | --- |
| Missing registration token | 401 |
| Malformed token, unknown token id, or secret hash mismatch | 401 with the same invalid-token class |
| Expired token | 401 with expired-token class |
| Revoked token | 403 with revoked-token class |
| Invalid machine/device payload | 400 |
| Project scope unavailable | 403 without exposing unrelated project facts |
| ProjectBackendAccess concurrent create conflict | Re-check active access, otherwise 409 |
| Database/internal failure | 500 with fixed internal message |
| Registration token used against `/ws/backend` | Reject as invalid backend auth token |

### 5. Good/Base/Bad Cases

- Good: Linux runner starts with `adrt_...`, claims once, stores `backend_id + relay_ws_url + auth_token`, then connects to `/ws/backend` with the returned backend auth token.
- Good: Same machine restarts and repeats claim with the same capability slot; backend id and relay token are reused unless a later explicit rotate path changes backend auth.
- Good: One machine claims with project A's token, later with project B's token (same slot). It resolves to the SAME machine-level `backend_id`; two `ProjectBackendAccess` rows (A and B) grant it to both projects. No second backend record is created.
- Base: Token metadata list shows `token_prefix`, `last_used_at` and `last_claimed_backend_id` for operator diagnostics.
- Bad: Runner stores a user access token or sends registration token to `/ws/backend`; this mixes enrollment with runtime relay auth and breaks revocation/audit boundaries.

### 6. Tests Required

- Domain tests assert token plaintext parse/build, secret hash verify, status ordering `revoked > expired > active`, and no plaintext storage in token entity persistence.
- Repository tests assert create/list/get/revoke/record_usage roundtrip and `token_secret_hash` is persisted while plaintext is not.
- Application tests assert claim success, repeated claim idempotency, expired/revoked/invalid token failures, machine-level (project-independent) `backend_id`, user-scope owner fields + `registration_source` on both desktop and runner paths, active `ProjectBackendAccess` side effect, and the four authorization cases (owner allowed / non-owner no-grant denied / non-owner active-grant allowed / revoked denied) on both single and list paths.
- API tests assert management routes require project edit permission, public claim route does not require `CurrentUser`, and claim error responses map to stable HTTP status classes.
- Relay regression tests assert registration token cannot authenticate `/ws/backend`, returned backend `auth_token` can, and backend id mismatch is rejected.
- Contract check asserts runner token DTOs remain generated in `backend-contracts.ts`.

### 7. Wrong vs Correct

#### Wrong

```text
runner -> /ws/backend?token=adrt_<token_id>_<secret>
```

#### Correct

```text
runner -> POST /api/local-runtime/runner/claim with adrt_<token_id>_<secret>
server -> { backend_id, relay_ws_url, auth_token }
runner -> /ws/backend?token=<backend auth_token>
```

## Scenario: Headless Local Runner CLI, Config, And Status

### 1. Scope / Trigger

- Trigger: 独立 `agentdash-local` runner 需要作为服务器守护进程运行，入口必须把 enrollment、relay 凭据、配置合并、状态诊断和服务管理命令稳定下来。
- Scope: `agentdash-local` CLI、TOML config、environment variables、runner claim client、credentials write-back、status snapshot、service command plan。

### 2. Signatures

CLI commands:

```text
agentdash-local run [--config <path>] [--server-url <url>] [--registration-token <adrt_...>] [--backend-id <id>] [--relay-ws-url <url>] [--auth-token <token>] [--runner-name <name>] [--workspace-root <path>...]
agentdash-local setup [--config <path>] [--server-url <url>] [--registration-token <adrt_...>] [--runner-name <name>] [--workspace-root <path>...] [--install-service] [--start] [--dry-run] [--json] [--non-interactive]
agentdash-local doctor [--config <path>] [--json]
agentdash-local status [--config <path>] [--json]
agentdash-local service install [--config <path>]
agentdash-local service uninstall [--config <path>]
agentdash-local service start [--config <path>]
agentdash-local service stop [--config <path>]
agentdash-local service status [--config <path>]
agentdash-local machine-identity
```

Runner claim call:

```text
POST {server_url}/api/local-runtime/runner/claim
Authorization: Bearer adrt_<token_id>_<secret>
Content-Type: application/json
```

Status snapshot file:

```text
{state_dir}/runner-status.json
```

### 3. Contracts

- Config precedence is `CLI > environment > config file > embedded default > platform default`; this keeps service units deterministic while allowing one-off debugging overrides and environment-specific release artifacts.
- Environment keys use the `AGENTDASH_RUNNER_` prefix for runner concerns: `CONFIG`, `SERVER_URL`, `REGISTRATION_TOKEN`, `BACKEND_ID`, `RELAY_WS_URL`, `AUTH_TOKEN`, `RUNNER_NAME`, `STATE_DIR`, `LOG_PATH`, `WORKSPACE_ROOTS`, and `EXECUTOR_ENABLED`.
- Embedded default keys are limited to non-secret packaging hints: `AGENTDASH_RUNNER_DEFAULT_SERVER_URL`, `AGENTDASH_RUNNER_DEFAULT_NAME_PREFIX`, and `AGENTDASH_RUNNER_DEFAULT_WORKSPACE_ROOT`. Generic runner artifacts can omit them; cloud-hosted or customer-specific artifacts can compile the target server origin into the binary.
- `setup` is the canonical first-install orchestration path. It resolves defaults, collects missing inputs, writes runner config, performs registration-token claim, persists server-issued credentials, optionally installs/starts the OS service, and prints a redacted summary.
- `setup --dry-run` reports the planned config path, service actions and missing fields without config writes, claim calls, or service lifecycle actions.
- `doctor` is a read-only diagnostics path. It checks config readability, credential/enrollment presence, service state, status snapshot freshness, log path writability and lightweight server reachability while keeping token-bearing values redacted or omitted.
- `registration_token` is an enrollment credential. It is only sent to `/api/local-runtime/runner/claim`; WebSocket relay authentication uses the returned `auth_token`.
- Successful claim writes `backend_id`, `relay_ws_url`, `auth_token`, and registration metadata back to the TOML config through an atomic replace so service restart does not re-require a plaintext token in the environment.
- `status --json` reports local configuration and latest snapshot facts without starting an HTTP server or binding an inbound business port.
- Log and status output redacts token-bearing fields and bearer/query token fragments before displaying operator diagnostics.
- `service` subcommands own OS service integration. Linux uses systemd; Windows uses SCM with `agentdash-local service run --config ...` as the native service entrypoint. Unsupported platforms must return an explicit unsupported response and must not imply a service is installed when no OS registration happened.

### 4. Validation & Error Matrix

| Condition | Required behavior |
| --- | --- |
| Missing `backend_id + relay_ws_url + auth_token` and no `registration_token` | `run` returns configuration error before opening WebSocket |
| `registration_token` present but claim endpoint returns 401/403 | `run` reports enrollment failure and does not persist relay credentials |
| Claim succeeds but config write-back fails | `run` reports persistence failure before treating enrollment as durable |
| Config file contains malformed TOML | command returns config parse error with path context |
| Environment and CLI both provide the same field | CLI value wins |
| `status --json` without snapshot | returns configured identity plus disconnected/unknown runtime state |
| Service install unsupported on current platform | returns explicit unsupported status and no OS mutation |
| Token appears in config, URL query, JSON, or bearer header | operator-facing output prints redacted value |

### 5. Operational Rationale

- Linux first boot can provide only `AGENTDASH_RUNNER_SERVER_URL` and `AGENTDASH_RUNNER_REGISTRATION_TOKEN`; runner claims once, writes relay credentials, and later service restarts can use the persisted server-issued relay auth token.
- Server install instructions should prefer `agentdash-local setup --registration-token ... --install-service --start`; explicit config editing and `service install/start` remain useful diagnostic and recovery steps.
- Windows service installation records a stable SCM command that points to `agentdash-local service run --config ...`; SCM stop/shutdown maps to the runner shutdown signal so WebSocket and status snapshots stop gracefully.
- `agentdash-local status --json` gives support scripts and cloud diagnostics a stable local status surface without adding an inbound HTTP health port.
- Enrollment and relay authentication stay separate because registration-token revocation, backend relay auth rotation, and ProjectBackendAccess auditing have different lifecycles.
- Canonical flow: `registration_token -> claim -> persisted backend relay credentials -> WebSocket connect`.

### 6. Tests Required

- CLI tests assert command parsing for `setup`, `doctor`, `run`, `status`, `service *`, and `machine-identity`.
- Config tests assert precedence order, TOML roundtrip, embedded defaults, environment key parsing, workspace root parsing, and missing credential validation.
- Claim tests assert request path, Authorization header, response mapping into runtime config, and atomic credentials write-back.
- Redaction tests assert JSON fields, bearer headers, query params, and common token names are masked.
- Setup tests assert dry-run has no config, claim or service mutation and setup summaries omit token-bearing values.
- Doctor tests assert read-only human/JSON output shape with missing config, partial enrollment, complete credentials and stale snapshot cases.
- Service tests assert Linux systemd command execution through an executor abstraction, Windows SCM command formation, native service run entrypoint parsing, status mapping, and no accidental OS mutation in dry/unit contexts.
- Status tests assert JSON output shape with and without a snapshot file.

### 7. Canonical Flow

```text
agentdash-local setup --registration-token adrt_... --install-service --start
claim response -> backend_id + relay_ws_url + auth_token
runner connects to relay_ws_url with auth_token
```

## Scenario: Desktop Tray, Background Runtime, And Settings Bridge

### 1. Scope / Trigger

- Trigger: Windows desktop full installer needs behave like a resident desktop app: close-to-tray, explicit quit, runtime lifecycle menu, startup preferences, and a stable frontend bridge.
- Scope: `agentdash-local-tauri` tray/menu/window lifecycle, Tauri commands, `desktop-app-settings.json`, `packages/app-tauri` desktop bridge, cloud API origin packaging, and explicit opt-in Desktop API hosting at loopback port `17301`.

### 2. Signatures

Tauri commands:

```rust
async fn desktop_settings_load() -> Result<DesktopAppSettings, String>;
async fn desktop_settings_save(settings: DesktopAppSettings) -> Result<DesktopAppSettings, String>;
async fn desktop_autostart_is_enabled() -> Result<DesktopAutostartStatus, String>;
async fn desktop_autostart_set_enabled(enabled: bool) -> Result<DesktopAutostartStatus, String>;
async fn desktop_quit_request(app: tauri::AppHandle, state: tauri::State<'_, DesktopState>) -> Result<(), String>;
```

Embedded runner host:

```rust
pub struct DesktopRunnerHost;

impl DesktopRunnerHost {
    pub async fn ensure_started_with<F, Fut>(&self, build_config: F) -> anyhow::Result<LocalRuntimeSnapshot>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = anyhow::Result<LocalRuntimeConfig>>;

    pub async fn stop(&self, reason: StopReason) -> anyhow::Result<()>;
    pub async fn restart(&self) -> anyhow::Result<LocalRuntimeSnapshot>;
    pub async fn snapshot(&self) -> Option<LocalRuntimeSnapshot>;
}
```

Frontend bridge:

```ts
window.__AGENTDASH_DESKTOP_APP__ = {
  loadSettings(): Promise<DesktopAppSettings>
  saveSettings(settings: DesktopAppSettings): Promise<DesktopAppSettings>
  getAutostartStatus(): Promise<DesktopAutostartStatus>
  setAutostartEnabled(enabled: boolean): Promise<DesktopAutostartStatus>
  quit(): Promise<void>
}
```

Settings file:

```text
{app_config_dir}/desktop-app-settings.json
```

Desktop release build:

```text
pnpm run desktop:bundle -- --desktop-defaults desktop-defaults.json
```

Default cloud origin:

```text
AGENTDASH_DEFAULT_CLOUD_ORIGIN=https://agentdash.example.com
```

Desktop defaults JSON:

```json
{
  "default_cloud_origin": "https://agentdash.example.com"
}
```

### 3. Contracts

- Closing the main window hides it to tray by default; the process continues so a running local runtime is not interrupted by an ordinary window close.
- Explicit quit is a distinct command path. It sets an in-process quit flag, stops the managed local runtime, cleans sidecars, and then exits the Tauri process.
- Tray menu exposes `Open AgentDash`, runtime start/stop/status actions, and explicit quit. Runtime start uses the saved profile; it does not silently create a profile when none exists.
- `start_minimized_to_tray` controls first window visibility after setup. When false, setup shows/focuses the main window after Desktop API initialization; when true, the tray-resident process stays hidden.
- `launch_at_login`, `start_minimized_to_tray`, and `auto_connect_local_runtime` persist in `desktop-app-settings.json` and are surfaced through the frontend bridge.
- Default desktop packaging uses `external` API mode. `AGENTDASH_DEFAULT_CLOUD_ORIGIN`, `--api-origin`, or `--default-cloud-origin` must provide the remote server origin that the dashboard uses for business HTTP API calls.
- `--desktop-defaults` carries a non-secret JSON defaults file into the desktop frontend bundle as `agentdash-desktop-defaults.json`; the desktop frontend reads this file at runtime before creating an auto-connect profile.
- `--default-cloud-origin` is a shortcut that produces the same carried `default_cloud_origin` value without hand-writing a defaults file.
- `default_cloud_origin` 在没有单独 `--api-origin` 时作为 packaged Dashboard API origin。desktop embedded runner 的 Local Runtime profile server URL 会被规范化为当前 Dashboard API origin；`default_cloud_origin` 不创建 `backend_id`，也不嵌入 access/registration/relay token。
- Autostart commands return `DesktopAutostartStatus { supported, enabled, message }`; the UI must treat `supported=false` as a product capability state, not as a command failure.
- Builtin Desktop API remains loopback-only at `127.0.0.1:17301` when explicitly selected; its presence does not change local runtime/runner WebSocket relay communication.
- Tauri registers the single-instance lifecycle plugin before `.manage(...)` and `.setup(...)`, so a second desktop launch forwards to the first process to restore/focus the main window while the first process remains the only Desktop API and embedded runner owner.
- `DesktopState` holds `agentdash-local::DesktopRunnerHost` as the embedded runner host. Tauri commands only adapt command payloads, shell-selected Dashboard API origin, retry status updates and `Result<_, String>` errors; `agentdash-local` owns desktop profile/request normalization, profile/settings file IO, desktop access-token ensure, response validation, `LocalRuntimeConfig` projection, runtime reuse, serialized start, stop, restart, snapshot and logs because packaged desktop and standalone local runner share the same execution surface.
- `DesktopRunnerHost::ensure_started_with` serializes config construction and runtime start in one critical section. Existing `starting` or `running` snapshots are returned as-is; stopped or failed handles are cleaned before a new config is built, so repeated tray, settings, and web auto-connect requests converge to one claim/start path.
- Desktop runner snapshot state is `idle | disabled | waiting_for_auth | waiting_for_api | claiming | starting | running | retrying | error | stopping | stopped`. The host uses `idle/disabled/waiting_*` before a runtime handle exists, `claiming` while calling `/api/local-runtime/ensure`, and projects relay reconnects as `retrying`, so settings UI can explain both supervisor and relay phases without parsing logs.
- `LocalRuntimeStatus.owner = "desktop_embedded_runner"` and `registration_source = "desktop_access_token"` for the desktop embedded host. Standalone service runner rows remain identified by backend projection `registration_source = "runner_registration_token"`, so UI can keep lifecycle owner and enrollment source separate.
- `DesktopAppSettings.auto_connect_local_runtime` is the global desktop auto-connect gate. `LocalRuntimeProfile.auto_start` marks whether the saved profile should participate in native startup. Automatic native startup requires both and then waits for Web bridge to report that the Dashboard has a current user; profile persistence keeps startup configuration facts such as workspace roots and `executor_enabled`, not bearer credentials. Manual start/retry/tray commands still call the same host service.
- `external` Desktop API mode only chooses the Dashboard API origin. It does not disable the embedded desktop runner host, because cloud API authority and local execution lifecycle are separate facts.
- Web Dashboard auto-connect is a request bridge, not lifecycle ownership. It uses current-user availability as the authenticated intent gate, passes the current bearer token only when one exists, reuses an in-flight promise, and uses bounded retry for transient native/API readiness failures. If the Dashboard has a current user but no bearer token, the bridge still calls the same native ensure path so `/api/local-runtime/ensure` can either accept another configured auth source or return an actionable auth/API error in the native snapshot.
- Desktop embedded runner enrollment origin follows the current Desktop Dashboard API origin. In release external builds that origin is provided by `AGENTDASH_DEFAULT_CLOUD_ORIGIN` / `--api-origin`; in `pnpm dev:desktop` it is provided by `VITE_API_ORIGIN` and `AGENTDASH_DESKTOP_API_ORIGIN` pointing at the local dev `agentdash-server`. Frontend defaults resolve Local Runtime server URL as `API_ORIGIN -> default_cloud_origin -> 127.0.0.1:17301`, and Tauri normalizes profile/start request `server_url` to `desktop_api_config().origin`; persisted profile values never override the current Dashboard API origin.
- Auto-connect failures are surfaced through native runtime snapshot/logs. Browser console output from the bridge should not include caught error objects because errors may contain request context or token-bearing diagnostics.

### 4. Validation & Error Matrix

| Condition | Required behavior |
| --- | --- |
| User clicks window close | prevent close, hide window, keep process alive |
| User chooses explicit tray quit or frontend quit command | set quit flag, stop runtime, clean sidecars, exit |
| Runtime start requested from tray without saved profile | log/report clear no-profile status, do not fabricate backend identity |
| Settings file missing | load default settings |
| Settings file malformed | return settings load error instead of silently discarding operator intent |
| Windows autostart enabled | write HKCU Run value for installed/current app exe, persist `launch_at_login=true`, and reject setup/installer exe paths |
| Autostart unsupported on non-Windows platform | return `supported=false`, stable message, and no OS registry/service mutation |
| Desktop API port unavailable | app reports Desktop API error state; Dashboard waits for `/api/health` before rendering |
| Second desktop instance launched | restore/focus the existing main window and keep the original process as the only Desktop API/runtime owner |
| Runtime ensure requested concurrently | serialize claim/config/start and return the active `starting` or `running` snapshot to later callers |
| Auto-connect has no current user | wait for login availability, clear retry state, and skip profile writes or runtime claim |
| Auto-connect has current user but no bearer token | call the native ensure path with an empty bearer field; if the server rejects it, snapshot/logs report an auth/API error rather than staying in `waiting_for_auth` |
| Native startup has `auto_connect_local_runtime=false` | report `state=disabled` and do not claim until an explicit manual start/retry request arrives |
| Native startup has no auto-start profile | report `state=idle` and wait for login bridge/profile save or manual start |
| Profile contains an old remote or old development `server_url` while Dashboard API origin changed | normalize saved profile and runtime start request to the current Desktop Dashboard API origin |
| Auto-connect transient failure | schedule bounded retry while leaving diagnostics in runtime snapshot/logs |
| Auto-connect disabled or bridge unavailable | return without scheduling retry |

### 5. Operational Rationale

- Close-to-tray preserves the Tauri process so ordinary window management does not interrupt long local runtime work.
- The tray `Quit AgentDash` item performs an intentional shutdown and stops the managed runtime before process exit.
- `start_minimized_to_tray=true` gives a resident launch path for users who want AgentDash available without opening the dashboard.
- Window visibility and process lifetime are separate because background execution should not depend on whether the dashboard surface is visible.
- Single-instance ownership keeps Desktop API port binding, tray ownership, and embedded runner claim/start state in one process, which makes package launch behavior match the user expectation of one resident desktop app.
- The embedded host boundary keeps claim/start serialization next to `LocalRuntimeManager`, while Tauri stays focused on desktop lifecycle and the web app stays focused on intent and status display.
- Canonical auto-connect flow: authenticated Dashboard resolves current user -> desktop bridge ensures defaults/profile and passes the current bearer token when available -> Tauri command passes the shell-selected Dashboard API origin into `agentdash-local` -> `agentdash-local` normalizes the request, performs desktop ensure, projects `LocalRuntimeConfig`, and `DesktopRunnerHost` serializes start -> runtime snapshot/logs report outcome.
- Canonical origin flow: deployment or dev script selects Desktop Dashboard API origin -> Web Dashboard HTTP calls and desktop embedded runner ensure both use that origin -> relay credentials returned by that server decide the backend connection target.
- Canonical flow: window close hides; explicit quit exits.

### 6. Tests Required

- Rust tests assert settings default/load/save behavior and malformed file error behavior.
- Rust tests assert close request is prevented unless the explicit quit flag is set.
- Rust tests assert tray runtime actions call the existing runtime manager/profile path and do not create ad hoc identities.
- Rust checks assert Tauri single-instance plugin registration remains before setup-managed process initialization.
- Rust tests assert `DesktopRunnerHost` reuses `starting/running` snapshots and serializes concurrent ensure calls around config construction and runtime start.
- Rust tests assert Tauri profile/start normalization always uses current Desktop Dashboard API origin even when profile `server_url` contains an old remote origin.
- Rust tests assert Windows autostart command/value formation, setup exe rejection, and unsupported status shape on non-Windows platforms.
- TS typecheck asserts the desktop bridge contract is available to `app-tauri` without importing Tauri APIs into shared Web Dashboard components.
- Frontend tests assert Local Runtime defaults prefer current `API_ORIGIN` over packaged `default_cloud_origin`; auto-connect skips when current user is unavailable, still reaches native ensure when current user exists without a bearer token, normalizes old profile `server_url` to current Dashboard API origin, reuses in-flight start, retries bounded transient failures, and does not log caught error objects.
- Manual Windows validation asserts install, launch, close-to-tray, tray restore, runtime start/stop/status, explicit quit, and uninstall cleanup.
- Manual Windows validation asserts repeated double-click or login autostart plus manual launch leaves one desktop process and one embedded runner owner.

### 7. Canonical Lifecycle

```text
CloseRequested -> prevent default -> hide main window
desktop_quit_request/tray quit -> stop runtime -> app.exit(0)
Authenticated web intent -> runtime_start -> DesktopRunnerHost.ensure_started_with -> LocalRuntimeSnapshot
Desktop Dashboard API origin -> local-runtime/ensure origin -> relay credentials
```

## Scenario: Runtime Diagnostics Snapshot And Relay State

### 1. Scope / Trigger

- Trigger: 桌面设置页需要同时展示 Cloud API、Desktop API、Local Runtime、独立 Runner、WebSocket relay、registration 与日志状态，且不能从日志文本或 `backend.online` 猜测 relay 连接过程。
- Scope: `LocalRuntimeStatus.relay_connection`、`ws_client::RelayConnectionStatus`、`RuntimeDiagnosticsSnapshot` 前端 view-model、Desktop settings bridge、runner registration source projection。

### 2. Signatures

Rust desktop runtime snapshot:

```rust
pub struct LocalRuntimeStatus {
    pub state: LocalRuntimeState,
    pub owner: String,
    pub registration_source: Option<String>,
    pub backend_id: String,
    pub name: String,
    pub workspace_roots: Vec<String>,
    pub executor_enabled: bool,
    pub mcp_server_count: usize,
    pub message: Option<String>,
    pub last_error: Option<String>,
    pub last_attempt_at: Option<String>,
    pub next_retry_at: Option<String>,
    pub retry_count: Option<u32>,
    pub relay_connection: Option<ws_client::RelayConnectionStatus>,
}

pub struct RelayConnectionStatus {
    pub state: RelayConnectionState,
    pub target: Option<String>,
    pub last_connected_at: Option<String>,
    pub last_disconnected_at: Option<String>,
    pub last_error: Option<String>,
    pub retry_count: Option<u32>,
    pub next_retry_at: Option<String>,
    pub registered_backend_id: Option<String>,
}
```

Frontend diagnostics view-model:

```ts
type RuntimeDiagnosticsSnapshot = {
  generated_at: string
  cloud_api: ApiLayerStatus
  desktop_api: DesktopApiLayerStatus | null
  local_runtime: LocalRuntimeLayerStatus | null
  runner: RunnerLayerStatus | null
  relay_connection: RelayConnectionStatus | null
  registration: RuntimeRegistrationStatus | null
  logs: LocalLogEvent[]
  settings: DesktopRuntimeSettings | null
}
```

Backend `/backends` projection includes:

```ts
registration_source: string | null
```

### 3. Contracts

- `ws_client` writes `RelayConnectionStatus` at `connecting`、`registered`、`reconnecting`、`disconnected` lifecycle points through a watch channel owned by `LocalRuntimeManager`。
- `DesktopRunnerHost` writes supervisor facts into `LocalRuntimeStatus` even before `LocalRuntimeManager` has a running handle. `owner` and `registration_source` identify the desktop embedded host and desktop access-token enrollment path; `last_error`、`last_attempt_at`、`next_retry_at` and `retry_count` explain claim/API/auth retry phases.
- `registered_backend_id` is only set after relay `RegisterAck` confirms the backend registration. Reconnecting/disconnected snapshots preserve the last confirmed `last_connected_at` and `registered_backend_id` so the UI can show the last healthy relay fact.
- `target` and `last_error` must be redacted before entering local runtime snapshots or logs.
- Frontend `createRuntimeDiagnosticsSnapshot()` consumes structured facts only. It must not parse logs to infer state and must not treat `backend.online` as relay handshake state.
- Independent runner rows in diagnostics come from explicit `registration_source === "runner_registration_token"` or remote backend projection. A stopped desktop local backend with `desktop_access_token` must not be displayed as a service-managed Runner.
- Desktop settings controls use the Desktop App bridge (`desktop_settings_load/save`, autostart status commands). Generic Web Dashboard code does not import Tauri APIs directly.
- Logs copy/export uses the same redaction surface as display formatting and covers `token`、`access_token`、`refresh_token`、`auth_token`、`relay_token`、`registration_token` and Bearer credentials.

### 4. Validation & Error Matrix

| Condition | Required behavior |
| --- | --- |
| Relay starts connecting | `relay_connection.state = connecting`, retry count set, target redacted |
| Relay receives `RegisterAck` | `state = registered`, `last_connected_at` set, `registered_backend_id` set |
| Relay disconnects after successful registration | `state = reconnecting/disconnected`, last confirmed `last_connected_at` and `registered_backend_id` retained |
| Relay error includes token-bearing URL/header/message | snapshot/log/copy output redacts token-bearing values |
| Supervisor error includes token-bearing URL/header/message | `last_error` redacts token-bearing values before UI display |
| Runtime snapshot is absent | local runtime layer is `disabled`; UI may show runner projection separately |
| Backend has `registration_source=desktop_access_token` and runtime is stopped | UI does not classify it as independent runner |
| Backend has `registration_source=runner_registration_token` | UI may show it as service-managed/read-only runner |
| Desktop settings bridge unavailable | Web-only settings page hides desktop-only controls |

### 5. Good/Base/Bad Cases

- Good: Desktop runtime connects, receives `RegisterAck`, then network drops; UI shows Local Runtime process state plus relay reconnecting state with the last confirmed backend id.
- Good: Linux runner appears in cloud `/backends` with `registration_source=runner_registration_token`; desktop settings page shows it as service-managed and does not render desktop runtime restart controls for it.
- Base: No runtime snapshot exists in browser-only Web Dashboard; diagnostics omits Desktop API/settings controls and still shows Cloud API state.
- Bad: UI scans log text for “WebSocket 连接成功” to decide relay state; logs are diagnostics evidence, not a state fact source.
- Bad: UI treats any `backend_type=local` backend as independent runner; desktop local runtime and service runner have different lifecycle owners.

### 6. Tests Required

- Rust tests assert relay reconnect/disconnect snapshots preserve confirmed `last_connected_at` and `registered_backend_id` after registration.
- Rust tests assert runtime state transitions preserve `relay_connection` in `LocalRuntimeStatus`.
- Rust redaction tests cover Bearer, URL query, key-value, and JSON fields for `token/access_token/refresh_token/auth_token/relay_token/registration_token`.
- Frontend mapper tests assert runtime diagnostics do not infer relay state from logs or backend online state.
- Frontend mapper tests assert a stopped desktop-access-token local backend is not shown as an independent runner.
- Type checks assert `app-tauri` exposes Desktop App bridge fields without importing Tauri APIs into `packages/views` or shared Web components.

### 7. Wrong vs Correct

#### Wrong

```ts
const relayState = logs.some((line) => line.message.includes("注册成功"))
  ? "registered"
  : backend.online
    ? "registered"
    : "disconnected"
```

#### Correct

```ts
const relayState = localRuntimeSnapshot?.relay_connection?.state ?? "not_configured"
```

### Profile

- `agentdash-local::runtime_paths` 是本机 runtime 路径事实源；数据库、机器身份、extension artifact cache、runtime profile 和本机 MCP servers 配置都从同一个 `local-runtime` data root 派生，原因是这些文件共同服务本机后端生命周期，Tauri 壳只负责通过 command 调用本机 runtime。
- `LocalRuntimeProfile` 持久化在 `local-runtime/config/local-runtime-profile.json`（snake_case）。
- `DesktopAppSettings` 持久化在 `local-runtime/config/desktop-app-settings.json`（snake_case），由 `agentdash-local` 读写；Tauri autostart command 只负责 OS 登录项变更，并把变更后的 `launch_at_login` 写回 local settings API。
- 本机 MCP servers 配置持久化在 `local-runtime/config/local-mcp-servers.json`。
- 每次 profile load/save/start 都必须用 `agentdash-local` 机器身份覆盖 canonical machine id
- `access_token` 可以为空，server 在无 token 时通过自身认证 provider 解析当前用户
- `workspace_roots` 表达显式登记的 workspace root 集合；为空时不构成异常，也不限制本机目录浏览。执行类能力仍以 session `mount_root_ref` 为当前 workspace root 边界。
- 本机目录浏览是 setup 选择器能力，默认允许全盘浏览；workspace detect/register 成功后产生目录事实，session prompt / file tool / shell 才进入执行边界。

### Relay Prompt / Event Lifecycle

- Cloud relay connector 在发送 `command.prompt` 前注册 session event sink，原因是 local runtime 可以在 `response.prompt` 前推送 session notification 或 terminal state。
- Relay executor discovery 读取 backend registry 维护的在线 executor 快照；`AgentConnector::list_executors()` 是同步接口，不能在同步 discovery 路径里临时 `block_on` registry 的 async 状态查询。
- Backend registry 的 pending command 归属到具体 `backend_id`；backend 断连时释放该 backend 的 pending sender，让调用方立即收到 response dropped，而不是等待 command timeout。
- Cloud 侧用 `backend_execution_leases` 记录 relay session turn 对 backend 的执行占用。`runtime_health` 只表达连接健康，workspace inventory / binding 只表达目录事实，执行空闲/忙碌由 active lease 投影。
- Session launch 负责把 backend selection intent 解析成已 claim 的 backend execution placement，并把 `backend_id + lease_id + selection_mode` 放进 connector `ExecutionContext`。Relay connector 不再从 VFS mount 自行猜测执行 backend。
- Relay session sink 记录 `session_id -> backend_id + lease_id + sender`，原因是 cancel、terminal release 与 backend disconnect cleanup 都必须落到实际承载该 session 的 backend，而不是广播或重新猜测。
- Relay prompt 自动选择 backend 时先筛选在线且提供目标 executor 的 backend，再按 active lease count 升序与 backend_id 稳定排序；capacity / weight 不属于第一版调度输入。
- `/backends/runtime-summary` 是前端展示执行空闲/忙碌与可分配状态的汇总投影；该投影由 application 层合并 runtime health、backend registry executor snapshot 与 active backend execution leases，前端消费该投影，不从 runtime health 或 executor snapshot 自行推断。
- Local runtime 的 session notification forwarder 按 `session_id` 唯一运行；同一 relay session 的 follow-up prompt 复用现有 forwarder，保证同一条 session event 只有一个 relay 转发路径。
- Relay protocol 顶层信封保留在 `agentdash-relay/src/protocol.rs`；握手、心跳和 capability discovery payload 位于 `agentdash-relay/src/protocol/handshake.rs`，prompt / discovery / workspace / tool / VFS materialization / terminal / session event / MCP payload 位于 `agentdash-relay/src/protocol/` 对应子模块。顶层信封和子协议 payload 分离，原因是 wire format 必须集中稳定，而各子协议会按本机能力独立演进。

### Extension Artifact Cache

- `agentdash-local` 通过 local-runtime archive download API 获取 Project scoped extension package artifact；请求使用 backend relay bearer token，云端按 token 解析 backend 并校验 Project backend access。
- cache key 使用 `artifact_id + archive_digest`，原因是同一 artifact 重新发布或 digest 改变时必须形成新的本机缓存目录。
- 下载后必须校验 archive sha256 digest，再把 `.agentdash-extension.tgz` 解包到可清理 cache 目录。
- 解包只接受 archive 内相对普通文件路径；Extension Host 读取 cache 中的 package 内容，不在安装路径执行 npm/pnpm install 或 package lifecycle scripts。

### Local TS Extension Host

- `agentdash-local` 管理 Node-based extension host 子进程，通过 stdio JSON line 协议执行 activate / reload / invoke / health。
- Extension Host 内部位于 `agentdash-local/src/extensions/host/`，由 `manager.rs` 管理生命周期、`process.rs` 管理 Node stdio request-response、`protocol.rs` 定义 runner 消息、`permission_guard.rs` 执行 host API 权限裁决、`schema.rs` 执行 JSON Schema 子集校验、`runner/agentdash-extension-host-runner.mjs` 承载 JS runtime 源码，`runner.rs` 只负责 `include_str!` 嵌入，原因是本机插件执行、协议、权限、schema validation 和 runner 分发会独立演进。
- Extension bundle 作为 trusted local extension 在 Node runner context 中加载 self-contained ESM，原因是当前执行面使用本机 Node host 子进程承载插件代码；Host API facade 提供产品权限、协议稳定性与审计入口，不把 Node `vm` 作为不受信代码的安全隔离边界。
- `api.local.getProfile()` 由 Rust host API facade 返回 username、platform、arch、backend/project/session 与 workspace root 摘要，原因是本机 profile 是 local runtime 的事实源。
- Host API 运行时裁决使用当前 action 或 provider channel method 的 `permissions` 声明；manifest 顶层 capability 用于安装摘要、依赖解析、可用性诊断和审计，原因是当前插件执行模型是 trusted local extension，不把顶层 capability 重复做成 deny path。
- `ctx.api.runtime.invoke()` 优先调用当前 Project 已预加载 extension host 中注册的 runtime action；跨 extension action 调用要求当前 action 或 channel method 声明 `runtime.invoke:<action_key>` 或 `runtime.invoke`，并由 runner 限制 invocation depth，原因是 RuntimeGateway 已在 relay payload 中提供 Project enabled extension host surface，本机 runner 可以在同一 host process 内完成可信工具模型下的快速路由。
- Protocol channel 使用 canonical provider channel key 作为 projection、routing 和 trace 事实；runner 提供 `ctx.api.channels.self()` 与 dependency alias sugar，Gateway 和 local host 仍记录 canonical provider extension/channel/method，原因是 authoring 体验不应改变审计与依赖解析事实。
- packaged mode 直接消费 `ExtensionArtifactCacheEntry.unpacked_dir`，原因是 artifact cache 已完成 archive digest 校验与安全解包。
- action exception 和 host process exit 投影为 host 调用错误，原因是 extension host 故障应隔离在插件执行面内，保留 `agentdash-local` 主进程生命周期。
- Relay `command.extension_action_invoke` 进入本机 CommandHandler 后调用 TS Extension Host，原因是 RuntimeGateway 只拥有 action/trace/placement 意图，具体插件执行发生在 local runtime。
- Extension action/channel relay payload 携带 session workspace context 时，`root_ref` 来自当前 session VFS default mount；TS Extension Host 将它作为 workspace/process Host API 的默认 root，原因是插件执行目录必须跟随本次 session 的工作区事实。
- Relay payload 携带 package artifact 时，CommandHandler 先按 `artifact_id + archive_digest` 准备本机 cache，再用 extension key、backend id、project/session id、session workspace root 与 registered workspace roots 激活 TS Extension Host，原因是 packaged extension 的执行上下文由 Project 安装、session workspace 和本机登记事实共同确定。

## Scenario: Local Relay Command Routing And Extension Host API

### 1. Scope / Trigger

- Trigger: 本机 relay 命令同时覆盖 prompt、workspace、tool、materialization、MCP、extension 与 terminal；路由层必须保持薄入口，执行依赖由职责域 handler 持有。
- Scope: `LocalCommandRouter`、domain command handlers、extension action/channel relay payload、Extension Host workspace/process/env/http/runtime/channel Host API。

### 2. Signatures

```rust
pub struct LocalCommandRouterConfig {
    pub backend_id: String,
    pub workspace_roots: Vec<PathBuf>,
    pub tool_executor: ToolExecutor,
    pub session_runtime: Option<SessionRuntimeServices>,
    pub connector: Option<Arc<dyn AgentConnector>>,
    pub mcp_manager: Option<Arc<McpClientManager>>,
    pub workspace_contract_config: WorkspaceContractRuntimeConfig,
    pub extension_host: LocalExtensionHostManager,
    pub extension_artifact_api_base_url: String,
    pub extension_artifact_access_token: String,
    pub extension_artifact_cache_root: PathBuf,
    pub event_tx: mpsc::UnboundedSender<RelayMessage>,
}

pub async fn resolve_host_api(
    active: Option<&ActiveExtension>,
    api_key: &str,
    params: &serde_json::Value,
) -> Result<serde_json::Value, LocalExtensionHostError>;
```

### 3. Contracts

- `LocalCommandRouter` 只匹配 `RelayMessage` envelope 并转发到 domain handler；prompt/session、workspace detect/browse、tool calls、materialization、MCP、extension 和 terminal 各自持有需要的依赖。
- `CommandExtensionActionInvoke` 与 `CommandExtensionChannelInvoke` 由 extension handler 准备 package artifact cache、activation context 和 workspace context 后进入 `LocalExtensionHostManager`。
- Extension Host workspace/process Host API 的默认 root 来自 activation/session workspace context；Host API 参数不接受调用方覆盖 raw workspace root。
- Process Host API 运行态权限键是 `process.exec`、`process.shell`、`process.env.set` 与 `process.env.set:{KEY}`。
- Schema validation 使用同一 JSON Schema 子集：`true/false` schema、`type`、`required`、`properties`、`additionalProperties: false`、`items`、`enum` 和 `const`。

### 4. Validation & Error Matrix

| Condition | Required behavior |
| --- | --- |
| Relay message 属于 prompt/workspace/tool/materialization/MCP/extension/terminal | Router 转交对应 handler |
| Extension Host API 缺少 active extension | 返回 host API 调用错误 |
| Host API 调用缺少 action 或 channel invocation context | 返回 permission denied |
| action/channel method 未声明 Host API permission | 返回 permission denied |
| `process.exec` 设置 env key 但未声明 `process.env.set` 或 `process.env.set:{KEY}` | 返回 permission denied |
| Host API 参数尝试传 raw workspace root | 使用 activation workspace context，越界路径按 workspace guard 失败 |
| action/channel output 不满足 output schema | 返回 host 调用错误 |

### 5. Good/Base/Bad Cases

- Good: `CommandToolShellExec` 只进入 tool handler，terminal manager 不参与普通 shell tool execution。
- Good: extension action 携带 session `root_ref`，workspace/read/process cwd 统一从 activation context 解析。
- Base: 本机无 `mcp_manager` 时，MCP handler 返回 relay response error，不影响其它 relay command 域。
- Boundary mismatch: handler 直接读取 router 上其它域依赖会让 relay 命令之间形成隐式耦合。
- Canonical flow: router 只做 envelope dispatch；domain handler 内完成依赖访问、payload validation 与 response projection。

### 6. Tests Required

- `agentdash-local` handler tests 覆盖 prompt/workspace/tool/materialization/MCP/extension/terminal 的 relay response 分发。
- `agentdash-local extensions::host` 测试 process permission、workspace root context、schema validation 和 action/channel output validation。
- Extension runner tests 覆盖 `ctx.api.process.exec/shell` 和 `ctx.api.channels` 的 action/channel invocation context 透传。

### 7. Boundary Mismatch / Canonical

#### Boundary Mismatch

```rust
// domain handler 从全量 router 状态读取不属于本域的依赖
router.extension_host.invoke(...);
router.terminal_manager.spawn(...);
```

#### Canonical

```rust
match message {
    RelayMessage::CommandExtensionActionInvoke { id, payload } => {
        vec![self.extension.handle_extension_action_invoke(id, payload).await]
    }
    RelayMessage::CommandTerminalSpawn { id, payload } => {
        vec![self.terminal.handle_terminal_spawn(id, payload)]
    }
    _ => ...
}
```

## Scenario: Relay And Local MCP Resolved Transport

### 1. Scope / Trigger

- Trigger: Runtime construction can resolve MCP transport from final VFS facts; relay/direct/local runtime must consume the resolved runtime server surface.
- Scope: cloud `McpRelayProvider`, relay MCP command payloads, local `CommandHandler`, `McpClientManager`, local MCP probe, prompt `mcp_servers` parser, and HTTP/SSE/stdio transport execution.

### 2. Signatures

```rust
#[serde(tag = "type", rename_all = "snake_case")]
pub enum McpTransportConfigRelay {
    Http { url: String, headers: Vec<McpHttpHeaderRelay> },
    Sse { url: String, headers: Vec<McpHttpHeaderRelay> },
    Stdio {
        command: String,
        args: Vec<String>,
        env: Vec<McpEnvVarRelay>,
        cwd: Option<String>,
    },
}

pub struct McpServerRelay {
    pub name: String,
    pub transport: McpTransportConfigRelay,
}

pub struct CommandMcpListToolsPayload {
    pub server: McpServerRelay,
}

pub struct CommandMcpCallToolPayload {
    pub server: McpServerRelay,
    pub tool_name: String,
    pub arguments: Option<serde_json::Map<String, serde_json::Value>>,
}

pub struct CommandMcpProbeTransportPayload {
    pub transport: McpTransportConfigRelay,
}

pub struct ResponseMcpProbeTransportPayload {
    pub status: String, // "ok" | "error" | "unsupported"
    pub latency_ms: Option<u64>,
    pub tools: Option<Vec<McpToolInfoRelay>>,
    pub error: Option<String>,
}

pub struct McpProbeResult {
    pub ok: bool,
    pub tool_count: usize,
    pub message: String,
}
```

Local manager connection key:

```rust
fn connection_key(entry: &ResolvedMcpServerEntry) -> Result<String, anyhow::Error> {
    let raw = serde_json::to_vec(&entry.transport)?;
    let digest = Sha256::digest(raw);
    Ok(format!("{}{digest:x}", connection_key_prefix(&entry.name)?))
}
```

### 3. Contracts

- Cloud relay MCP list/call sends `McpServerRelay { name, transport }` converted from `RuntimeMcpServer` through `agentdash_application::mcp_relay_adapter::runtime_mcp_server_to_relay`.
- Backend selection may still use `server.name` to find a backend that declared the capability; command execution uses the payload `server.transport`.
- Local `McpClientManager::capability_entries()` reports static configured server names as backend capabilities. It is not the source for runtime-resolved transport.
- Local `McpClientManager::list_tools()` and `call_tool()` convert payload `McpServerRelay` through `agentdash_application::mcp_relay_adapter::relay_mcp_server_to_runtime` and connect with that transport.
- Local manager accepts project-scoped relay declarations by default; the project AgentFrame MCP surface is the runtime declaration source.
- Local `mcp_protect_mode` defaults to `false`. When enabled in `local-backend.json`, the static local MCP catalog becomes the protect-mode allowlist and requires an exact server name + resolved transport match before connecting.
- Connection pool identity is `server name + stable SHA-256 hash(serialized resolved transport)`. Same-name servers from different runtime contexts must not share a client when URL, headers, env, or cwd differ.
- `close(server_name)` closes all pooled connections whose exact server-name prefix matches that name.
- stdio execution applies resolved `env` and `cwd` to the spawned process.
- HTTP/SSE execution passes resolved `headers` into `StreamableHttpClientTransportConfig::custom_headers`; invalid header names/values fail the connection with a diagnostic.
- Relay prompt `mcp_servers` parser accepts resolved servers with HTTP/SSE `headers` and stdio `cwd`, then projects them as `RuntimeMcpServer`.
- One-shot relay probe uses the provided transport directly and never enters the manager connection pool.
- One-shot relay probe failures return `ResponseMcpProbeTransportPayload { status: "error", ... }` with `error: None` at the relay envelope. Local runtime panel probe failures return `McpProbeResult { ok: false, ... }`. Connectivity failure is a probe result, not a command transport failure.

### 4. Validation & Error Matrix

| Condition | Required behavior |
| --- | --- |
| Relay list/call payload lacks server transport | Protocol/serialization test failure |
| Payload server name is absent from local static MCP catalog while protect mode is disabled | Connect using the project-scoped resolved declaration |
| Payload server name or resolved transport is absent from local static MCP catalog while protect mode is enabled | Return `local_policy_denied` before opening a client |
| Same server name with different resolved transport | Different connection keys and different clients |
| Same server name with identical resolved transport | Same connection key and client reuse allowed |
| HTTP/SSE header name or value is invalid | Connection fails with header diagnostic |
| stdio cwd is present | Spawned process receives `current_dir(cwd)` |
| stdio env contains resolved facts | Spawned process receives those env vars |
| Relay one-shot probe cannot connect or times out | Return payload `status="error"` with diagnostic |
| Local runtime panel probe cannot connect | Return `{ ok: false, tool_count: 0, message }` |
| `close("foo")` runs while `foo:child` also has a pooled client | Only exact JSON-string-prefix keys for `foo` are closed |

### 5. Good/Base/Bad Cases

- Good: Two AgentRun runtime contexts both use server name `p4-tools`, but their resolved `x-p4-client` headers differ; local manager creates two connection keys and does not reuse the client.
- Good: A stdio `RuntimeMcpServer` carries `env=[P4CLIENT=demo]` and `cwd=F:/work/demo`; the local process starts with both values applied.
- Good: Relay list/call for HTTP MCP sends headers generated by runtime binding, and local HTTP worker receives those headers.
- Base: A static local MCP server declaration still reports its name via `capability_entries()` and can be probed through the same manager path.
- Boundary mismatch: Cloud sends only a server name and expects local config to reconstruct runtime-specific query/header/env/cwd.
- Canonical flow: Cloud sends the resolved server declaration; local applies protect-mode policy when enabled, keys the connection by name plus transport hash, and connects with the payload transport.

### 6. Tests Required

- Relay protocol serialization test asserts `CommandMcpListToolsPayload.server` and `CommandMcpCallToolPayload.server` include name plus full transport.
- Cloud relay provider test asserts list/call converts `RuntimeMcpServer` into `McpServerRelay`, preserving HTTP/SSE headers and stdio cwd/env.
- Local manager tests assert connection key uses server name and stable transport hash, same-name/different-transport isolation, exact close prefix behavior, protect-mode policy denial, header preservation, and stdio env/cwd preservation.
- Local command handler test asserts relay one-shot probe returns payload `status="error"` rather than relay envelope error for connection failures and timeouts.
- Prompt parser tests assert `mcp_servers` entries preserve HTTP/SSE headers and stdio cwd.
- Direct/local HTTP helper tests assert custom headers are passed to rmcp streamable HTTP worker and invalid headers produce diagnostics.

### 7. Non-canonical / Canonical

#### Non-canonical

```json
{
  "command": "mcp.list_tools",
  "payload": { "server_name": "p4-tools" }
}
```

#### Canonical

```json
{
  "command": "mcp.list_tools",
  "payload": {
    "server": {
      "name": "p4-tools",
      "transport": {
        "type": "http",
        "url": "http://127.0.0.1:7357/mcp?p4_client=demo",
        "headers": [{ "name": "x-p4-client", "value": "demo" }]
      }
    }
  }
}
```

### 样式与依赖

- `@agentdash/ui/styles.css` 是 Web/Tauri 共享的唯一全局样式入口
- Local Runtime UI 不直接 import Tauri API，只依赖 `@agentdash/core` 的 `LocalRuntimeClient` port
- 桌面端打开外部网页时通过 `open_external_url` command（仅允许 http/https）

## Validation Matrix

| Condition | Required behavior |
|---|---|
| API 尚未启动 | DashboardHost 展示 starting 状态并轮询 |
| API 端口占用 | `state = error`，UI 展示错误 |
| `/api/health` 非 2xx | 不渲染 Dashboard |
| profile 不存在 | `profile_load()` 返回 `null` |
| runtime 有 Running session | `runtime_restart()` 拒绝 |
| MCP probe 失败 | 返回 `{ ok: false }`，不升级成 command error |
| Tauri CLI 缺失 | 仓库依赖 `@tauri-apps/cli`，不要求全局安装 |

## 禁止模式

- 在 `app-tauri` 复制 Web Dashboard 组件
- Dashboard 绕过 `agentdash-api` 的 Repository/API 契约
- 用 hostname / 随机 UUID 拼 `backend_id`
- 开发脚本直接 POST `/api/backends` 或写死 `backend_id`
- 多个入口各自生成 `machine_id`
- 依赖全局 `cargo tauri`（应使用 `pnpm exec tauri`）
- 在 `app-tauri` / `views` 追加全局 CSS
