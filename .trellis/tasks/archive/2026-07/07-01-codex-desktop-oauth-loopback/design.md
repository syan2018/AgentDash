# 修复桌面包 Codex OAuth 本机回调 - Design

## Architecture Boundary

Codex OAuth is split by authority:

- **Tauri desktop host** owns device-local OAuth work: PKCE verifier generation, loopback listener lifecycle, state validation, browser opening, timeout/cancel cleanup.
- **Cloud `agentdash-api`** owns business authority: current user extraction, provider/credential target authorization, flow status, token exchange with OpenAI, account id extraction, encrypted credential persistence.
- **React settings UI** owns presentation only: start/cancel buttons, status polling, completion refresh, and unavailable state when no desktop bridge exists.

This matches the existing desktop spec: release desktop bundles default to `external` API mode because business data belongs to the cloud server, while local runtime / desktop commands expose device capabilities.

## Canonical Flow

```text
Settings UI
  -> Tauri command codex_oauth_start(api_origin, access_token, provider_id, target)
Tauri
  -> bind localhost callback on 1455 for 127.0.0.1 and ::1
  -> generate state + PKCE verifier/challenge
  -> POST cloud prepare with state + challenge + redirect_uri
Cloud API
  -> validate user/provider/target
  -> create short-lived flow
  -> return auth_url + flow_id + expires_at
Tauri
  -> return StartCodexOAuthResponse to UI
UI
  -> open auth_url through existing open_external_url bridge
Browser/OpenAI
  -> redirect to http://localhost:1455/auth/callback?code=...&state=...
Tauri
  -> validate state
  -> POST cloud complete with code + verifier + state + redirect_uri
Cloud API
  -> verify flow and PKCE challenge
  -> exchange authorization code
  -> save encrypted Codex credential
  -> mark flow completed
UI
  -> poll cloud status until completed/failed
```

## Cloud API Contract

Add generated DTOs in `agentdash-contracts::integration::llm_provider`:

```rust
pub enum CodexOAuthCredentialTargetDto {
    GlobalProvider,
    UserByok,
}

pub struct PrepareCodexOAuthRequest {
    pub state: String,
    pub code_challenge: String,
    pub redirect_uri: String,
}

pub struct CompleteCodexOAuthRequest {
    pub code: String,
    pub state: String,
    pub code_verifier: String,
    pub redirect_uri: String,
}

pub struct FailCodexOAuthRequest {
    pub message: String,
}
```

Use separate endpoints for global vs user targets so existing authorization semantics stay explicit:

```text
POST /api/llm-providers/{id}/codex-oauth/desktop/prepare
POST /api/llm-providers/{id}/user-credential/codex-oauth/desktop/prepare
POST /api/llm-providers/codex-oauth/{flow_id}/complete
POST /api/llm-providers/codex-oauth/{flow_id}/fail
GET  /api/llm-providers/codex-oauth/{flow_id}
POST /api/llm-providers/codex-oauth/{flow_id}/cancel
```

`prepare` returns the existing `StartCodexOAuthResponse` shape (`flow_id`, `auth_url`, `expires_at`) so the current UI wizard can keep its status model. The response auth URL is built by cloud API because `CODEX_OAUTH_AUTHORIZE_URL`, `CODEX_OAUTH_CLIENT_ID`, scopes, and extra Codex params already live there.

`complete` returns `CodexOAuthStatusResponse` and is the only route that performs token exchange. It checks:

- current authenticated user owns the flow
- provider id and target match the prepared flow
- flow is pending and not expired
- submitted `state` matches the prepared flow
- SHA-256 challenge derived from `code_verifier` matches the prepared `code_challenge`
- `redirect_uri` matches the prepared redirect URI

`fail` is for trusted desktop local callback failures after prepare, such as timeout or callback state mismatch. It marks the cloud status failed with a sanitized message so polling UI does not stay pending.

## Cloud Flow Store

Use an in-memory, short-lived flow store matching the existing OAuth status model:

```rust
struct CodexOAuthPreparedFlow {
    provider_id: Uuid,
    user_id: String,
    target: CodexOAuthCredentialTarget,
    state: String,
    code_challenge: String,
    redirect_uri: String,
    expires_at: DateTime<Utc>,
    status: OAuthFlowStatus,
}
```

No database migration is planned. These OAuth flows are interactive, single-device, and time-limited; durable credential storage still happens through the existing LLM Provider repositories after token exchange succeeds.

## Tauri Local Flow Store

Add a focused module under `crates/agentdash-local-tauri/src/`, for example `codex_oauth.rs`, instead of growing `main.rs`. It should provide commands similar to:

```rust
#[tauri::command]
async fn codex_oauth_start(
    state: State<'_, DesktopState>,
    request: DesktopCodexOAuthStartRequest,
) -> Result<StartCodexOAuthResponse, String>;

#[tauri::command]
async fn codex_oauth_cancel(
    state: State<'_, DesktopState>,
    flow_id: String,
) -> Result<(), String>;
```

`DesktopCodexOAuthStartRequest` carries `api_origin`, `access_token`, `provider_id`, and target (`global_provider | user_byok`). The token is used only for authenticated prepare/complete/cancel/fail calls and must never be logged.

Local state stores `flow_id -> cancel_tx` after cloud prepare succeeds. Cancellation shuts down local accept and calls cloud cancel.

## Loopback Listener

The redirect URI remains `http://localhost:1455/auth/callback`. Because browsers may resolve `localhost` to IPv6 first, the local listener must cover both loopback families:

- bind `127.0.0.1:1455`
- bind `[::1]:1455` when the platform supports it
- accept the first valid callback
- shut down the sibling listener and flow timeout after success/failure/cancel

If the port is occupied, start fails before browser launch with a clear UI message.

## Frontend Integration

Extend the desktop app bridge in `packages/app-tauri/src/desktopSettings.ts` and the global bridge registration in `packages/app-tauri/src/App.tsx` with Codex OAuth start/cancel operations.

In `packages/app-web`, keep `OAuthLoginWizard` generic but make Codex OAuth actions choose the desktop bridge when available. Without a desktop bridge, the Codex OAuth button should be disabled or show a concise unavailable message because pure cloud web cannot receive a user-machine `localhost` callback.

Frontend service additions consume generated DTOs from `llm-provider-contracts.ts` as the single wire source, keeping drift detection in the Rust contract generator and TypeScript check path.

## Security And Logging

- `code`, `code_verifier`, access token, refresh token, and bearer tokens remain memory-only operational values and are omitted from logs, diagnostics, UI messages, and flow status messages.
- `state` can be treated as opaque flow correlation but should still be logged only when needed and never with token values.
- Cloud complete errors use stable user-facing messages; detailed OpenAI response bodies should be sanitized before status exposure.
- `open_external_url` remains http/https only.

## Trade-offs

- Keeping token exchange in cloud avoids duplicating Codex credential parsing and persistence in Tauri and keeps long-lived refresh tokens inside the existing encrypted backend storage path.
- Letting cloud build `auth_url` keeps OpenAI constants and Codex extra params in one owner while still moving loopback networking to the desktop host.
- In-memory cloud flow state is sufficient because the flow is interactive and short-lived. If the cloud API restarts during login, the user restarts OAuth; no stored credential is partially committed.

## Rollback Shape

The change should be landed as a coherent replacement of the Codex OAuth path. If validation fails before release, revert the Codex OAuth bridge/API changes together; no database rollback is expected.
